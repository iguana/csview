# csviewai Feature Roadmap

## Tier 1 — MVP (launch features)

- [ ] **1. NL Query → Filter/Expression** — User types natural language, LLM translates to structured filter AST, app evaluates deterministically. LLM only sees schema + 5 sample values.
- [ ] **2. Auto-Summary / Data Profile Report** — Local stats pipeline computes everything, LLM writes narrative. Persisted as Markdown.
- [ ] **3. Column Transform ("Derive a column")** — LLM generates regex/math/case-map expression from NL, app evaluates across all rows locally.

## Tier 2 — Core analytics

- [ ] **4. Anomaly Detection + Explanation** — Local IQR/z-score detection, LLM explains flagged rows.
- [ ] **5. Smart Grouping & Pivot Narration** — LLM parses group-by intent, app computes aggregation, LLM narrates result.
- [ ] **6. Data Quality Audit** — Local regex/pattern matching for inconsistencies, LLM writes remediation report.

## Tier 3 — Differentiating

- [ ] **7. "Ask This Data" Chat** — Conversational sidebar with routing (query/stats/reasoning), grounded answers with citations.
- [ ] **8. Report Builder** — LLM generates report spec (sections, queries, charts), app executes, exports PDF/HTML/Markdown.
- [ ] **9. Join / Merge Assistant** — LLM suggests join keys, app performs join deterministically, LLM narrates mismatches.

## Tier 4 — Specialized

- [ ] **10. Regulatory / Compliance Check** — Local PII regex detection, LLM classifies ambiguous cases.
- [ ] **11. Forecast / Trend Projection** — Local regression, LLM narrates trend + caveats.

---

## Foundation (required for all features)

- [x] Cargo workspace with shared `csview-engine` crate
- [x] Expression evaluator (FilterExpr + TransformExpr AST + deterministic evaluator) — 80 tests
- [x] Extended stats (median, IQR, z-score, percentiles, correlations, regression) — 11 tests
- [x] SQLite data store (CSV → SQLite import, query, mutations, export) — 24 tests
- [x] Join engine (inner/left/right/full joins) — 5 tests
- [x] Data quality detector (PII, case issues, type mismatches) — 9 tests
- [x] LLM client (Claude API via reqwest, prompt templates for all 11 features)
- [x] SQLite persistence (reports, chat history, account with migrations)
- [x] AI Tauri binary scaffold (`src-tauri-ai/`) — 25 Tauri commands, macOS menu with AI submenu
- [x] AI frontend scaffold (`src-ai/`) — 11 feature panels + shared component reuse
- [x] Account/API key management (AccountPanel + set_api_key/get_account_status commands)
