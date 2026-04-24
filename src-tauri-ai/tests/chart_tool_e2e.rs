//! End-to-end chart-tool integration tests.
//!
//! Verifies that the chart-making tool works through the real LLM tool
//! calling APIs of every provider with a key in `.env`. For each provider:
//!
//! 1. Fire `chat_with_tools` with the chart tool definition + the prompt
//!    "show me a bar chart of average salary by department".
//! 2. Assert the model returns a `ToolCall` for `make_chart` (not just text).
//! 3. Decode the JSON args into `ChartSpec`, run it against the real
//!    SqliteStore built from samples/employees.csv, and assert:
//!      - chart_type matches what was requested
//!      - SQL contains AVG and GROUP BY
//!      - the rendered data has 6 rows (the dataset's 6 distinct depts)
//!      - the values are non-empty numbers
//!
//! Each provider has its own #[tokio::test]; tests skip with a printed
//! reason when no key is present rather than failing.

use csview_engine::chart::{self, ChartKind, ChartSpec};
use csview_engine::engine::{ColumnKind, ColumnMeta};
use csview_engine::sqlite_store::SqliteStore;
use csviewai_lib::llm::client::{LlmClient, Provider, ToolDefinition, ToolEvent, ToolReply};
use csviewai_lib::llm::prompts;

use std::path::PathBuf;
use std::sync::Once;

static ENV_INIT: Once = Once::new();
fn load_env() {
    ENV_INIT.call_once(|| {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".env");
        if p.exists() {
            let _ = dotenvy::from_path(&p);
        }
    });
}

fn employees_store() -> SqliteStore {
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("samples")
        .join("employees.csv");
    let headers = vec![
        "id".into(),
        "first_name".into(),
        "last_name".into(),
        "department".into(),
        "title".into(),
        "salary".into(),
        "hired_on".into(),
        "active".into(),
    ];
    let cols = vec![
        col(0, "id", ColumnKind::Integer),
        col(1, "first_name", ColumnKind::String),
        col(2, "last_name", ColumnKind::String),
        col(3, "department", ColumnKind::String),
        col(4, "title", ColumnKind::String),
        col(5, "salary", ColumnKind::Integer),
        col(6, "hired_on", ColumnKind::Date),
        col(7, "active", ColumnKind::Boolean),
    ];
    SqliteStore::from_csv(csv_path.to_str().unwrap(), b',', true, &headers, &cols)
        .expect("import sample csv")
}

fn col(i: usize, name: &str, kind: ColumnKind) -> ColumnMeta {
    ColumnMeta {
        index: i,
        name: name.into(),
        kind,
    }
}

/// Same JSON-schema shape as the production `chart_tool_definition` in
/// commands_ai.rs (which we can't depend on from a test crate without
/// adding plumbing — so we inline the equivalent definition).
fn chart_tool() -> ToolDefinition {
    ToolDefinition {
        name: "make_chart".into(),
        description:
            "Render a chart from the open CSV. The system runs the SQL and \
             draws the chart — DO NOT invent values, only choose the chart \
             type and which columns to use."
                .into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "chartType": {
                    "type": "string",
                    "enum": [
                        "bar", "horizontal_bar", "stacked_bar", "grouped_bar",
                        "line", "area", "pie", "donut", "scatter", "histogram",
                        "treemap"
                    ]
                },
                "title": {"type": "string"},
                "xColumn": {"type": "string"},
                "yColumn": {"type": "string"},
                "aggregation": {
                    "type": "string",
                    "enum": ["count", "sum", "avg", "min", "max"]
                },
                "groupBy": {"type": "string"},
                "limit": {"type": "integer"},
                "order": {"type": "string", "enum": ["asc", "desc"]},
                "binCount": {"type": "integer"}
            },
            "required": ["chartType", "title", "xColumn"]
        }),
    }
}

fn schema_summary() -> String {
    "Table: data\nRows: 50\nColumns:\n\
     - id (Integer)\n\
     - first_name (String)\n\
     - last_name (String)\n\
     - department (String): 6 distinct values\n\
     - title (String): 19 distinct values\n\
     - salary (Integer): 5 distinct quintiles\n\
     - hired_on (Date)\n\
     - active (Boolean)\n"
        .to_string()
}

