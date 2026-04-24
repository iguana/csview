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
use tauri::{Emitter, Manager};
use tauri_plugin_cli::CliExt;

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

            // Load .env file if present (for dev convenience).
            if let Ok(contents) = std::fs::read_to_string(".env") {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') { continue; }
                    if let Some((k, v)) = line.split_once('=') {
                        std::env::set_var(k.trim(), v.trim());
                    }
                }
            }

            // Restore saved API key from DB, or auto-detect from env vars.
            let stored: Option<(String, String)> = conn
                .query_row(
                    "SELECT api_key, model FROM account ORDER BY id DESC LIMIT 1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .ok();

            let state = AiAppState::new(conn);

            if let Some((compound_key, model)) = stored {
                // compound_key = "provider:actual_key"
                if let Some((prov_str, actual_key)) = compound_key.split_once(':') {
                    let provider = match prov_str {
                        "openai" => crate::llm::client::Provider::OpenAI,
                        "google" => crate::llm::client::Provider::Google,
                        _ => crate::llm::client::Provider::Anthropic,
                    };
                    *state.llm.lock() = Some(crate::llm::LlmClient::new(provider, actual_key.to_string(), model));
                }
            } else {
                // No saved key — try env vars
                crate::commands_ai::auto_detect_keys(&state);
            }

            app.manage(state);

            // Handle CLI --file argument.
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
                let main = app.get_webview_window("main").unwrap();
                tauri::async_runtime::spawn(async move {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    let _ = main.emit("cli-open-file", path);
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building csviewai")
        .run(|app_handle, event| {
            // macOS file-open events (Finder, Dock drop, Open With…).
            if let tauri::RunEvent::Opened { urls } = event {
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        let target = app_handle
                            .webview_windows()
                            .into_iter()
                            .find(|(_, w)| w.is_focused().unwrap_or(false))
                            .map(|(_, w)| w)
                            .or_else(|| app_handle.get_webview_window("main"));
                        if let Some(window) = target {
                            let _ = window.emit(
                                "cli-open-file",
                                path.to_string_lossy().to_string(),
                            );
                        }
                    }
                }
            }
        });
}

#[tauri::command]
fn cli_initial_file() -> Option<String> {
    None
}
