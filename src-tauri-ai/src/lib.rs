//! csviewai — AI-powered CSV analysis desktop application.
//!
//! This crate is the Tauri backend for the csviewai binary.  It is completely
//! independent of `csview_lib`; it only imports `csview-engine` for the shared
//! CSV / SQLite / stats / join / quality primitives.

pub mod commands_ai;
pub mod commands_csv;
pub mod db;
pub mod llm;
pub mod menu;
pub mod state;

use state::AiAppState;
use std::sync::OnceLock;
use parking_lot::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_cli::CliExt;

/// Paths queued at cold start (CLI arg, `CSVIEW_AUTOLOAD`, or macOS Open With…
/// events that arrive *before* `setup()` has run). On macOS, `RunEvent::Opened`
/// can fire during `applicationWillFinishLaunching`, before Tauri's setup hook
/// runs and `app.manage(state)` is called — accessing `app.state::<T>()` at
/// that point would panic across the Objective-C boundary. Module-level
/// globals sidestep that lifetime issue.
static PENDING_OPEN_PATHS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static FRONTEND_READY: OnceLock<Mutex<bool>> = OnceLock::new();

fn pending_paths() -> &'static Mutex<Vec<String>> {
    PENDING_OPEN_PATHS.get_or_init(|| Mutex::new(Vec::new()))
}
fn frontend_ready() -> &'static Mutex<bool> {
    FRONTEND_READY.get_or_init(|| Mutex::new(false))
}

/// Entry point — called from `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_cli::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .menu(|app| menu::build(app))
        .on_menu_event(|app, event| {
            let id = event.id().as_ref().to_string();
            if !id.starts_with("menu.") {
                return;
            }
            let key = id.strip_prefix("menu.").unwrap_or(&id).to_string();
            let target = app
                .webview_windows()
                .into_iter()
                .find(|(_, w)| w.is_focused().unwrap_or(false))
                .map(|(_, w)| w)
                .or_else(|| app.get_webview_window("main"));
            if let Some(window) = target {
                let _ = window.emit("menu-action", key);
            }
        })
        .invoke_handler(tauri::generate_handler![
            // CSV operations
            commands_csv::open_csv,
            commands_csv::query_data,
            commands_csv::read_range,
            commands_csv::update_cell,
            commands_csv::insert_row,
            commands_csv::delete_rows,
            commands_csv::delete_column,
            commands_csv::save_csv,
            commands_csv::save_csv_as,
            commands_csv::get_schema,
            commands_csv::close_file,
            commands_csv::new_window,
            commands_csv::open_in_new_window,
            // Account
            commands_ai::set_api_key,
            commands_ai::get_account_status,
            commands_ai::fetch_provider_models,
            commands_ai::make_chart,
            // Feature 1 — NL query
            commands_ai::nl_query,
            // Feature 2 — Data profile
            commands_ai::generate_profile,
            // Feature 3 — Column transform
            commands_ai::nl_transform,
            // Feature 4 — Anomaly detection
            commands_ai::detect_anomalies_cmd,
            // Feature 5 — Smart group
            commands_ai::smart_group,
            // Feature 6 — Quality audit
            commands_ai::audit_quality,
            // Feature 7 — Chat
            commands_ai::new_chat_session,
            commands_ai::chat_message,
            // Feature 8 — Report builder
            commands_ai::generate_report,
            // Feature 9 — Join
            commands_ai::suggest_join,
            commands_ai::execute_join,
            // Feature 10 — Compliance
            commands_ai::compliance_scan,
            // Feature 11 — Forecast
            commands_ai::forecast,
            // Misc
            cli_initial_file,
        ])
        .setup(|app| {
            // Open (or create) the persistent app database.
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not resolve app data directory");
            std::fs::create_dir_all(&data_dir)
                .expect("could not create app data directory");
            let db_path = data_dir.join("csviewai.db");
            let conn = db::init_app_db(&db_path)
                .expect("could not initialise app database");
            db::migrations::run_migrations(&conn)
                .expect("could not run database migrations");

            // Load .env file if present (dev-only convenience). In a packaged
            // .app the CWD is usually `/` so this just no-ops.
            if let Ok(contents) = std::fs::read_to_string(".env") {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') { continue; }
                    if let Some((k, v)) = line.split_once('=') {
                        std::env::set_var(k.trim(), v.trim());
                    }
                }
            }

            let state = AiAppState::new(conn);

            // Single source of truth for picking the LLM client at startup —
            // loads the saved account row (validating non-empty model) and
            // falls back to env vars. See `commands_ai::auto_detect_keys`.
            crate::commands_ai::auto_detect_keys(&state);

            // Handle CLI --file argument and CSVIEW_AUTOLOAD. Buffer into
            // pending_open_paths so the frontend picks it up via
            // `cli_initial_file` once it's mounted.
            let mut initial_path: Option<String> = None;
            if let Ok(matches) = app.cli().matches() {
                if let Some(arg) = matches.args.get("file") {
                    if let Some(val) = arg.value.as_str() {
                        initial_path = Some(val.to_string());
                    }
                }
            }
            if initial_path.is_none() {
                if let Ok(val) = std::env::var("CSVIEW_AUTOLOAD") {
                    if !val.is_empty() {
                        initial_path = Some(val);
                    }
                }
            }
            if let Some(path) = initial_path {
                pending_paths().lock().push(path);
            }

            app.manage(state);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building csviewai")
        .run(|app_handle, event| {
            // macOS file-open events (Finder, Dock drop, Open With…).
            // NOTE: this can fire *before* `setup()` runs, so we must not
            // touch `app.state::<AiAppState>()` here — it would panic across
            // the Objective-C boundary. Use module-level globals instead.
            if let tauri::RunEvent::Opened { urls } = event {
                let ready = *frontend_ready().lock();
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        let path_str = path.to_string_lossy().to_string();
                        if ready {
                            let target = app_handle
                                .webview_windows()
                                .into_iter()
                                .find(|(_, w)| w.is_focused().unwrap_or(false))
                                .map(|(_, w)| w)
                                .or_else(|| app_handle.get_webview_window("main"));
                            if let Some(window) = target {
                                let _ = window.emit("cli-open-file", path_str);
                            }
                        } else {
                            // Frontend hasn't mounted yet — buffer until it
                            // calls `cli_initial_file` to drain.
                            pending_paths().lock().push(path_str);
                        }
                    }
                }
            }
        });
}

/// Drain any paths queued during cold start (CLI arg, env autoload, or macOS
/// `Opened` events fired before the frontend mounted) and mark the frontend
/// as ready so subsequent opens flow through the live `cli-open-file` event.
#[tauri::command]
fn cli_initial_file() -> Vec<String> {
    *frontend_ready().lock() = true;
    pending_paths().lock().drain(..).collect()
}
