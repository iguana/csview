//! CSV engine: paged row access, sorting, search, and stats.
//!
//! Design:
//! - On open, scan the file to build a row index (byte offsets) so random
//!   access is O(1) without holding the whole file in memory.
//! - For small files (<= SMALL_FILE_BYTES), we also cache parsed rows in RAM.
//! - Headers are detected heuristically but can be toggled by the caller.
//! - Sort builds a permutation over the row index. Search streams linearly.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Files up to this size are fully loaded into memory as parsed rows.
pub const SMALL_FILE_BYTES: u64 = 8 * 1024 * 1024; // 8MB
/// Number of rows to deliver in the initial sample.
pub const SAMPLE_ROWS: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("csv parse error: {0}")]
    Csv(#[from] csv::Error),
    #[error("out of range")]
    OutOfRange,
    #[error("empty file")]
    Empty,
}

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    Integer,
    Float,
    Boolean,
    Date,
    String,
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub index: usize,
    pub name: String,
    pub kind: ColumnKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvMetadata {
    pub file_id: String,
    pub path: String,
    pub size_bytes: u64,
    pub row_count: usize,
    pub column_count: usize,
    pub has_header: bool,
    pub columns: Vec<ColumnMeta>,
    pub delimiter: u8,
    pub sample: Vec<Vec<String>>,
    pub fully_loaded: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortKey {
    pub column: usize,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub column: usize,
    pub count: usize,
    pub empty: usize,
    pub unique: usize,
    pub numeric_count: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub sum: Option<f64>,
    pub shortest: Option<String>,
    pub longest: Option<String>,
    pub top_values: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub row: usize,
    pub column: usize,
    pub preview: String,
}

/// Either parsed rows in memory or byte offsets into the original file.
enum Storage {
    InMemory {
        rows: Vec<Vec<String>>,
    },
    Paged {
        path: PathBuf,
        /// Byte offsets of each data row's start, plus a terminal offset at EOF.
        offsets: Vec<u64>,
        delimiter: u8,
    },
}

pub struct CsvFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub delimiter: u8,
    pub has_header: bool,
    pub headers: Vec<String>,
    pub columns: Vec<ColumnMeta>,
    pub row_count: usize,
    pub dirty: bool,
    storage: Storage,
    sort_order: Option<Vec<usize>>,
}

