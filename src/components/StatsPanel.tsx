import type { ColumnMeta, ColumnStats } from "../lib/types";
import { formatCount, formatNumber } from "../lib/format";

export interface StatsPanelProps {
  column: ColumnMeta | null;
  stats: ColumnStats | null;
  loading: boolean;
  totalRows: number;
  onClose?: () => void;
}

function CloseButton({ onClose }: { onClose?: () => void }) {
  if (!onClose) return null;
  return (
    <button
      className="sidebar-close"
      onClick={onClose}
      aria-label="Hide sidebar"
      title="Hide sidebar (⌘B)"
    >
      ×
    </button>
  );
}

export function StatsPanel({
  column,
  stats,
  loading,
  totalRows,
  onClose,
}: StatsPanelProps) {
  if (!column) {
    return (
      <aside className="sidebar" aria-label="Column statistics">
        <div className="sidebar-header">
          <h3>Column stats</h3>
          <CloseButton onClose={onClose} />
        </div>
        <div className="sub">Click a column header to see its statistics.</div>
      </aside>
    );
  }

  const percentPresent = stats
    ? ((stats.count - stats.empty) / Math.max(1, stats.count)) * 100
    : 0;

  const maxTopCount = stats?.top_values.length
    ? Math.max(...stats.top_values.map(([, c]) => c))
    : 0;

  return (
    <aside className="sidebar" aria-label="Column statistics">
      <div className="sidebar-header">
        <h3>
          {column.name}
          <span
            className={`kind ${column.kind}`}
            style={{
              marginLeft: 8,
              fontSize: 9,
              padding: "1px 5px",
              borderRadius: 3,
              textTransform: "uppercase",
              fontWeight: 700,
              color: "var(--badge-text)",
              background: `var(--kind-${column.kind})`,
            }}
          >
            {column.kind}
          </span>
        </h3>
        <CloseButton onClose={onClose} />
      </div>
      <div className="sub">
        Column {column.index + 1} of {formatCount(totalRows)} row
        {totalRows === 1 ? "" : "s"}
      </div>

      {loading && <div className="sub">Computing…</div>}

      {stats && (
        <>
          <div className="section">
            <div className="section-title">Counts</div>
            <div className="stat-row">
              <span className="label">Total</span>
              <span className="value">{formatCount(stats.count)}</span>
            </div>
            <div className="stat-row">
              <span className="label">Non-empty</span>
              <span className="value">
                {formatCount(stats.count - stats.empty)} (
                {percentPresent.toFixed(1)}%)
              </span>
            </div>
            <div className="stat-row">
              <span className="label">Empty</span>
              <span className="value">{formatCount(stats.empty)}</span>
            </div>
            <div className="stat-row">
              <span className="label">Unique</span>
              <span className="value">{formatCount(stats.unique)}</span>
            </div>
          </div>

          {stats.numeric_count > 0 && (
            <div className="section">
              <div className="section-title">Numeric</div>
              <div className="stat-row">
                <span className="label">Numeric cells</span>
                <span className="value">{formatCount(stats.numeric_count)}</span>
              </div>
              <div className="stat-row">
                <span className="label">Min</span>
                <span className="value">{formatNumber(stats.min)}</span>
              </div>
              <div className="stat-row">
                <span className="label">Max</span>
                <span className="value">{formatNumber(stats.max)}</span>
              </div>
              <div className="stat-row">
                <span className="label">Mean</span>
                <span className="value">{formatNumber(stats.mean)}</span>
              </div>
              <div className="stat-row">
                <span className="label">Sum</span>
                <span className="value">{formatNumber(stats.sum)}</span>
              </div>
            </div>
          )}

          {stats.top_values.length > 0 && (
            <div className="section">
              <div className="section-title">Top values</div>
              {stats.top_values.map(([value, count]) => (
                <div key={value + count} className="top-value">
                  <span className="val">{value === "" ? "(empty)" : value}</span>
                  <span className="bar">
                    <span
                      className="fill"
                      style={{
                        width: `${(count / maxTopCount) * 100}%`,
                      }}
                    />
                  </span>
                  <span>{formatCount(count)}</span>
                </div>
              ))}
            </div>
          )}

          {(stats.shortest || stats.longest) && (
            <div className="section">
              <div className="section-title">Text</div>
              {stats.shortest && (
                <div className="stat-row">
                  <span className="label">Shortest</span>
                  <span className="value" title={stats.shortest}>
                    {stats.shortest.length > 18
                      ? stats.shortest.slice(0, 18) + "…"
                      : stats.shortest}
                  </span>
                </div>
              )}
              {stats.longest && (
                <div className="stat-row">
                  <span className="label">Longest</span>
                  <span className="value" title={stats.longest}>
                    {stats.longest.length > 18
                      ? stats.longest.slice(0, 18) + "…"
                      : stats.longest}
                  </span>
                </div>
              )}
            </div>
          )}
        </>
      )}
    </aside>
  );
}
