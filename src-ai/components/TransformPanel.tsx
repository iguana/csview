import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { TransformResult } from "../lib/types-ai";

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}

export interface TransformPanelProps {
  fileId: string | null;
  onProcessing: (loading: boolean) => void;
  /** Called when the user clicks "Apply" so the caller can add the column */
  onApply?: (result: TransformResult, columnName: string) => void;
}

export function TransformPanel({ fileId, onProcessing, onApply }: TransformPanelProps) {
  const [description, setDescription] = useState("");
  const [result, setResult] = useState<TransformResult | null>(null);
  const [columnName, setColumnName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleGenerate = useCallback(async () => {
    const q = description.trim();
    if (!q || !fileId) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const r = await aiApi.nlTransform(fileId, q);
      setResult(r);
      setColumnName(r.column_name);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [description, fileId, onProcessing]);

  const handleApply = useCallback(() => {
    if (!result || !columnName.trim()) return;
    onApply?.(result, columnName.trim());
  }, [result, columnName, onApply]);

  const PREVIEW_LIMIT = 10;

  return (
    <div className="ai-panel transform-panel">
      <div className="ai-panel-header">
        <h3>Column Transform</h3>
        <p className="ai-panel-sub">
          Describe a new derived column in plain English and preview the result.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to create transforms.</div>
      ) : (
        <>
          <div className="ai-input-row">
            <input
              type="text"
              className="ai-text-input"
              placeholder='e.g. "Extract the year from the date column" or "Combine first and last name"'
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleGenerate();
              }}
              disabled={loading}
              aria-label="Transform description"
            />
            <button
              className="primary"
              onClick={() => void handleGenerate()}
              disabled={loading || !description.trim()}
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

          {result && (
            <div className="transform-result">
              <div className="transform-expression-wrap">
                <div className="transform-expression-label">SQL Expression</div>
                <pre className="transform-expression"><code>{result.expression}</code></pre>
              </div>

              <div className="transform-col-name">
                <label>
                  Column name
                  <input
                    type="text"
                    value={columnName}
                    onChange={(e) => setColumnName(e.target.value)}
                    className="transform-col-input"
                    aria-label="New column name"
                  />
                </label>
              </div>

              <div className="transform-preview-wrap">
                <div className="transform-preview-label">
                  Preview (first {Math.min(result.preview.length, PREVIEW_LIMIT)} values)
                </div>
                <div className="transform-preview">
                  {result.preview.slice(0, PREVIEW_LIMIT).map((val, i) => (
                    <div key={i} className="transform-preview-row">
                      <span className="transform-preview-idx">{i + 1}</span>
                      <span className="transform-preview-val">{val}</span>
                    </div>
                  ))}
                </div>
              </div>

              {onApply && (
                <button
                  className="primary"
                  onClick={handleApply}
                  disabled={!columnName.trim()}
                  title="Add this column to the dataset"
                >
                  Apply — Add Column
                </button>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
