//! Chart spec → SQL → executed chart data.
//!
//! The LLM picks the *shape* of a chart (which kind, which columns,
//! which aggregation) via a tool call. The actual numbers are produced
//! here, deterministically, from the open SQLite store. No model-generated
//! values reach the rendered chart.
//!
//! # Supported chart types
//!
//! | Type             | What it shows                                        | Required fields                       |
//! |------------------|------------------------------------------------------|---------------------------------------|
//! | `bar`            | Single series of categorical → numeric               | x, y or aggregation                   |
//! | `horizontal_bar` | Same as `bar` but rotated (long category labels)     | x, y or aggregation                   |
//! | `stacked_bar`    | Categorical x with sub-series stacked                | x, y, group_by                        |
//! | `grouped_bar`    | Categorical x with sub-series side-by-side           | x, y, group_by                        |
//! | `line`           | Ordered x → y (single or multi-series)               | x, y, optional group_by               |
//! | `area`           | Filled `line` (good for cumulative / time series)    | x, y, optional group_by               |
//! | `pie`            | Categorical share of total                           | x, y or aggregation                   |
//! | `donut`          | Same as pie with a hole (modern look)                | x, y or aggregation                   |
//! | `scatter`        | x vs y point cloud                                   | x, y                                  |
//! | `histogram`      | Distribution of one numeric column (auto-bucketed)   | x                                     |
//! | `treemap`        | Hierarchical area-by-value                           | x, y or aggregation                   |
//!
//! # SQL safety
//!
//! All column names referenced in `ChartSpec` are quoted with double-quote
//! identifiers. We do NOT interpolate raw user-supplied SQL — the LLM only
//! picks columns that exist in the schema we hand it.

use serde::{Deserialize, Serialize};

use crate::sqlite_store::SqliteStore;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    Bar,
    HorizontalBar,
    StackedBar,
    GroupedBar,
    Line,
    Area,
    Pie,
    Donut,
    Scatter,
    Histogram,
    Treemap,
}

