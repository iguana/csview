//! Native macOS menu bar with AI-extended items.
//!
//! Extends the standard File/Edit/View/Window structure from the free csview
//! app with an "AI" top-level menu and additional Edit items.

use tauri::menu::{
    AboutMetadataBuilder, Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
    SubmenuBuilder,
};
use tauri::{AppHandle, Wry};

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    // --- Application menu (macOS) -------------------------------------------
    let about = AboutMetadataBuilder::new()
        .name(Some("csviewai"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .copyright(Some("AI-powered CSV analysis for macOS"))
        .build();

    let app_menu = SubmenuBuilder::new(app, "csviewai")
        .about(Some(about))
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    // --- File ---------------------------------------------------------------
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(
            &MenuItemBuilder::with_id("menu.new_window", "New Window")
                .accelerator("CmdOrCtrl+N")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.open", "Open…")
                .accelerator("CmdOrCtrl+O")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.open_in_new_window", "Open CSV in New Window…")
                .accelerator("CmdOrCtrl+Shift+O")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.save", "Save")
                .accelerator("CmdOrCtrl+S")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.save_as", "Save As…")
                .accelerator("CmdOrCtrl+Shift+S")
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    // --- Edit ---------------------------------------------------------------
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(
            &MenuItemBuilder::with_id("menu.undo", "Undo")
                .accelerator("CmdOrCtrl+Z")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.redo", "Redo")
                .accelerator("CmdOrCtrl+Shift+Z")
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.find", "Find…")
                .accelerator("CmdOrCtrl+F")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.insert_row", "Insert Row")
                .accelerator("CmdOrCtrl+Return")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.delete_row", "Delete Row…")
                .accelerator("CmdOrCtrl+Backspace")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.delete_column", "Delete Column…")
                .accelerator("CmdOrCtrl+Shift+Backspace")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.autosize_column", "Auto-Size Column")
                .accelerator("CmdOrCtrl+Alt+0")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.autosize_all_columns", "Auto-Size All Columns")
                .build(app)?,
        )
        .separator()
        .item(&MenuItemBuilder::with_id("menu.clear_sort", "Clear Sort").build(app)?)
        .separator()
        // AI quick-access items in Edit.
        .item(
            &MenuItemBuilder::with_id("menu.ai_query", "AI Query…")
                .accelerator("CmdOrCtrl+Shift+Q")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.ai_profile", "Data Profile")
                .accelerator("CmdOrCtrl+Shift+P")
                .build(app)?,
        )
        .build()?;

    // --- View ---------------------------------------------------------------
    let theme_mode_submenu = SubmenuBuilder::new(app, "Theme")
        .item(&MenuItemBuilder::with_id("menu.theme.system", "Auto (System)").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.theme.light", "Light").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.theme.dark", "Dark").build(app)?)
        .build()?;

    let palette_submenu = SubmenuBuilder::new(app, "Palette")
        .item(&MenuItemBuilder::with_id("menu.palette.parchment", "Parchment").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.palette.noir", "Noir").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.palette.solarized", "Solarized").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.palette.ocean", "Ocean").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.palette.forest", "Forest").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.palette.graphite", "Graphite").build(app)?)
        .build()?;

    let row_height_submenu = SubmenuBuilder::new(app, "Row Height")
        .item(&MenuItemBuilder::with_id("menu.row_height.compact", "Compact").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.row_height.normal", "Normal").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.row_height.tall", "Tall").build(app)?)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .item(
            &MenuItemBuilder::with_id("menu.toggle_sidebar", "Toggle Sidebar")
                .accelerator("CmdOrCtrl+B")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id("menu.toggle_header", "Toggle Header Row").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.freeze_rows_to_cursor", "Freeze Rows to Cursor")
                .accelerator("CmdOrCtrl+Alt+R")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(
                "menu.freeze_columns_to_cursor",
                "Freeze Columns to Cursor",
            )
            .accelerator("CmdOrCtrl+Alt+F")
            .build(app)?,
        )
        .item(&MenuItemBuilder::with_id("menu.unfreeze_all", "Unfreeze All").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.hide_row", "Hide Row")
                .accelerator("CmdOrCtrl+Alt+H")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.hide_column", "Hide Column")
                .accelerator("CmdOrCtrl+Shift+0")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu.show_all_hidden", "Show All Hidden")
                .accelerator("CmdOrCtrl+Shift+H")
                .build(app)?,
        )
        .separator()
        .item(&row_height_submenu)
        .separator()
        .item(&theme_mode_submenu)
        .item(&palette_submenu)
        .separator()
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;

    // --- AI -----------------------------------------------------------------
    let ai_menu = SubmenuBuilder::new(app, "AI")
        .item(
            &MenuItemBuilder::with_id("menu.ai.chat", "Chat")
                .accelerator("CmdOrCtrl+Shift+C")
                .build(app)?,
        )
        .separator()
        .item(&MenuItemBuilder::with_id("menu.ai.profile", "Data Profile").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.ai.transform", "Column Transform").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.ai.anomaly", "Anomaly Detection").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.ai.quality", "Data Quality Audit").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("menu.ai.report", "Report Builder").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.ai.join", "Join Assistant").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("menu.ai.compliance", "Compliance Scan").build(app)?)
        .item(&MenuItemBuilder::with_id("menu.ai.forecast", "Forecast").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.ai.settings", "AI Settings…").build(app)?,
        )
        .build()?;

    // --- Window -------------------------------------------------------------
    let window_menu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .separator()
        .item(&MenuItemBuilder::with_id("menu.new_window", "New Window").build(app)?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    MenuBuilder::new(app)
        .item(&app_menu)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&ai_menu)
        .item(&window_menu)
        .build()
}
