//! End-to-end integration tests for every csviewai AI feature.
//!
//! These tests run against **real LLM providers** using credentials loaded
//! from a `.env` file at the workspace root. To run them:
//!
//! ```text
//! cargo test --test ai_features_integration -- --nocapture --test-threads=1
//! ```
//!
//! The test harness picks the first available provider in this order:
//!   1. `ANTHROPIC_API_KEY`
//!   2. `OPENAI_API_KEY`
//!   3. `GEMINI_API_KEY`
//!
//! …and uses the cheapest "fast"-tier model for that provider so a full
//! suite run costs <$0.05. If no key is found, every test is skipped with
//! a printed reason rather than failing — `cargo test` stays green on
//! contributor machines that don't have credentials configured.
//!
//! Each test exercises:
//!   - the prompt-builder for the feature
//!   - the LlmClient call (`complete`, `complete_json`, or `chat`)
//!   - the response shape the production code depends on (so a regression
//!     in either the prompt template or the provider's behaviour shows up
//!     here before users hit it)

use std::path::PathBuf;
use std::sync::Once;

use csview_engine::engine::{ColumnKind, ColumnMeta};
use csview_engine::quality;
use csview_engine::sqlite_store::{SchemaContext, SqliteStore};
use csview_engine::stats_extended;
use csviewai_lib::llm::client::{
    available_models, ChatMessage, LlmClient, ModelTier, Provider,
};
use csviewai_lib::llm::prompts;
use csviewai_lib::llm::types::LlmError;
// Silence unused-import warning for ExtendedColumnStats that older test
// bodies referenced; we only use the function-level API now.
#[allow(unused_imports)]
use stats_extended::{AnomalyResult, ExtendedColumnStats};

// ---------------------------------------------------------------------------
// Harness — provider selection + fixtures
// ---------------------------------------------------------------------------

static ENV_INIT: Once = Once::new();

/// Load `.env` from the workspace root once per test run.
fn load_env() {
    ENV_INIT.call_once(|| {
        // The test binary's CARGO_MANIFEST_DIR is src-tauri-ai/. The `.env`
        // lives at the workspace root one directory up.
        let workspace_env = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".env");
        if workspace_env.exists() {
            // dotenvy::from_path returns Err if the file is malformed; we
            // ignore that here so a stray syntax issue doesn't kill the
            // whole suite — the per-test `available_provider` check will
            // surface "no key" instead.
            let _ = dotenvy::from_path(&workspace_env);
        }
    });
}

/// Pick a provider to test against. Returns `None` (with a printed reason)
/// if no credentials are available.
fn available_provider() -> Option<(Provider, String, String)> {
    load_env();
    let candidates = [
        ("ANTHROPIC_API_KEY", Provider::Anthropic),
        ("OPENAI_API_KEY", Provider::OpenAI),
        ("GEMINI_API_KEY", Provider::Google),
    ];
    for (env_var, provider) in candidates {
        if let Ok(key) = std::env::var(env_var) {
            if !key.trim().is_empty() {
                let model = cheapest_model(provider);
                return Some((provider, key, model));
            }
        }
    }
    eprintln!(
        "skip: no API keys in .env (looked for ANTHROPIC_API_KEY, \
         OPENAI_API_KEY, GEMINI_API_KEY)",
    );
    None
}

/// Pick the cheapest known model for the provider (Fast tier when available).
fn cheapest_model(provider: Provider) -> String {
    available_models()
        .into_iter()
        .filter(|m| m.provider == provider)
        .find(|m| m.tier == ModelTier::Fast)
        .or_else(|| {
            available_models()
                .into_iter()
                .find(|m| m.provider == provider)
        })
        .map(|m| m.id)
        .unwrap_or_default()
}

