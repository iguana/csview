import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { ProfileReport } from "../lib/types-ai";
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

export interface ProfilePanelProps {
  fileId: string | null;
  onProcessing: (loading: boolean) => void;
}

export function ProfilePanel({ fileId, onProcessing }: ProfilePanelProps) {
  const [report, setReport] = useState<ProfileReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const handleGenerate = useCallback(async () => {
    if (!fileId) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const r = await aiApi.generateProfile(fileId);
      setReport(r);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [fileId, onProcessing]);

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

  return (
    <div className="ai-panel profile-panel">
      <div className="ai-panel-header">
        <h3>Data Profile</h3>
        <p className="ai-panel-sub">
          AI-generated statistical summary with insights for each column.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to generate a profile.</div>
      ) : (
        <>
          <div className="profile-actions">
            <button
              className="primary"
              onClick={() => void handleGenerate()}
              disabled={loading}
            >
              {loading ? "Generating…" : report ? "Regenerate Profile" : "Generate Profile"}
            </button>
            {report && (
              <button onClick={() => void handleCopy()}>
                {copied ? "Copied!" : "Copy Markdown"}
              </button>
            )}
          </div>

          {loading && (
            <div className="ai-processing-hint">
              Analysing columns and writing profile… This may take a moment.
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
            <div className="profile-report">
              <SimpleMarkdown content={report.markdown} />
            </div>
          )}
        </>
      )}
    </div>
  );
}
