//! Application-wide shared state for csviewai.
//!
//! `AiAppState` is registered with Tauri's `manage()` system and accessed
//! from all command handlers via `State<AiAppState>`.

use std::collections::HashMap;

use parking_lot::Mutex;

use csview_engine::sqlite_store::SqliteStore;

/// Central application state.
///
/// # Locking discipline
///
/// Each field is wrapped in a `Mutex`.  Command handlers must:
/// 1. Lock the needed mutex to extract data.
/// 2. **Drop the guard** before any `.await` point (do not hold across async).
/// 3. Re-lock if further mutable access is required after the await.
pub struct AiAppState {
    /// One `SqliteStore` per open file, keyed by a UUID v4 `file_id`.
    pub stores: Mutex<HashMap<String, SqliteStore>>,

    /// Maps `file_id` → original filesystem path, for `save_csv`.
    pub open_paths: Mutex<HashMap<String, String>>,

    /// Persistent SQLite database for account, chat history, reports.
    pub app_db: Mutex<rusqlite::Connection>,

    /// Optional LLM client — `None` until the user provides an API key.
    pub llm: Mutex<Option<crate::llm::LlmClient>>,
}

impl AiAppState {
    /// Construct the initial state with the given persistent database connection.
    #[must_use]
    pub fn new(app_db: rusqlite::Connection) -> Self {
        Self {
            stores: Mutex::new(HashMap::new()),
            open_paths: Mutex::new(HashMap::new()),
            app_db: Mutex::new(app_db),
            llm: Mutex::new(None),
        }
    }
}
