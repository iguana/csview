pub mod commands;
pub mod engine;
pub mod menu;

use commands::AppState;
use tauri::{Emitter, Manager};
use tauri_plugin_cli::CliExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_cli::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::default())
        .menu(|app| menu::build(app))
        .on_menu_event(|app, event| {
            let id = event.id().as_ref().to_string();
            if !id.starts_with("menu.") {
                return;
            }
            let key = id.strip_prefix("menu.").unwrap_or(&id).to_string();
            // Route to the focused webview window so multi-window apps work.
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
            commands::open_csv,
            commands::read_range,
            commands::search_csv,
            commands::compute_stats,
            commands::sort_csv,
            commands::close_csv,
            commands::reload_with_header,
            commands::update_cell,
            commands::insert_row,
            commands::delete_rows,
            commands::delete_column,
            commands::insert_column,
            commands::save_csv,
            commands::save_csv_as,
            commands::open_in_new_window,
            commands::new_window,
            cli_initial_file,
        ])
        .setup(|app| {
            let mut initial_path: Option<String> = None;
            if let Ok(matches) = app.cli().matches() {
                if let Some(arg) = matches.args.get("file") {
                    if let Some(val) = arg.value.as_str() {
                        initial_path = Some(val.to_string());
                    }
                }
            }
            // Env fallback — useful for automated smoke-testing and debugging.
            if initial_path.is_none() {
                if let Ok(val) = std::env::var("CSVIEW_AUTOLOAD") {
                    if !val.is_empty() {
                        initial_path = Some(val);
                    }
                }
            }
            if let Some(path) = initial_path {
                let main = app.get_webview_window("main").unwrap();
                let demo = std::env::var("CSVIEW_DEMO").is_ok();
                let theme_override = std::env::var("CSVIEW_THEME").ok();
                tauri::async_runtime::spawn(async move {
                    // Give the webview a moment to register listeners.
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    if let Some(t) = theme_override {
                        let _ = main.emit("csview-theme", t);
                    }
                    let _ = main.emit("cli-open-file", path);
                    if demo {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        let sidebar_collapsed =
                            std::env::var("CSVIEW_SIDEBAR_COLLAPSED").is_ok();
                        let palette_override = std::env::var("CSVIEW_PALETTE").ok();
                        let frozen_rows = std::env::var("CSVIEW_FROZEN_ROWS")
                            .ok()
                            .and_then(|s| s.parse::<usize>().ok());
                        let frozen_cols = std::env::var("CSVIEW_FROZEN_COLS")
                            .ok()
                            .and_then(|s| s.parse::<usize>().ok());
                        let hidden_cols: Option<Vec<usize>> = std::env::var("CSVIEW_HIDDEN_COLS")
                            .ok()
                            .map(|s| {
                                s.split(',')
                                    .filter_map(|x| x.trim().parse::<usize>().ok())
                                    .collect()
                            });
                        let jump_to_row = std::env::var("CSVIEW_JUMP_TO_ROW")
                            .ok()
                            .and_then(|s| s.parse::<usize>().ok());
                        let hidden_rows: Option<Vec<usize>> = std::env::var("CSVIEW_HIDDEN_ROWS")
                            .ok()
                            .map(|s| {
                                s.split(',')
                                    .filter_map(|x| x.trim().parse::<usize>().ok())
                                    .collect()
                            });
                        let payload = serde_json::json!({
                            "sort": [{ "column": 5, "direction": "desc" }],
                            "activeCell": { "row": 2, "col": 4 },
                            "search": "Engineer",
                            "sidebarCollapsed": sidebar_collapsed,
                            "palette": palette_override,
                            "frozenRows": frozen_rows,
                            "frozenColumns": frozen_cols,
                            "hiddenColumns": hidden_cols,
                            "hiddenRows": hidden_rows,
                            "jumpToRow": jump_to_row,
                        });
                        let _ = main.emit("csview-demo", payload);
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building csview")
        .run(|app_handle, event| {
            // Fires when macOS asks us to open a file via Finder, Dock drop,
            // Open With… menu, or `open -a csview foo.csv` — anything that
            // uses the Launch Services file-open protocol rather than argv.
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
    // Future: read persisted CLI-provided path; kept to preserve invoke API stability.
    None
}