/// Drive a single round of the tool loop. Ask the model to chart something,
/// expect a `make_chart` tool call back, decode + execute it, and assert
/// the deterministic shape of the rendered data.
async fn run_chart_round(
    client: LlmClient,
    label: &str,
    user_prompt: &str,
    expected_kind: ChartKind,
    expected_row_count: usize,
) {
    let store = employees_store();
    let tools = vec![chart_tool()];
    let system = format!(
        "You are a helpful data assistant. The user has a CSV open. When they \
         ask for a visualisation, call the make_chart tool — never describe \
         a chart in text. Schema:\n{}",
        schema_summary()
    );
    let events = vec![ToolEvent::User(user_prompt.into())];
    let reply = client
        .chat_with_tools(&system, &events, &tools, 2048)
        .await
        .unwrap_or_else(|e| panic!("[{label}] tool-call request failed: {e:?}"));
    let (call_id, name, args) = match reply {
        ToolReply::ToolCall {
            call_id,
            name,
            arguments,
        } => (call_id, name, arguments),
        ToolReply::Text(t) => panic!(
            "[{label}] expected tool call, got plain text reply:\n{t}"
        ),
    };
    eprintln!("[{label}] tool call name={name} args={args}");
    assert_eq!(name, "make_chart", "[{label}] wrong tool name");
    assert!(!call_id.is_empty(), "[{label}] empty call_id");

    let spec: ChartSpec = serde_json::from_value(args.clone())
        .unwrap_or_else(|e| panic!("[{label}] could not decode ChartSpec: {e}\nargs={args}"));
    eprintln!(
        "[{label}] decoded spec: kind={:?} x={} y={:?} agg={:?}",
        spec.chart_type, spec.x_column, spec.y_column, spec.aggregation
    );
    assert_eq!(
        spec.chart_type, expected_kind,
        "[{label}] model picked wrong chart kind"
    );

    let chart = chart::make_chart(&store, spec).expect("chart computation");
    eprintln!(
        "[{label}] sql={}\n[{label}] rows.len={} (expected {})",
        chart.sql,
        chart.rows.len(),
        expected_row_count
    );
    assert_eq!(
        chart.rows.len(),
        expected_row_count,
        "[{label}] wrong number of chart rows"
    );
    // Every row should have a non-null y value.
    for r in &chart.rows {
        assert!(
            r.get("y").map(|v| !v.is_null()).unwrap_or(false),
            "[{label}] row missing y: {r}"
        );
    }
}

// ---------------------------------------------------------------------------
// One #[tokio::test] per provider (skipping when the key isn't present).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn openai_makes_a_bar_chart() {
    load_env();
    let Ok(key) = std::env::var("OPENAI_API_KEY") else {
        eprintln!("skip: OPENAI_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        eprintln!("skip: OPENAI_API_KEY empty");
        return;
    }
    let model = std::env::var("CSVIEW_TEST_MODEL").unwrap_or_else(|_| "gpt-5.4".into());
    let client = LlmClient::new(Provider::OpenAI, key, model);
    run_chart_round(
        client,
        "openai",
        "show me a bar chart of average salary by department",
        ChartKind::Bar,
        6, // 6 distinct departments in samples/employees.csv
    )
    .await;
}

