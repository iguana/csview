import type { ColumnMeta } from "../lib/types";
import { formatCount } from "../lib/format";

export interface RowPanelProps {
  rowIndex: number;
  totalRows: number;
  columns: ColumnMeta[];
  values: string[] | undefined;
  onClose: () => void;
}

const numericKinds = new Set(["integer", "float"]);

export function RowPanel({
  rowIndex,
  totalRows,
  columns,
  values,
  onClose,
}: RowPanelProps) {
  return (
    <aside className="sidebar" aria-label="Row details" data-testid="row-panel">
      <div className="sidebar-header">
        <h3>Row {formatCount(rowIndex + 1)}</h3>
        <button
          className="sidebar-close"
          onClick={onClose}
          aria-label="Deselect row"
          title="Deselect row"
        >
          ×
        </button>
      </div>
      <div className="sub">
        Showing {columns.length} field{columns.length === 1 ? "" : "s"} · of{" "}
        {formatCount(totalRows)} row{totalRows === 1 ? "" : "s"}
      </div>

      {!values ? (
        <div className="sub">Loading row…</div>
      ) : (
        <div className="row-fields" data-testid="row-fields">
          {columns.map((col) => {
            const value = values[col.index] ?? "";
            const isNumeric = numericKinds.has(col.kind);
            const isEmpty = value === "";
            return (
              <div className="row-field" key={col.index}>
                <div className="row-field-label">
                  <span className={`kind ${col.kind}`}>{col.kind.slice(0, 3)}</span>
                  <span className="name">{col.name}</span>
                </div>
                <div
                  className={`row-field-value ${isNumeric ? "numeric" : ""} ${
                    isEmpty ? "empty" : ""
                  }`}
                  title={value}
                >
                  {isEmpty ? "—" : value}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </aside>
  );
}
