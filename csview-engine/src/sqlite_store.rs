//! SQLite-backed data engine for `csview-engine`.
//!
//! Imports a CSV file into an in-memory SQLite database and supports
//! arbitrary SQL SELECT queries, cell-level mutations, schema introspection
//! for LLM prompt generation, and CSV export.
//!
//! # Example
//!
//! ```no_run
//! use csview_engine::sqlite_store::SqliteStore;
//! use csview_engine::engine::{ColumnKind, ColumnMeta};
//!
//! let columns = vec![
//!     ColumnMeta { index: 0, name: "id".into(),   kind: ColumnKind::Integer },
//!     ColumnMeta { index: 1, name: "name".into(), kind: ColumnKind::String  },
//! ];
//! let headers = vec!["id".to_string(), "name".to_string()];
//! let store = SqliteStore::from_csv("data.csv", b',', true, &headers, &columns).unwrap();
//! let result = store.query("SELECT * FROM data WHERE id > 5").unwrap();
//! println!("{} rows", result.row_count);
//! ```

use std::fmt;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use rusqlite::{Connection, ToSql, types::Value as SqlValue};

use crate::engine::{ColumnKind, ColumnMeta};

// ---------------------------------------------------------------------------
// Public output types
// ---------------------------------------------------------------------------

/// The result of executing a SELECT query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    /// Column names from the result set.
    pub columns: Vec<String>,
    /// Rows, each value serialised to a JSON scalar.
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Number of rows returned.
    pub row_count: usize,
    /// The SQL that produced this result.
    pub sql: String,
}

/// Schema information intended for LLM prompt construction.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaContext {
    pub table_name: String,
    pub columns: Vec<SchemaColumn>,
    pub row_count: usize,
    /// A few raw CSV rows for context (as string vectors).
    pub sample_rows: Vec<Vec<String>>,
}

/// Per-column schema information.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaColumn {
    pub index: usize,
    /// Sanitised name used as the SQLite column identifier.
    pub name: String,
    /// Original CSV header (may differ from `name` when sanitised).
    pub original_name: String,
    pub kind: ColumnKind,
    /// Fraction of rows that are NULL / empty (0.0–1.0).
    pub nullable_pct: f64,
    /// Approximate number of distinct non-NULL values.
    pub unique_count: usize,
    /// Up to five representative distinct values.
    pub sample_values: Vec<String>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by [`SqliteStore`] operations.
#[derive(Debug, thiserror::Error)]
pub enum SqliteError {
    #[error("sqlite error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid query: {0}")]
    InvalidQuery(String),
}

// ---------------------------------------------------------------------------
// SqliteStore
// ---------------------------------------------------------------------------

/// An in-memory SQLite database populated from a single CSV file.
///
/// The table is always named `data`.  Column names are sanitised to be valid
/// SQLite identifiers; the original names are preserved in the schema for
/// display / LLM purposes.
pub struct SqliteStore {
    conn: Connection,
    table_name: String,
    /// Sanitised column metadata (same order as CSV).
    columns: Vec<ColumnMeta>,
    /// Original CSV header strings, parallel to `columns`.
    original_headers: Vec<String>,
    row_count: usize,
    /// Path to the source CSV file (retained for diagnostics / re-export).
    #[allow(dead_code)]
    source_path: String,
    /// Field delimiter used when the CSV was imported.
    #[allow(dead_code)]
    delimiter: u8,
    /// Whether the CSV had a header row.
    #[allow(dead_code)]
    has_header: bool,
}

// ---------------------------------------------------------------------------
// Identifier helpers
// ---------------------------------------------------------------------------

/// Sanitise a CSV header into a valid, unambiguous SQLite identifier.
///
/// Rules applied:
/// - Replace every run of non-alphanumeric / non-underscore characters with `_`.
/// - Strip leading digits by prepending `col_`.
/// - De-duplicate: if the name already exists in `seen`, append `_N`.
/// - Truncate to 128 bytes (SQLite has no hard limit, but this keeps things
///   reasonable).
fn sanitise_col_name(raw: &str, seen: &mut std::collections::HashSet<String>) -> String {
    // Replace non-word chars.
    let mut name: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    // Collapse consecutive underscores.
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    // Strip trailing underscores.
    let name = name.trim_matches('_');
    let mut name = if name.is_empty() {
        "col".to_string()
    } else {
        name.to_string()
    };

    // Must not start with a digit.
    if name.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        name = format!("col_{name}");
    }

    // Truncate.
    if name.len() > 128 {
        name.truncate(128);
    }

    // De-duplicate.
    if seen.contains(&name) {
        let base = name.clone();
        let mut n = 2u32;
        loop {
            let candidate = format!("{base}_{n}");
            if !seen.contains(&candidate) {
                name = candidate;
                break;
            }
            n += 1;
        }
    }
    seen.insert(name.clone());
    name
}

