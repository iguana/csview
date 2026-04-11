import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ColumnMeta, SortDirection, SortKey } from "../lib/types";
import { RangeCache } from "../lib/rangeCache";
import { highlightText } from "../lib/highlight";

export interface CellCoord {
  row: number;
  col: number;
}

export interface DataGridHandle {
  /** Imperatively start editing the active cell with the given initial text. */
  beginEdit: (initial?: string) => void;
  /** Focus the grid so it starts receiving keyboard events. */
  focus: () => void;
}

export interface DataGridProps {
  columns: ColumnMeta[];
  rowCount: number;
  sortKeys: SortKey[];
  onSortChange: (keys: SortKey[]) => void;
  cache: RangeCache;
  cacheVersion: number;
  searchHitRows: Set<number>;
  highlightQuery: string;
  onSelectColumn: (column: number) => void;
  selectedColumn: number | null;
  /** Active cell (focus target). Also doubles as the selected row. */
  activeCell: CellCoord | null;
  onActiveCellChange: (cell: CellCoord | null) => void;
  /** Commit a cell edit — caller persists to backend. */
  onCellCommit: (row: number, col: number, value: string) => void;
  onCopy: (cells: CellCoord[]) => void;
  onCut: (cells: CellCoord[]) => void;
  onPaste: () => void;
  onDeleteRows: (rows: number[]) => void;
  rowHeight: number;
  jumpToRow: number | null;
}

const DEFAULT_COL_WIDTH = 160;
const ROW_INDEX_WIDTH = 60;
const HEADER_HEIGHT = 34;

const numericKinds = new Set(["integer", "float"]);