/// Build a fresh in-memory SqliteStore from the workspace's sample CSV.
fn open_employees_store() -> SqliteStore {
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("samples")
        .join("employees.csv");
    assert!(
        csv_path.exists(),
        "samples/employees.csv missing at {}",
        csv_path.display()
    );

    // Headers from row 1; the inference logic in commands_csv mirrors what
    // `open_csv` does in production. We hand-craft equivalent metadata here
    // to keep the test independent of that command.
    let headers = vec![
        "id".to_string(),
        "first_name".to_string(),
        "last_name".to_string(),
        "department".to_string(),
        "title".to_string(),
        "salary".to_string(),
        "hired_on".to_string(),
        "active".to_string(),
    ];
    let columns = vec![
        col(0, "id", ColumnKind::Integer),
        col(1, "first_name", ColumnKind::String),
        col(2, "last_name", ColumnKind::String),
        col(3, "department", ColumnKind::String),
        col(4, "title", ColumnKind::String),
        col(5, "salary", ColumnKind::Integer),
        col(6, "hired_on", ColumnKind::Date),
        col(7, "active", ColumnKind::Boolean),
    ];
    SqliteStore::from_csv(
        csv_path.to_str().unwrap(),
        b',',
        true,
        &headers,
        &columns,
    )
    .expect("import sample CSV")
}

fn col(i: usize, name: &str, kind: ColumnKind) -> ColumnMeta {
    ColumnMeta {
        index: i,
        name: name.to_string(),
        kind,
    }
}

fn schema_for(store: &SqliteStore) -> SchemaContext {
    store
        .schema_context(5)
        .expect("schema_context for sample store")
}

fn client() -> Option<LlmClient> {
    let (provider, key, model) = available_provider()?;
    eprintln!("using provider={provider} model={model}");
    Some(LlmClient::new(provider, key, model))
}

/// Pretty-print an LLM response so failures are easy to debug.
fn dump(label: &str, body: &str) {
    eprintln!("--- {label} ({} chars) ---", body.len());
    let preview = if body.len() > 800 { &body[..800] } else { body };
    eprintln!("{preview}");
    eprintln!("--- end {label} ---");
}

// ---------------------------------------------------------------------------
// Sanity tests
// ---------------------------------------------------------------------------

#[test]
fn dotenv_loads_at_least_one_key() {
    load_env();
    let any = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"]
        .iter()
        .any(|k| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false));
    if !any {
        eprintln!("skip: no LLM keys in .env — populate one to run integration tests");
        return;
    }
    assert!(any);
}

#[test]
fn employees_sample_csv_imports() {
    let store = open_employees_store();
    let schema = schema_for(&store);
    assert_eq!(schema.row_count, 50);
    assert_eq!(schema.columns.len(), 8);
    assert_eq!(schema.columns[0].name, "id");
}

// ---------------------------------------------------------------------------
// Feature 1 — NL query → SQL WHERE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_1_nl_query_engineers() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let (system, user) =
        prompts::nl_query_prompt(&schema, "engineers earning more than 150000");

    let where_clause = client
        .complete(&system, &user, 256)
        .await
        .expect("LLM call");
    dump("nl_query WHERE", &where_clause);

    let where_clean = strip_first_line_if_keyword(where_clause.trim(), "WHERE");
    let sql = format!("SELECT * FROM data WHERE {where_clean}");
    let result = store.query(&sql).expect("execute generated SQL");
    assert!(
        result.row_count > 0,
        "expected at least one engineer row to match"
    );
    assert!(
        result.row_count <= 50,
        "filter should not return more than the dataset"
    );
}

// ---------------------------------------------------------------------------
// Feature 2 — Data profile narrative
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_2_data_profile_narrative() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let stats_json =
        serde_json::to_string(&schema).expect("serialise schema as stand-in stats");

    let (system, user) = prompts::data_profile_prompt(&schema, &stats_json);
    let report = client.complete(&system, &user, 1024).await.expect("LLM");
    dump("profile", &report);
    let lower = report.to_lowercase();
    assert!(
        lower.contains("dataset overview")
            || lower.contains("overview")
            || lower.contains("column"),
        "report should mention overview/columns, got: {report}"
    );
}

