//! CSV operation commands backed by `SqliteStore`.
//!
//! These replace the in-memory paged-file approach of the free csview app with
//! a full in-memory SQLite store that supports arbitrary queries and edits.

use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use csview_engine::engine::{ColumnKind, ColumnMeta};
use csview_engine::sqlite_store::{QueryResult, SchemaColumn, SchemaContext, SqliteStore};

use crate::state::AiAppState;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Serialisable command error sent to the frontend.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum CommandError {
    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("unknown file_id: {0}")]
    UnknownFile(String),

    #[error("sqlite error: {0}")]
    Sqlite(String),

    #[error("csv error: {0}")]
    Csv(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("invalid argument: {0}")]
    InvalidArg(String),
}

impl From<csview_engine::sqlite_store::SqliteError> for CommandError {
    fn from(e: csview_engine::sqlite_store::SqliteError) -> Self {
        Self::Sqlite(e.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Returned by `open_csv` and `execute_join` to give the frontend everything
/// it needs to display the file immediately.
#[derive(Debug, Serialize)]
pub struct FileInfo {
    /// UUID v4 handle used in all subsequent commands for this file.
    pub file_id: String,
    pub path: String,
    pub row_count: usize,
    pub columns: Vec<SchemaColumn>,
    pub table_name: String,
}

// ---------------------------------------------------------------------------
// Internal CSV-open helpers
// ---------------------------------------------------------------------------

/// Sniff the field delimiter by reading the first 8 KiB of the file.
///
/// Falls back to `,` when no candidate wins.
fn sniff_delimiter(path: &Path) -> u8 {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("tsv"))
        .unwrap_or(false)
    {
        return b'\t';
    }

    let Ok(file) = fs::File::open(path) else {
        return b',';
    };
    let mut rdr = BufReader::new(file);
    let mut buf = vec![0u8; 8192];
    let n = rdr.read(&mut buf).unwrap_or(0);
    buf.truncate(n);

    let first_line: &[u8] = buf.split(|&b| b == b'\n').next().unwrap_or(&[]);
    let candidates = [b',', b'\t', b';', b'|'];
    let mut best = (b',', 0usize);
    for &c in &candidates {
        let cnt = first_line.iter().filter(|&&b| b == c).count();
        if cnt > best.1 {
            best = (c, cnt);
        }
    }
    best.0
}

/// Return `(headers, has_header)` for a CSV file.
fn detect_headers(path: &Path, delimiter: u8) -> Result<(Vec<String>, bool), CommandError> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)
        .map_err(|e| CommandError::Csv(e.to_string()))?;

    let mut iter = rdr.records();
    let first = match iter.next() {
        Some(Ok(r)) => r,
        Some(Err(e)) => return Err(CommandError::Csv(e.to_string())),
        None => return Err(CommandError::Csv("empty file".into())),
    };
    let second = iter.next().and_then(|r| r.ok());

    let first_row: Vec<String> = first.iter().map(str::to_string).collect();
    let has_header = looks_like_header(&first_row, second.as_ref());

    if has_header {
        Ok((first_row, true))
    } else {
        let n = first_row.len();
        let headers: Vec<String> = (1..=n).map(|i| format!("col_{i}")).collect();
        Ok((headers, false))
    }
}

fn looks_like_header(first: &[String], second: Option<&csv::StringRecord>) -> bool {
    if first.is_empty() {
        return false;
    }
    let second = match second {
        Some(s) => s,
        None => return first.iter().all(|v| !v.is_empty() && v.parse::<f64>().is_err()),
    };
    let first_numeric = first.iter().filter(|v| v.parse::<f64>().is_ok()).count();
    let second_numeric = second.iter().filter(|v| v.parse::<f64>().is_ok()).count();
    if first_numeric == 0 && second_numeric > 0 {
        return true;
    }
    let all_non_empty = first.iter().all(|v| !v.is_empty());
    let short = first.iter().all(|v| v.len() < 64);
    let mut seen = std::collections::HashSet::new();
    let distinct = first.iter().all(|v| seen.insert(v.as_str()));
    all_non_empty && short && distinct && first_numeric == 0
}

/// Infer column types from a sample of data rows.
fn infer_columns(headers: &[String], sample: &[Vec<String>]) -> Vec<ColumnMeta> {
    headers
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut int_c = 0usize;
            let mut float_c = 0usize;
            let mut non_empty = 0usize;
            for row in sample {
                let v = row.get(i).map(String::as_str).unwrap_or("");
                if v.is_empty() {
                    continue;
                }
                non_empty += 1;
                if v.parse::<i64>().is_ok() {
                    int_c += 1;
                } else if v.parse::<f64>().is_ok() {
                    float_c += 1;
                }
            }
            let kind = if non_empty == 0 {
                ColumnKind::Empty
            } else if int_c == non_empty {
                ColumnKind::Integer
            } else if int_c + float_c == non_empty {
                ColumnKind::Float
            } else {
                ColumnKind::String
            };
            ColumnMeta { index: i, name: name.clone(), kind }
        })
        .collect()
}

/// Public wrapper around `infer_columns` — used by `commands_ai::execute_join`.
pub fn infer_columns_pub(headers: &[String], sample: &[Vec<String>]) -> Vec<ColumnMeta> {
    infer_columns(headers, sample)
}