export const DataGrid = forwardRef<DataGridHandle, DataGridProps>(
  function DataGrid(props, ref) {
    const {
      columns,
      rowCount,
      sortKeys,
      onSortChange,
      cache,
      cacheVersion,
      searchHitRows,
      highlightQuery,
      onSelectColumn,
      selectedColumn,
      activeCell,
      onActiveCellChange,
      onCellCommit,
      onCopy,
      onCut,
      onPaste,
      onDeleteRows,
      rowHeight,
      jumpToRow,
    } = props;

    const scrollRef = useRef<HTMLDivElement>(null);
    const innerRef = useRef<HTMLDivElement>(null);
    const editInputRef = useRef<HTMLInputElement>(null);

    const [columnWidths, setColumnWidths] = useState<Record<number, number>>({});
    const [resizing, setResizing] = useState<number | null>(null);
    const [, forceTick] = useState(0);
    const [editingCell, setEditingCell] = useState<CellCoord | null>(null);
    const [editValue, setEditValue] = useState("");

    useEffect(() => {
      setColumnWidths({});
    }, [columns.length]);

    const widthFor = useCallback(
      (index: number) => columnWidths[index] ?? DEFAULT_COL_WIDTH,
      [columnWidths],
    );

    const totalWidth = useMemo(
      () => ROW_INDEX_WIDTH + columns.reduce((acc, c) => acc + widthFor(c.index), 0),
      [columns, widthFor],
    );

    const rowVirtualizer = useVirtualizer({
      count: rowCount,
      getScrollElement: () => scrollRef.current,
      estimateSize: () => rowHeight,
      overscan: 12,
    });

    useEffect(() => {
      rowVirtualizer.measure();
    }, [rowHeight, rowCount, rowVirtualizer]);

    useEffect(() => {
      if (jumpToRow != null) {
        rowVirtualizer.scrollToIndex(jumpToRow, { align: "center" });
      }
    }, [jumpToRow, rowVirtualizer]);

    const virtualItems = rowVirtualizer.getVirtualItems();

    useEffect(() => {
      if (virtualItems.length === 0) return;
      const first = virtualItems[0].index;
      const last = virtualItems[virtualItems.length - 1].index + 1;
      void cache.ensure(first, last).then(() => {
        forceTick((t) => t + 1);
      });
    }, [virtualItems, cache, cacheVersion]);

    // --- Sorting on header click ---
    const onHeaderClick = useCallback(
      (e: React.MouseEvent, col: ColumnMeta) => {
        if ((e.target as HTMLElement).classList.contains("resize-handle")) return;
        onSelectColumn(col.index);
        const existing = sortKeys.find((k) => k.column === col.index);
        const shift = e.shiftKey;
        let next: SortKey[];
        if (!existing) {
          next = shift
            ? [...sortKeys, { column: col.index, direction: "asc" }]
            : [{ column: col.index, direction: "asc" }];
        } else {
          const flip: SortDirection =
            existing.direction === "asc" ? "desc" : "asc";
          next = sortKeys.map((k) =>
            k.column === col.index ? { ...k, direction: flip } : k,
          );
          if (!shift) next = next.filter((k) => k.column === col.index);
          if (existing.direction === "desc") {
            next = sortKeys.filter((k) => k.column !== col.index);
          }
        }
        onSortChange(next);
      },
      [sortKeys, onSortChange, onSelectColumn],
    );

    // --- Column resizing ---
    const startResize = useCallback(
      (e: React.MouseEvent, columnIndex: number) => {
        e.preventDefault();
        e.stopPropagation();
        setResizing(columnIndex);
        const startX = e.clientX;
        const startWidth = widthFor(columnIndex);
        const onMove = (ev: MouseEvent) => {
          const delta = ev.clientX - startX;
          setColumnWidths((prev) => ({
            ...prev,
            [columnIndex]: Math.max(60, startWidth + delta),
          }));
        };
        const onUp = () => {
          setResizing(null);
          window.removeEventListener("mousemove", onMove);
          window.removeEventListener("mouseup", onUp);
        };
        window.addEventListener("mousemove", onMove);
        window.addEventListener("mouseup", onUp);
      },
      [widthFor],
    );

    // --- Cell interaction ---
    const beginEdit = useCallback(
      (initial?: string) => {
        if (!activeCell) return;
        const row = cache.get(activeCell.row);
        const current = row ? (row[activeCell.col] ?? "") : "";
        setEditingCell({ ...activeCell });
        setEditValue(initial != null ? initial : current);
        // Focus the input after it mounts.
        setTimeout(() => editInputRef.current?.focus(), 0);
      },
      [activeCell, cache],
    );

    useImperativeHandle(
      ref,
      () => ({
        beginEdit,
        focus: () => scrollRef.current?.focus(),
      }),
      [beginEdit],
    );

    const commitEdit = useCallback(() => {
      if (!editingCell) return;
      onCellCommit(editingCell.row, editingCell.col, editValue);
      setEditingCell(null);
      setEditValue("");
      // Return focus to the grid so keyboard nav works again.
      setTimeout(() => scrollRef.current?.focus(), 0);
    }, [editingCell, editValue, onCellCommit]);

    const cancelEdit = useCallback(() => {
      setEditingCell(null);
      setEditValue("");
      setTimeout(() => scrollRef.current?.focus(), 0);
    }, []);

    const onCellClick = useCallback(
      (e: React.MouseEvent, row: number, col: number) => {
        e.stopPropagation();
        onActiveCellChange({ row, col });
      },
      [onActiveCellChange],
    );

    const onCellDoubleClick = useCallback(
      (_e: React.MouseEvent, row: number, col: number) => {
        onActiveCellChange({ row, col });
        setTimeout(() => beginEdit(), 0);
      },
      [onActiveCellChange, beginEdit],
    );

    // --- Keyboard navigation ---
    const moveActive = useCallback(
      (drow: number, dcol: number) => {
        if (!activeCell) {
          onActiveCellChange({ row: 0, col: 0 });
          return;
        }
        const nextRow = Math.max(0, Math.min(rowCount - 1, activeCell.row + drow));
        const nextCol = Math.max(
          0,
          Math.min(columns.length - 1, activeCell.col + dcol),
        );
        onActiveCellChange({ row: nextRow, col: nextCol });
        rowVirtualizer.scrollToIndex(nextRow, { align: "auto" });
      },
      [activeCell, rowCount, columns.length, onActiveCellChange, rowVirtualizer],
    );

    const onKeyDown = useCallback(
      (e: React.KeyboardEvent<HTMLDivElement>) => {
        if (editingCell) {
          // Let the input handle its own keys.
          return;
        }
        const meta = e.metaKey || e.ctrlKey;
        // Copy / cut / paste
        if (meta && !e.shiftKey) {
          if (e.key === "c" || e.key === "C") {
            if (activeCell) {
              e.preventDefault();
              onCopy([activeCell]);
            }
            return;
          }
          if (e.key === "x" || e.key === "X") {
            if (activeCell) {
              e.preventDefault();
              onCut([activeCell]);
            }
            return;
          }
          if (e.key === "v" || e.key === "V") {
            e.preventDefault();
            onPaste();
            return;
          }
        }
        // Navigation
        if (!activeCell && rowCount > 0) {
          if (
            e.key === "ArrowUp" ||
            e.key === "ArrowDown" ||
            e.key === "ArrowLeft" ||
            e.key === "ArrowRight" ||
            e.key === "Enter"
          ) {
            e.preventDefault();
            onActiveCellChange({ row: 0, col: 0 });
            return;
          }
        }
        if (!activeCell) return;
        switch (e.key) {
          case "ArrowUp":
            e.preventDefault();
            moveActive(-1, 0);
            return;
          case "ArrowDown":
            e.preventDefault();
            moveActive(1, 0);
            return;
          case "ArrowLeft":
            e.preventDefault();
            moveActive(0, -1);
            return;
          case "ArrowRight":
          case "Tab":
            e.preventDefault();
            moveActive(0, e.shiftKey ? -1 : 1);
            return;
          case "Home":
            e.preventDefault();
            onActiveCellChange({
              row: activeCell.row,
              col: 0,
            });
            return;
          case "End":
            e.preventDefault();
            onActiveCellChange({
              row: activeCell.row,
              col: columns.length - 1,
            });
            return;
          case "PageDown":
            e.preventDefault();
            moveActive(20, 0);
            return;
          case "PageUp":
            e.preventDefault();
            moveActive(-20, 0);
            return;
          case "Enter":
          case "F2":
            e.preventDefault();
            beginEdit();
            return;
          case "Delete":
          case "Backspace":
            if (meta || e.shiftKey) {
              e.preventDefault();
              onDeleteRows([activeCell.row]);
              return;
            }
            e.preventDefault();
            onCellCommit(activeCell.row, activeCell.col, "");
            return;
        }
        // Any printable character starts an edit with that character.
        if (e.key.length === 1 && !meta) {
          e.preventDefault();
          beginEdit(e.key);
        }
      },
      [
        editingCell,
        activeCell,
        rowCount,
        columns.length,
        moveActive,
        onActiveCellChange,
        beginEdit,
        onCopy,
        onCut,
        onPaste,
        onCellCommit,
        onDeleteRows,
      ],
    );

    const onEditInputKeyDown = useCallback(
      (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commitEdit();
          moveActive(1, 0);
        } else if (e.key === "Escape") {
          e.preventDefault();
          cancelEdit();
        } else if (e.key === "Tab") {
          e.preventDefault();
          commitEdit();
          moveActive(0, e.shiftKey ? -1 : 1);
        }
      },
      [commitEdit, cancelEdit, moveActive],
    );

    const renderCellText = (text: string): React.ReactNode =>
      highlightText(text, highlightQuery);

    const sortLabelFor = (columnIndex: number): string => {
      const idx = sortKeys.findIndex((k) => k.column === columnIndex);
      if (idx === -1) return "";
      const key = sortKeys[idx];
      const arrow = key.direction === "asc" ? "▲" : "▼";
      if (sortKeys.length > 1) return `${arrow}${idx + 1}`;
      return arrow;
    };

    return (
      <div
        className="grid"
        ref={scrollRef}
        tabIndex={0}
        onKeyDown={onKeyDown}
        data-testid="datagrid-scroll"
      >
        <div
          className="grid-inner"
          ref={innerRef}
          style={{
            width: totalWidth,
            height: rowVirtualizer.getTotalSize() + HEADER_HEIGHT,
          }}
        >
          <div
            className="grid-header-row"
            style={{ width: totalWidth, height: HEADER_HEIGHT }}
            role="row"
          >
            <div
              className="row-index-header"
              style={{ height: HEADER_HEIGHT }}
              aria-label="Row number"
            >
              #
            </div>
            {columns.map((col) => {
              const sortLabel = sortLabelFor(col.index);
              const isSelected = selectedColumn === col.index;
              return (
                <div
                  key={col.index}
                  className="grid-header-cell"
                  role="columnheader"
                  aria-sort={
                    sortLabel.startsWith("▲")
                      ? "ascending"
                      : sortLabel.startsWith("▼")
                        ? "descending"
                        : "none"
                  }
                  style={{
                    width: widthFor(col.index),
                    height: HEADER_HEIGHT,
                    background: isSelected ? "var(--bg-row-hover)" : undefined,
                  }}
                  onClick={(e) => onHeaderClick(e, col)}
                  data-testid={`header-${col.index}`}
                >
                  <span className={`kind ${col.kind}`}>{col.kind.slice(0, 3)}</span>
                  <span className="name">{col.name}</span>
                  <span className="sort">{sortLabel}</span>
                  <div
                    className={`resize-handle ${resizing === col.index ? "dragging" : ""}`}
                    onMouseDown={(e) => startResize(e, col.index)}
                    data-testid={`resize-${col.index}`}
                  />
                </div>
              );
            })}
          </div>

          {virtualItems.map((virtualRow) => {
            const row = cache.get(virtualRow.index);
            const isHit = searchHitRows.has(virtualRow.index);
            const isSelectedRow =
              activeCell != null && activeCell.row === virtualRow.index;
            return (
              <div
                key={virtualRow.key}
                className={`grid-row ${isHit ? "hit" : ""} ${
                  isSelectedRow ? "row-selected" : ""
                }`}
                role="row"
                data-testid={`row-${virtualRow.index}`}
                style={{
                  height: rowHeight,
                  transform: `translateY(${virtualRow.start + HEADER_HEIGHT}px)`,
                  width: totalWidth,
                }}
              >
                <div className="row-index-cell" style={{ height: rowHeight }}>
                  {virtualRow.index + 1}
                </div>
                {columns.map((col) => {
                  const value = row ? (row[col.index] ?? "") : "";
                  const numeric = numericKinds.has(col.kind);
                  const isActive =
                    activeCell != null &&
                    activeCell.row === virtualRow.index &&
                    activeCell.col === col.index;
                  const isEditingThis =
                    editingCell != null &&
                    editingCell.row === virtualRow.index &&
                    editingCell.col === col.index;
                  return (
                    <div
                      key={col.index}
                      className={`grid-cell ${numeric ? "numeric" : ""} ${
                        row ? "" : "loading"
                      } ${isActive ? "cell-active" : ""}`}
                      role="cell"
                      style={{ width: widthFor(col.index), height: rowHeight }}
                      title={value}
                      onClick={(e) => onCellClick(e, virtualRow.index, col.index)}
                      onDoubleClick={(e) =>
                        onCellDoubleClick(e, virtualRow.index, col.index)
                      }
                      data-testid={`cell-${virtualRow.index}-${col.index}`}
                    >
                      {isEditingThis ? (
                        <input
                          ref={editInputRef}
                          className="cell-edit-input"
                          value={editValue}
                          onChange={(ev) => setEditValue(ev.target.value)}
                          onBlur={commitEdit}
                          onKeyDown={onEditInputKeyDown}
                          autoFocus
                          data-testid={`cell-edit-${virtualRow.index}-${col.index}`}
                        />
                      ) : row ? (
                        renderCellText(value)
                      ) : (
                        "…"
                      )}
                    </div>
                  );
                })}
              </div>
            );
          })}
        </div>
      </div>
    );
  },
);