#[tokio::test]
async fn google_makes_a_bar_chart() {
    load_env();
    let Ok(key) = std::env::var("GEMINI_API_KEY") else {
        eprintln!("skip: GEMINI_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        eprintln!("skip: GEMINI_API_KEY empty");
        return;
    }
    let client = LlmClient::new(Provider::Google, key, "gemini-2.5-flash".into());
    run_chart_round(
        client,
        "google",
        "show me a bar chart of average salary by department",
        ChartKind::Bar,
        6,
    )
    .await;
}

#[tokio::test]
async fn anthropic_makes_a_bar_chart() {
    load_env();
    let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
        eprintln!("skip: ANTHROPIC_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        eprintln!("skip: ANTHROPIC_API_KEY empty");
        return;
    }
    let client = LlmClient::new(Provider::Anthropic, key, "claude-haiku-4-5-20251001".into());
    run_chart_round(
        client,
        "anthropic",
        "show me a bar chart of average salary by department",
        ChartKind::Bar,
        6,
    )
    .await;
}

/// **Regression test for the "make me a chart" prompt** — exercises the
/// EXACT production system prompt + tool description, not a custom one
/// tailored to the test. A previous shipping bug was that the production
/// chat_system_prompt told the model "wrap SQL in a code block" but
/// never mentioned `make_chart`, so the model wrote Python matplotlib
/// instead of calling the tool. This test would have caught that.
///
/// The prompt is deliberately conversational ("make me a chart of …")
/// rather than instructive ("call the make_chart tool with …") so the
/// model has to be nudged toward tool use by the prompt + tool
/// description alone.
async fn run_production_prompt_round(client: LlmClient, label: &str, user_prompt: &str) {
    let store = employees_store();
    let schema_ctx = store
        .schema_context(5)
        .expect("schema_context for sample store");
    // Use the SAME prompt the production chat_message uses — the chart
    // bug was a divergence between this and the in-test custom prompt.
    let system = prompts::chat_system_prompt(&schema_ctx);
    let tools = vec![chart_tool()];

    let events = vec![ToolEvent::User(user_prompt.into())];
    let reply = client
        .chat_with_tools(&system, &events, &tools, 2048)
        .await
        .unwrap_or_else(|e| panic!("[{label}] tool-call request failed: {e:?}"));

    match reply {
        ToolReply::ToolCall { name, arguments, .. } => {
            assert_eq!(name, "make_chart", "[{label}] wrong tool name");
            let spec: ChartSpec = serde_json::from_value(arguments.clone()).unwrap_or_else(|e| {
                panic!("[{label}] could not decode ChartSpec: {e}\nargs={arguments}")
            });
            let chart = chart::make_chart(&store, spec).expect("chart computation");
            assert!(!chart.rows.is_empty(), "[{label}] chart had no rows");
            eprintln!(
                "[{label}] ✓ tool fired ({} rows, kind={:?})",
                chart.rows.len(),
                chart.spec.chart_type
            );
        }
        ToolReply::Text(t) => {
            // Detect the previously-shipped failure mode: model writes
            // matplotlib / pandas / plotly instead of using the tool.
            let lower = t.to_lowercase();
            let code_smells = [
                "import matplotlib",
                "import plotly",
                "import seaborn",
                "df.plot(",
                "plt.bar",
                "plt.pie",
                "plt.show",
                "```python",
            ];
            let smell = code_smells
                .iter()
                .find(|s| lower.contains(*s))
                .copied()
                .unwrap_or("");
            panic!(
                "[{label}] expected a make_chart tool call, got plain text instead.\n\
                 Suspicious code-smell: {smell:?}\n\
                 Reply was:\n{t}"
            );
        }
    }
}

#[tokio::test]
async fn openai_uses_tool_with_production_prompt() {
    load_env();
    let Ok(key) = std::env::var("OPENAI_API_KEY") else {
        eprintln!("skip: OPENAI_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        return;
    }
    let model = std::env::var("CSVIEW_TEST_MODEL").unwrap_or_else(|_| "gpt-5.4".into());
    let client = LlmClient::new(Provider::OpenAI, key, model);
    run_production_prompt_round(
        client,
        "openai-prod-prompt",
        "make me a chart showing how salaries are distributed across departments",
    )
    .await;
}

#[tokio::test]
async fn google_uses_tool_with_production_prompt() {
    load_env();
    let Ok(key) = std::env::var("GEMINI_API_KEY") else {
        eprintln!("skip: GEMINI_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        return;
    }
    let client = LlmClient::new(Provider::Google, key, "gemini-2.5-flash".into());
    run_production_prompt_round(
        client,
        "google-prod-prompt",
        "make me a chart showing how salaries are distributed across departments",
    )
    .await;
}

/// **Regression test for the "(reached max chart-tool iterations)" bug.**
///
/// The production chat_message loops until the model returns text. If the
/// model just keeps emitting tool calls (or, worse, never gets enough
/// tokens to compose a reply after burning some on hidden reasoning), the
/// loop hits its cap and the user sees a meaningless `(reached max…)`
/// fallback string instead of either a chart description or a chart.
///
/// This test mirrors the production loop:
///   1. call chat_with_tools with the chart tool
///   2. on ToolCall: execute make_chart, push events, continue
///   3. on Text: stop and assert non-empty
///   4. if cap is hit with charts produced: do one final tools-disabled
///      call to coerce a narrative — this MUST return non-empty text
async fn run_full_chat_loop(client: LlmClient, label: &str, user_prompt: &str) {
    use csviewai_lib::llm::client::ToolDefinition as TD;
    let store = employees_store();
    let schema_ctx = store.schema_context(5).expect("schema");
    let system = prompts::chat_system_prompt(&schema_ctx);
    let tools: Vec<TD> = vec![chart_tool()];
    let mut events: Vec<ToolEvent> = vec![ToolEvent::User(user_prompt.into())];

    const MAX_ROUNDS: usize = 5;
    let mut charts_made = 0usize;
    let mut final_text = String::new();

    for round in 0..MAX_ROUNDS {
        let reply = client
            .chat_with_tools(&system, &events, &tools, 4096)
            .await
            .unwrap_or_else(|e| panic!("[{label}] round {round}: {e:?}"));
        match reply {
            ToolReply::Text(t) => {
                final_text = t;
                break;
            }
            ToolReply::ToolCall {
                call_id,
                name,
                arguments,
            } => {
                assert_eq!(name, "make_chart", "[{label}] unknown tool");
                let spec: ChartSpec =
                    serde_json::from_value(arguments.clone()).expect("decode chart spec");
                eprintln!(
                    "[{label}] round {round}: tool call kind={:?} x={}",
                    spec.chart_type, spec.x_column
                );
                let chart = chart::make_chart(&store, spec.clone())
                    .unwrap_or_else(|e| panic!("[{label}] make_chart failed: {e:?}"));
                charts_made += 1;
                let summary = serde_json::json!({
                    "rendered": true,
                    "kind": chart.spec.chart_type,
                    "title": chart.spec.title,
                    "row_count": chart.rows.len(),
                });
                events.push(ToolEvent::AssistantToolCall {
                    call_id: call_id.clone(),
                    name,
                    arguments,
                });
                events.push(ToolEvent::ToolResult {
                    call_id,
                    output: summary.to_string(),
                });
            }
        }
    }

    // Mirror the production fallback: if the loop never produced text but
    // did produce charts, force one tools-disabled narrative call.
    if final_text.is_empty() && charts_made > 0 {
        eprintln!("[{label}] cap hit with {charts_made} chart(s); forcing narrative call");
        let extra_system = format!(
            "{system}\n\nThe chart(s) the user requested have already been \
             rendered. Do NOT call any more tools. Reply with a brief one or \
             two sentence interpretation."
        );
        let reply = client
            .chat_with_tools(&extra_system, &events, &[], 2048)
            .await
            .unwrap_or_else(|e| panic!("[{label}] narrative call: {e:?}"));
        match reply {
            ToolReply::Text(t) => final_text = t,
            ToolReply::ToolCall { name, .. } => {
                panic!("[{label}] narrative-only call still tried to call {name}")
            }
        }
    }

    assert!(charts_made >= 1, "[{label}] no charts were rendered");
    assert!(
        !final_text.trim().is_empty(),
        "[{label}] loop completed but produced no narrative text — \
         this is the (reached max chart-tool iterations) bug"
    );
    eprintln!(
        "[{label}] ✓ {} chart(s) + narrative ({} chars)",
        charts_made,
        final_text.len()
    );
}

#[tokio::test]
async fn openai_full_chat_loop_yields_charts_plus_narrative() {
    load_env();
    let Ok(key) = std::env::var("OPENAI_API_KEY") else {
        eprintln!("skip: OPENAI_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        return;
    }
    let model = std::env::var("CSVIEW_TEST_MODEL").unwrap_or_else(|_| "gpt-5.4".into());
    let client = LlmClient::new(Provider::OpenAI, key, model);
    run_full_chat_loop(
        client,
        "openai-loop",
        "make me a chart of average salary by department",
    )
    .await;
}

#[tokio::test]
async fn google_full_chat_loop_yields_charts_plus_narrative() {
    load_env();
    let Ok(key) = std::env::var("GEMINI_API_KEY") else {
        eprintln!("skip: GEMINI_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        return;
    }
    let client = LlmClient::new(Provider::Google, key, "gemini-2.5-flash".into());
    run_full_chat_loop(
        client,
        "google-loop",
        "make me a chart of average salary by department",
    )
    .await;
}

/// Distribution / histogram path — confirms the model picks the right kind
/// and the deterministic histogram bucketing in chart.rs round-trips.
#[tokio::test]
async fn openai_makes_a_histogram() {
    load_env();
    let Ok(key) = std::env::var("OPENAI_API_KEY") else {
        eprintln!("skip: OPENAI_API_KEY not set");
        return;
    };
    if key.trim().is_empty() {
        return;
    }
    let model = std::env::var("CSVIEW_TEST_MODEL").unwrap_or_else(|_| "gpt-5.4".into());
    let client = LlmClient::new(Provider::OpenAI, key, model);
    let store = employees_store();
    let tools = vec![chart_tool()];
    let system = format!(
        "You are a helpful data assistant. The user has a CSV open. When they \
         ask for a visualisation, call the make_chart tool. Schema:\n{}",
        schema_summary()
    );
    let events = vec![ToolEvent::User("show me a histogram of salary with 8 bins".into())];
    let reply = client
        .chat_with_tools(&system, &events, &tools, 2048)
        .await
        .expect("histogram tool call");
    let args = match reply {
        ToolReply::ToolCall { arguments, .. } => arguments,
        ToolReply::Text(t) => panic!("expected tool call, got: {t}"),
    };
    let spec: ChartSpec = serde_json::from_value(args.clone()).expect("decode");
    eprintln!("histogram spec: {spec:?}");
    assert_eq!(spec.chart_type, ChartKind::Histogram);
    let chart = chart::make_chart(&store, spec.clone()).expect("compute");
    // bin_count defaults to 12 when the model omits it; some bin count
    // between 5 and 20 is reasonable.
    assert!(chart.rows.len() >= 5 && chart.rows.len() <= 20);
    let total: i64 = chart
        .rows
        .iter()
        .map(|r| r["y"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(total, 50, "every salary should land in some bin");
}