impl CsvFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with(path, None)
    }

    /// Open a CSV file. If `force_header` is set, override heuristic detection.
    pub fn open_with<P: AsRef<Path>>(path: P, force_header: Option<bool>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let size_bytes = std::fs::metadata(&path)?.len();
        let delimiter = sniff_delimiter(&path)?;

        let (headers, has_header) = read_headers(&path, delimiter, force_header)?;

        let file = if size_bytes <= SMALL_FILE_BYTES {
            let rows = read_all_data_rows(&path, delimiter, has_header)?;
            let row_count = rows.len();
            let columns = infer_columns(&headers, &rows);
            Self {
                path,
                size_bytes,
                delimiter,
                has_header,
                headers,
                columns,
                row_count,
                dirty: false,
                storage: Storage::InMemory { rows },
                sort_order: None,
            }
        } else {
            let offsets = build_offsets(&path, has_header)?;
            let row_count = offsets.len().saturating_sub(1);
            let sample = read_sample_for_inference(&path, delimiter, has_header, 1024)?;
            let columns = infer_columns(&headers, &sample);
            Self {
                path: path.clone(),
                size_bytes,
                delimiter,
                has_header,
                headers,
                columns,
                row_count,
                dirty: false,
                storage: Storage::Paged {
                    path,
                    offsets,
                    delimiter,
                },
                sort_order: None,
            }
        };
        if file.column_count() == 0 {
            Err(EngineError::Empty)
        } else {
            Ok(file)
        }
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn is_fully_loaded(&self) -> bool {
        matches!(self.storage, Storage::InMemory { .. })
    }

    /// Read a range of display rows, honoring any active sort.
    pub fn read_range(&self, start: usize, end: usize) -> Result<Vec<Vec<String>>> {
        if start >= self.row_count {
            return Ok(Vec::new());
        }
        let end = end.min(self.row_count);
        let indices: Vec<usize> = (start..end)
            .map(|i| self.resolve_index(i))
            .collect();
        self.read_by_indices(&indices)
    }

    fn resolve_index(&self, display_idx: usize) -> usize {
        match &self.sort_order {
            Some(order) => order[display_idx],
            None => display_idx,
        }
    }

    fn read_by_indices(&self, indices: &[usize]) -> Result<Vec<Vec<String>>> {
        match &self.storage {
            Storage::InMemory { rows } => {
                let mut out = Vec::with_capacity(indices.len());
                for &i in indices {
                    if let Some(r) = rows.get(i) {
                        out.push(r.clone());
                    }
                }
                Ok(out)
            }
            Storage::Paged { path, offsets, delimiter } => {
                let mut file = File::open(path)?;
                let mut out = Vec::with_capacity(indices.len());
                for &i in indices {
                    if i + 1 >= offsets.len() {
                        return Err(EngineError::OutOfRange);
                    }
                    let start = offsets[i];
                    let end = offsets[i + 1];
                    let len = (end - start) as usize;
                    let mut buf = vec![0u8; len];
                    file.seek(SeekFrom::Start(start))?;
                    file.read_exact(&mut buf)?;
                    let mut rdr = csv::ReaderBuilder::new()
                        .has_headers(false)
                        .delimiter(*delimiter)
                        .flexible(true)
                        .from_reader(&buf[..]);
                    let mut rec = csv::StringRecord::new();
                    if rdr.read_record(&mut rec)? {
                        out.push(rec.iter().map(|s| s.to_string()).collect());
                    } else {
                        out.push(Vec::new());
                    }
                }
                Ok(out)
            }
        }
    }

    /// Apply a multi-column sort. Empty keys clears sort.
    pub fn sort(&mut self, keys: &[SortKey]) -> Result<()> {
        if keys.is_empty() {
            self.sort_order = None;
            return Ok(());
        }
        // Collect the columns we need to sort on for all rows.
        let n = self.row_count;
        let mut order: Vec<usize> = (0..n).collect();
        // Snapshot sort keys for all rows up front (avoids repeated file reads).
        let needed_cols: Vec<usize> = keys.iter().map(|k| k.column).collect();
        let values = self.collect_sort_values(&needed_cols)?;
        let kinds: Vec<ColumnKind> = keys
            .iter()
            .map(|k| self.columns.get(k.column).map(|c| c.kind).unwrap_or(ColumnKind::String))
            .collect();
        order.sort_by(|&a, &b| {
            for (idx, key) in keys.iter().enumerate() {
                let av = &values[idx][a];
                let bv = &values[idx][b];
                let ord = compare(av, bv, kinds[idx]);
                let ord = match key.direction {
                    SortDirection::Asc => ord,
                    SortDirection::Desc => ord.reverse(),
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            a.cmp(&b)
        });
        self.sort_order = Some(order);
        Ok(())
    }

    fn collect_sort_values(&self, cols: &[usize]) -> Result<Vec<Vec<String>>> {
        let mut out: Vec<Vec<String>> = cols.iter().map(|_| Vec::with_capacity(self.row_count)).collect();
        match &self.storage {
            Storage::InMemory { rows } => {
                for row in rows {
                    for (i, &c) in cols.iter().enumerate() {
                        out[i].push(row.get(c).cloned().unwrap_or_default());
                    }
                }
            }
            Storage::Paged { path, delimiter, .. } => {
                let file = File::open(path)?;
                let mut rdr = csv::ReaderBuilder::new()
                    .has_headers(self.has_header)
                    .delimiter(*delimiter)
                    .flexible(true)
                    .from_reader(BufReader::new(file));
                for rec in rdr.records() {
                    let rec = rec?;
                    for (i, &c) in cols.iter().enumerate() {
                        out[i].push(rec.get(c).unwrap_or("").to_string());
                    }
                }
            }
        }
        Ok(out)
    }

    /// Text search across all cells. Returns up to `limit` hits.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let needle = query.to_lowercase();
        let mut hits = Vec::new();
        let mut scan = |row: usize, fields: &[String]| -> bool {
            for (col, v) in fields.iter().enumerate() {
                if v.to_lowercase().contains(&needle) {
                    hits.push(SearchHit {
                        row,
                        column: col,
                        preview: v.clone(),
                    });
                    if hits.len() >= limit {
                        return true;
                    }
                }
            }
            false
        };
        match &self.storage {
            Storage::InMemory { rows } => {
                for (i, row) in rows.iter().enumerate() {
                    let display_row = self.row_to_display(i);
                    if scan(display_row, row) {
                        break;
                    }
                }
            }
            Storage::Paged { path, delimiter, .. } => {
                let file = File::open(path)?;
                let mut rdr = csv::ReaderBuilder::new()
                    .has_headers(self.has_header)
                    .delimiter(*delimiter)
                    .flexible(true)
                    .from_reader(BufReader::new(file));
                let mut i = 0usize;
                for rec in rdr.records() {
                    let rec = rec?;
                    let fields: Vec<String> = rec.iter().map(|s| s.to_string()).collect();
                    let display_row = self.row_to_display(i);
                    if scan(display_row, &fields) {
                        break;
                    }
                    i += 1;
                }
            }
        }
        hits.sort_by_key(|h| h.row);
        Ok(hits)
    }

    fn row_to_display(&self, raw: usize) -> usize {
        match &self.sort_order {
            Some(order) => order.iter().position(|&x| x == raw).unwrap_or(raw),
            None => raw,
        }
    }

    /// Compute stats for a single column.
    pub fn stats(&self, column: usize) -> Result<ColumnStats> {
        if column >= self.column_count() {
            return Err(EngineError::OutOfRange);
        }
        let mut count = 0usize;
        let mut empty = 0usize;
        let mut numeric_count = 0usize;
        let mut min_num: Option<f64> = None;
        let mut max_num: Option<f64> = None;
        let mut sum: f64 = 0.0;
        let mut shortest: Option<String> = None;
        let mut longest: Option<String> = None;
        let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        let mut accumulate = |value: &str| {
            count += 1;
            if value.is_empty() {
                empty += 1;
                *freq.entry(String::new()).or_insert(0) += 1;
                return;
            }
            if let Ok(n) = value.parse::<f64>() {
                numeric_count += 1;
                sum += n;
                min_num = Some(min_num.map_or(n, |m| m.min(n)));
                max_num = Some(max_num.map_or(n, |m| m.max(n)));
            }
            match &shortest {
                Some(s) if s.len() <= value.len() => {}
                _ => shortest = Some(value.to_string()),
            }
            match &longest {
                Some(l) if l.len() >= value.len() => {}
                _ => longest = Some(value.to_string()),
            }
            *freq.entry(value.to_string()).or_insert(0) += 1;
        };

        match &self.storage {
            Storage::InMemory { rows } => {
                for row in rows {
                    let v = row.get(column).map(|s| s.as_str()).unwrap_or("");
                    accumulate(v);
                }
            }
            Storage::Paged { path, delimiter, .. } => {
                let file = File::open(path)?;
                let mut rdr = csv::ReaderBuilder::new()
                    .has_headers(self.has_header)
                    .delimiter(*delimiter)
                    .flexible(true)
                    .from_reader(BufReader::new(file));
                for rec in rdr.records() {
                    let rec = rec?;
                    let v = rec.get(column).unwrap_or("");
                    accumulate(v);
                }
            }
        }

        let unique = freq.len();
        let mean = if numeric_count > 0 {
            Some(sum / numeric_count as f64)
        } else {
            None
        };
        let sum_opt = if numeric_count > 0 { Some(sum) } else { None };

        let mut top: Vec<(String, usize)> = freq.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top.truncate(5);

        Ok(ColumnStats {
            column,
            count,
            empty,
            unique,
            numeric_count,
            min: min_num,
            max: max_num,
            mean,
            sum: sum_opt,
            shortest,
            longest,
            top_values: top,
        })
    }

    pub fn metadata(&self, file_id: String) -> Result<CsvMetadata> {
        let sample = self.read_range(0, SAMPLE_ROWS.min(self.row_count))?;
        Ok(CsvMetadata {
            file_id,
            path: self.path.to_string_lossy().into_owned(),
            size_bytes: self.size_bytes,
            row_count: self.row_count,
            column_count: self.column_count(),
            has_header: self.has_header,
            columns: self.columns.clone(),
            delimiter: self.delimiter,
            sample,
            fully_loaded: self.is_fully_loaded(),
            dirty: self.dirty,
        })
    }

    // ---------- editing ----------

    /// Materialize a paged file into memory so it can be edited and saved.
    pub fn materialize(&mut self) -> Result<()> {
        if matches!(self.storage, Storage::InMemory { .. }) {
            return Ok(());
        }
        let rows = match &self.storage {
            Storage::Paged {
                path, delimiter, ..
            } => read_all_data_rows(path, *delimiter, self.has_header)?,
            _ => unreachable!(),
        };
        self.storage = Storage::InMemory { rows };
        Ok(())
    }

    fn rows_mut(&mut self) -> Result<&mut Vec<Vec<String>>> {
        self.materialize()?;
        match &mut self.storage {
            Storage::InMemory { rows } => Ok(rows),
            _ => unreachable!(),
        }
    }

    /// Update a cell at a display row index (accounting for active sort).
    /// Grows the row if the column index is beyond current width.
    pub fn update_cell(&mut self, display_row: usize, col: usize, value: String) -> Result<()> {
        if display_row >= self.row_count {
            return Err(EngineError::OutOfRange);
        }
        let raw = self.resolve_index(display_row);
        let rows = self.rows_mut()?;
        if raw >= rows.len() {
            return Err(EngineError::OutOfRange);
        }
        let row = &mut rows[raw];
        while row.len() <= col {
            row.push(String::new());
        }
        row[col] = value;
        self.dirty = true;
        Ok(())
    }

    /// Insert a new row at `display_at`. If None, append at the end.
    /// Passing None for `values` inserts a blank row.
    pub fn insert_row(
        &mut self,
        display_at: Option<usize>,
        values: Option<Vec<String>>,
    ) -> Result<usize> {
        let cols = self.column_count();
        let new_row = values.unwrap_or_else(|| vec![String::new(); cols]);
        // Resolve the raw index before taking a mutable borrow on storage.
        let raw_index = match display_at {
            Some(i) if i < self.row_count => self.resolve_index(i),
            _ => self.row_count,
        };
        let rows = self.rows_mut()?;
        rows.insert(raw_index, new_row);
        let new_len = rows.len();
        self.row_count = new_len;
        if let Some(order) = &mut self.sort_order {
            for r in order.iter_mut() {
                if *r >= raw_index {
                    *r += 1;
                }
            }
            // Append the new row at the end of the display order (cheapest).
            order.push(raw_index);
        }
        self.dirty = true;
        Ok(raw_index)
    }

    /// Delete rows by display index. Handles multiple deletions consistently.
    pub fn delete_rows(&mut self, display_rows: &[usize]) -> Result<()> {
        if display_rows.is_empty() {
            return Ok(());
        }
        let mut raw: Vec<usize> = display_rows
            .iter()
            .filter(|&&r| r < self.row_count)
            .map(|&r| self.resolve_index(r))
            .collect();
        raw.sort_unstable();
        raw.dedup();
        let removed: std::collections::HashSet<usize> = raw.iter().copied().collect();

        let rows = self.rows_mut()?;
        // Remove from highest to lowest to preserve indices.
        for &r in raw.iter().rev() {
            if r < rows.len() {
                rows.remove(r);
            }
        }
        self.row_count = rows.len();

        // Rebuild sort order, dropping removed rows and re-mapping indices.
        if let Some(order) = self.sort_order.take() {
            let new_order: Vec<usize> = order
                .into_iter()
                .filter(|i| !removed.contains(i))
                .map(|i| {
                    // Count how many removed indices were strictly below `i`.
                    let below = raw.iter().take_while(|&&r| r < i).count();
                    i - below
                })
                .collect();
            if !new_order.is_empty() {
                self.sort_order = Some(new_order);
            }
        }
        self.dirty = true;
        Ok(())
    }

    /// Delete a column. Returns the removed header name plus the column's
    /// values (in original storage order) so callers can restore on undo.
    pub fn delete_column(&mut self, col: usize) -> Result<(String, Vec<String>)> {
        if col >= self.column_count() {
            return Err(EngineError::OutOfRange);
        }
        if self.column_count() == 1 {
            // Refuse to delete the only column — the engine treats a 0-column
            // file as empty and would error out on the next read.
            return Err(EngineError::OutOfRange);
        }
        let removed_name = self.headers.get(col).cloned().unwrap_or_default();
        let rows = self.rows_mut()?;
        let mut removed_values: Vec<String> = Vec::with_capacity(rows.len());
        for row in rows.iter_mut() {
            if col < row.len() {
                removed_values.push(row.remove(col));
            } else {
                removed_values.push(String::new());
            }
        }
        // Update headers + columns and re-index downstream column entries.
        self.headers.remove(col);
        self.columns.remove(col);
        for c in self.columns.iter_mut().skip(col) {
            c.index = c.index.saturating_sub(1);
        }
        // Drop any active sort that referenced columns at-or-after the
        // deleted index — stale columns would point past the new schema.
        if let Some(_order) = &self.sort_order {
            // We don't track which columns the sort uses here, but the sort
            // permutation is over rows only, so it's still valid.
        }
        self.dirty = true;
        Ok((removed_name, removed_values))
    }

    /// Insert a column at `at` with `name` and per-row `values`. Used to
    /// restore a deleted column on undo. `values.len()` must equal `row_count`.
    pub fn insert_column(
        &mut self,
        at: usize,
        name: String,
        values: Vec<String>,
    ) -> Result<()> {
        if at > self.column_count() {
            return Err(EngineError::OutOfRange);
        }
        let rows = self.rows_mut()?;
        if values.len() != rows.len() {
            return Err(EngineError::OutOfRange);
        }
        for (row, v) in rows.iter_mut().zip(values.into_iter()) {
            // Pad short rows so the insert lands at the right column index.
            while row.len() < at {
                row.push(String::new());
            }
            row.insert(at, v);
        }
        self.headers.insert(at, name.clone());
        // Re-infer the kind for the restored column from the in-memory rows.
        let sample_rows: Vec<Vec<String>> = match &self.storage {
            Storage::InMemory { rows } => rows.iter().take(SAMPLE_ROWS).cloned().collect(),
            _ => unreachable!(),
        };
        // Build a one-element header list to call into infer_columns.
        let new_meta = infer_columns(&[name.clone()], &sample_rows.iter().map(|r| {
            r.get(at).cloned().map(|v| vec![v]).unwrap_or_else(|| vec![String::new()])
        }).collect::<Vec<_>>())
            .into_iter()
            .next()
            .unwrap_or(ColumnMeta {
                index: at,
                name,
                kind: ColumnKind::String,
            });
        // Push at position `at`, then re-index everything downstream.
        let mut meta = new_meta;
        meta.index = at;
        self.columns.insert(at, meta);
        for c in self.columns.iter_mut().skip(at + 1) {
            c.index += 1;
        }
        self.dirty = true;
        Ok(())
    }

    /// Write the current (possibly edited, possibly sorted) data to `path`.
    /// If `path` matches `self.path`, updates in place and clears the dirty flag.
    pub fn save_to(&mut self, path: &Path) -> Result<()> {
        self.materialize()?;
        let rows_ref = match &self.storage {
            Storage::InMemory { rows } => rows,
            _ => unreachable!(),
        };
        let ordered: Vec<&Vec<String>> = match &self.sort_order {
            Some(order) => order.iter().map(|&i| &rows_ref[i]).collect(),
            None => rows_ref.iter().collect(),
        };

        let mut wtr = csv::WriterBuilder::new()
            .delimiter(self.delimiter)
            .from_path(path)?;
        if self.has_header {
            wtr.write_record(&self.headers)?;
        }
        for row in ordered {
            wtr.write_record(row)?;
        }
        wtr.flush()?;

        if path == self.path {
            self.dirty = false;
        } else {
            self.path = path.to_path_buf();
            self.dirty = false;
        }
        // Refresh size on disk.
        self.size_bytes = std::fs::metadata(&self.path)?.len();
        Ok(())
    }
}

// ----- helpers -----

fn sniff_delimiter(path: &Path) -> Result<u8> {
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; 8192.min(std::fs::metadata(path)?.len() as usize)];
    file.read_exact(&mut buf).ok();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext == "tsv" {
        return Ok(b'\t');
    }
    let first_line: Vec<u8> = buf.split(|&b| b == b'\n').next().unwrap_or(&[]).to_vec();
    let candidates = [b',', b'\t', b';', b'|'];
    let mut best = (b',', 0usize);
    for c in candidates {
        let cnt = first_line.iter().filter(|&&b| b == c).count();
        if cnt > best.1 {
            best = (c, cnt);
        }
    }
    Ok(best.0)
}