/// Quote an identifier with double-quotes for safe embedding in SQL.
///
/// Internal double-quotes are escaped by doubling them.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

// ---------------------------------------------------------------------------
// SQLite type mapping
// ---------------------------------------------------------------------------

fn col_kind_to_sqlite_type(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Integer => "INTEGER",
        ColumnKind::Float => "REAL",
        ColumnKind::Boolean => "INTEGER",
        _ => "TEXT",
    }
}

// ---------------------------------------------------------------------------
// rusqlite value → serde_json::Value
// ---------------------------------------------------------------------------

fn sql_value_to_json(v: SqlValue) -> serde_json::Value {
    match v {
        SqlValue::Null => serde_json::Value::Null,
        SqlValue::Integer(i) => serde_json::Value::Number(i.into()),
        SqlValue::Real(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        SqlValue::Text(s) => serde_json::Value::String(s),
        SqlValue::Blob(b) => serde_json::Value::String(format!("<blob {} bytes>", b.len())),
    }
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl SqliteStore {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Import a CSV file into an in-memory SQLite database.
    ///
    /// - `path` — path to the CSV file.
    /// - `delimiter` — field delimiter byte (e.g. `b','` or `b'\t'`).
    /// - `has_header` — when `true` the first row is treated as headers;
    ///   when `false` columns are named `col_1`, `col_2`, …
    /// - `headers` — the parsed header strings (may be auto-generated).
    /// - `columns` — [`ColumnMeta`] slice describing inferred column types.
    ///
    /// All data rows are inserted inside a single SQLite transaction for
    /// efficiency.  An index is created on every column to speed up filtering.
    pub fn from_csv(
        path: &str,
        delimiter: u8,
        has_header: bool,
        headers: &[String],
        columns: &[ColumnMeta],
    ) -> Result<Self, SqliteError> {
        let conn = Connection::open_in_memory()?;

        // --- Build sanitised column names -----------------------------------
        let mut seen = std::collections::HashSet::new();
        let sanitised_names: Vec<String> = if has_header {
            headers
                .iter()
                .map(|h| sanitise_col_name(h, &mut seen))
                .collect()
        } else {
            (1..=headers.len())
                .map(|i| {
                    let auto = format!("col_{i}");
                    seen.insert(auto.clone());
                    auto
                })
                .collect()
        };
        let original_headers = headers.to_vec();

        // Merge sanitised names into ColumnMeta.
        let san_columns: Vec<ColumnMeta> = columns
            .iter()
            .enumerate()
            .map(|(i, cm)| ColumnMeta {
                index: cm.index,
                name: sanitised_names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("col_{}", i + 1)),
                kind: cm.kind,
            })
            .collect();

        // --- CREATE TABLE ---------------------------------------------------
        let col_defs: Vec<String> = san_columns
            .iter()
            .map(|cm| {
                format!(
                    "{} {}",
                    quote_ident(&cm.name),
                    col_kind_to_sqlite_type(cm.kind)
                )
            })
            .collect();
        let create_sql = format!("CREATE TABLE data ({});", col_defs.join(", "));
        conn.execute_batch(&create_sql)?;

        // --- Bulk INSERT inside a transaction --------------------------------
        let placeholders: Vec<String> = (0..san_columns.len()).map(|_| "?".to_string()).collect();
        let col_list: Vec<String> = san_columns
            .iter()
            .map(|cm| quote_ident(&cm.name))
            .collect();
        let insert_sql = format!(
            "INSERT INTO data ({}) VALUES ({})",
            col_list.join(", "),
            placeholders.join(", ")
        );

        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(has_header)
            .from_path(path)?;

        conn.execute_batch("BEGIN")?;
        let mut stmt = conn.prepare(&insert_sql)?;
        let mut row_count: usize = 0;

        for result in rdr.records() {
            let record = result?;
            let params: Vec<Box<dyn ToSql>> = san_columns
                .iter()
                .enumerate()
                .map(|(i, cm)| -> Box<dyn ToSql> {
                    let raw = record.get(i).unwrap_or("").trim();
                    if raw.is_empty() {
                        return Box::new(rusqlite::types::Null);
                    }
                    match cm.kind {
                        ColumnKind::Integer => {
                            if let Ok(v) = raw.parse::<i64>() {
                                Box::new(v)
                            } else {
                                Box::new(raw.to_string())
                            }
                        }
                        ColumnKind::Float => {
                            if let Ok(v) = raw.parse::<f64>() {
                                Box::new(v)
                            } else {
                                Box::new(raw.to_string())
                            }
                        }
                        ColumnKind::Boolean => {
                            let lower = raw.to_lowercase();
                            let flag: i64 =
                                if matches!(lower.as_str(), "true" | "1" | "yes" | "t" | "y") {
                                    1
                                } else {
                                    0
                                };
                            Box::new(flag)
                        }
                        _ => Box::new(raw.to_string()),
                    }
                })
                .collect();

            let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
            stmt.execute(params_refs.as_slice())?;
            row_count += 1;
        }
        conn.execute_batch("COMMIT")?;
        drop(stmt);

        // --- Create indices on every column ----------------------------------
        for cm in &san_columns {
            let idx_sql = format!(
                "CREATE INDEX IF NOT EXISTS idx_{name} ON data ({col});",
                name = cm.name,
                col = quote_ident(&cm.name)
            );
            conn.execute_batch(&idx_sql)?;
        }

        Ok(Self {
            conn,
            table_name: "data".to_string(),
            columns: san_columns,
            original_headers,
            row_count,
            source_path: path.to_string(),
            delimiter,
            has_header,
        })
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Execute a SELECT query and return structured results.
    ///
    /// The SQL must start with `SELECT` and must not contain DDL keywords
    /// (`DROP`, `ALTER`, `CREATE`, `ATTACH`).
    ///
    /// # Errors
    ///
    /// Returns [`SqliteError::InvalidQuery`] if validation fails, or
    /// [`SqliteError::Db`] for SQLite execution errors.
    pub fn query(&self, sql: &str) -> Result<QueryResult, SqliteError> {
        Self::validate_select(sql)?;
        self.execute_query(sql)
    }

    /// Execute a raw mutation statement (`UPDATE` / `INSERT` / `DELETE`).
    ///
    /// Returns the number of rows affected.
    ///
    /// # Errors
    ///
    /// Returns [`SqliteError::InvalidQuery`] if the statement is not a
    /// recognised DML operation.
    pub fn execute(&self, sql: &str) -> Result<usize, SqliteError> {
        Self::validate_mutation(sql)?;
        let affected = self.conn.execute(sql, [])?;
        Ok(affected)
    }

    /// Read a contiguous range of rows from the table.
    ///
    /// Useful for virtual-scroll grid display.  `order_by` should be the
    /// bare column name or `"column DESC"` — it is appended verbatim after
    /// `ORDER BY`.
    pub fn read_range(
        &self,
        offset: usize,
        limit: usize,
        order_by: Option<&str>,
    ) -> Result<QueryResult, SqliteError> {
        let sql = match order_by {
            Some(ob) => format!(
                "SELECT * FROM data ORDER BY {ob} LIMIT {limit} OFFSET {offset}"
            ),
            None => format!("SELECT * FROM data LIMIT {limit} OFFSET {offset}"),
        };
        self.execute_query(&sql)
    }

    /// Return schema context suitable for embedding in LLM prompts.
    ///
    /// `sample_rows` controls how many raw data rows are included in the
    /// context (capped at 100).
    pub fn schema_context(&self, sample_rows: usize) -> Result<SchemaContext, SqliteError> {
        let sample_limit = sample_rows.min(100);
        let total = self.row_count as f64;

        let mut schema_cols: Vec<SchemaColumn> = Vec::with_capacity(self.columns.len());

        for (i, cm) in self.columns.iter().enumerate() {
            let qcol = quote_ident(&cm.name);

            // Null count.
            let null_count: i64 = self.conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM data WHERE {col} IS NULL",
                    col = qcol
                ),
                [],
                |row| row.get(0),
            )?;

            // Distinct count.
            let unique_count: i64 = self.conn.query_row(
                &format!(
                    "SELECT COUNT(DISTINCT {col}) FROM data WHERE {col} IS NOT NULL",
                    col = qcol
                ),
                [],
                |row| row.get(0),
            )?;

            // Sample values (up to 5 distinct, non-null, cast to text).
            let sample_sql = format!(
                "SELECT DISTINCT CAST({col} AS TEXT) FROM data \
                 WHERE {col} IS NOT NULL LIMIT 5",
                col = qcol
            );
            let mut sample_stmt = self.conn.prepare(&sample_sql)?;
            let sample_values: Vec<String> = sample_stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();

            let nullable_pct = if total > 0.0 {
                null_count as f64 / total
            } else {
                0.0
            };

            schema_cols.push(SchemaColumn {
                index: i,
                name: cm.name.clone(),
                original_name: self
                    .original_headers
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| cm.name.clone()),
                kind: cm.kind,
                nullable_pct,
                unique_count: unique_count as usize,
                sample_values,
            });
        }

        // Raw sample rows.
        let raw_sql = format!("SELECT * FROM data LIMIT {sample_limit}");
        let col_count = self.columns.len();
        let mut raw_stmt = self.conn.prepare(&raw_sql)?;
        let sample_rows_data: Vec<Vec<String>> = raw_stmt
            .query_map([], |row| {
                let mut cells = Vec::with_capacity(col_count);
                for j in 0..col_count {
                    let cell: String = row
                        .get::<_, Option<String>>(j)?
                        .unwrap_or_default();
                    cells.push(cell);
                }
                Ok(cells)
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(SchemaContext {
            table_name: self.table_name.clone(),
            columns: schema_cols,
            row_count: self.row_count,
            sample_rows: sample_rows_data,
        })
    }

    // -----------------------------------------------------------------------
    // Mutations
    // -----------------------------------------------------------------------

    /// Update a single cell identified by its SQLite `rowid`.
    ///
    /// `column` must be a sanitised column name (as stored in `self.columns`).
    pub fn update_cell(&self, rowid: i64, column: &str, value: &str) -> Result<(), SqliteError> {
        let sql = format!(
            "UPDATE data SET {col} = ?1 WHERE rowid = ?2",
            col = quote_ident(column)
        );
        self.conn.execute(&sql, rusqlite::params![value, rowid])?;
        Ok(())
    }

    /// Insert a new row.  `values` is a slice of `(column_name, text_value)` pairs.
    ///
    /// Returns the `rowid` of the newly created row.
    pub fn insert_row(&self, values: &[(&str, &str)]) -> Result<i64, SqliteError> {
        if values.is_empty() {
            return Err(SqliteError::InvalidQuery("no values provided".into()));
        }
        let cols: Vec<String> = values.iter().map(|(c, _)| quote_ident(c)).collect();
        let placeholders: Vec<String> = (1..=values.len())
            .map(|i| format!("?{i}"))
            .collect();
        let sql = format!(
            "INSERT INTO data ({}) VALUES ({})",
            cols.join(", "),
            placeholders.join(", ")
        );
        let params: Vec<&dyn ToSql> = values.iter().map(|(_, v)| v as &dyn ToSql).collect();
        self.conn.execute(&sql, params.as_slice())?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Delete rows by their `rowid`s.
    ///
    /// Returns the number of rows deleted.
    pub fn delete_rows(&self, rowids: &[i64]) -> Result<usize, SqliteError> {
        if rowids.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<String> = (1..=rowids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "DELETE FROM data WHERE rowid IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn ToSql> = rowids.iter().map(|r| r as &dyn ToSql).collect();
        let affected = self.conn.execute(&sql, params.as_slice())?;
        Ok(affected)
    }

    /// Add a derived column computed from a SQL expression.
    ///
    /// The column is added as `TEXT` and then back-filled via `UPDATE`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use csview_engine::sqlite_store::SqliteStore;
    /// # let store: SqliteStore = unimplemented!();
    /// store.add_column("annual_salary", "salary * 12").unwrap();
    /// ```
    pub fn add_column(&self, name: &str, sql_expr: &str) -> Result<(), SqliteError> {
        let qname = quote_ident(name);
        let alter_sql = format!("ALTER TABLE data ADD COLUMN {qname} TEXT;");
        self.conn.execute_batch(&alter_sql)?;
        let update_sql = format!("UPDATE data SET {qname} = ({sql_expr});");
        self.conn.execute_batch(&update_sql)?;
        Ok(())
    }

    /// Drop a column by its zero-based position in `self.columns`.
    ///
    /// Uses SQLite's `ALTER TABLE … DROP COLUMN` (available from SQLite 3.35).
    /// Updates the in-memory schema (`columns`, `original_headers`) so future
    /// queries and exports stay consistent.
    pub fn delete_column(&mut self, col_index: usize) -> Result<String, SqliteError> {
        if col_index >= self.columns.len() {
            return Err(SqliteError::InvalidQuery(format!(
                "column index {col_index} out of range"
            )));
        }
        if self.columns.len() <= 1 {
            return Err(SqliteError::InvalidQuery(
                "cannot delete the only remaining column".into(),
            ));
        }
        let removed = self.columns[col_index].clone();
        let qname = quote_ident(&removed.name);
        let sql = format!("ALTER TABLE data DROP COLUMN {qname};");
        self.conn.execute_batch(&sql)?;
        // Mirror the schema change in the in-memory metadata.
        self.columns.remove(col_index);
        self.original_headers.remove(col_index);
        // Re-index downstream entries so callers can continue to use index().
        for c in self.columns.iter_mut().skip(col_index) {
            c.index = c.index.saturating_sub(1);
        }
        Ok(removed.name)
    }

    // -----------------------------------------------------------------------
    // Export
    // -----------------------------------------------------------------------

    /// Export the current table contents to a CSV file.
    ///
    /// `delimiter` is the output field separator (e.g. `b','`).
    pub fn export_csv(&self, path: &str, delimiter: u8) -> Result<(), SqliteError> {
        let file = File::create(Path::new(path))?;
        let mut writer = csv::WriterBuilder::new()
            .delimiter(delimiter)
            .from_writer(BufWriter::new(file));

        // Header row using original names (not sanitised identifiers).
        writer.write_record(&self.original_headers)?;

        let col_count = self.columns.len();
        let mut stmt = self.conn.prepare("SELECT * FROM data")?;
        let rows = stmt.query_map([], |row| {
            let mut cells = Vec::with_capacity(col_count);
            for j in 0..col_count {
                let val: SqlValue = row.get(j)?;
                let cell = match val {
                    SqlValue::Null => String::new(),
                    SqlValue::Integer(i) => i.to_string(),
                    SqlValue::Real(f) => f.to_string(),
                    SqlValue::Text(s) => s,
                    SqlValue::Blob(b) => format!("<blob {} bytes>", b.len()),
                };
                cells.push(cell);
            }
            Ok(cells)
        })?;

        for row in rows {
            writer.write_record(row?)?;
        }
        writer.flush()?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Total number of rows in the table.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// Sanitised column metadata.
    #[must_use]
    pub fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    /// The table name (always `"data"`).
    #[must_use]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// The underlying [`Connection`] for advanced / escape-hatch operations.
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    // -----------------------------------------------------------------------
    // Validation helpers
    // -----------------------------------------------------------------------

    /// Assert that a SQL string is a safe SELECT statement.
    fn validate_select(sql: &str) -> Result<(), SqliteError> {
        let upper = sql.trim().to_uppercase();
        if !upper.starts_with("SELECT") {
            return Err(SqliteError::InvalidQuery(
                "only SELECT queries are allowed".into(),
            ));
        }
        for forbidden in &["DROP", "ALTER", "CREATE", "ATTACH"] {
            if upper.contains(forbidden) {
                return Err(SqliteError::InvalidQuery(format!(
                    "DDL keyword {forbidden} is not allowed"
                )));
            }
        }
        Ok(())
    }

    /// Assert that a SQL string is a DML mutation (UPDATE / INSERT / DELETE).
    fn validate_mutation(sql: &str) -> Result<(), SqliteError> {
        let upper = sql.trim().to_uppercase();
        if !(upper.starts_with("UPDATE")
            || upper.starts_with("INSERT")
            || upper.starts_with("DELETE"))
        {
            return Err(SqliteError::InvalidQuery(
                "only UPDATE, INSERT, or DELETE statements are allowed".into(),
            ));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn execute_query(&self, sql: &str) -> Result<QueryResult, SqliteError> {
        let mut stmt = self.conn.prepare(sql)?;
        let col_names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let col_count = col_names.len();

        let rows: Vec<Vec<serde_json::Value>> = stmt
            .query_map([], |row| {
                let mut cells = Vec::with_capacity(col_count);
                for j in 0..col_count {
                    let v: SqlValue = row.get(j)?;
                    cells.push(sql_value_to_json(v));
                }
                Ok(cells)
            })?
            .filter_map(|r| r.ok())
            .collect();

        let row_count = rows.len();
        Ok(QueryResult {
            columns: col_names,
            rows,
            row_count,
            sql: sql.to_string(),
        })
    }
}

impl fmt::Display for SqliteStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SqliteStore {{ table: {}, rows: {}, columns: {} }}",
            self.table_name,
            self.row_count,
            self.columns.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Write CSV content to a temp file and return the file (kept alive).
    fn write_csv(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    /// Build ColumnMeta from `(name, kind)` pairs.
    fn make_columns(specs: &[(&str, ColumnKind)]) -> (Vec<String>, Vec<ColumnMeta>) {
        let headers: Vec<String> = specs.iter().map(|(n, _)| n.to_string()).collect();
        let cols: Vec<ColumnMeta> = specs
            .iter()
            .enumerate()
            .map(|(i, (n, k))| ColumnMeta {
                index: i,
                name: n.to_string(),
                kind: *k,
            })
            .collect();
        (headers, cols)
    }

    /// Convenience: import CSV text.
    fn import(csv_text: &str, specs: &[(&str, ColumnKind)]) -> (SqliteStore, NamedTempFile) {
        let f = write_csv(csv_text);
        let (headers, cols) = make_columns(specs);
        let store = SqliteStore::from_csv(f.path().to_str().unwrap(), b',', true, &headers, &cols)
            .unwrap();
        (store, f)
    }

    // -----------------------------------------------------------------------
    // Import tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_import_csv_basic() {
        let csv = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,Chicago\n";
        let (store, _f) = import(
            csv,
            &[
                ("name", ColumnKind::String),
                ("age", ColumnKind::Integer),
                ("city", ColumnKind::String),
            ],
        );
        assert_eq!(store.row_count(), 3);
        assert_eq!(store.columns().len(), 3);
        assert_eq!(store.columns()[0].name, "name");
        assert_eq!(store.columns()[1].name, "age");
        assert_eq!(store.columns()[2].name, "city");
    }

    #[test]
    fn test_import_csv_with_types() {
        let csv = "id,score,active\n1,9.5,true\n2,7.1,false\n";
        let (store, _f) = import(
            csv,
            &[
                ("id", ColumnKind::Integer),
                ("score", ColumnKind::Float),
                ("active", ColumnKind::Boolean),
            ],
        );
        // Integer column should store as INTEGER.
        let result = store.query("SELECT typeof(id) FROM data LIMIT 1").unwrap();
        assert_eq!(result.rows[0][0], serde_json::json!("integer"));
        // Float column should store as REAL.
        let result = store.query("SELECT typeof(score) FROM data LIMIT 1").unwrap();
        assert_eq!(result.rows[0][0], serde_json::json!("real"));
        // Boolean is INTEGER.
        let result = store.query("SELECT typeof(active) FROM data LIMIT 1").unwrap();
        assert_eq!(result.rows[0][0], serde_json::json!("integer"));
    }

    #[test]
    fn test_import_csv_no_header() {
        let csv = "Alice,30\nBob,25\n";
        let f = write_csv(csv);
        let (headers, cols) = make_columns(&[("col_1", ColumnKind::String), ("col_2", ColumnKind::Integer)]);
        let store = SqliteStore::from_csv(
            f.path().to_str().unwrap(),
            b',',
            false,
            &headers,
            &cols,
        )
        .unwrap();
        // No-header: we pass synthetic names, they must be used.
        assert_eq!(store.row_count(), 2);
        let result = store.query("SELECT * FROM data").unwrap();
        assert_eq!(result.columns[0], "col_1");
        assert_eq!(result.columns[1], "col_2");
    }

    #[test]
    fn test_import_large_batch() {
        // Build 10 000 rows.
        let mut csv = "id,value\n".to_string();
        for i in 0..10_000_u32 {
            csv.push_str(&format!("{i},{}\n", i * 2));
        }
        let (store, _f) = import(
            &csv,
            &[("id", ColumnKind::Integer), ("value", ColumnKind::Integer)],
        );
        assert_eq!(store.row_count(), 10_000);
        let r = store.query("SELECT COUNT(*) FROM data").unwrap();
        assert_eq!(r.rows[0][0], serde_json::json!(10_000_i64));
    }

    #[test]
    fn test_import_csv_with_spaces_in_headers() {
        let csv = "first name,last name,zip code\nAlice,Smith,10001\n";
        let (store, _f) = import(
            csv,
            &[
                ("first name", ColumnKind::String),
                ("last name", ColumnKind::String),
                ("zip code", ColumnKind::Integer),
            ],
        );
        // Spaces must be replaced with underscores.
        assert_eq!(store.columns()[0].name, "first_name");
        assert_eq!(store.columns()[1].name, "last_name");
        assert_eq!(store.columns()[2].name, "zip_code");
        // Data must still be queryable.
        let r = store.query("SELECT first_name FROM data").unwrap();
        assert_eq!(r.rows[0][0], serde_json::json!("Alice"));
    }

    // -----------------------------------------------------------------------
    // Query tests
    // -----------------------------------------------------------------------

    fn sample_store() -> (SqliteStore, NamedTempFile) {
        let csv = "name,department,salary\n\
                   Alice,Engineering,180000\n\
                   Bob,Marketing,120000\n\
                   Charlie,Engineering,160000\n\
                   Diana,Marketing,130000\n\
                   Eve,Engineering,200000\n";
        import(
            csv,
            &[
                ("name", ColumnKind::String),
                ("department", ColumnKind::String),
                ("salary", ColumnKind::Integer),
            ],
        )
    }

    #[test]
    fn test_query_select_all() {
        let (store, _f) = sample_store();
        let result = store.query("SELECT * FROM data").unwrap();
        assert_eq!(result.row_count, 5);
        assert_eq!(result.columns.len(), 3);
    }

    #[test]
    fn test_query_where_equals() {
        let (store, _f) = sample_store();
        let result = store
            .query("SELECT name FROM data WHERE name = 'Alice'")
            .unwrap();
        assert_eq!(result.row_count, 1);
        assert_eq!(result.rows[0][0], serde_json::json!("Alice"));
    }

    #[test]
    fn test_query_where_numeric() {
        let (store, _f) = sample_store();
        let result = store
            .query("SELECT name FROM data WHERE salary > 150000")
            .unwrap();
        // Alice (180k), Charlie (160k), Eve (200k) → 3 rows.
        assert_eq!(result.row_count, 3);
    }

    #[test]
    fn test_query_group_by() {
        let (store, _f) = sample_store();
        let result = store
            .query("SELECT department, COUNT(*) AS cnt FROM data GROUP BY department ORDER BY department")
            .unwrap();
        assert_eq!(result.row_count, 2);
        // Engineering appears 3 times.
        let eng_row = result
            .rows
            .iter()
            .find(|r| r[0] == serde_json::json!("Engineering"))
            .unwrap();
        assert_eq!(eng_row[1], serde_json::json!(3_i64));
    }

    #[test]
    fn test_query_aggregate() {
        let (store, _f) = sample_store();
        let result = store
            .query("SELECT AVG(salary), MIN(salary), MAX(salary) FROM data")
            .unwrap();
        assert_eq!(result.row_count, 1);
        let avg = result.rows[0][0].as_f64().unwrap();
        assert!((avg - 158_000.0).abs() < 1.0);
        assert_eq!(result.rows[0][1], serde_json::json!(120_000_i64));
        assert_eq!(result.rows[0][2], serde_json::json!(200_000_i64));
    }

    #[test]
    fn test_query_order_by() {
        let (store, _f) = sample_store();
        let result = store
            .query("SELECT name, salary FROM data ORDER BY salary DESC")
            .unwrap();
        assert_eq!(result.rows[0][0], serde_json::json!("Eve"));
        assert_eq!(result.rows[4][0], serde_json::json!("Bob"));
    }

    #[test]
    fn test_query_like() {
        let (store, _f) = sample_store();
        let result = store
            .query("SELECT name FROM data WHERE name LIKE 'A%'")
            .unwrap();
        assert_eq!(result.row_count, 1);
        assert_eq!(result.rows[0][0], serde_json::json!("Alice"));
    }

    #[test]
    fn test_query_subquery() {
        let (store, _f) = sample_store();
        let result = store
            .query(
                "SELECT name, salary FROM data \
                 WHERE salary > (SELECT AVG(salary) FROM data)",
            )
            .unwrap();
        // 158 000 average; Alice (180k), Charlie (160k), Eve (200k).
        assert_eq!(result.row_count, 3);
    }

    // -----------------------------------------------------------------------
    // Validation / rejection tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_reject_drop_table() {
        let (store, _f) = sample_store();
        let err = store.query("SELECT * FROM data; DROP TABLE data").unwrap_err();
        assert!(matches!(err, SqliteError::InvalidQuery(_)));
    }

    #[test]
    fn test_reject_create_table() {
        let (store, _f) = sample_store();
        let err = store
            .query("SELECT * FROM (CREATE TABLE foo (x TEXT))")
            .unwrap_err();
        assert!(matches!(err, SqliteError::InvalidQuery(_)));
    }

    #[test]
    fn test_reject_alter() {
        let (store, _f) = sample_store();
        let err = store
            .query("SELECT * FROM data WHERE name = 'Alice'; ALTER TABLE data ADD COLUMN x TEXT")
            .unwrap_err();
        assert!(matches!(err, SqliteError::InvalidQuery(_)));
    }

    #[test]
    fn test_reject_attach() {
        let (store, _f) = sample_store();
        let err = store
            .query("SELECT * FROM data; ATTACH DATABASE ':memory:' AS tmp")
            .unwrap_err();
        assert!(matches!(err, SqliteError::InvalidQuery(_)));
    }

    #[test]
    fn test_reject_non_select() {
        let (store, _f) = sample_store();
        let err = store
            .query("INSERT INTO data (name) VALUES ('Zara')")
            .unwrap_err();
        assert!(matches!(err, SqliteError::InvalidQuery(_)));
    }

    // -----------------------------------------------------------------------
    // Mutation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_cell() {
        let (store, _f) = sample_store();
        // Get Alice's rowid.
        let r = store
            .query("SELECT rowid FROM data WHERE name = 'Alice'")
            .unwrap();
        let rowid = r.rows[0][0].as_i64().unwrap();
        store.update_cell(rowid, "salary", "999999").unwrap();
        let r = store
            .query("SELECT salary FROM data WHERE name = 'Alice'")
            .unwrap();
        // salary column is INTEGER; 999999 as text is stored and returned.
        let val = r.rows[0][0].as_i64().unwrap();
        assert_eq!(val, 999_999);
    }

    #[test]
    fn test_insert_row() {
        let (store, _f) = sample_store();
        let new_rowid = store
            .insert_row(&[("name", "Zara"), ("department", "Legal"), ("salary", "95000")])
            .unwrap();
        assert!(new_rowid > 0);
        let r = store
            .query("SELECT COUNT(*) FROM data")
            .unwrap();
        assert_eq!(r.rows[0][0], serde_json::json!(6_i64));
    }

    #[test]
    fn test_delete_rows() {
        let (store, _f) = sample_store();
        // Delete Bob and Diana (rowids retrieved first).
        let r = store
            .query("SELECT rowid FROM data WHERE department = 'Marketing' ORDER BY name")
            .unwrap();
        let rowids: Vec<i64> = r.rows.iter().map(|row| row[0].as_i64().unwrap()).collect();
        assert_eq!(rowids.len(), 2);
        let deleted = store.delete_rows(&rowids).unwrap();
        assert_eq!(deleted, 2);
        let r2 = store.query("SELECT COUNT(*) FROM data").unwrap();
        assert_eq!(r2.rows[0][0], serde_json::json!(3_i64));
    }

    #[test]
    fn test_add_column() {
        let (store, _f) = sample_store();
        store.add_column("annual_bonus", "salary * 0.1").unwrap();
        let r = store
            .query("SELECT annual_bonus FROM data WHERE name = 'Alice'")
            .unwrap();
        // 180000 * 0.1 = 18000, stored as TEXT.
        let val: f64 = r.rows[0][0].as_str().unwrap().parse().unwrap();
        assert!((val - 18_000.0).abs() < 1.0);
    }

    // -----------------------------------------------------------------------
    // read_range tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_range_basic() {
        let (store, _f) = sample_store();
        let r = store.read_range(0, 2, None).unwrap();
        assert_eq!(r.row_count, 2);
    }

    #[test]
    fn test_read_range_with_order() {
        let (store, _f) = sample_store();
        let r = store.read_range(0, 3, Some("salary DESC")).unwrap();
        // First row should be Eve (200k).
        let name_col = r.columns.iter().position(|c| c == "name").unwrap();
        assert_eq!(r.rows[0][name_col], serde_json::json!("Eve"));
    }

    // -----------------------------------------------------------------------
    // Schema context test
    // -----------------------------------------------------------------------

    #[test]
    fn test_schema_context() {
        let (store, _f) = sample_store();
        let ctx = store.schema_context(3).unwrap();
        assert_eq!(ctx.table_name, "data");
        assert_eq!(ctx.row_count, 5);
        assert_eq!(ctx.columns.len(), 3);
        assert!(ctx.sample_rows.len() <= 3);

        // Column name check.
        let dept_col = ctx.columns.iter().find(|c| c.name == "department").unwrap();
        assert!(dept_col.unique_count == 2);
        // No nulls in sample data.
        assert_eq!(dept_col.nullable_pct, 0.0);
        // Sample values should include both departments.
        assert!(dept_col.sample_values.len() <= 5);
    }

    // -----------------------------------------------------------------------
    // Export test
    // -----------------------------------------------------------------------

    #[test]
    fn test_export_csv() {
        let (store, _f) = sample_store();
        let out = NamedTempFile::new().unwrap();
        store
            .export_csv(out.path().to_str().unwrap(), b',')
            .unwrap();

        // Re-import and verify.
        let (headers2, cols2) = make_columns(&[
            ("name", ColumnKind::String),
            ("department", ColumnKind::String),
            ("salary", ColumnKind::Integer),
        ]);
        let store2 = SqliteStore::from_csv(
            out.path().to_str().unwrap(),
            b',',
            true,
            &headers2,
            &cols2,
        )
        .unwrap();
        assert_eq!(store2.row_count(), 5);
        let r = store2.query("SELECT name FROM data ORDER BY name").unwrap();
        assert_eq!(r.rows[0][0], serde_json::json!("Alice"));
    }

    // -----------------------------------------------------------------------
    // Sanitise helper unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_sanitise_col_name_spaces() {
        let mut seen = std::collections::HashSet::new();
        let name = sanitise_col_name("first name", &mut seen);
        assert_eq!(name, "first_name");
    }

    #[test]
    fn test_sanitise_col_name_dedup() {
        let mut seen = std::collections::HashSet::new();
        let a = sanitise_col_name("value", &mut seen);
        let b = sanitise_col_name("value", &mut seen);
        assert_eq!(a, "value");
        assert_eq!(b, "value_2");
    }

    #[test]
    fn test_sanitise_col_name_leading_digit() {
        let mut seen = std::collections::HashSet::new();
        let name = sanitise_col_name("123abc", &mut seen);
        assert_eq!(name, "col_123abc");
    }
}
