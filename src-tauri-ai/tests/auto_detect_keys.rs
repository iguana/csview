//! Regression tests for the LLM-client startup loader.
//!
//! Reproduces the bugs that v0.2.0 hit in production:
//!   1. `auto_detect_keys` must REFUSE to build an `LlmClient` from a saved
//!      account row that has an empty model — otherwise downstream LLM calls
//!      send `"model": ""` and providers reject the request.
//!   2. Empty-model rows in the DB must fall through to env-var detection
//!      rather than silently leaving `state.llm` populated with junk.
//!
//! These tests don't require any LLM credentials — they only check the
//! plumbing around state.llm, the DB row parsing, and validation.

use csviewai_lib::commands_ai::auto_detect_keys;
use csviewai_lib::state::AiAppState;
use rusqlite::Connection;
use std::sync::{Mutex, MutexGuard};

/// Tests in this file mutate process-wide env vars. Cargo runs them in a
/// shared process (one binary per integration-test file), so they must
/// serialise around a single guard to avoid cross-test pollution. Each
/// test starts by acquiring this mutex AND clearing the API-key env vars.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_and_clean() -> MutexGuard<'static, ()> {
    let g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    clear_env_keys();
    g
}

/// Build a fresh in-memory app DB with the same schema as the real one.
fn fresh_app_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS account (
            id         INTEGER PRIMARY KEY,
            api_key    TEXT    NOT NULL,
            model      TEXT    NOT NULL DEFAULT '',
            created_at TEXT    NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )
    .expect("schema");
    conn
}

fn clear_env_keys() {
    for k in ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"] {
        std::env::remove_var(k);
    }
}

#[test]
fn empty_db_with_no_env_vars_leaves_llm_unset() {
    let _g = lock_and_clean();
    let state = AiAppState::new(fresh_app_db());
    auto_detect_keys(&state);
    assert!(
        state.llm.lock().is_none(),
        "with no DB row and no env vars, state.llm must stay None"
    );
}

#[test]
fn saved_row_with_empty_model_does_not_produce_a_client() {
    // Reproduces the v0.2.0 bug where a leftover row written by an earlier
    // build (which didn't validate the model field) caused the next launch
    // to construct `LlmClient::new(provider, key, "")`. Subsequent OpenAI
    // / Anthropic calls then bounced with "you must provide a model".
    let _g = lock_and_clean();
    let conn = fresh_app_db();
    conn.execute(
        "INSERT INTO account (id, api_key, model) VALUES (1, ?1, '')",
        rusqlite::params!["openai:sk-test-redacted"],
    )
    .unwrap();
    let state = AiAppState::new(conn);
    auto_detect_keys(&state);
    assert!(
        state.llm.lock().is_none(),
        "empty model in saved row must NOT seed the LLM client"
    );
}

#[test]
fn saved_row_with_unknown_provider_prefix_does_not_produce_a_client() {
    let _g = lock_and_clean();
    let conn = fresh_app_db();
    conn.execute(
        "INSERT INTO account (id, api_key, model) VALUES (1, ?1, 'gpt-4.1')",
        rusqlite::params!["nopesky:sk-bogus"],
    )
    .unwrap();
    let state = AiAppState::new(conn);
    auto_detect_keys(&state);
    assert!(state.llm.lock().is_none());
}

#[test]
fn valid_saved_row_seeds_the_client_with_the_persisted_model() {
    let _g = lock_and_clean();
    let conn = fresh_app_db();
    conn.execute(
        "INSERT INTO account (id, api_key, model) VALUES (1, ?1, 'gpt-4.1-mini')",
        rusqlite::params!["openai:sk-test-restoreme"],
    )
    .unwrap();
    let state = AiAppState::new(conn);
    auto_detect_keys(&state);
    let guard = state.llm.lock();
    let client = guard.as_ref().expect("client should be restored");
    assert_eq!(client.model(), "gpt-4.1-mini");
}

#[test]
fn empty_model_row_falls_through_to_env_var() {
    // Empty-model row + ANTHROPIC env var → env var wins, with the
    // hardcoded default Anthropic model.
    let _g = lock_and_clean();
    std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-fake");
    let conn = fresh_app_db();
    conn.execute(
        "INSERT INTO account (id, api_key, model) VALUES (1, ?1, '')",
        rusqlite::params!["openai:sk-test-redacted"],
    )
    .unwrap();
    let state = AiAppState::new(conn);
    auto_detect_keys(&state);
    let guard = state.llm.lock();
    let client = guard.as_ref().expect("env var should provide a client");
    assert_eq!(
        client.model(),
        "claude-sonnet-4-20250514",
        "anthropic env-var path should use its default model"
    );
    clear_env_keys();
}