fn read_headers(
    path: &Path,
    delimiter: u8,
    force: Option<bool>,
) -> Result<(Vec<String>, bool)> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)?;
    let mut iter = rdr.records();
    let first = match iter.next() {
        Some(r) => r?,
        None => return Err(EngineError::Empty),
    };
    let second = iter.next().transpose()?;

    let first_row: Vec<String> = first.iter().map(|s| s.to_string()).collect();
    let has_header = match force {
        Some(v) => v,
        None => looks_like_header(&first_row, second.as_ref()),
    };

    if has_header {
        Ok((first_row, true))
    } else {
        let col_count = first_row.len();
        let headers: Vec<String> = (0..col_count).map(|i| format!("Column {}", i + 1)).collect();
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
    // Heuristic: if the first row is all-text and the second row has any numerics,
    // or if the first row contains at least one all-text cell where the second row
    // is numeric at the same index, assume header.
    let first_numeric = first.iter().filter(|v| v.parse::<f64>().is_ok()).count();
    let second_numeric = second.iter().filter(|v| v.parse::<f64>().is_ok()).count();
    if first_numeric == 0 && second_numeric > 0 {
        return true;
    }
    // Distinct, all non-empty, short values? Likely a header.
    let all_non_empty = first.iter().all(|v| !v.is_empty());
    let short = first.iter().all(|v| v.len() < 64);
    let distinct = {
        let mut set: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for v in first {
            if !set.insert(v.as_str()) {
                return false;
            }
        }
        true
    };
    all_non_empty && short && distinct && first_numeric == 0
}

