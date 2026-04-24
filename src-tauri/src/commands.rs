use std::collections::HashMap;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewUrl, WebviewWindowBuilder};

use crate::engine::{
    ColumnStats, CsvFile, CsvMetadata, EngineError, SearchHit, SortKey,
};

#[derive(Default)]
pub struct AppState {
    pub files: Mutex<HashMap<String, CsvFile>>,
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    message: String,
}

impl From<EngineError> for CommandError {
    fn from(e: EngineError) -> Self {
        Self { message: e.to_string() }
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

type Res<T> = Result<T, CommandError>;

#[tauri::command]
pub fn open_csv(
    state: State<'_, AppState>,
    path: String,
    force_header: Option<bool>,
) -> Res<CsvMetadata> {
    let csv = CsvFile::open_with(&path, force_header)?;
    let file_id = uuid::Uuid::new_v4().to_string();
    let meta = csv.metadata(file_id.clone())?;
    state.files.lock().insert(file_id, csv);
    Ok(meta)
}

#[tauri::command]
pub fn read_range(
    state: State<'_, AppState>,
    file_id: String,
    start: usize,
    end: usize,
) -> Res<Vec<Vec<String>>> {
    let files = state.files.lock();
    let csv = files.get(&file_id).ok_or_else(|| "unknown file_id".to_string())?;
    Ok(csv.read_range(start, end)?)
}

#[tauri::command]
pub fn search_csv(
    state: State<'_, AppState>,
    file_id: String,
    query: String,
    limit: usize,
) -> Res<Vec<SearchHit>> {
    let files = state.files.lock();
    let csv = files.get(&file_id).ok_or_else(|| "unknown file_id".to_string())?;
    Ok(csv.search(&query, limit)?)
}

#[tauri::command]
pub fn compute_stats(
    state: State<'_, AppState>,
    file_id: String,
    column: usize,
) -> Res<ColumnStats> {
    let files = state.files.lock();
    let csv = files.get(&file_id).ok_or_else(|| "unknown file_id".to_string())?;
    Ok(csv.stats(column)?)
}

#[tauri::command]
pub fn sort_csv(
    state: State<'_, AppState>,
    file_id: String,
    keys: Vec<SortKey>,
) -> Res<()> {
    let mut files = state.files.lock();
    let csv = files.get_mut(&file_id).ok_or_else(|| "unknown file_id".to_string())?;
    csv.sort(&keys)?;
    Ok(())
}

#[tauri::command]
pub fn close_csv(state: State<'_, AppState>, file_id: String) -> Res<()> {
    state.files.lock().remove(&file_id);
    Ok(())
}

#[tauri::command]
pub fn reload_with_header(
    state: State<'_, AppState>,
    file_id: String,
    has_header: bool,
) -> Res<CsvMetadata> {
    let files = state.files.lock();
    let existing = files.get(&file_id).ok_or_else(|| "unknown file_id".to_string())?;
    let path = existing.path.clone();
    drop(files);
    let csv = CsvFile::open_with(&path, Some(has_header))?;
    let meta = csv.metadata(file_id.clone())?;
    state.files.lock().insert(file_id, csv);
    Ok(meta)
}

// ---------- editing ----------

#[tauri::command]
pub fn update_cell(
    state: State<'_, AppState>,
    file_id: String,
    row: usize,
    column: usize,
    value: String,
) -> Res<CsvMetadata> {
    let mut files = state.files.lock();
    let csv = files
        .get_mut(&file_id)
        .ok_or_else(|| "unknown file_id".to_string())?;
    csv.update_cell(row, column, value)?;
    Ok(csv.metadata(file_id)?)
}

#[tauri::command]
pub fn insert_row(
    state: State<'_, AppState>,
    file_id: String,
    at: Option<usize>,
    values: Option<Vec<String>>,
) -> Res<CsvMetadata> {
    let mut files = state.files.lock();
    let csv = files
        .get_mut(&file_id)
        .ok_or_else(|| "unknown file_id".to_string())?;
    csv.insert_row(at, values)?;
    Ok(csv.metadata(file_id)?)
}

#[tauri::command]
pub fn delete_rows(
    state: State<'_, AppState>,
    file_id: String,
    rows: Vec<usize>,
) -> Res<CsvMetadata> {
    let mut files = state.files.lock();
    let csv = files
        .get_mut(&file_id)
        .ok_or_else(|| "unknown file_id".to_string())?;
    csv.delete_rows(&rows)?;
    Ok(csv.metadata(file_id)?)
}

#[derive(Debug, serde::Serialize)]
pub struct DeleteColumnResult {
    pub metadata: CsvMetadata,
    /// Removed header name; preserved so the frontend can restore it on undo.
    pub removed_name: String,
    /// Per-row values that were deleted (in raw storage order).
    pub removed_values: Vec<String>,
}

#[tauri::command]
pub fn delete_column(
    state: State<'_, AppState>,
    file_id: String,
    column: usize,
) -> Res<DeleteColumnResult> {
    let mut files = state.files.lock();
    let csv = files
        .get_mut(&file_id)
        .ok_or_else(|| "unknown file_id".to_string())?;
    let (removed_name, removed_values) = csv.delete_column(column)?;
    let metadata = csv.metadata(file_id)?;
    Ok(DeleteColumnResult {
        metadata,
        removed_name,
        removed_values,
    })
}

#[tauri::command]
pub fn insert_column(
    state: State<'_, AppState>,
    file_id: String,
    at: usize,
    name: String,
    values: Vec<String>,
) -> Res<CsvMetadata> {
    let mut files = state.files.lock();
    let csv = files
        .get_mut(&file_id)
        .ok_or_else(|| "unknown file_id".to_string())?;
    csv.insert_column(at, name, values)?;
    Ok(csv.metadata(file_id)?)
}

#[tauri::command]
pub fn save_csv(
    state: State<'_, AppState>,
    file_id: String,
) -> Res<CsvMetadata> {
    let mut files = state.files.lock();
    let csv = files
        .get_mut(&file_id)
        .ok_or_else(|| "unknown file_id".to_string())?;
    let path = csv.path.clone();
    csv.save_to(&path)?;
    Ok(csv.metadata(file_id)?)
}

#[tauri::command]
pub fn save_csv_as(
    state: State<'_, AppState>,
    file_id: String,
    path: String,
) -> Res<CsvMetadata> {
    let mut files = state.files.lock();
    let csv = files
        .get_mut(&file_id)
        .ok_or_else(|| "unknown file_id".to_string())?;
    csv.save_to(&PathBuf::from(&path))?;
    Ok(csv.metadata(file_id)?)
}

// ---------- multi-window ----------

#[tauri::command]
pub fn open_in_new_window(app: AppHandle, path: String) -> Res<()> {
    spawn_window(&app, Some(path))
}

#[tauri::command]
pub fn new_window(app: AppHandle) -> Res<()> {
    spawn_window(&app, None)
}

fn spawn_window(app: &AppHandle, path: Option<String>) -> Res<()> {
    let label = format!("w{}", uuid::Uuid::new_v4().simple());
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::default())
        .title("csview")
        .inner_size(1280.0, 800.0)
        .min_inner_size(640.0, 400.0)
        .title_bar_style(tauri::TitleBarStyle::Transparent)
        .hidden_title(true)
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(p) = path {
        // Give the new webview a moment to mount its listeners before the
        // startup file event is delivered.
        let w = window.clone();
        tauri::async_runtime::spawn(async move {
            std::thread::sleep(std::time::Duration::from_millis(700));
            let _ = w.emit("cli-open-file", p);
        });
    }
    Ok(())
}
