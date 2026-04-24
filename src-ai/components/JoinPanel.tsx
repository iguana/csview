import { useState, useCallback } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { aiApi, csvApi } from "../lib/api-ai";
import type { FileInfo, JoinSuggestion } from "../lib/types-ai";

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}

const JOIN_TYPES = ["inner", "left", "right", "full"] as const;
type JoinType = (typeof JOIN_TYPES)[number];

export interface JoinPanelProps {
  fileId: string | null;
  onProcessing: (loading: boolean) => void;
  /** Called after a successful join with the new file info */
  onJoinComplete?: (result: FileInfo) => void;
}

export function JoinPanel({ fileId, onProcessing, onJoinComplete }: JoinPanelProps) {
  const [rightPath, setRightPath] = useState<string | null>(null);
  const [rightFile, setRightFile] = useState<FileInfo | null>(null);
  const [suggestion, setSuggestion] = useState<JoinSuggestion | null>(null);
  const [leftKey, setLeftKey] = useState("");
  const [rightKey, setRightKey] = useState("");
  const [joinType, setJoinType] = useState<JoinType>("inner");
  const [joinResult, setJoinResult] = useState<FileInfo | null>(null);
  const [suggesting, setSuggesting] = useState(false);
  const [joining, setJoining] = useState(false);
  const [loadingRight, setLoadingRight] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handlePickFile = useCallback(async () => {
    try {
      const selection = await openDialog({
        multiple: false,
        filters: [
          { name: "CSV / TSV", extensions: ["csv", "tsv", "txt"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof selection !== "string") return;
      setRightPath(selection);
      setSuggestion(null);
      setJoinResult(null);
      setError(null);
      setLoadingRight(true);
      onProcessing(true);
      try {
        const info = await csvApi.openCsv(selection);
        setRightFile(info);
      } catch (e) {
        setError(errMsg(e));
        setRightFile(null);
      } finally {
        setLoadingRight(false);
        onProcessing(false);
      }
    } catch (e) {
      setError(errMsg(e));
    }
  }, [onProcessing]);

  const handleSuggest = useCallback(async () => {
    if (!fileId || !rightFile) return;
    setError(null);
    setSuggesting(true);
    onProcessing(true);
    try {
      const s = await aiApi.suggestJoin(fileId, rightFile.fileId);
      setSuggestion(s);
      setLeftKey(s.leftKey);
      setRightKey(s.rightKey);
      setJoinType(s.joinType as JoinType);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setSuggesting(false);
      onProcessing(false);
    }
  }, [fileId, rightFile, onProcessing]);

  const handleJoin = useCallback(async () => {
    if (!fileId || !rightFile || !leftKey || !rightKey) return;
    setError(null);
    setJoining(true);
    onProcessing(true);
    try {
      const result = await aiApi.executeJoin(
        fileId,
        rightFile.fileId,
        leftKey,
        rightKey,
        joinType,
      );
      setJoinResult(result);
      onJoinComplete?.(result);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setJoining(false);
      onProcessing(false);
    }
  }, [fileId, rightFile, leftKey, rightKey, joinType, onProcessing, onJoinComplete]);

  const leftColumns = rightFile === null ? [] : (rightFile.columns ?? []);
  const basename = (p: string) => {
    const idx = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return idx >= 0 ? p.slice(idx + 1) : p;
  };

  return (
    <div className="ai-panel join-panel">
      <div className="ai-panel-header">
        <h3>Join Assistant</h3>
        <p className="ai-panel-sub">
          Load a second CSV and let AI suggest the best join keys and strategy.
        </p>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to use the join assistant.</div>
      ) : (
        <>
          <div className="join-file-section">
            <div className="join-file-label">Second CSV file</div>
            {rightPath ? (
              <div className="join-file-chosen">
                <span className="join-file-name">{basename(rightPath)}</span>
                {rightFile && (
                  <span className="join-file-meta">
                    {rightFile.rowCount.toLocaleString()} rows · {rightFile.columns.length} cols
                  </span>
                )}
                <button onClick={() => void handlePickFile()} disabled={loadingRight}>
                  Change…
                </button>
              </div>
            ) : (
              <button onClick={() => void handlePickFile()} disabled={loadingRight}>
                {loadingRight ? "Loading…" : "Pick CSV…"}
              </button>
            )}
          </div>

          {rightFile && (
            <>
              <button
                className="primary"
                onClick={() => void handleSuggest()}
                disabled={suggesting}
              >
                {suggesting ? "Analysing…" : "Suggest Join"}
              </button>

              {suggestion && (
                <div className="join-suggestion">
                  <div className="join-suggestion-label">AI Suggestion</div>
                  <p className="join-suggestion-explanation">{suggestion.explanation}</p>
                </div>
              )}

              <div className="join-config">
                <div className="join-config-row">
                  <label className="join-config-label">
                    Left key
                    <input
                      type="text"
                      value={leftKey}
                      onChange={(e) => setLeftKey(e.target.value)}
                      placeholder="Column in left file"
                      aria-label="Left join key"
                    />
                  </label>
                  <label className="join-config-label">
                    Right key
                    <input
                      type="text"
                      value={rightKey}
                      onChange={(e) => setRightKey(e.target.value)}
                      placeholder="Column in right file"
                      list="right-cols"
                      aria-label="Right join key"
                    />
                    <datalist id="right-cols">
                      {leftColumns.map((c) => (
                        <option key={c.name} value={c.name} />
                      ))}
                    </datalist>
                  </label>
                </div>

                <div className="join-type-row">
                  <div className="join-type-label">Join type</div>
                  <div className="join-type-options">
                    {JOIN_TYPES.map((jt) => (
                      <label key={jt} className={`join-type-opt ${joinType === jt ? "active" : ""}`}>
                        <input
                          type="radio"
                          name="join-type"
                          value={jt}
                          checked={joinType === jt}
                          onChange={() => setJoinType(jt)}
                        />
                        {jt}
                      </label>
                    ))}
                  </div>
                </div>

                <button
                  className="primary"
                  onClick={() => void handleJoin()}
                  disabled={joining || !leftKey.trim() || !rightKey.trim()}
                >
                  {joining ? "Joining…" : "Execute Join"}
                </button>
              </div>

              {joinResult && (
                <div className="join-result">
                  <div className="join-result-title">Join complete</div>
                  <div className="join-result-stats">
                    <span>{joinResult.rowCount.toLocaleString()} rows</span>
                    <span>{joinResult.columns.length} columns</span>
                  </div>
                  <div className="join-result-path">{joinResult.path}</div>
                </div>
              )}
            </>
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
        </>
      )}
    </div>
  );
}
