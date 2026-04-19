import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { NlQueryResult, QueryResult } from "../lib/types-ai";

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}

function ResultTable({ result }: { result: QueryResult }) {
  const maxRows = 50;
  const display = result.rows.slice(0, maxRows);
  return (
    <div className="query-result-wrap">
      <div className="query-result-meta">
        {result.row_count} row{result.row_count === 1 ? "" : "s"} returned
      </div>
      <div className="query-result-scroll">
        <table className="query-result-table">
          <thead>
            <tr>
              {result.columns.map((col) => (
                <th key={col}>{col}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {display.map((row, ri) => (
              <tr key={ri}>
                {row.map((cell, ci) => (
                  <td key={ci}>{cell == null ? <em className="null-cell">null</em> : String(cell)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {result.row_count > maxRows && (
          <div className="query-result-truncated">
            Showing first {maxRows} of {result.row_count} rows.
          </div>
        )}
      </div>
    </div>
  );
}

export interface QueryPanelProps {
  fileId: string | null;
  /** Called with the SQL so the caller can optionally highlight matching rows */
  onApplyFilter?: (sql: string) => void;
  onProcessing: (loading: boolean) => void;
}

export function QueryPanel({ fileId, onApplyFilter, onProcessing }: QueryPanelProps) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<NlQueryResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const handleRun = useCallback(async () => {
    const q = query.trim();
    if (!q || !fileId) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const res = await aiApi.nlQuery(fileId, q);
      setResult(res);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [query, fileId, onProcessing]);

  const handleCopySQL = useCallback(async () => {
    if (!result?.sql) return;
    try {
      await navigator.clipboard.writeText(result.sql);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard may be unavailable
    }
  }, [result]);

  return (
    <div className="ai-panel query-panel">
      <div className="ai-panel-header">
        <h3>Natural Language Query</h3>
        <p className="ai-panel-sub">Describe what you want to find and the AI will write the SQL.</p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to run queries.</div>
      ) : (
        <>
          <div className="ai-input-row">
            <input
              type="text"
              className="ai-text-input"
              placeholder='e.g. "Show me all sales over $10,000 in Q4"'
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleRun();
              }}
              disabled={loading}
              aria-label="Natural language query"
            />
            <button
              className="primary"
              onClick={() => void handleRun()}
              disabled={loading || !query.trim()}
            >
              {loading ? "Running…" : "Run"}
            </button>
          </div>

          {error && (
            <div className="ai-error-banner">
              {error}
              <button
                className="error-dismiss"
                onClick={() => setError(null)}
                aria-label="Dismiss"
              >
                ×
              </button>
            </div>
          )}

          {result && (
            <div className="query-results">
              <div className="query-explanation">{result.explanation}</div>

              <div className="query-sql-block">
                <div className="query-sql-header">
                  <span className="query-sql-label">Generated SQL</span>
                  <div className="query-sql-actions">
                    <button onClick={() => void handleCopySQL()} title="Copy SQL">
                      {copied ? "Copied!" : "Copy SQL"}
                    </button>
                    {onApplyFilter && (
                      <button
                        className="primary"
                        onClick={() => onApplyFilter(result.sql)}
                        title="Highlight matching rows in the grid"
                      >
                        Apply as filter
                      </button>
                    )}
                  </div>
                </div>
                <pre><code>{result.sql}</code></pre>
              </div>

              <ResultTable result={result.result} />
            </div>
          )}
        </>
      )}
    </div>
  );
}