fn read_all_data_rows(path: &Path, delimiter: u8, has_header: bool) -> Result<Vec<Vec<String>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(has_header)
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)?;
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }
    Ok(rows)
}

fn read_sample_for_inference(
    path: &Path,
    delimiter: u8,
    has_header: bool,
    limit: usize,
) -> Result<Vec<Vec<String>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(has_header)
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)?;
    let mut rows = Vec::with_capacity(limit);
    for rec in rdr.records().take(limit) {
        let rec = rec?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }
    Ok(rows)
}

/// Build byte offsets of data rows, skipping the header line if present.
/// Honors quoted newlines so embedded `\n` inside `"..."` does not split a row.
fn build_offsets(path: &Path, has_header: bool) -> Result<Vec<u64>> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut offsets: Vec<u64> = Vec::new();
    let mut pos: u64 = 0;
    let mut line_no = 0usize;
    let mut in_quotes = false;
    let mut line_start = 0u64;
    let mut has_content = false;

    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            if has_content && !(has_header && line_no == 0) {
                offsets.push(line_start);
            }
            break;
        }
        let mut consumed = 0;
        for &b in chunk {
            consumed += 1;
            has_content = true;
            if b == b'"' {
                in_quotes = !in_quotes;
            } else if b == b'\n' && !in_quotes {
                if !(has_header && line_no == 0) {
                    offsets.push(line_start);
                }
                line_no += 1;
                line_start = pos + 1;
                has_content = false;
            }
            pos += 1;
        }
        reader.consume(consumed);
    }
    offsets.push(pos); // terminal offset
    Ok(offsets)
}

