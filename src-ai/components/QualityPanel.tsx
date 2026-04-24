import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { QualityIssueSer, QualityReport } from "../lib/types-ai";
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

function issueSeverity(type: string): "high" | "medium" | "low" {
  const high = ["missing", "duplicate", "invalid"];
  const medium = ["inconsistent", "outlier", "format"];
  if (high.some((h) => type.toLowerCase().includes(h))) return "high";
  if (medium.some((m) => type.toLowerCase().includes(m))) return "medium";
  return "low";
}

function IssueRow({ issue }: { issue: QualityIssueSer }) {
  const sev = issueSeverity(issue.issueType);
  return (
    <tr className={`quality-issue-row severity-${sev}`}>
      <td className="issue-row-num">{issue.row + 1}</td>
      <td>{issue.column}</td>
      <td>
        <span className={`issue-badge severity-${sev}`}>{issue.issueType}</span>
      </td>
      <td className="issue-value" title={issue.value}>{issue.value}</td>
      <td className="issue-suggestion">
        {issue.suggestion ?? <em className="no-suggestion">—</em>}
      </td>
    </tr>
  );
}

export interface QualityPanelProps {
  fileId: string | null;
  onProcessing: (loading: boolean) => void;
}

export function QualityPanel({ fileId, onProcessing }: QualityPanelProps) {
  const [report, setReport] = useState<QualityReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleAudit = useCallback(async () => {
    if (!fileId) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const r = await aiApi.auditQuality(fileId);
      setReport(r);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [fileId, onProcessing]);

  return (
    <div className="ai-panel quality-panel">
      <div className="ai-panel-header">
        <h3>Quality Audit</h3>
        <p className="ai-panel-sub">
          Scan for missing values, duplicates, format errors, and other data quality issues.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to run a quality audit.</div>
      ) : (
        <>
          <button
            className="primary"
            onClick={() => void handleAudit()}
            disabled={loading}
          >
            {loading ? "Auditing…" : report ? "Re-audit" : "Audit Quality"}
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

          {loading && (
            <div className="ai-processing-hint">
              Scanning data quality… This may take a moment for large files.
            </div>
          )}

          {report && (
            <div className="quality-results">
              {report.issues.length > 0 && (
                <div className="quality-summary">
                  <div className="quality-summary-title">Issue Summary</div>
                  <div className="quality-summary-grid">
                    {Object.entries(
                      report.issues.reduce<Record<string, number>>((acc, issue) => {
                        acc[issue.issueType] = (acc[issue.issueType] ?? 0) + 1;
                        return acc;
                      }, {})
                    ).map(([type, count]) => (
                      <div
                        key={type}
                        className={`quality-summary-chip severity-${issueSeverity(type)}`}
                      >
                        <span className="summary-chip-count">{count}</span>
                        <span className="summary-chip-type">{type}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {report.issues.length > 0 ? (
                <div className="issue-table-wrap">
                  <div className="issue-table-count">
                    {report.issues.length} issue{report.issues.length === 1 ? "" : "s"} found
                  </div>
                  <table className="issue-table">
                    <thead>
                      <tr>
                        <th>Row</th>
                        <th>Column</th>
                        <th>Type</th>
                        <th>Value</th>
                        <th>Suggestion</th>
                      </tr>
                    </thead>
                    <tbody>
                      {report.issues.map((issue, i) => (
                        <IssueRow key={i} issue={issue} />
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="ai-empty-state quality-clean">
                  No data quality issues detected.
                </div>
              )}

              {report.markdown && (
                <div className="quality-narrative">
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
