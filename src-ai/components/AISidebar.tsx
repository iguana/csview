import { type ReactNode } from "react";
import type { FileInfo } from "../lib/types-ai";
import { ChatPanel } from "./ChatPanel";
import { QueryPanel } from "./QueryPanel";
import { ProfilePanel } from "./ProfilePanel";
import { TransformPanel } from "./TransformPanel";
import { AnomalyPanel } from "./AnomalyPanel";
import { QualityPanel } from "./QualityPanel";
import { ReportPanel } from "./ReportPanel";
import { JoinPanel } from "./JoinPanel";
import { CompliancePanel } from "./CompliancePanel";
import { ForecastPanel } from "./ForecastPanel";
import { AccountPanel } from "./AccountPanel";
import type { AccountStatus, TransformResult } from "../lib/types-ai";

// ---------------------------------------------------------------------------
// Tab definitions
// ---------------------------------------------------------------------------

export type AiTab =
  | "chat"
  | "query"
  | "profile"
  | "transform"
  | "anomaly"
  | "quality"
  | "report"
  | "join"
  | "compliance"
  | "forecast"
  | "settings";

const TABS: { id: AiTab; label: string; icon: string }[] = [
  { id: "chat", label: "Chat", icon: "💬" },
  { id: "query", label: "Query", icon: "⌕" },
  { id: "profile", label: "Profile", icon: "◈" },
  { id: "transform", label: "Transform", icon: "⟳" },
  { id: "anomaly", label: "Anomaly", icon: "⚠" },
  { id: "quality", label: "Quality", icon: "✓" },
  { id: "report", label: "Report", icon: "☰" },
  { id: "join", label: "Join", icon: "⊕" },
  { id: "compliance", label: "PII", icon: "⚑" },
  { id: "forecast", label: "Forecast", icon: "↗" },
  { id: "settings", label: "Settings", icon: "⚙" },
];

// ---------------------------------------------------------------------------
// AISidebar props
// ---------------------------------------------------------------------------

export interface AISidebarProps {
  activeTab: AiTab;
  onTabChange: (tab: AiTab) => void;
  fileInfo: FileInfo | null;
  apiKeySet: boolean;
  isProcessing: boolean;
  onProcessing: (loading: boolean) => void;
  onStatusChange: (status: AccountStatus) => void;
  onJumpToRow?: (row: number) => void;
  onApplyFilter?: (sql: string) => void;
  onTransformApply?: (result: TransformResult, columnName: string) => void;
  onJoinComplete?: (result: FileInfo) => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function AISidebar({
  activeTab,
  onTabChange,
  fileInfo,
  apiKeySet,
  isProcessing,
  onProcessing,
  onStatusChange,
  onJumpToRow,
  onApplyFilter,
  onTransformApply,
  onJoinComplete,
}: AISidebarProps) {
  const fileId = fileInfo?.file_id ?? null;
  const columns = fileInfo?.columns.map((c) => c.name) ?? [];

  const requiresKey = activeTab !== "settings" && !apiKeySet;

  let panel: ReactNode;

  if (requiresKey) {
    panel = (
      <div className="ai-requires-key">
        <div className="ai-requires-key-icon">⚙</div>
        <div className="ai-requires-key-title">API Key Required</div>
        <p className="ai-requires-key-body">
          Set your Anthropic API key in Settings to unlock AI features.
        </p>
        <button
          className="primary"
          onClick={() => onTabChange("settings")}
        >
          Open Settings
        </button>
      </div>
    );
  } else {
    switch (activeTab) {
      case "chat":
        panel = (
          <ChatPanel
            fileId={fileId}
            onProcessing={onProcessing}
          />
        );
        break;
      case "query":
        panel = (
          <QueryPanel
            fileId={fileId}
            onApplyFilter={onApplyFilter}
            onProcessing={onProcessing}
          />
        );
        break;
      case "profile":
        panel = (
          <ProfilePanel
            fileId={fileId}
            onProcessing={onProcessing}
          />
        );
        break;
      case "transform":
        panel = (
          <TransformPanel
            fileId={fileId}
            onProcessing={onProcessing}
            onApply={onTransformApply}
          />
        );
        break;
      case "anomaly":
        panel = (
          <AnomalyPanel
            fileId={fileId}
            columns={columns}
            onProcessing={onProcessing}
            onJumpToRow={onJumpToRow}
          />
        );
        break;
      case "quality":
        panel = (
          <QualityPanel
            fileId={fileId}
            onProcessing={onProcessing}
          />
        );
        break;
      case "report":
        panel = (
          <ReportPanel
            fileId={fileId}
            onProcessing={onProcessing}
          />
        );
        break;
      case "join":
        panel = (
          <JoinPanel
            fileId={fileId}
            onProcessing={onProcessing}
            onJoinComplete={onJoinComplete}
          />
        );
        break;
      case "compliance":
        panel = (
          <CompliancePanel
            fileId={fileId}
            onProcessing={onProcessing}
          />
        );
        break;
      case "forecast":
        panel = (
          <ForecastPanel
            fileId={fileId}
            columns={columns}
            onProcessing={onProcessing}
          />
        );
        break;
      case "settings":
        panel = <AccountPanel onStatusChange={onStatusChange} />;
        break;
      default:
        panel = null;
    }
  }

  return (
    <div className="ai-sidebar">
      {/* Tab bar */}
      <div className="ai-tab-bar" role="tablist" aria-label="AI features">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            className={`ai-tab ${activeTab === tab.id ? "active" : ""}`}
            role="tab"
            aria-selected={activeTab === tab.id}
            onClick={() => onTabChange(tab.id)}
            title={tab.label}
          >
            <span className="ai-tab-icon" aria-hidden>{tab.icon}</span>
            <span className="ai-tab-label">{tab.label}</span>
          </button>
        ))}
      </div>

      {/* Panel area */}
      <div
        className="ai-panel-area"
        role="tabpanel"
        aria-label={TABS.find((t) => t.id === activeTab)?.label}
      >
        {isProcessing && (
          <div className="ai-loading-overlay" aria-live="polite" aria-label="AI processing">
            <span className="ai-dots"><span /><span /><span /></span>
          </div>
        )}
        {panel}
      </div>
    </div>
  );
}