fn infer_columns(headers: &[String], sample: &[Vec<String>]) -> Vec<ColumnMeta> {
    let mut cols = Vec::with_capacity(headers.len());
    for (i, name) in headers.iter().enumerate() {
        let mut int_c = 0;
        let mut float_c = 0;
        let mut bool_c = 0;
        let mut date_c = 0;
        let mut non_empty = 0;
        for row in sample {
            let v = row.get(i).map(|s| s.as_str()).unwrap_or("");
            if v.is_empty() {
                continue;
            }
            non_empty += 1;
            if v.parse::<i64>().is_ok() {
                int_c += 1;
            } else if v.parse::<f64>().is_ok() {
                float_c += 1;
            } else if matches!(v.to_lowercase().as_str(), "true" | "false") {
                bool_c += 1;
            } else if looks_like_date(v) {
                date_c += 1;
            }
        }
        let kind = if non_empty == 0 {
            ColumnKind::Empty
        } else if int_c == non_empty {
            ColumnKind::Integer
        } else if int_c + float_c == non_empty {
            ColumnKind::Float
        } else if bool_c == non_empty {
            ColumnKind::Boolean
        } else if date_c == non_empty {
            ColumnKind::Date
        } else {
            ColumnKind::String
        };
        cols.push(ColumnMeta { index: i, name: name.clone(), kind });
    }
    cols
}

fn looks_like_date(v: &str) -> bool {
    // YYYY-MM-DD or YYYY/MM/DD or MM/DD/YYYY (very loose)
    let bytes = v.as_bytes();
    if bytes.len() < 8 || bytes.len() > 32 {
        return false;
    }
    let digit_count = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    let sep_count = bytes
        .iter()
        .filter(|b| matches!(**b, b'-' | b'/' | b':' | b' ' | b'T'))
        .count();
    digit_count >= 6 && sep_count >= 2
}