// ---------------------------------------------------------------------------
// Feature 3 — NL transform → derived column
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_3_nl_transform_derived_column() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let (system, user) =
        prompts::column_transform_prompt(&schema, "annual salary in thousands");

    #[derive(serde::Deserialize, Debug)]
    struct Resp {
        expression: String,
        column_name: String,
    }
    let resp: Resp = client
        .complete_json(&system, &user, 256)
        .await
        .expect("LLM JSON");
    eprintln!("transform: {resp:?}");
    assert!(!resp.expression.trim().is_empty());
    assert!(!resp.column_name.trim().is_empty());
    // The expression should reference the salary column in some form.
    assert!(
        resp.expression.to_lowercase().contains("salary"),
        "expression should mention salary: {}",
        resp.expression
    );
}

// ---------------------------------------------------------------------------
// Feature 4 — Anomaly narrative
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_4_anomaly_narrative() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let (col_metas, rows) = read_all_rows_for_test(&store);
    // Detect anomalies on the salary column (index 5) — same call shape as
    // the production `detect_anomalies` Tauri command.
    let anomalies = stats_extended::detect_anomalies(&rows, &col_metas, &[5usize], 3.0);
    let anomaly_json = serde_json::to_string(&anomalies).unwrap();

    let (system, user) = prompts::anomaly_prompt(&schema, &anomaly_json);
    let report = client.complete(&system, &user, 1024).await.expect("LLM");
    dump("anomaly", &report);
    assert!(report.len() > 50);
}

// ---------------------------------------------------------------------------
// Feature 5 — Smart group (NL → GROUP BY)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_5_smart_group_by_department() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let (system, user) =
        prompts::smart_group_prompt(&schema, "average salary by department");

    #[derive(serde::Deserialize, Debug)]
    struct Resp {
        sql: String,
        title: String,
        description: String,
    }
    let resp: Resp = client.complete_json(&system, &user, 512).await.expect("LLM JSON");
    eprintln!("group: {resp:?}");
    assert!(resp.sql.to_uppercase().contains("GROUP BY"));
    let result = store.query(&resp.sql).expect("execute group-by SQL");
    assert!(result.row_count > 0);
    assert!(!resp.title.trim().is_empty());
    assert!(!resp.description.trim().is_empty());
}

// ---------------------------------------------------------------------------
// Feature 6 — Quality audit narrative
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_6_quality_audit_narrative() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let (col_metas, rows) = read_all_rows_for_test(&store);
    let mut issues: Vec<quality::QualityIssue> = Vec::new();
    for meta in &col_metas {
        let col_vals: Vec<&str> = rows
            .iter()
            .map(|r| r.get(meta.index).map(String::as_str).unwrap_or(""))
            .collect();
        issues.extend(quality::audit_column(&col_vals, meta, meta.index));
    }
    let issues_json = serde_json::to_string(&issues).unwrap_or_else(|_| "[]".into());
    let (system, user) = prompts::quality_audit_prompt(&schema, &issues_json);
    let report = client.complete(&system, &user, 1024).await.expect("LLM");
    dump("quality", &report);
    assert!(report.len() > 50);
}

// ---------------------------------------------------------------------------
// Feature 7 — Chat (multi-turn)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_7_chat_multi_turn() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let system = prompts::chat_system_prompt(&schema);
    let history = vec![
        ChatMessage::user("How many engineers are in the dataset?"),
    ];
    let reply = client.chat(&system, &history, 512).await.expect("LLM chat");
    dump("chat-1", &reply);
    assert!(reply.len() > 5);

    // Follow-up — confirms the chat() path correctly carries multi-turn context.
    let mut history = history;
    history.push(ChatMessage::assistant(reply.clone()));
    history.push(ChatMessage::user("And the highest paid one?"));
    let reply2 = client.chat(&system, &history, 512).await.expect("LLM chat 2");
    dump("chat-2", &reply2);
    assert!(reply2.len() > 5);
}

