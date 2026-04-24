import { useCallback, useEffect, useState } from "react";
import { aiApi } from "../lib/api-ai";
import type { AccountStatus, AvailableModel } from "../lib/types-ai";

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}

const PROVIDERS = [
  { id: "openai", label: "OpenAI", placeholder: "sk-proj-...", hint: "platform.openai.com/api-keys" },
  { id: "google", label: "Google", placeholder: "AIza...", hint: "aistudio.google.com/apikey" },
  { id: "anthropic", label: "Anthropic", placeholder: "sk-ant-...", hint: "console.anthropic.com/settings/keys" },
] as const;

const TIER_LABELS: Record<string, string> = {
  reasoning: "Reasoning",
  balanced: "Balanced",
  fast: "Fast",
};

export interface AccountPanelProps {
  onStatusChange?: (status: AccountStatus) => void;
}

export function AccountPanel({ onStatusChange }: AccountPanelProps) {
  const [status, setStatus] = useState<AccountStatus | null>(null);
  const [provider, setProvider] = useState("openai");
  const [apiKey, setApiKey] = useState("");
  const [selectedModel, setSelectedModel] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  // When the user clicks "Refresh from API", the live catalogue replaces
  // the hardcoded list for the rest of the session. null = haven't asked
  // the API yet → fall back to status.availableModels.
  const [fetchedModels, setFetchedModels] = useState<AvailableModel[] | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshedAt, setRefreshedAt] = useState<Date | null>(null);

  // Load current status on mount
  useEffect(() => {
    aiApi.getAccountStatus().then((s) => {
      setStatus(s);
      if (s.hasApiKey && s.provider) {
        setProvider(s.provider.toLowerCase());
        setSelectedModel(s.model);
      }
      onStatusChange?.(s);
    }).catch(() => {});
  }, [onStatusChange]);

  // When the user has hit Refresh, the live API list overrides the static
  // catalogue (filtered to the current provider). Otherwise fall back to
  // whatever the backend handed us at startup.
  const allModels = fetchedModels ?? status?.availableModels ?? [];
  const filteredModels = allModels.filter(
    (m) => m.provider.toLowerCase() === provider,
  );

  // A new provider selection invalidates a list fetched for the previous
  // provider — clear the cache so the user sees the static fallback (or
  // can hit Refresh again with the appropriate key).
  useEffect(() => {
    setFetchedModels(null);
    setRefreshedAt(null);
  }, [provider]);

  const handleRefreshModels = useCallback(async () => {
    if (!apiKey.trim()) {
      setError("Enter the API key first, then click Refresh");
      return;
    }
    setRefreshing(true);
    setError(null);
    try {
      const live = await aiApi.fetchProviderModels(provider, apiKey.trim());
      if (live.length === 0) {
        setError("Provider returned no chat-completable models");
      } else {
        setFetchedModels(live);
        setRefreshedAt(new Date());
        // If the previously-selected model isn't in the new list, jump to
        // the first reasoning-tier (or first overall) entry.
        if (!live.find((m) => m.id === selectedModel)) {
          const reasoning = live.find((m) => m.tier === "reasoning");
          setSelectedModel(reasoning?.id ?? live[0].id);
        }
      }
    } catch (e) {
      setError(`Could not fetch models: ${errMsg(e)}`);
    } finally {
      setRefreshing(false);
    }
  }, [apiKey, provider, selectedModel]);

  // Group by tier
  const tiers = ["reasoning", "balanced", "fast"];
  const modelsByTier = tiers
    .map((tier) => ({
      tier,
      label: TIER_LABELS[tier] ?? tier,
      models: filteredModels.filter((m) => m.tier === tier),
    }))
    .filter((g) => g.models.length > 0);

  // Auto-select first balanced model when provider changes
  useEffect(() => {
    if (filteredModels.length > 0 && !filteredModels.find((m) => m.id === selectedModel)) {
      const balanced = filteredModels.find((m) => m.tier === "balanced");
      setSelectedModel(balanced?.id ?? filteredModels[0].id);
    }
  }, [provider, filteredModels, selectedModel]);

  const provInfo = PROVIDERS.find((p) => p.id === provider) ?? PROVIDERS[0];

  const handleSave = useCallback(async () => {
    if (!apiKey.trim()) {
      setError("API key is required");
      return;
    }
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      await aiApi.setApiKey(provider, apiKey.trim(), selectedModel);
      const s = await aiApi.getAccountStatus();
      setStatus(s);
      setSuccess(`Connected to ${provInfo.label} / ${selectedModel}`);
      onStatusChange?.(s);
      setApiKey("");
      setTimeout(() => setSuccess(null), 5000);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setSaving(false);
    }
  }, [provider, apiKey, selectedModel, onStatusChange, provInfo.label]);

  return (
    <div className="ai-panel account-panel" data-testid="account-panel">
      <div className="ai-panel-header">
        <h3>AI Settings</h3>
        <p className="ai-panel-sub">
          Connect to an AI provider to enable all features.
        </p>
      </div>

      {/* Connection status */}
      {status?.hasApiKey && (
        <div className="connection-status connected">
          <span className="status-dot" />
          <span>
            Connected · {status.provider} · {status.model}
          </span>
        </div>
      )}

      {/* Provider selector */}
      <div className="account-field">
        <label className="account-label">Provider</label>
        <div className="provider-buttons">
          {PROVIDERS.map((p) => (
            <button
              key={p.id}
              className={`provider-btn ${provider === p.id ? "active" : ""}`}
              onClick={() => setProvider(p.id)}
              type="button"
              data-testid={`provider-${p.id}`}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>

      {/* API Key */}
      <div className="account-field">
        <label className="account-label">
          {provInfo.label} API Key
        </label>
        <input
          type="password"
          className="account-input"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder={provInfo.placeholder}
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleSave();
          }}
          autoComplete="off"
          data-testid="api-key-input"
        />
        <div className="account-hint">
          Get your key at {provInfo.hint}
        </div>
      </div>

      {/* Model selector */}
      <div className="account-field">
        <div className="account-label-row">
          <label className="account-label">Model</label>
          <button
            type="button"
            className="model-refresh-btn"
            onClick={() => void handleRefreshModels()}
            disabled={refreshing || !apiKey.trim()}
            title={
              !apiKey.trim()
                ? "Enter the API key first"
                : "Fetch the latest models available to this key from the provider's API"
            }
            data-testid="refresh-models-btn"
          >
            {refreshing ? "Refreshing…" : "↻ Refresh from API"}
          </button>
        </div>
        {refreshedAt && (
          <div className="account-hint">
            Showing {filteredModels.length} live models from{" "}
            {provInfo.label} (fetched {refreshedAt.toLocaleTimeString()}).
          </div>
        )}
        {!refreshedAt && (
          <div className="account-hint">
            Built-in list shown — click Refresh to load the latest models
            available to your {provInfo.label} key.
          </div>
        )}
        <div className="model-list">
          {modelsByTier.map(({ tier, label, models }) => (
            <div key={tier} className="model-tier-group">
              <div className="model-tier-label">{label}</div>
              {models.map((m: AvailableModel) => (
                <label
                  key={m.id}
                  className={`model-option ${selectedModel === m.id ? "selected" : ""}`}
                  data-testid={`model-${m.id}`}
                >
                  <input
                    type="radio"
                    name="ai-model"
                    value={m.id}
                    checked={selectedModel === m.id}
                    onChange={() => setSelectedModel(m.id)}
                  />
                  <div className="model-info">
                    <span className="model-name">{m.name}</span>
                    <span className="model-desc">{m.description}</span>
                  </div>
                </label>
              ))}
            </div>
          ))}
        </div>
      </div>

      {/* Messages */}
      {error && (
        <div className="ai-error-banner">
          {error}
          <button className="error-dismiss" onClick={() => setError(null)} aria-label="Dismiss">×</button>
        </div>
      )}
      {success && <div className="ai-success-banner">{success}</div>}

      {/* Save button */}
      <div className="account-actions">
        <button
          className="primary"
          onClick={() => void handleSave()}
          disabled={saving || !apiKey.trim() || !selectedModel}
          title={!selectedModel ? "Select a model first" : undefined}
          data-testid="save-key-btn"
        >
          {saving ? "Connecting…" : "Save & Connect"}
        </button>
      </div>

      <div className="account-note">
        Your API key is stored locally on this device and sent only to the
        selected provider's API endpoint. It is never shared with anyone else.
      </div>
    </div>
  );
}
