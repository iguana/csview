//! Prompt templates for all AI features.
//!
//! Each function returns `(system_prompt, user_message)`.  Keep system prompts
//! precise so the model knows the exact output format expected.

use csview_engine::sqlite_store::SchemaContext;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn schema_summary(ctx: &SchemaContext) -> String {
    let cols: Vec<String> = ctx
        .columns
        .iter()
        .map(|c| {
            let samples = c.sample_values.join(", ");
            format!(
                "  - {name} ({kind:?}): {unique} distinct values, {null:.0}% null. Samples: [{samples}]",
                name = c.name,
                kind = c.kind,
                unique = c.unique_count,
                null = c.nullable_pct * 100.0,
            )
        })
        .collect();
    format!(
        "Table: {table}\nRows: {rows}\nColumns:\n{cols}",
        table = ctx.table_name,
        rows = ctx.row_count,
        cols = cols.join("\n"),
    )
}

// ---------------------------------------------------------------------------
// Feature 1 — Natural-language query → SQL WHERE clause
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to translate a user question into a SQL WHERE clause.
///
/// The caller wraps the clause in `SELECT * FROM data WHERE <clause>`.
pub fn nl_query_prompt(ctx: &SchemaContext, user_query: &str) -> (String, String) {
    let system = r#"You translate natural language questions about CSV data into SQL WHERE clauses for SQLite.
You will receive the table schema. Respond with ONLY the WHERE clause — no SELECT, no FROM, no WHERE keyword itself, no semicolon.
Use the exact column names provided. Use standard SQLite syntax.
If the question cannot be expressed as a WHERE clause (e.g. it asks for aggregates), still return the closest filter that captures the intent.
Examples of valid output:
  salary > 150000 AND department = 'Engineering'
  LOWER(status) = 'active' OR LOWER(status) = 'pending'
  age BETWEEN 25 AND 40"#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nQuestion: {query}",
        schema = schema_summary(ctx),
        query = user_query,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 2 — Data profile narrative
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to write a Markdown data-profile report from pre-computed stats.
pub fn data_profile_prompt(ctx: &SchemaContext, stats_json: &str) -> (String, String) {
    let system = r#"You are a data analyst. Write a concise Markdown data profile report.
You will receive pre-computed column statistics. Reference specific numbers.
Required sections: ## Overview, ## Column Analysis, ## Patterns & Insights, ## Data Quality Notes.
Be concise — aim for under 600 words. Do not include JSON in the report."#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nStatistics:\n{stats}",
        schema = schema_summary(ctx),
        stats = stats_json,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 3 — Column transform
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to generate a SQLite expression for a derived column.
///
/// Expected JSON response: `{"expression": "...", "column_name": "..."}`.
pub fn column_transform_prompt(ctx: &SchemaContext, user_query: &str) -> (String, String) {
    let system = r#"Generate a SQLite SQL expression to derive a new column value from existing columns.
Output ONLY valid JSON (no markdown fences) in exactly this shape:
{"expression": "<sqlite expression>", "column_name": "<snake_case_name>"}
The expression should work in: UPDATE data SET new_col = (<expression>)
Use only SQLite built-in functions. Examples:
{"expression": "UPPER(name)", "column_name": "name_upper"}
{"expression": "salary * 12", "column_name": "annual_salary"}
{"expression": "substr(email, instr(email, '@') + 1)", "column_name": "email_domain"}"#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nRequest: {query}",
        schema = schema_summary(ctx),
        query = user_query,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 4 — Anomaly detection narrative
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to interpret numeric anomaly results.
pub fn anomaly_prompt(ctx: &SchemaContext, anomaly_json: &str) -> (String, String) {
    let system = r#"You are a data quality analyst. Interpret statistical anomaly results and write a Markdown summary.
You will receive pre-computed anomaly scores (z-scores, IQR outliers) for numeric columns.
Format:
## Anomaly Summary
One paragraph overview.
## Column Findings
Bullet list per column with notable outliers, if any.
## Recommendations
Short list of suggested next steps.
Keep the response under 400 words."#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nAnomaly results:\n{anomalies}",
        schema = schema_summary(ctx),
        anomalies = anomaly_json,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 5 — Smart grouping / aggregation
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM for a SQL GROUP BY query matching the user request.
///
/// Expected JSON response:
/// `{"sql": "SELECT ... GROUP BY ...", "title": "...", "description": "..."}`.
pub fn smart_group_prompt(ctx: &SchemaContext, user_query: &str) -> (String, String) {
    let system = r#"Translate a natural-language aggregation request into a SQLite GROUP BY query.
Return ONLY valid JSON (no markdown fences):
{"sql": "<full SELECT ... GROUP BY ... query>", "title": "<short title>", "description": "<one sentence>"}
Rules:
- The query must start with SELECT and be valid SQLite.
- Use only columns from the schema.
- Include ORDER BY where it makes the result more readable."#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nRequest: {query}",
        schema = schema_summary(ctx),
        query = user_query,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 6 — Data quality audit
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to write a quality audit report from pre-computed issues.
pub fn quality_audit_prompt(ctx: &SchemaContext, issues_json: &str) -> (String, String) {
    let system = r#"You are a data quality auditor. Interpret the provided quality issues and write a Markdown report.
Sections: ## Executive Summary, ## Issue Breakdown (table), ## Severity Assessment, ## Remediation Suggestions.
Be specific and reference column names and counts. Keep it under 500 words."#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nQuality issues:\n{issues}",
        schema = schema_summary(ctx),
        issues = issues_json,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 7 — Conversational chat
// ---------------------------------------------------------------------------

/// System prompt for open-ended chat about a dataset.
///
/// Two hard rules at the top — both have caused production bugs when
/// removed:
///  1. Visualisation requests MUST go through the `make_chart` tool.
///     Without this the model writes matplotlib/plotly code blocks
///     instead of producing a real chart.
///  2. Never invent values — only reference data through SQL the user
///     can audit (or the tool, which runs SQL deterministically).
pub fn chat_system_prompt(ctx: &SchemaContext) -> String {
    format!(
        r#"You are an expert data analyst helping a user explore a CSV dataset open in this app.

Tool use:
- A `make_chart` tool is available to you. ANY time the user asks for a
  chart, plot, graph, visualisation, distribution, breakdown, or "show
  me", you MUST call `make_chart` — never write Python, matplotlib,
  plotly, vega, or any other code-block that draws a chart. The app
  renders the chart from the tool's structured output; if you write
  code instead, the user sees nothing.
- Pick the chart_type that fits the question (bar/pie/donut for
  categorical share, line/area for ordered series, scatter for x↔y,
  histogram for one-column distributions, stacked_bar/grouped_bar
  when there's a second grouping dimension). Always supply a clear
  title.
- After the tool returns, write a one- or two-sentence narrative that
  highlights what the chart shows. Do NOT restate the numbers — the
  chart already shows them.

Answering data questions without a chart:
- Use only the column names listed in the schema below.
- When you write SQL, wrap it in a ```sql code block. Don't invent
  values; describe what the SQL would return rather than guessing.

Be concise. Skip filler.

Schema:
{schema}"#,
        schema = schema_summary(ctx),
    )
}

// ---------------------------------------------------------------------------
// Feature 8 — Report builder
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to generate a structured Markdown report.
///
/// Expected JSON response: `{"title": "...", "markdown": "..."}`.
pub fn report_builder_prompt(ctx: &SchemaContext, request: &str) -> (String, String) {
    let system = r#"Generate a business-style Markdown data report from a CSV dataset description and user request.
Return ONLY valid JSON (no markdown fences):
{"title": "<report title>", "markdown": "<full markdown report>"}
The markdown should be well-structured with headings, bullet points, and clear insights.
Reference the schema columns by name. Keep under 800 words."#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nReport request: {request}",
        schema = schema_summary(ctx),
        request = request,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 9 — Join suggestion
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to suggest a join between two tables.
///
/// Expected JSON response:
/// `{"left_key": "...", "right_key": "...", "join_type": "INNER|LEFT|RIGHT", "rationale": "..."}`.
pub fn join_suggestion_prompt(left: &SchemaContext, right: &SchemaContext) -> (String, String) {
    let system = r#"Suggest the best join strategy between two CSV datasets.
Return ONLY valid JSON (no markdown fences):
{"left_key": "<column>", "right_key": "<column>", "join_type": "INNER", "rationale": "<one sentence>"}
join_type must be one of: INNER, LEFT, RIGHT, FULL.
Choose columns that are likely foreign-key / primary-key relationships based on name and sample values."#
        .to_string();

    let user = format!(
        "Left table:\n{left}\n\nRight table:\n{right}",
        left = schema_summary(left),
        right = schema_summary(right),
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 10 — Compliance scan
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to write a compliance/PII report from pre-scanned results.
pub fn compliance_prompt(ctx: &SchemaContext, pii_json: &str) -> (String, String) {
    let system = r#"You are a data privacy compliance analyst. Interpret PII scan results and write a Markdown compliance report.
Sections: ## PII Summary, ## Detected PII Columns (with type and risk level), ## GDPR / CCPA Considerations, ## Recommended Actions.
Be practical and specific. Keep under 500 words."#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nPII scan results:\n{pii}",
        schema = schema_summary(ctx),
        pii = pii_json,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Feature 11 — Forecast / trend analysis
// ---------------------------------------------------------------------------

/// Prompt that asks the LLM to interpret regression/forecast results.
pub fn forecast_prompt(
    ctx: &SchemaContext,
    x_col: &str,
    y_col: &str,
    regression_json: &str,
) -> (String, String) {
    let system = r#"You are a data scientist. Interpret a linear regression result and write a brief Markdown forecast report.
Sections: ## Trend Summary, ## Model Quality, ## Forecast Interpretation, ## Caveats.
Explain findings in plain language. Include the regression equation. Keep under 350 words."#
        .to_string();

    let user = format!(
        "Schema:\n{schema}\n\nX column: {x}\nY column: {y}\n\nRegression results:\n{regression}",
        schema = schema_summary(ctx),
        x = x_col,
        y = y_col,
        regression = regression_json,
    );

    (system, user)
}