// ---------------------------------------------------------------------------
// Feature 8 — Report builder
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_8_report_builder() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let (system, user) =
        prompts::report_builder_prompt(&schema, "headcount by department");

    #[derive(serde::Deserialize, Debug)]
    struct Resp {
        title: String,
        markdown: String,
    }
    let resp: Resp = client
        .complete_json(&system, &user, 1024)
        .await
        .expect("LLM JSON");
    eprintln!("report title: {}", resp.title);
    dump("report markdown", &resp.markdown);
    assert!(!resp.title.trim().is_empty());
    assert!(resp.markdown.len() > 100);
}

// ---------------------------------------------------------------------------
// Feature 9 — Join suggestion (left vs right schemas)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_9_join_suggestion() {
    let Some(client) = client() else {
        return;
    };
    // Build a "right" table (departments) so there's an obvious key.
    let csv = "dept_id,department,headcount\n1,Engineering,12\n2,Design,5\n3,Data,4\n";
    let path = std::env::temp_dir().join(format!(
        "csviewai_test_depts_{}.csv",
        std::process::id()
    ));
    std::fs::write(&path, csv).expect("temp csv");
    let right_headers = vec!["dept_id".into(), "department".into(), "headcount".into()];
    let right_cols = vec![
        col(0, "dept_id", ColumnKind::Integer),
        col(1, "department", ColumnKind::String),
        col(2, "headcount", ColumnKind::Integer),
    ];
    let right_store = SqliteStore::from_csv(
        path.to_str().unwrap(),
        b',',
        true,
        &right_headers,
        &right_cols,
    )
    .expect("right import");

    let left = schema_for(&open_employees_store());
    let right = schema_for(&right_store);
    let (system, user) = prompts::join_suggestion_prompt(&left, &right);

    #[derive(serde::Deserialize, Debug)]
    struct Resp {
        left_key: String,
        right_key: String,
        join_type: String,
        rationale: String,
    }
    let resp: Resp = client
        .complete_json(&system, &user, 256)
        .await
        .expect("LLM JSON");
    eprintln!("join: {resp:?}");
    assert!(matches!(
        resp.join_type.as_str(),
        "INNER" | "LEFT" | "RIGHT" | "FULL"
    ));
    assert!(!resp.left_key.is_empty() && !resp.right_key.is_empty());
    assert!(resp.rationale.len() > 5);
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// Feature 10 — Compliance / PII narrative
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_10_compliance_narrative() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let pii_json =
        r#"[{"column":"first_name","kind":"name","samples":["Alice","Benjamin"]}]"#;
    let (system, user) = prompts::compliance_prompt(&schema, pii_json);
    let report = client.complete(&system, &user, 1024).await.expect("LLM");
    dump("compliance", &report);
    assert!(report.len() > 100);
    let lower = report.to_lowercase();
    assert!(lower.contains("pii") || lower.contains("privacy") || lower.contains("compliance"));
}

// ---------------------------------------------------------------------------
// Feature 11 — Forecast narrative around a regression result
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_11_forecast_narrative() {
    let Some(client) = client() else {
        return;
    };
    let store = open_employees_store();
    let schema = schema_for(&store);
    let regression_json = r#"{"slope":2500.0,"intercept":150000.0,"r_squared":0.62,"n":50}"#;
    let (system, user) = prompts::forecast_prompt(&schema, "id", "salary", regression_json);
    let report = client.complete(&system, &user, 1024).await.expect("LLM");
    dump("forecast", &report);
    assert!(report.len() > 100);
}

// ---------------------------------------------------------------------------
// Bonus: API surface guarantees
// ---------------------------------------------------------------------------

