//! Persistent SQLite database schema for the csviewai application.
//!
//! Stores account credentials, chat history, reports, and query history.
//! The database lives in the Tauri app-data directory and is separate from
//! the in-memory per-file `SqliteStore` instances.

use std::path::Path;

/// Open (or create) the application database and run all DDL migrations.
///
/// # Errors
///
/// Returns a `rusqlite::Error` if the database cannot be opened or the schema
/// cannot be initialised.
pub fn init_app_db(path: &Path) -> Result<rusqlite::Connection, rusqlite::Error> {
    let conn = rusqlite::Connection::open(path)?;

    // WAL mode for better concurrent read performance.
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS account (
            id         INTEGER PRIMARY KEY,
            api_key    TEXT    NOT NULL,
            model      TEXT    NOT NULL DEFAULT 'claude-sonnet-4-20250514',
            created_at TEXT    NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS chat_sessions (
            id         TEXT PRIMARY KEY,
            file_path  TEXT NOT NULL,
            title      TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS chat_messages (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT    NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
            role       TEXT    NOT NULL,
            content    TEXT    NOT NULL,
            created_at TEXT    NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS reports (
            id          TEXT PRIMARY KEY,
            file_path   TEXT NOT NULL,
            report_type TEXT NOT NULL,
            title       TEXT,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS query_history (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path     TEXT NOT NULL,
            nl_query      TEXT NOT NULL,
            generated_sql TEXT NOT NULL,
            created_at    TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_chat_messages_session
            ON chat_messages(session_id);
        CREATE INDEX IF NOT EXISTS idx_reports_file
            ON reports(file_path);
        CREATE INDEX IF NOT EXISTS idx_query_history_file
            ON query_history(file_path);
        ",
    )?;

    Ok(conn)
}
