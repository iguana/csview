//! Schema version management.
//!
//! Simple linear migration approach: each version is an integer stored in
//! `PRAGMA user_version`.  Migrations run in order until the database is at
//! the current version.

use rusqlite::Connection;

/// Current schema version expected by this binary.
pub const CURRENT_VERSION: u32 = 1;

/// Apply any outstanding migrations to `conn`.
///
/// # Errors
///
/// Returns a `rusqlite::Error` if a migration statement fails.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 1 {
        // Version 1 — initial schema (already applied by `init_app_db`).
        // Nothing extra to do; just bump the version.
        conn.execute_batch(&format!("PRAGMA user_version = {CURRENT_VERSION}"))?;
    }

    // Future migrations:
    // if version < 2 {
    //     conn.execute_batch("ALTER TABLE reports ADD COLUMN format TEXT DEFAULT 'markdown'")?;
    //     conn.execute_batch("PRAGMA user_version = 2")?;
    // }

    Ok(())
}