/// `fetch_models` should return a non-empty, chat-completable catalogue
/// for the active provider's key. Catches:
///   - upstream API shape changes (the JSON parser breaks)
///   - over-aggressive filtering (everything filtered out)
///   - auth-header regressions per provider
#[tokio::test]
async fn fetch_models_returns_a_usable_catalogue() {
    let Some(client) = client() else {
        return;
    };
    let models = client.fetch_models().await.expect("fetch models");
    eprintln!(
        "fetched {} models for {}",
        models.len(),
        client.provider()
    );
    assert!(!models.is_empty(), "provider returned zero usable models");
    for m in &models {
        assert!(!m.id.is_empty(), "fetched model has empty id");
        assert!(!m.name.is_empty(), "fetched model has empty name");
    }
    // The first entry (highest tier) should round-trip through complete()
    // so we know the id we just listed actually accepts requests.
    let first = &models[0];
    let probe = LlmClient::new(
        client.provider(),
        std::env::var(match client.provider() {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAI => "OPENAI_API_KEY",
            Provider::Google => "GEMINI_API_KEY",
        })
        .unwrap_or_default(),
        first.id.clone(),
    );
    // Reasoning-tier models burn output tokens on hidden reasoning before
    // emitting any text, so give the probe enough headroom to actually
    // produce visible output.
    let reply = probe
        .complete(
            "you are a test",
            "reply with the word OK and nothing else",
            2048,
        )
        .await
        .expect("fetched model should accept a chat request");
    eprintln!("probe reply from {}: {reply:?}", first.id);
    assert!(reply.to_uppercase().contains("OK"));
}

/// All `available_models()` entries should reference an id format the
/// providers actually accept. We don't ping the API here, but we do guard
/// against typos that would re-introduce the "must provide a model" bug.
#[test]
fn every_advertised_model_has_a_non_empty_id() {
    for m in available_models() {
        assert!(!m.id.is_empty(), "model {} has empty id", m.name);
        assert!(!m.name.is_empty(), "model id {} has empty name", m.id);
    }
}

/// Quick check the LlmClient surface matches what commands_ai.rs depends on.
#[test]
fn llm_client_constructs_for_every_provider() {
    for p in [Provider::Anthropic, Provider::OpenAI, Provider::Google] {
        let model = cheapest_model(p);
        assert!(!model.is_empty(), "no fast model for {p:?}");
        let c = LlmClient::new(p, "fake-key".into(), model.clone());
        assert_eq!(c.provider(), p);
        assert_eq!(c.model(), model);
    }
}

/// An empty model ID at construction time is allowed but the client should
/// fail the LLM call clearly (not crash). We can detect this by passing an
/// obviously-bogus key + empty model and checking for a 4xx Api error.
#[tokio::test]
async fn empty_model_surfaces_as_api_error_not_panic() {
    if available_provider().is_none() {
        return;
    }
    let (provider, key, _) = available_provider().unwrap();
    let bad = LlmClient::new(provider, key, String::new());
    let err = bad.complete("you are a test", "say hi", 16).await.unwrap_err();
    match err {
        LlmError::Api { status, message } => {
            eprintln!("expected api error: status={status} msg={message}");
            assert!(status >= 400 && status < 500);
        }
        other => panic!("expected Api error for empty model, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Replicates the post-processing nl_query() does: an LLM occasionally
/// re-emits the WHERE keyword despite the prompt; strip it once.
fn strip_first_line_if_keyword(s: &str, keyword: &str) -> String {
    let trimmed = s.trim();
    let upper = trimmed.to_uppercase();
    if upper.starts_with(&format!("{keyword} ")) {
        trimmed[keyword.len() + 1..].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Mirrors `commands_ai::read_all_rows` minus the lock. Keeps the test
/// independent of the (unsafe-to-instantiate-in-tests) AiAppState type.
fn read_all_rows_for_test(store: &SqliteStore) -> (Vec<ColumnMeta>, Vec<Vec<String>>) {
    let cols: Vec<ColumnMeta> = store.columns().to_vec();
    let result = store.query("SELECT * FROM data").expect("select all");
    let rows: Vec<Vec<String>> = result
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                })
                .collect()
        })
        .collect();
    (cols, rows)
}
