import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { ForecastReport } from "../lib/types-ai";
import { SimpleMarkdown } from "./SimpleMarkdown";

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}


export interface ForecastPanelProps {
  fileId: string | null;
  columns: string[];
  onProcessing: (loading: boolean) => void;
}

export function ForecastPanel({ fileId, columns, onProcessing }: ForecastPanelProps) {
  const [xCol, setXCol] = useState("");
  const [yCol, setYCol] = useState("");
  const [report, setReport] = useState<ForecastReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleForecast = useCallback(async () => {
    if (!fileId || !xCol || !yCol) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const r = await aiApi.forecast(fileId, xCol, yCol);
      setReport(r);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [fileId, xCol, yCol, onProcessing]);

  const r2Display = report
    ? `${(report.rSquared * 100).toFixed(1)}%`
    : null;
  const r2Quality = report
    ? report.rSquared >= 0.8
      ? "good"
      : report.rSquared >= 0.5
        ? "moderate"
        : "poor"
    : null;

  return (
    <div className="ai-panel forecast-panel">
      <div className="ai-panel-header">
        <h3>Forecast</h3>
        <p className="ai-panel-sub">
          Linear regression between two numeric columns with AI narrative.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to run a forecast.</div>
      ) : (
        <>
          <div className="forecast-cols">
            <label className="forecast-col-label">
              X axis (independent)
              <select
                value={xCol}
                onChange={(e) => setXCol(e.target.value)}
                className="forecast-select"
                aria-label="X column"
              >
                <option value="">Select column…</option>
                {columns.map((c) => (
                  <option key={c} value={c}>{c}</option>
                ))}
              </select>
            </label>
            <label className="forecast-col-label">
              Y axis (dependent)
              <select
                value={yCol}
                onChange={(e) => setYCol(e.target.value)}
                className="forecast-select"
                aria-label="Y column"
              >
                <option value="">Select column…</option>
                {columns.map((c) => (
                  <option key={c} value={c}>{c}</option>
                ))}
              </select>
            </label>
          </div>

          <button
            className="primary"
            onClick={() => void handleForecast()}
            disabled={loading || !xCol || !yCol || xCol === yCol}
          >
            {loading ? "Forecasting…" : "Run Forecast"}
          </button>

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

          {report && (
            <div className="forecast-results">
              <div className="forecast-stats">
                <div className="forecast-stat">
                  <span className="forecast-stat-label">Slope</span>
                  <span className="forecast-stat-value">{report.slope.toFixed(4)}</span>
                </div>
                <div className="forecast-stat">
                  <span className="forecast-stat-label">Intercept</span>
                  <span className="forecast-stat-value">{report.intercept.toFixed(4)}</span>
                </div>
                <div className="forecast-stat">
                  <span className="forecast-stat-label">R²</span>
                  <span className={`forecast-stat-value r2-${r2Quality}`}>
                    {r2Display}
                  </span>
                </div>
              </div>

              {report.markdown && (
                <div className="forecast-narrative">
                  <SimpleMarkdown content={report.markdown} />
                </div>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
