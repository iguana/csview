import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { Report } from "../lib/types-ai";
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

export interface ReportPanelProps {
  fileId: string | null;
  onProcessing: (loading: boolean) => void;
}

export function ReportPanel({ fileId, onProcessing }: ReportPanelProps) {
  const [request, setRequest] = useState("");
  const [report, setReport] = useState<Report | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const handleGenerate = useCallback(async () => {
    const q = request.trim();
    if (!q || !fileId) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const r = await aiApi.generateReport(fileId, q);
      setReport(r);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [request, fileId, onProcessing]);

  const handleCopy = useCallback(async () => {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(report.markdown);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  }, [report]);

  const handleExport = useCallback(() => {
    if (!report) return;
    const blob = new Blob([report.markdown], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${report.title.replace(/\s+/g, "_").toLowerCase()}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }, [report]);

  return (
    <div className="ai-panel report-panel">
      <div className="ai-panel-header">
        <h3>Report Builder</h3>
        <p className="ai-panel-sub">
          Describe the report you want and the AI will generate a structured document.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to generate a report.</div>
      ) : (
        <>
          <div className="ai-input-row">
            <input
              type="text"
              className="ai-text-input"
              placeholder='e.g. "Sales summary by region for Q4 with trends"'
              value={request}
              onChange={(e) => setRequest(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleGenerate();
              }}
              disabled={loading}
              aria-label="Report description"
            />
            <button
              className="primary"
              onClick={() => void handleGenerate()}
              disabled={loading || !request.trim()}
            >
              {loading ? "Generating…" : "Generate"}
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

          {loading && (
            <div className="ai-processing-hint">
              Writing report… This may take a moment.
            </div>
          )}

          {report && (
            <div className="report-content">
              <div className="report-header">
                <div className="report-title">{report.title}</div>
                <div className="report-meta">
                  Generated {new Date(report.generated_at).toLocaleString()}
                </div>
                <div className="report-actions">
                  <button onClick={() => void handleCopy()}>
                    {copied ? "Copied!" : "Copy Markdown"}
                  </button>
                  <button onClick={handleExport}>Export .md</button>
                </div>
              </div>
              <SimpleMarkdown content={report.markdown} />
            </div>
          )}
        </>
      )}
    </div>
  );
}
