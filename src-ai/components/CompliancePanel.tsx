import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { ComplianceIssue, ComplianceReport } from "../lib/types-ai";
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

const PII_SEVERITY: Record<string, "high" | "medium" | "low"> = {
  ssn: "high",
  credit_card: "high",
  passport: "high",
  email: "medium",
  phone: "medium",
  name: "low",
  address: "medium",
  date_of_birth: "high",
  ip_address: "low",
};

function piiSeverity(piiType: string): "high" | "medium" | "low" {
  const key = piiType.toLowerCase().replace(/\s+/g, "_");
  return PII_SEVERITY[key] ?? "medium";
}

function ComplianceIssueCard({ issue }: { issue: ComplianceIssue }) {
  const sev = piiSeverity(issue.pii_type);
  const [expanded, setExpanded] = useState(false);

  return (
    <div className={`compliance-issue-card severity-${sev}`}>
      <div className="compliance-issue-header" onClick={() => setExpanded((v) => !v)}>
        <span className={`issue-badge severity-${sev}`}>{issue.pii_type}</span>
        <span className="compliance-col">{issue.column}</span>
        <span className="compliance-count">{issue.count} occurrence{issue.count === 1 ? "" : "s"}</span>
        <span className="compliance-expand">{expanded ? "▲" : "▼"}</span>
      </div>
      {expanded && (
        <div className="compliance-samples">
          <div className="compliance-samples-label">Sample values</div>
          <div className="compliance-samples-list">
            {issue.sample_values.map((v, i) => (
              <code key={i} className="compliance-sample">{v}</code>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export interface CompliancePanelProps {
  fileId: string | null;
  onProcessing: (loading: boolean) => void;
}

export function CompliancePanel({ fileId, onProcessing }: CompliancePanelProps) {
  const [report, setReport] = useState<ComplianceReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleScan = useCallback(async () => {
    if (!fileId) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const r = await aiApi.complianceScan(fileId);
      setReport(r);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [fileId, onProcessing]);

  // Group issues by PII type
  const grouped = report
    ? report.issues.reduce<Record<string, ComplianceIssue[]>>((acc, issue) => {
        const key = issue.pii_type;
        if (!acc[key]) acc[key] = [];
        acc[key].push(issue);
        return acc;
      }, {})
    : {};

  return (
    <div className="ai-panel compliance-panel">
      <div className="ai-panel-header">
        <h3>Compliance / PII Scan</h3>
        <p className="ai-panel-sub">
          Detect personally identifiable information (PII) — emails, SSNs, phone numbers, and more.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to run a compliance scan.</div>
      ) : (
        <>
          <button
            className="primary"
            onClick={() => void handleScan()}
            disabled={loading}
          >
            {loading ? "Scanning…" : report ? "Re-scan" : "Scan for PII"}
          </button>

          {loading && (
            <div className="ai-processing-hint">
              Scanning columns for PII patterns…
            </div>
          )}

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
            <div className="compliance-results">
              {report.issues.length === 0 ? (
                <div className="ai-empty-state">
                  No PII detected in this dataset.
                </div>
              ) : (
                <>
                  <div className="compliance-summary">
                    {report.issues.length} PII field{report.issues.length === 1 ? "" : "s"} found
                    across {Object.keys(grouped).length} type{Object.keys(grouped).length === 1 ? "" : "s"}
                  </div>
                  <div className="compliance-issues">
                    {report.issues.map((issue, i) => (
                      <ComplianceIssueCard key={i} issue={issue} />
                    ))}
                  </div>
                </>
              )}

              {report.narrative && (
                <div className="compliance-narrative">
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
