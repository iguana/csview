import { useState, useCallback } from "react";
import { aiApi } from "../lib/api-ai";
import type { AccountStatus } from "../lib/types-ai";

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}

export interface AccountPanelProps {
  onStatusChange?: (status: AccountStatus) => void;
}

export function AccountPanel({ onStatusChange }: AccountPanelProps) {
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [status, setStatus] = useState<AccountStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const handleSave = useCallback(async () => {
    if (!apiKey.trim()) return;
    setError(null);
    setSaving(true);
    setSaved(false);
    try {
      await aiApi.setApiKey(apiKey.trim());
      const s = await aiApi.getAccountStatus();
      setStatus(s);
      setSaved(true);
      onStatusChange?.(s);
      setApiKey("");
      setTimeout(() => setSaved(false), 3000);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setSaving(false);
    }
  }, [apiKey, onStatusChange]);

  const handleTest = useCallback(async () => {
    setError(null);
    setTesting(true);
    try {
      const s = await aiApi.getAccountStatus();
      setStatus(s);
      onStatusChange?.(s);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setTesting(false);
    }
  }, [onStatusChange]);

  return (
    <div className="ai-panel account-panel">
      <div className="ai-panel-header">
        <h3>AI Settings</h3>
        <p className="ai-panel-sub">Configure your API key to enable all AI features.</p>
      </div>

      {status && (
        <div className={`connection-status ${status.has_key ? "connected" : "disconnected"}`}>
          <span className="status-dot" />
          <span>
            {status.has_key
              ? `Connected · ${status.model}`
              : "No API key configured"}
          </span>
        </div>
      )}

      <div className="account-form">
        <label className="account-label">
          Anthropic API Key
          <input
            type="password"
            className="account-input"
            placeholder="sk-ant-…"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleSave();
            }}
            autoComplete="off"
            aria-label="Anthropic API Key"
          />
        </label>

        <p className="account-hint">
          Get your key at{" "}
          <span className="account-link">console.anthropic.com</span>. The key is
          stored securely in the system keychain and never transmitted except to
          Anthropic's API.
        </p>

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

        {saved && (
          <div className="ai-success-banner">
            API key saved successfully.
          </div>
        )}

        <div className="account-actions">
          <button
            className="primary"
            onClick={() => void handleSave()}
            disabled={saving || !apiKey.trim()}
          >
            {saving ? "Saving…" : "Save Key"}
          </button>
          <button
            onClick={() => void handleTest()}
            disabled={testing}
          >
            {testing ? "Testing…" : "Test Connection"}
          </button>
        </div>
      </div>
    </div>
  );
}
