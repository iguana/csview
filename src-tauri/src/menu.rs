//! Native macOS menu bar: File / Edit / View / Window. Custom items have
//! stable string IDs so the frontend can dispatch on them. Accelerators live
//! here (not in JS) so macOS handles them as first-class menu shortcuts.

use tauri::menu::{
    AboutMetadataBuilder, Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
    SubmenuBuilder,
};
use tauri::{AppHandle, Wry};

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    // --- Application menu (macOS) -------------------------------------------
    let about = AboutMetadataBuilder::new()
        .name(Some("csview"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .copyright(Some("A fast, clean CSV viewer for macOS"))
        .build();

    let app_menu = SubmenuBuilder::new(app, "csview")
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
    // Undo/redo use custom IDs because we back them with our own history
    // stack; cut/copy/paste/selectAll stay predefined so they route to native
    // text inputs (search box, cell editor) correctly.
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
            &MenuItemBuilder::with_id("menu.delete_row", "Delete Row")
                .accelerator("CmdOrCtrl+Backspace")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.clear_sort", "Clear Sort").build(app)?,
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
        .item(
            &MenuItemBuilder::with_id("menu.toggle_header", "Toggle Header Row")
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

    // --- Window -------------------------------------------------------------
    let window_menu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu.new_window", "New Window")
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    MenuBuilder::new(app)
        .item(&app_menu)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&window_menu)
        .build()
}