fn compare(a: &str, b: &str, kind: ColumnKind) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match kind {
        ColumnKind::Integer | ColumnKind::Float => {
            let ap = a.parse::<f64>();
            let bp = b.parse::<f64>();
            match (ap, bp) {
                (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
                (Ok(_), Err(_)) => Ordering::Less,
                (Err(_), Ok(_)) => Ordering::Greater,
                (Err(_), Err(_)) => a.cmp(b),
            }
        }
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[allow(dead_code)]
    fn write_csv(contents: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f
    }

    fn write_csv_named(contents: &str, ext: &str) -> NamedTempFile {
        let f = tempfile::Builder::new()
            .suffix(&format!(".{}", ext))
            .tempfile()
            .unwrap();
        std::fs::write(f.path(), contents).unwrap();
        f
    }

    #[test]
    fn opens_simple_csv_with_header() {
        let f = write_csv_named("name,age,city\nalice,30,NY\nbob,25,LA\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert!(csv.has_header);
        assert_eq!(csv.column_count(), 3);
        assert_eq!(csv.row_count, 2);
        assert_eq!(csv.headers, vec!["name", "age", "city"]);
        assert_eq!(csv.columns[1].kind, ColumnKind::Integer);
    }

    #[test]
    fn detects_no_header_for_all_numeric_first_row() {
        let f = write_csv_named("1,2,3\n4,5,6\n7,8,9\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert!(!csv.has_header);
        assert_eq!(csv.row_count, 3);
        assert_eq!(csv.headers, vec!["Column 1", "Column 2", "Column 3"]);
    }

    #[test]
    fn force_header_override() {
        let f = write_csv_named("1,2,3\n4,5,6\n", "csv");
        let csv = CsvFile::open_with(f.path(), Some(true)).unwrap();
        assert!(csv.has_header);
        assert_eq!(csv.row_count, 1);
        assert_eq!(csv.headers, vec!["1", "2", "3"]);
    }

    #[test]
    fn reads_range() {
        let f = write_csv_named("a,b\n1,x\n2,y\n3,z\n4,w\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        let rows = csv.read_range(1, 3).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "2");
        assert_eq!(rows[1][0], "3");
    }

    #[test]
    fn single_column_numeric_sort_asc_desc() {
        let f = write_csv_named("n\n3\n1\n2\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.sort(&[SortKey { column: 0, direction: SortDirection::Asc }])
            .unwrap();
        let rows = csv.read_range(0, 3).unwrap();
        assert_eq!(rows[0][0], "1");
        assert_eq!(rows[1][0], "2");
        assert_eq!(rows[2][0], "3");
        csv.sort(&[SortKey { column: 0, direction: SortDirection::Desc }])
            .unwrap();
        let rows = csv.read_range(0, 3).unwrap();
        assert_eq!(rows[0][0], "3");
        assert_eq!(rows[2][0], "1");
    }

    #[test]
    fn multi_column_sort() {
        let f = write_csv_named(
            "city,age\nNY,30\nLA,25\nNY,20\nLA,40\n",
            "csv",
        );
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.sort(&[
            SortKey { column: 0, direction: SortDirection::Asc },
            SortKey { column: 1, direction: SortDirection::Desc },
        ])
        .unwrap();
        let rows = csv.read_range(0, 4).unwrap();
        assert_eq!(rows[0], vec!["LA", "40"]);
        assert_eq!(rows[1], vec!["LA", "25"]);
        assert_eq!(rows[2], vec!["NY", "30"]);
        assert_eq!(rows[3], vec!["NY", "20"]);
    }

    #[test]
    fn clear_sort() {
        let f = write_csv_named("n\n3\n1\n2\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.sort(&[SortKey { column: 0, direction: SortDirection::Asc }])
            .unwrap();
        csv.sort(&[]).unwrap();
        let rows = csv.read_range(0, 3).unwrap();
        assert_eq!(rows[0][0], "3");
        assert_eq!(rows[1][0], "1");
        assert_eq!(rows[2][0], "2");
    }

    #[test]
    fn search_case_insensitive() {
        let f = write_csv_named("name,city\nAlice,NYC\nBob,LA\nalicia,SF\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        let hits = csv.search("ali", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].row, 0);
        assert_eq!(hits[1].row, 2);
    }

    #[test]
    fn search_limit() {
        let f = write_csv_named(
            "n\nAA\nAB\nAC\nAD\nAE\n",
            "csv",
        );
        let csv = CsvFile::open(f.path()).unwrap();
        let hits = csv.search("a", 3).unwrap();
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn column_stats_numeric() {
        let f = write_csv_named("n\n1\n2\n3\n4\n5\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        let s = csv.stats(0).unwrap();
        assert_eq!(s.count, 5);
        assert_eq!(s.numeric_count, 5);
        assert_eq!(s.min, Some(1.0));
        assert_eq!(s.max, Some(5.0));
        assert_eq!(s.mean, Some(3.0));
        assert_eq!(s.sum, Some(15.0));
        assert_eq!(s.unique, 5);
    }

    #[test]
    fn column_stats_string_top_values() {
        let f = write_csv_named(
            "tag\napple\napple\nbanana\napple\ncherry\nbanana\n",
            "csv",
        );
        let csv = CsvFile::open(f.path()).unwrap();
        let s = csv.stats(0).unwrap();
        assert_eq!(s.count, 6);
        assert_eq!(s.numeric_count, 0);
        assert_eq!(s.unique, 3);
        assert_eq!(s.top_values[0], ("apple".to_string(), 3));
        assert_eq!(s.top_values[1], ("banana".to_string(), 2));
    }

    #[test]
    fn empty_cells_counted() {
        let f = write_csv_named("a,b\n1,\n2,x\n,\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        let s = csv.stats(1).unwrap();
        assert_eq!(s.count, 3);
        assert_eq!(s.empty, 2);
    }

    #[test]
    fn sniffs_tsv_by_extension() {
        let f = write_csv_named("a\tb\n1\t2\n", "tsv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert_eq!(csv.delimiter, b'\t');
        assert_eq!(csv.column_count(), 2);
    }

    #[test]
    fn sniffs_semicolon() {
        let f = write_csv_named("a;b;c\n1;2;3\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert_eq!(csv.delimiter, b';');
        assert_eq!(csv.column_count(), 3);
    }

    #[test]
    fn quoted_fields_with_commas() {
        let f = write_csv_named(
            "name,bio\n\"Smith, John\",\"Hello, world\"\n",
            "csv",
        );
        let csv = CsvFile::open(f.path()).unwrap();
        let rows = csv.read_range(0, 1).unwrap();
        assert_eq!(rows[0][0], "Smith, John");
        assert_eq!(rows[0][1], "Hello, world");
    }

    #[test]
    fn metadata_contains_sample() {
        let f = write_csv_named("a,b\n1,2\n3,4\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        let meta = csv.metadata("fid".into()).unwrap();
        assert_eq!(meta.file_id, "fid");
        assert_eq!(meta.row_count, 2);
        assert_eq!(meta.sample.len(), 2);
        assert!(meta.fully_loaded);
    }

    #[test]
    fn out_of_range_read_is_empty() {
        let f = write_csv_named("a\n1\n2\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert!(csv.read_range(10, 20).unwrap().is_empty());
    }

    #[test]
    fn paged_read_range_matches_in_memory() {
        // Force paged mode by writing a file > SMALL_FILE_BYTES.
        let padding = "x".repeat(40);
        let mut contents = String::from("idx,label,filler\n");
        let rows = 250_000;
        for i in 0..rows {
            contents.push_str(&format!("{},row{},{}\n", i, i, padding));
        }
        assert!(contents.len() as u64 > SMALL_FILE_BYTES);
        let f = write_csv_named(&contents, "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert!(!csv.is_fully_loaded(), "expected paged mode");
        assert_eq!(csv.row_count, rows);
        let head = csv.read_range(0, 3).unwrap();
        assert_eq!(head[0][0], "0");
        assert_eq!(head[0][1], "row0");
        assert_eq!(head[2][0], "2");
        let tail = csv.read_range(rows - 2, rows).unwrap();
        assert_eq!(tail[1][0], (rows - 1).to_string());
        assert_eq!(tail[1][1], format!("row{}", rows - 1));
    }

    #[test]
    fn header_detection_mixed_types() {
        let f = write_csv_named("id,name,score\n1,alice,9.5\n2,bob,7.2\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert!(csv.has_header);
        assert_eq!(csv.columns[0].kind, ColumnKind::Integer);
        assert_eq!(csv.columns[1].kind, ColumnKind::String);
        assert_eq!(csv.columns[2].kind, ColumnKind::Float);
    }

    #[test]
    fn date_column_inference() {
        let f = write_csv_named("when\n2024-01-15\n2024-02-20\n2024-03-01\n", "csv");
        let csv = CsvFile::open(f.path()).unwrap();
        assert_eq!(csv.columns[0].kind, ColumnKind::Date);
    }

    // ---------- editing tests ----------

    #[test]
    fn update_cell_sets_dirty_and_value() {
        let f = write_csv_named("a,b\n1,x\n2,y\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        assert!(!csv.dirty);
        csv.update_cell(0, 1, "ZZ".into()).unwrap();
        assert!(csv.dirty);
        let rows = csv.read_range(0, 2).unwrap();
        assert_eq!(rows[0][1], "ZZ");
        assert_eq!(rows[1][1], "y");
    }

    #[test]
    fn update_cell_respects_sort() {
        let f = write_csv_named("n\n3\n1\n2\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.sort(&[SortKey { column: 0, direction: SortDirection::Asc }])
            .unwrap();
        // Display row 0 is the smallest (=1); update it.
        csv.update_cell(0, 0, "9".into()).unwrap();
        let rows = csv.read_range(0, 3).unwrap();
        assert_eq!(rows[0][0], "9");
    }

    #[test]
    fn update_cell_out_of_range() {
        let f = write_csv_named("a\n1\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        assert!(csv.update_cell(10, 0, "x".into()).is_err());
    }

    #[test]
    fn insert_row_appends_and_increments_row_count() {
        let f = write_csv_named("a,b\n1,x\n2,y\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.insert_row(None, None).unwrap();
        assert_eq!(csv.row_count, 3);
        let rows = csv.read_range(0, 3).unwrap();
        assert_eq!(rows[2], vec!["", ""]);
        assert!(csv.dirty);
    }

    #[test]
    fn insert_row_with_values() {
        let f = write_csv_named("a,b\n1,x\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.insert_row(Some(0), Some(vec!["9".into(), "zz".into()]))
            .unwrap();
        assert_eq!(csv.row_count, 2);
        let rows = csv.read_range(0, 2).unwrap();
        assert_eq!(rows[0], vec!["9", "zz"]);
        assert_eq!(rows[1], vec!["1", "x"]);
    }

    #[test]
    fn delete_column_removes_and_returns_values() {
        let f = write_csv_named("a,b,c\n1,x,foo\n2,y,bar\n3,z,baz\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        let (name, vals) = csv.delete_column(1).unwrap();
        assert_eq!(name, "b");
        assert_eq!(vals, vec!["x", "y", "z"]);
        assert_eq!(csv.column_count(), 2);
        assert_eq!(csv.headers, vec!["a", "c"]);
        assert!(csv.dirty);
        let rows = csv.read_range(0, 3).unwrap();
        assert_eq!(rows[0], vec!["1", "foo"]);
        assert_eq!(rows[2], vec!["3", "baz"]);
    }

    #[test]
    fn delete_then_insert_column_round_trips() {
        let f = write_csv_named("a,b,c\n1,x,foo\n2,y,bar\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        let (name, vals) = csv.delete_column(1).unwrap();
        csv.insert_column(1, name, vals).unwrap();
        assert_eq!(csv.column_count(), 3);
        assert_eq!(csv.headers, vec!["a", "b", "c"]);
        let rows = csv.read_range(0, 2).unwrap();
        assert_eq!(rows[0], vec!["1", "x", "foo"]);
        assert_eq!(rows[1], vec!["2", "y", "bar"]);
    }

    #[test]
    fn delete_column_refuses_when_only_one_left() {
        let f = write_csv_named("a\n1\n2\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        assert!(csv.delete_column(0).is_err());
    }

    #[test]
    fn delete_rows_removes_multiple() {
        let f = write_csv_named("a\n1\n2\n3\n4\n5\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.delete_rows(&[1, 3]).unwrap();
        assert_eq!(csv.row_count, 3);
        let rows = csv.read_range(0, 3).unwrap();
        assert_eq!(rows[0][0], "1");
        assert_eq!(rows[1][0], "3");
        assert_eq!(rows[2][0], "5");
        assert!(csv.dirty);
    }

    #[test]
    fn delete_rows_with_sort() {
        let f = write_csv_named("n\n3\n1\n2\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.sort(&[SortKey { column: 0, direction: SortDirection::Asc }])
            .unwrap();
        // Display order: 1, 2, 3. Delete display row 0 (value "1").
        csv.delete_rows(&[0]).unwrap();
        assert_eq!(csv.row_count, 2);
        let rows = csv.read_range(0, 2).unwrap();
        assert_eq!(rows[0][0], "2");
        assert_eq!(rows[1][0], "3");
    }

    #[test]
    fn save_writes_edits_and_clears_dirty() {
        let f = write_csv_named("a,b\n1,x\n2,y\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.update_cell(0, 1, "NEW".into()).unwrap();
        assert!(csv.dirty);
        csv.save_to(&f.path().to_path_buf()).unwrap();
        assert!(!csv.dirty);
        // Re-read from disk and verify.
        let csv2 = CsvFile::open(f.path()).unwrap();
        let rows = csv2.read_range(0, 2).unwrap();
        assert_eq!(rows[0][1], "NEW");
    }

    #[test]
    fn save_as_writes_to_new_path_and_updates_self_path() {
        let f = write_csv_named("a\n1\n2\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        csv.update_cell(0, 0, "99".into()).unwrap();
        let dest = tempfile::Builder::new()
            .suffix(".csv")
            .tempfile()
            .unwrap();
        csv.save_to(dest.path()).unwrap();
        assert_eq!(csv.path, dest.path());
        assert!(!csv.dirty);
        let csv2 = CsvFile::open(dest.path()).unwrap();
        let rows = csv2.read_range(0, 2).unwrap();
        assert_eq!(rows[0][0], "99");
    }

    #[test]
    fn save_preserves_header_and_delimiter() {
        let f = write_csv_named("name;age\nalice;30\nbob;25\n", "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        assert_eq!(csv.delimiter, b';');
        csv.update_cell(0, 1, "31".into()).unwrap();
        csv.save_to(&f.path().to_path_buf()).unwrap();
        let contents = std::fs::read_to_string(f.path()).unwrap();
        assert!(contents.contains("name;age"));
        assert!(contents.contains("alice;31"));
    }

    #[test]
    fn materialize_converts_paged_to_in_memory() {
        let padding = "x".repeat(40);
        let mut contents = String::from("a,b\n");
        for i in 0..200_000 {
            contents.push_str(&format!("{},{}\n", i, padding));
        }
        assert!(contents.len() as u64 > SMALL_FILE_BYTES);
        let f = write_csv_named(&contents, "csv");
        let mut csv = CsvFile::open(f.path()).unwrap();
        assert!(!csv.is_fully_loaded());
        csv.materialize().unwrap();
        assert!(csv.is_fully_loaded());
        assert_eq!(csv.row_count, 200_000);
    }
}