impl ChartKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::HorizontalBar => "horizontal_bar",
            Self::StackedBar => "stacked_bar",
            Self::GroupedBar => "grouped_bar",
            Self::Line => "line",
            Self::Area => "area",
            Self::Pie => "pie",
            Self::Donut => "donut",
            Self::Scatter => "scatter",
            Self::Histogram => "histogram",
            Self::Treemap => "treemap",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregation {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl Aggregation {
    fn sql(self, column: &str) -> String {
        match self {
            Self::Count => "COUNT(*)".to_string(),
            Self::Sum => format!("SUM({column})"),
            Self::Avg => format!("AVG({column})"),
            Self::Min => format!("MIN({column})"),
            Self::Max => format!("MAX({column})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

/// What the LLM tool call hands us. All column references are validated
/// against the schema before any SQL is executed.
///
/// `annotation` is REQUIRED so the model writes the human-readable
/// description at the same time as it picks the chart shape — there's
/// no follow-up "describe what you just did" round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSpec {
    pub chart_type: ChartKind,
    pub title: String,
    /// One- or two-sentence interpretation of what this chart shows.
    /// Rendered under the title — no separate narrative LLM call needed.
    #[serde(default)]
    pub annotation: String,
    /// X-axis column (or pie/donut category, or histogram source column).
    pub x_column: String,
    /// Y-axis column. Required unless `aggregation = Count`. Empty
    /// strings are treated the same as omitted (some models fill the
    /// field with `""` for `count` even when the schema says it's
    /// optional).
    #[serde(default, deserialize_with = "deserialize_optional_nonempty_string")]
    pub y_column: Option<String>,
    /// Optional aggregation; when set, rows are grouped by `x_column`
    /// (and `group_by` if present). When `aggregation = Count`,
    /// `y_column` is ignored.
    #[serde(default)]
    pub aggregation: Option<Aggregation>,
    /// Optional second grouping column for stacked / grouped bars and
    /// multi-series lines/areas. Empty strings → None.
    #[serde(default, deserialize_with = "deserialize_optional_nonempty_string")]
    pub group_by: Option<String>,
    /// Cap the number of resulting rows (after sort).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Sort key for the final result. Sorts by the y-axis value.
    #[serde(default)]
    pub order: Option<SortOrder>,
    /// Histogram bucket count. Defaults to 12 if omitted.
    #[serde(default)]
    pub bin_count: Option<usize>,
}

/// Treat `""` and `null` the same as a missing field. Models occasionally
/// supply empty strings for optional columns (e.g. `yColumn: ""` alongside
/// `aggregation: "count"`) — without this, validation rejects the call as
/// "unknown column: " which then leaks back to the user as a tool error.
fn deserialize_optional_nonempty_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(d)?;
    Ok(opt.filter(|s| !s.trim().is_empty()))
}

/// What the frontend renders. `data` is opaque JSON shaped to match the
/// chart kind so the React component can map it directly into recharts
/// data props.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartData {
    pub spec: ChartSpec,
    /// The SQL we ran. Surfaced so the user can audit / re-run / paste.
    pub sql: String,
    /// Either an array of `{ x, y }` for single-series charts, or an
    /// array of `{ x, [series_label]: y, ... }` for multi-series.
    pub rows: Vec<serde_json::Value>,
    /// For multi-series charts: list of distinct series labels (legend).
    /// Empty for single-series.
    pub series: Vec<String>,
    /// Echo of the x-axis column name so the chart axis can label itself.
    pub x_label: String,
    /// Best-effort label for the y-axis (e.g. "Avg salary", "Count").
    pub y_label: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ChartError {
    #[error("unknown column: {0}")]
    UnknownColumn(String),
    #[error("invalid chart spec: {0}")]
    Invalid(String),
    #[error("sql error: {0}")]
    Sql(#[from] crate::sqlite_store::SqliteError),
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Validate the spec, build SQL, execute against the open store, return
/// frontend-ready chart data.
pub fn make_chart(store: &SqliteStore, spec: ChartSpec) -> Result<ChartData, ChartError> {
    let columns: Vec<String> = store.columns().iter().map(|c| c.name.clone()).collect();
    validate_columns(&spec, &columns)?;

    if spec.chart_type == ChartKind::Histogram {
        return build_histogram(store, spec);
    }

    let (sql, x_label, y_label, series) = build_chart_sql(&spec)?;
    let result = store.query(&sql)?;
    let rows = if series.is_empty() {
        // Single series: each row is { x, y }.
        result
            .rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "x": r.first().cloned().unwrap_or(serde_json::Value::Null),
                    "y": r.get(1).cloned().unwrap_or(serde_json::Value::Null),
                })
            })
            .collect()
    } else {
        // Multi-series: pivot the wide form into one row per x with a
        // column per distinct series label.
        pivot_multi_series(&result, &series)
    };

    Ok(ChartData {
        spec,
        sql,
        rows,
        series,
        x_label,
        y_label,
    })
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_columns(spec: &ChartSpec, schema: &[String]) -> Result<(), ChartError> {
    let in_schema = |c: &str| schema.iter().any(|s| s == c);
    if !in_schema(&spec.x_column) {
        return Err(ChartError::UnknownColumn(spec.x_column.clone()));
    }
    if let Some(y) = &spec.y_column {
        if !in_schema(y) {
            return Err(ChartError::UnknownColumn(y.clone()));
        }
    }
    if let Some(g) = &spec.group_by {
        if !in_schema(g) {
            return Err(ChartError::UnknownColumn(g.clone()));
        }
    }
    // Multi-series chart requires both group_by AND aggregation+y_column.
    if matches!(
        spec.chart_type,
        ChartKind::StackedBar | ChartKind::GroupedBar
    ) {
        if spec.group_by.is_none() {
            return Err(ChartError::Invalid(
                "stacked/grouped bar requires `group_by`".into(),
            ));
        }
    }
    // Aggregation modes other than Count need a y_column.
    if let Some(agg) = spec.aggregation {
        if agg != Aggregation::Count && spec.y_column.is_none() {
            return Err(ChartError::Invalid(format!(
                "aggregation `{agg:?}` requires `y_column`"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SQL builders (excluding histogram, which has its own pipeline)
// ---------------------------------------------------------------------------

/// Returns `(sql, x_label, y_label, series_labels)`.
fn build_chart_sql(spec: &ChartSpec) -> Result<(String, String, String, Vec<String>), ChartError> {
    let x = quote_ident(&spec.x_column);
    let y_expr = match (spec.aggregation, &spec.y_column) {
        (Some(Aggregation::Count), _) => "COUNT(*)".to_string(),
        (Some(agg), Some(y)) => agg.sql(&quote_ident(y)),
        (Some(_), None) => {
            return Err(ChartError::Invalid(
                "aggregation requires y_column (except COUNT)".into(),
            ))
        }
        (None, Some(y)) => quote_ident(y),
        (None, None) => {
            return Err(ChartError::Invalid(
                "either y_column or aggregation must be provided".into(),
            ))
        }
    };
    let y_label = match (spec.aggregation, &spec.y_column) {
        (Some(Aggregation::Count), _) => "Count".to_string(),
        (Some(agg), Some(y)) => format!("{:?} {}", agg, y),
        (None, Some(y)) => y.clone(),
        _ => "value".into(),
    };

    let multi = spec.group_by.is_some();
    let group_clause = if let Some(g) = &spec.group_by {
        let g_quoted = quote_ident(g);
        format!(" GROUP BY {x}, {g_quoted}")
    } else if spec.aggregation.is_some() {
        format!(" GROUP BY {x}")
    } else {
        String::new()
    };

    let select = if let Some(g) = &spec.group_by {
        let g_quoted = quote_ident(g);
        format!("SELECT {x} AS x, {g_quoted} AS series, {y_expr} AS y")
    } else {
        format!("SELECT {x} AS x, {y_expr} AS y")
    };

    let order_clause = match spec.order {
        Some(SortOrder::Asc) => " ORDER BY y ASC".to_string(),
        Some(SortOrder::Desc) => " ORDER BY y DESC".to_string(),
        None if matches!(
            spec.chart_type,
            ChartKind::Line | ChartKind::Area | ChartKind::Scatter
        ) =>
        {
            // Time-series-ish chart types: keep natural x order.
            " ORDER BY x ASC".to_string()
        }
        None => String::new(),
    };
    let limit_clause = spec
        .limit
        .map(|n| format!(" LIMIT {n}"))
        .unwrap_or_default();

    let sql = format!(
        "{select} FROM data{group_clause}{order_clause}{limit_clause}"
    );

    let series_labels = if multi { vec![] } else { vec![] }; // populated post-query
    Ok((sql, spec.x_column.clone(), y_label, series_labels))
}

fn pivot_multi_series(
    result: &crate::sqlite_store::QueryResult,
    _series: &[String],
) -> Vec<serde_json::Value> {
    // The query is `SELECT x, series, y` — pivot into one row per x.
    use std::collections::BTreeMap;
    let mut by_x: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for row in &result.rows {
        let x = row
            .first()
            .map(|v| value_to_str(v))
            .unwrap_or_default();
        let series = row.get(1).map(|v| value_to_str(v)).unwrap_or_default();
        let y = row.get(2).cloned().unwrap_or(serde_json::Value::Null);
        by_x.entry(x).or_default().insert(series, y);
    }
    by_x
        .into_iter()
        .map(|(x, by_series)| {
            let mut obj = serde_json::Map::new();
            obj.insert("x".into(), serde_json::Value::String(x));
            for (series, y) in by_series {
                obj.insert(series, y);
            }
            serde_json::Value::Object(obj)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

fn build_histogram(store: &SqliteStore, spec: ChartSpec) -> Result<ChartData, ChartError> {
    let bins = spec.bin_count.unwrap_or(12).max(2);
    let x_quoted = quote_ident(&spec.x_column);
    // Pull min/max in one pass to compute bin width.
    let bounds_sql = format!(
        "SELECT MIN({x_quoted}) AS lo, MAX({x_quoted}) AS hi FROM data \
         WHERE {x_quoted} IS NOT NULL"
    );
    let bounds = store.query(&bounds_sql)?;
    let (lo, hi) = bounds
        .rows
        .first()
        .map(|r| {
            (
                r.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
                r.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
            )
        })
        .unwrap_or((0.0, 0.0));
    if (hi - lo).abs() < f64::EPSILON {
        // Degenerate column (all same value) — return one full bucket.
        return Ok(ChartData {
            spec: spec.clone(),
            sql: bounds_sql,
            rows: vec![serde_json::json!({
                "x": format!("{lo:.2}"),
                "y": bounds.row_count as i64,
            })],
            series: vec![],
            x_label: spec.x_column.clone(),
            y_label: "Count".into(),
        });
    }
    let bin_width = (hi - lo) / bins as f64;
    // Bucket each value into a bin index using SQLite arithmetic, then
    // count. CAST guards against integer division.
    // The max value sits exactly on the upper edge and would land in
    // bin_count (one past the end); MIN() clamps it back into the last
    // visible bin so every non-null value is counted somewhere.
    let last = bins - 1;
    let sql = format!(
        "SELECT \
            MIN(\
                CAST(((CAST({x} AS REAL) - {lo}) / {w}) AS INTEGER), \
                {last}\
            ) AS bin_idx, \
            COUNT(*) AS y \
         FROM data \
         WHERE {x} IS NOT NULL \
         GROUP BY bin_idx \
         ORDER BY bin_idx ASC",
        x = x_quoted,
        lo = lo,
        w = bin_width,
        last = last,
    );
    let result = store.query(&sql)?;
    let rows: Vec<serde_json::Value> = (0..bins)
        .map(|i| {
            let bin_lo = lo + i as f64 * bin_width;
            let bin_hi = bin_lo + bin_width;
            let count = result
                .rows
                .iter()
                .find(|r| r.first().and_then(|v| v.as_i64()) == Some(i as i64))
                .and_then(|r| r.get(1).and_then(|v| v.as_i64()))
                .unwrap_or(0);
            serde_json::json!({
                "x": format!("{:.1}–{:.1}", bin_lo, bin_hi),
                "y": count,
            })
        })
        .collect();
    Ok(ChartData {
        spec,
        sql,
        rows,
        series: vec![],
        x_label: format!("Bins of equal width over the range of the column"),
        y_label: "Count".into(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Quote an identifier for safe embedding in SQL. Mirrors the helper in
/// sqlite_store but kept local to avoid a cross-module pub.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn value_to_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{ColumnKind, ColumnMeta};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn employees_store() -> SqliteStore {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(
            b"id,first_name,department,salary\n\
              1,Alice,Engineering,180000\n\
              2,Bob,Engineering,150000\n\
              3,Chiara,Design,140000\n\
              4,Darius,Data,160000\n\
              5,Elena,Engineering,210000\n\
              6,Farrukh,Product,155000\n",
        )
        .unwrap();
        let headers = vec![
            "id".to_string(),
            "first_name".into(),
            "department".into(),
            "salary".into(),
        ];
        let cols = vec![
            ColumnMeta { index: 0, name: "id".into(), kind: ColumnKind::Integer },
            ColumnMeta { index: 1, name: "first_name".into(), kind: ColumnKind::String },
            ColumnMeta { index: 2, name: "department".into(), kind: ColumnKind::String },
            ColumnMeta { index: 3, name: "salary".into(), kind: ColumnKind::Integer },
        ];
        SqliteStore::from_csv(f.path().to_str().unwrap(), b',', true, &headers, &cols).unwrap()
    }

    fn spec(chart_type: ChartKind, x: &str) -> ChartSpec {
        ChartSpec {
            chart_type,
            title: "test".into(),
            annotation: "test annotation".into(),
            x_column: x.into(),
            y_column: None,
            aggregation: None,
            group_by: None,
            limit: None,
            order: None,
            bin_count: None,
        }
    }

    #[test]
    fn bar_chart_avg_salary_by_department() {
        let store = employees_store();
        let spec = ChartSpec {
            chart_type: ChartKind::Bar,
            title: "Avg salary by dept".into(),
            annotation: "Engineering pays the most on average.".into(),
            x_column: "department".into(),
            y_column: Some("salary".into()),
            aggregation: Some(Aggregation::Avg),
            group_by: None,
            limit: None,
            order: Some(SortOrder::Desc),
            bin_count: None,
        };
        let chart = make_chart(&store, spec).unwrap();
        assert!(chart.sql.contains("AVG"));
        assert!(chart.sql.contains("GROUP BY"));
        assert_eq!(chart.rows.len(), 4); // 4 distinct departments
        // Top entry should be Engineering (highest avg).
        let top = &chart.rows[0];
        assert_eq!(top["x"], "Engineering");
    }

    #[test]
    fn pie_chart_count_by_department() {
        let store = employees_store();
        let spec = ChartSpec {
            chart_type: ChartKind::Pie,
            title: "Headcount".into(),
            annotation: "Engineering has the largest headcount.".into(),
            x_column: "department".into(),
            y_column: None,
            aggregation: Some(Aggregation::Count),
            group_by: None,
            limit: None,
            order: None,
            bin_count: None,
        };
        let chart = make_chart(&store, spec).unwrap();
        assert!(chart.sql.contains("COUNT(*)"));
        // 4 distinct departments — Engineering count should be 3
        let eng = chart
            .rows
            .iter()
            .find(|r| r["x"] == "Engineering")
            .expect("Engineering row");
        assert_eq!(eng["y"].as_i64(), Some(3));
    }

    #[test]
    fn stacked_bar_requires_group_by() {
        let store = employees_store();
        let mut s = spec(ChartKind::StackedBar, "department");
        s.y_column = Some("salary".into());
        s.aggregation = Some(Aggregation::Sum);
        // group_by intentionally omitted.
        let spec = s;
        assert!(matches!(
            make_chart(&store, spec),
            Err(ChartError::Invalid(_))
        ));
    }

    #[test]
    fn unknown_column_rejected_before_sql_runs() {
        let store = employees_store();
        let mut s = spec(ChartKind::Bar, "title"); // 'title' not in schema
        s.y_column = Some("salary".into());
        s.aggregation = Some(Aggregation::Avg);
        let spec = s;
        assert!(matches!(
            make_chart(&store, spec),
            Err(ChartError::UnknownColumn(_))
        ));
    }

    #[test]
    fn histogram_buckets_into_n_bins() {
        let store = employees_store();
        let mut s = spec(ChartKind::Histogram, "salary");
        s.bin_count = Some(5);
        let spec = s;
        let chart = make_chart(&store, spec).unwrap();
        assert_eq!(chart.rows.len(), 5);
        let total: i64 = chart
            .rows
            .iter()
            .map(|r| r["y"].as_i64().unwrap_or(0))
            .sum();
        assert_eq!(total, 6); // all 6 rows landed in some bin
    }

    /// Regression for the production "(unknown column: )" loop:
    /// some models emit `yColumn: ""` alongside `aggregation: count`.
    /// The custom deserializer should fold "" → None so the call works.
    #[test]
    fn empty_string_optional_columns_are_treated_as_none() {
        let store = employees_store();
        let json = serde_json::json!({
            "chartType": "pie",
            "title": "Headcount by dept",
            "annotation": "Engineering dominates.",
            "xColumn": "department",
            "yColumn": "",       // ← was tripping validation as 'unknown column: '
            "aggregation": "count",
            "groupBy": "",       // also previously tripped
        });
        let spec: ChartSpec = serde_json::from_value(json).expect("deserialize");
        assert_eq!(spec.y_column, None);
        assert_eq!(spec.group_by, None);
        let chart = make_chart(&store, spec).expect("chart should render");
        assert_eq!(chart.rows.len(), 4);
    }

    /// Annotation field is part of ChartSpec and round-trips through JSON.
    #[test]
    fn annotation_is_preserved_through_serde() {
        let json = serde_json::json!({
            "chartType": "bar",
            "title": "x",
            "annotation": "Engineering pays the most.",
            "xColumn": "department",
            "yColumn": "salary",
            "aggregation": "avg",
        });
        let spec: ChartSpec = serde_json::from_value(json).unwrap();
        assert_eq!(spec.annotation, "Engineering pays the most.");
    }

    #[test]
    fn line_chart_keeps_natural_x_order_when_unsorted() {
        let store = employees_store();
        let mut s = spec(ChartKind::Line, "id");
        s.y_column = Some("salary".into());
        let spec = s;
        let chart = make_chart(&store, spec).unwrap();
        assert!(chart.sql.contains("ORDER BY x ASC"));
        assert_eq!(chart.rows.len(), 6);
        // First row should be the lowest id.
        assert_eq!(chart.rows[0]["x"].as_i64(), Some(1));
    }
}