/// Read up to `n` data rows from a CSV for type-inference purposes.
fn sample_rows(path: &Path, delimiter: u8, has_header: bool, n: usize) -> Vec<Vec<String>> {
    let Ok(mut rdr) = csv::ReaderBuilder::new()
        .has_headers(has_header)
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)
    else {
        return vec![];
    };
    rdr.records()
        .take(n)
        .filter_map(|r| r.ok())
        .map(|r| r.iter().map(str::to_string).collect())
        .collect()
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Open a CSV file and import it into an in-memory `SqliteStore`.
///
/// Returns a `FileInfo` containing the generated `file_id` which must be
/// passed to all subsequent commands.
#[tauri::command]
pub fn open_csv(state: State<'_, AiAppState>, path: String) -> Result<FileInfo, CommandError> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(CommandError::FileNotFound(path));
    }

    let delimiter = sniff_delimiter(p);
    let (headers, has_header) = detect_headers(p, delimiter)?;
    let sample = sample_rows(p, delimiter, has_header, 500);
    let columns = infer_columns(&headers, &sample);

    let store = SqliteStore::from_csv(&path, delimiter, has_header, &headers, &columns)
        .map_err(CommandError::from)?;

    let schema = store.schema_context(5).map_err(CommandError::from)?;

    let file_id = Uuid::new_v4().to_string();
    let info = FileInfo {
        file_id: file_id.clone(),
        path: path.clone(),
        row_count: store.row_count(),
        columns: schema.columns,
        table_name: store.table_name().to_string(),
    };

    // Record the source path and insert the store.
    state.stores.lock().insert(file_id.clone(), store);
    state.open_paths.lock().insert(file_id, path);
    Ok(info)
}

/// Execute an arbitrary SELECT query against the file's in-memory SQLite table.
#[tauri::command]
pub fn query_data(
    state: State<'_, AiAppState>,
    file_id: String,
    sql: String,
) -> Result<QueryResult, CommandError> {
    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    store.query(&sql).map_err(CommandError::from)
}

/// Read a page of rows for virtual-scroll grid display.
#[tauri::command]
pub fn read_range(
    state: State<'_, AiAppState>,
    file_id: String,
    offset: usize,
    limit: usize,
    order_by: Option<String>,
) -> Result<QueryResult, CommandError> {
    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    store
        .read_range(offset, limit, order_by.as_deref())
        .map_err(CommandError::from)
}

/// Update a single cell identified by its SQLite `rowid`.
#[tauri::command]
pub fn update_cell(
    state: State<'_, AiAppState>,
    file_id: String,
    rowid: i64,
    column: String,
    value: String,
) -> Result<(), CommandError> {
    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    store.update_cell(rowid, &column, &value).map_err(CommandError::from)
}

/// Insert a new row with the given column → value map.
///
/// Returns the new row's `rowid`.
#[tauri::command]
pub fn insert_row(
    state: State<'_, AiAppState>,
    file_id: String,
    values: HashMap<String, String>,
) -> Result<i64, CommandError> {
    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    let pairs: Vec<(String, String)> = values.into_iter().collect();
    let pairs_ref: Vec<(&str, &str)> =
        pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    store.insert_row(&pairs_ref).map_err(CommandError::from)
}

/// Delete the rows with the given `rowid`s.
///
/// Returns the number of rows deleted.
#[tauri::command]
pub fn delete_rows(
    state: State<'_, AiAppState>,
    file_id: String,
    rowids: Vec<i64>,
) -> Result<usize, CommandError> {
    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    store.delete_rows(&rowids).map_err(CommandError::from)
}

/// Export the current table contents back to the file's original path.
#[tauri::command]
pub fn save_csv(state: State<'_, AiAppState>, file_id: String) -> Result<(), CommandError> {
    let path = state
        .open_paths
        .lock()
        .get(&file_id)
        .cloned()
        .ok_or_else(|| CommandError::InvalidArg("original path not found — use save_csv_as".into()))?;

    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id))?;
    let delimiter = if path.to_lowercase().ends_with(".tsv") { b'\t' } else { b',' };
    store.export_csv(&path, delimiter).map_err(CommandError::from)
}

/// Export the current table contents to an explicit path.
#[tauri::command]
pub fn save_csv_as(
    state: State<'_, AiAppState>,
    file_id: String,
    path: String,
) -> Result<(), CommandError> {
    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    let delimiter = if path.to_lowercase().ends_with(".tsv") { b'\t' } else { b',' };
    store.export_csv(&path, delimiter).map_err(CommandError::from)
}

/// Return the full `SchemaContext` for the given file.
#[tauri::command]
pub fn get_schema(
    state: State<'_, AiAppState>,
    file_id: String,
) -> Result<SchemaContext, CommandError> {
    let stores = state.stores.lock();
    let store =
        stores.get(&file_id).ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    store.schema_context(5).map_err(CommandError::from)
}

/// Remove the file from the open-store map, freeing the in-memory SQLite DB.
#[tauri::command]
pub fn close_file(state: State<'_, AiAppState>, file_id: String) -> Result<(), CommandError> {
    state
        .stores
        .lock()
        .remove(&file_id)
        .ok_or_else(|| CommandError::UnknownFile(file_id.clone()))?;
    state.open_paths.lock().remove(&file_id);
    Ok(())
}
