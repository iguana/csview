import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { Anomaly, AnomalyReport } from "../lib/types-ai";
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

function zSeverity(z: number): "high" | "medium" | "low" {
  if (Math.abs(z) >= 3) return "high";
  if (Math.abs(z) >= 2) return "medium";
  return "low";
}

export interface AnomalyPanelProps {
  fileId: string | null;
  columns: string[];
  onProcessing: (loading: boolean) => void;
  /** Called when user clicks a row — jumps the grid to that row */
  onJumpToRow?: (row: number) => void;
}

export function AnomalyPanel({ fileId, columns, onProcessing, onJumpToRow }: AnomalyPanelProps) {
  const [selectedCols, setSelectedCols] = useState<Set<string>>(new Set());
  const [report, setReport] = useState<AnomalyReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleColumn = useCallback((col: string) => {
    setSelectedCols((prev) => {
      const next = new Set(prev);
      if (next.has(col)) next.delete(col);
      else next.add(col);
      return next;
    });
  }, []);

  const handleDetect = useCallback(async () => {
    if (!fileId) return;
    const cols = selectedCols.size > 0 ? [...selectedCols] : columns;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const r = await aiApi.detectAnomalies(fileId, cols);
      setReport(r);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [fileId, selectedCols, columns, onProcessing]);

  const rowKey = (a: Anomaly) => `${a.row}-${a.column}`;

  return (
    <div className="ai-panel anomaly-panel">
      <div className="ai-panel-header">
        <h3>Anomaly Detection</h3>
        <p className="ai-panel-sub">
          Detect statistical outliers using z-score analysis across numeric columns.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to detect anomalies.</div>
      ) : (
        <>
          {columns.length > 0 && (
            <div className="anomaly-col-select">
              <div className="anomaly-col-label">
                Columns to scan{" "}
                <span className="anomaly-col-hint">
                  (leave blank to scan all)
                </span>
              </div>
              <div className="anomaly-col-list">
                {columns.map((col) => (
                  <label key={col} className="anomaly-col-item">
                    <input
                      type="checkbox"
                      checked={selectedCols.has(col)}
                      onChange={() => toggleColumn(col)}
                    />
                    {col}
                  </label>
                ))}
              </div>
            </div>
          )}

          <button
            className="primary"
            onClick={() => void handleDetect()}
            disabled={loading}
          >
            {loading ? "Scanning…" : "Detect Anomalies"}
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
            <div className="anomaly-results">
              <div className="anomaly-count">
                Found {report.anomalies.length} anomal{report.anomalies.length === 1 ? "y" : "ies"}
              </div>

              {report.anomalies.length > 0 ? (
                <div className="issue-table-wrap">
                  <table className="issue-table">
                    <thead>
                      <tr>
                        <th>Row</th>
                        <th>Column</th>
                        <th>Value</th>
                        <th>Z-score</th>
                        <th>Reason</th>
                      </tr>
                    </thead>
                    <tbody>
                      {report.anomalies.map((a) => (
                        <tr
                          key={rowKey(a)}
                          className={`anomaly-row severity-${zSeverity(a.z_score)} ${onJumpToRow ? "clickable" : ""}`}
                          onClick={() => onJumpToRow?.(a.row)}
                          title={onJumpToRow ? "Click to jump to this row" : undefined}
                        >
                          <td className="anomaly-row-num">{a.row + 1}</td>
                          <td>{a.column}</td>
                          <td className="anomaly-value">{a.value}</td>
                          <td className="anomaly-z">
                            <span className={`issue-badge severity-${zSeverity(a.z_score)}`}>
                              {a.z_score.toFixed(2)}
                            </span>
                          </td>
                          <td className="anomaly-reason">{a.reason}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="ai-empty-state">No anomalies detected.</div>
              )}

              {report.narrative && (
                <div className="anomaly-narrative">
                  <SimpleMarkdown content={report.narrative} />
                </div>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
