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
  /** Auto-size a single column to fit the widest visible cell. */
  autoSizeColumn: (columnIndex: number) => void;
  /** Auto-size every visible column. */
  autoSizeAllColumns: () => void;
  /** Freeze the first N visible rows (clamped). */
  setFrozenRows: (n: number) => void;
  /** Freeze the first N visible columns (clamped). */
  setFrozenColumns: (n: number) => void;
  /** Clear both row and column freezes. */
  unfreezeAll: () => void;
  /** Hide a column by its data index. */
  hideColumn: (columnIndex: number) => void;
  /** Hide a row by its data index. */
  hideRow: (rowIndex: number) => void;
  /** Show every hidden row and column. */
  showAllHidden: () => void;
  /** Open the delete-row confirmation modal for the given rows. */
  requestDeleteRows: (rows: number[]) => void;
  /** Open the delete-column confirmation modal. */
  requestDeleteColumn: (columnIndex: number) => void;
  /** Read current display state — used by status bar / menu enablement. */
  getDisplayState: () => {
    frozenRows: number;
    frozenColumns: number;
    hiddenRowCount: number;
    hiddenColumnCount: number;
  };
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
  /** Actually delete rows. The grid confirms first; caller just performs. */
  onDeleteRows: (rows: number[]) => void;
  /** Optional: actually delete a column. If omitted, "Delete Column…" is hidden. */
  onDeleteColumn?: (columnIndex: number) => void;
  rowHeight: number;
  jumpToRow: number | null;
  /** Optional: report display-state changes upward (for status bar / menus). */
  onDisplayStateChange?: (state: {
    frozenRows: number;
    frozenColumns: number;
    hiddenRowCount: number;
    hiddenColumnCount: number;
  }) => void;
}

const DEFAULT_COL_WIDTH = 160;
const ROW_INDEX_WIDTH = 60;
const HEADER_HEIGHT = 34;
const MIN_COL_WIDTH = 60;
const MAX_AUTOSIZE_WIDTH = 800;
const MIN_ROW_HEIGHT = 18;
const MAX_ROW_HEIGHT = 600;
/** Per-side cell horizontal padding (matches `.grid-cell { padding: 0 10px }` in CSS). */
const CELL_HPADDING = 10;
/** Header content overhead: kind badge + sort badge + flex gaps + padding. */
const HEADER_CONTENT_OVERHEAD = 80;

const numericKinds = new Set(["integer", "float"]);

interface ContextMenuState {
  x: number;
  y: number;
  kind: "header" | "rowindex";
  /** For "header": column data index. For "rowindex": display-row index in the visible-row mapping. */
  index: number;
}

interface ConfirmState {
  message: string;
  detail?: string;
  confirmLabel: string;
  destructive?: boolean;
  onConfirm: () => void;
}

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
      onDeleteColumn,
      rowHeight,
      jumpToRow,
      onDisplayStateChange,
    } = props;

    const scrollRef = useRef<HTMLDivElement>(null);
    const innerRef = useRef<HTMLDivElement>(null);
    const editInputRef = useRef<HTMLInputElement>(null);
    const measureCanvasRef = useRef<HTMLCanvasElement | null>(null);

    const [columnWidths, setColumnWidths] = useState<Record<number, number>>({});
    const [rowHeights, setRowHeights] = useState<Record<number, number>>({});
    const [resizing, setResizing] = useState<number | null>(null);
    const [resizingRow, setResizingRow] = useState<number | null>(null);
    const [, forceTick] = useState(0);
    const [editingCell, setEditingCell] = useState<CellCoord | null>(null);
    const [editValue, setEditValue] = useState("");

    // --- Display-state: freeze + hide ---
    const [frozenRows, setFrozenRowsState] = useState(0);
    const [frozenColumns, setFrozenColumnsState] = useState(0);
    const [hiddenRows, setHiddenRows] = useState<Set<number>>(() => new Set());
    const [hiddenColumns, setHiddenColumns] = useState<Set<number>>(() => new Set());

    const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
    const [confirm, setConfirm] = useState<ConfirmState | null>(null);

    // Reset everything that's column-shaped when the column list changes.
    useEffect(() => {
      setColumnWidths({});
      setFrozenColumnsState(0);
      setHiddenColumns(new Set());
    }, [columns.length]);

    // Reset row-shaped state when row count changes meaningfully.
    useEffect(() => {
      setHiddenRows((prev) => {
        if (prev.size === 0) return prev;
        // Drop hidden indices past the new row count.
        const next = new Set<number>();
        for (const r of prev) if (r < rowCount) next.add(r);
        return next.size === prev.size ? prev : next;
      });
      setFrozenRowsState((n) => Math.min(n, rowCount));
      setRowHeights((prev) => {
        const keys = Object.keys(prev);
        if (keys.length === 0) return prev;
        const next: Record<number, number> = {};
        let changed = false;
        for (const k of keys) {
          const idx = Number(k);
          if (idx < rowCount) next[idx] = prev[idx];
          else changed = true;
        }
        return changed ? next : prev;
      });
    }, [rowCount]);

    // Bubble display-state up so the host can render menu enablement / chips.
    useEffect(() => {
      onDisplayStateChange?.({
        frozenRows,
        frozenColumns,
        hiddenRowCount: hiddenRows.size,
        hiddenColumnCount: hiddenColumns.size,
      });
    }, [frozenRows, frozenColumns, hiddenRows, hiddenColumns, onDisplayStateChange]);

    const widthFor = useCallback(
      (index: number) => columnWidths[index] ?? DEFAULT_COL_WIDTH,
      [columnWidths],
    );

    const heightFor = useCallback(
      (dataRowIdx: number) => rowHeights[dataRowIdx] ?? rowHeight,
      [rowHeights, rowHeight],
    );

    // --- Visible columns / rows mapping ---
    // Columns are filtered by hidden set. Frozen count refers to the first N
    // VISIBLE columns (Excel-style: hiding a column doesn't shift the freeze).
    const visibleColumns = useMemo(
      () => columns.filter((c) => !hiddenColumns.has(c.index)),
      [columns, hiddenColumns],
    );
    const effectiveFrozenColumns = Math.min(frozenColumns, visibleColumns.length);

    // Map of display-row index → real data-row index. When nothing is hidden
    // this is the identity (we avoid materializing the array in that case).
    const visibleRowIndices = useMemo<number[] | null>(() => {
      if (hiddenRows.size === 0) return null;
      const arr: number[] = [];
      for (let i = 0; i < rowCount; i++) {
        if (!hiddenRows.has(i)) arr.push(i);
      }
      return arr;
    }, [hiddenRows, rowCount]);
    const displayRowCount = visibleRowIndices ? visibleRowIndices.length : rowCount;
    const dataRowFor = useCallback(
      (displayIdx: number) =>
        visibleRowIndices ? visibleRowIndices[displayIdx] : displayIdx,
      [visibleRowIndices],
    );
    const effectiveFrozenRows = Math.min(frozenRows, displayRowCount);

    // Cumulative left offset for each frozen visible column (in CSS pixels),
    // measured from the left edge of the row-index cell.
    const frozenColLefts = useMemo(() => {
      const lefts: number[] = [];
      let acc = ROW_INDEX_WIDTH;
      for (let i = 0; i < effectiveFrozenColumns; i++) {
        lefts.push(acc);
        acc += widthFor(visibleColumns[i].index);
      }
      return lefts;
    }, [effectiveFrozenColumns, visibleColumns, widthFor]);
    const frozenColumnsRightEdge = useMemo(() => {
      let acc = ROW_INDEX_WIDTH;
      for (let i = 0; i < effectiveFrozenColumns; i++) {
        acc += widthFor(visibleColumns[i].index);
      }
      return acc;
    }, [effectiveFrozenColumns, visibleColumns, widthFor]);

    const totalWidth = useMemo(
      () =>
        ROW_INDEX_WIDTH +
        visibleColumns.reduce((acc, c) => acc + widthFor(c.index), 0),
      [visibleColumns, widthFor],
    );

    const rowVirtualizer = useVirtualizer({
      count: displayRowCount,
      getScrollElement: () => scrollRef.current,
      estimateSize: (i) => heightFor(dataRowFor(i)),
      overscan: 12,
    });

    useEffect(() => {
      rowVirtualizer.measure();
    }, [rowHeight, rowHeights, displayRowCount, rowVirtualizer]);

    useEffect(() => {
      if (jumpToRow != null) {
        // jumpToRow is a data-row index; translate to a display row.
        let target = jumpToRow;
        if (visibleRowIndices) {
          // Find the display index of this real row, or the nearest visible one.
          const idx = visibleRowIndices.indexOf(jumpToRow);
          target = idx >= 0 ? idx : 0;
        }
        rowVirtualizer.scrollToIndex(target, { align: "center" });
      }
    }, [jumpToRow, rowVirtualizer, visibleRowIndices]);

    const virtualItems = rowVirtualizer.getVirtualItems();

    // Ensure the cache always has visible-viewport rows AND any frozen rows.
    useEffect(() => {
      // Always hold the frozen band.
      if (effectiveFrozenRows > 0) {
        const realRows: number[] = [];
        for (let i = 0; i < effectiveFrozenRows; i++) {
          realRows.push(dataRowFor(i));
        }
        // Group into a single span where possible (the cache loads pages
        // anyway, so even a sparse list resolves to the same pages).
        const min = Math.min(...realRows);
        const max = Math.max(...realRows);
        void cache.ensure(min, max + 1).then(() => {
          forceTick((t) => t + 1);
        });
      }
      if (virtualItems.length === 0) return;
      const firstDisplay = virtualItems[0].index;
      const lastDisplay = virtualItems[virtualItems.length - 1].index;
      // Translate display range → data-row range.
      const firstData = dataRowFor(firstDisplay);
      const lastData = dataRowFor(lastDisplay);
      const lo = Math.min(firstData, lastData);
      const hi = Math.max(firstData, lastData) + 1;
      void cache.ensure(lo, hi).then(() => {
        forceTick((t) => t + 1);
      });
    }, [virtualItems, cache, cacheVersion, dataRowFor, effectiveFrozenRows]);

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
            [columnIndex]: Math.max(MIN_COL_WIDTH, startWidth + delta),
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

    // --- Row resizing (vertical) ---
    const startRowResize = useCallback(
      (e: React.MouseEvent, dataRowIdx: number) => {
        e.preventDefault();
        e.stopPropagation();
        setResizingRow(dataRowIdx);
        const startY = e.clientY;
        const startHeight = heightFor(dataRowIdx);
        const onMove = (ev: MouseEvent) => {
          const delta = ev.clientY - startY;
          const next = Math.max(
            MIN_ROW_HEIGHT,
            Math.min(MAX_ROW_HEIGHT, startHeight + delta),
          );
          setRowHeights((prev) => ({ ...prev, [dataRowIdx]: next }));
        };
        const onUp = () => {
          setResizingRow(null);
          window.removeEventListener("mousemove", onMove);
          window.removeEventListener("mouseup", onUp);
        };
        window.addEventListener("mousemove", onMove);
        window.addEventListener("mouseup", onUp);
      },
      [heightFor],
    );

    const resetRowHeight = useCallback((dataRowIdx: number) => {
      setRowHeights((prev) => {
        if (!(dataRowIdx in prev)) return prev;
        const next = { ...prev };
        delete next[dataRowIdx];
        return next;
      });
    }, []);

    // --- Auto-size column ---
    const getMeasureContext = useCallback(() => {
      if (!measureCanvasRef.current) {
        measureCanvasRef.current = document.createElement("canvas");
      }
      const ctx = measureCanvasRef.current.getContext("2d");
      if (!ctx) return null;
      const sample = innerRef.current?.querySelector<HTMLElement>(
        ".grid-cell, .grid-header-cell .name",
      );
      let font: string;
      if (sample) {
        const cs = window.getComputedStyle(sample);
        // Build a CSS font shorthand from the parts (Safari sometimes leaves
        // computed `font` blank when shorthand isn't set).
        font = `${cs.fontStyle} ${cs.fontVariant} ${cs.fontWeight} ${cs.fontSize} / ${cs.lineHeight} ${cs.fontFamily}`;
      } else {
        font =
          '13px -apple-system, BlinkMacSystemFont, "SF Pro Text", "Inter", system-ui, sans-serif';
      }
      ctx.font = font;
      return ctx;
    }, []);

    const autoSizeColumn = useCallback(
      (columnIndex: number) => {
        const ctx = getMeasureContext();
        if (!ctx) return;
        let widest = 0;
        for (const { index, row } of cache.loadedRows()) {
          if (hiddenRows.has(index)) continue;
          const value = row[columnIndex] ?? "";
          if (!value) continue;
          const w = ctx.measureText(value).width;
          if (w > widest) widest = w;
        }
        // Account for the header text + badges.
        const col = columns.find((c) => c.index === columnIndex);
        const headerNameWidth = col ? ctx.measureText(col.name).width : 0;
        const headerNeed = headerNameWidth + HEADER_CONTENT_OVERHEAD;
        const contentNeed = widest + CELL_HPADDING * 2 + 6;
        const target = Math.max(MIN_COL_WIDTH, Math.min(MAX_AUTOSIZE_WIDTH, Math.max(headerNeed, contentNeed)));
        setColumnWidths((prev) => ({ ...prev, [columnIndex]: Math.ceil(target) }));
      },
      [cache, columns, getMeasureContext, hiddenRows],
    );

    const autoSizeAllColumns = useCallback(() => {
      for (const c of visibleColumns) autoSizeColumn(c.index);
    }, [visibleColumns, autoSizeColumn]);

    // --- Hide / freeze actions ---
    const hideColumn = useCallback((columnIndex: number) => {
      setHiddenColumns((prev) => {
        const next = new Set(prev);
        next.add(columnIndex);
        return next;
      });
    }, []);

    const hideRow = useCallback((rowIndex: number) => {
      setHiddenRows((prev) => {
        const next = new Set(prev);
        next.add(rowIndex);
        return next;
      });
    }, []);

    const showAllHidden = useCallback(() => {
      setHiddenColumns(new Set());
      setHiddenRows(new Set());
    }, []);

    const setFrozenRows = useCallback(
      (n: number) => {
        const clamped = Math.max(0, Math.min(n, displayRowCount));
        setFrozenRowsState(clamped);
      },
      [displayRowCount],
    );
    const setFrozenColumns = useCallback(
      (n: number) => {
        const clamped = Math.max(0, Math.min(n, visibleColumns.length));
        setFrozenColumnsState(clamped);
      },
      [visibleColumns.length],
    );
    const unfreezeAll = useCallback(() => {
      setFrozenRowsState(0);
      setFrozenColumnsState(0);
    }, []);

    // --- Cell interaction ---
    const beginEdit = useCallback(
      (initial?: string) => {
        if (!activeCell) return;
        const row = cache.get(activeCell.row);
        const current = row ? (row[activeCell.col] ?? "") : "";
        setEditingCell({ ...activeCell });
        setEditValue(initial != null ? initial : current);
        setTimeout(() => editInputRef.current?.focus(), 0);
      },
      [activeCell, cache],
    );

    const requestDeleteRows = useCallback(
      (rows: number[]) => {
        if (rows.length === 0) return;
        setConfirm({
          message: `Delete ${rows.length} row${rows.length === 1 ? "" : "s"}?`,
          detail: rows.length === 1 ? `Row ${rows[0] + 1}` : undefined,
          confirmLabel: "Delete",
          destructive: true,
          onConfirm: () => onDeleteRows(rows),
        });
      },
      [onDeleteRows],
    );

    const requestDeleteColumn = useCallback(
      (columnIndex: number) => {
        if (!onDeleteColumn) return;
        const col = columns.find((c) => c.index === columnIndex);
        const name = col?.name ?? `Column ${columnIndex + 1}`;
        setConfirm({
          message: `Delete column "${name}"?`,
          detail: "This removes the column from every row. You can undo with ⌘Z.",
          confirmLabel: "Delete column",
          destructive: true,
          onConfirm: () => onDeleteColumn(columnIndex),
        });
      },
      [columns, onDeleteColumn],
    );

    useImperativeHandle(
      ref,
      () => ({
        beginEdit,
        focus: () => scrollRef.current?.focus(),
        autoSizeColumn,
        autoSizeAllColumns,
        setFrozenRows,
        setFrozenColumns,
        unfreezeAll,
        hideColumn,
        hideRow,
        showAllHidden,
        requestDeleteRows,
        requestDeleteColumn,
        getDisplayState: () => ({
          frozenRows,
          frozenColumns,
          hiddenRowCount: hiddenRows.size,
          hiddenColumnCount: hiddenColumns.size,
        }),
      }),
      [
        beginEdit,
        autoSizeColumn,
        autoSizeAllColumns,
        setFrozenRows,
        setFrozenColumns,
        unfreezeAll,
        hideColumn,
        hideRow,
        showAllHidden,
        requestDeleteRows,
        requestDeleteColumn,
        frozenRows,
        frozenColumns,
        hiddenRows,
        hiddenColumns,
      ],
    );

    const commitEdit = useCallback(() => {
      if (!editingCell) return;
      onCellCommit(editingCell.row, editingCell.col, editValue);
      setEditingCell(null);
      setEditValue("");
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
    // Movement is in DATA-row terms (the active cell stays anchored to a real
    // data row even when the visible mapping changes). When jumping by ±1 we
    // hop to the next visible neighbour.
    const moveActive = useCallback(
      (drow: number, dcol: number) => {
        if (!activeCell) {
          const firstRow = visibleRowIndices ? visibleRowIndices[0] ?? 0 : 0;
          const firstCol = visibleColumns[0]?.index ?? 0;
          onActiveCellChange({ row: firstRow, col: firstCol });
          return;
        }
        // Column step: walk through visibleColumns by index in that array.
        let nextCol = activeCell.col;
        if (dcol !== 0) {
          const visIdx = visibleColumns.findIndex((c) => c.index === activeCell.col);
          let target = visIdx === -1 ? 0 : visIdx + dcol;
          target = Math.max(0, Math.min(visibleColumns.length - 1, target));
          nextCol = visibleColumns[target]?.index ?? activeCell.col;
        }
        // Row step: walk through visibleRowIndices.
        let nextRow = activeCell.row;
        if (drow !== 0) {
          if (visibleRowIndices) {
            const visIdx = visibleRowIndices.indexOf(activeCell.row);
            let target = visIdx === -1 ? 0 : visIdx + drow;
            target = Math.max(0, Math.min(visibleRowIndices.length - 1, target));
            nextRow = visibleRowIndices[target] ?? activeCell.row;
            rowVirtualizer.scrollToIndex(target, { align: "auto" });
          } else {
            nextRow = Math.max(0, Math.min(rowCount - 1, activeCell.row + drow));
            rowVirtualizer.scrollToIndex(nextRow, { align: "auto" });
          }
        }
        onActiveCellChange({ row: nextRow, col: nextCol });
      },
      [
        activeCell,
        visibleColumns,
        visibleRowIndices,
        rowCount,
        onActiveCellChange,
        rowVirtualizer,
      ],
    );

    const onKeyDown = useCallback(
      (e: React.KeyboardEvent<HTMLDivElement>) => {
        if (editingCell) {
          // Let the input handle its own keys.
          return;
        }
        if (confirm) {
          // Modal traps keys.
          return;
        }
        const meta = e.metaKey || e.ctrlKey;
        // Copy / cut / paste
        if (meta && !e.shiftKey && !e.altKey) {
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
            const firstRow = visibleRowIndices ? visibleRowIndices[0] ?? 0 : 0;
            const firstCol = visibleColumns[0]?.index ?? 0;
            onActiveCellChange({ row: firstRow, col: firstCol });
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
              col: visibleColumns[0]?.index ?? activeCell.col,
            });
            return;
          case "End":
            e.preventDefault();
            onActiveCellChange({
              row: activeCell.row,
              col:
                visibleColumns[visibleColumns.length - 1]?.index ??
                activeCell.col,
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
          case "Escape":
            // Escape clears any open context menu first.
            if (contextMenu) {
              e.preventDefault();
              setContextMenu(null);
            }
            return;
          case "Delete":
          case "Backspace":
            if (meta || e.shiftKey) {
              e.preventDefault();
              requestDeleteRows([activeCell.row]);
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
        confirm,
        activeCell,
        rowCount,
        visibleRowIndices,
        visibleColumns,
        moveActive,
        onActiveCellChange,
        beginEdit,
        onCopy,
        onCut,
        onPaste,
        onCellCommit,
        requestDeleteRows,
        contextMenu,
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

    // --- Context menu ---
    const onHeaderContextMenu = useCallback(
      (e: React.MouseEvent, col: ColumnMeta) => {
        e.preventDefault();
        setContextMenu({ x: e.clientX, y: e.clientY, kind: "header", index: col.index });
      },
      [],
    );

    const onRowIndexContextMenu = useCallback(
      (e: React.MouseEvent, displayRow: number) => {
        e.preventDefault();
        setContextMenu({
          x: e.clientX,
          y: e.clientY,
          kind: "rowindex",
          index: dataRowFor(displayRow),
        });
      },
      [dataRowFor],
    );

    // Dismiss the context menu on any outside click / scroll / escape.
    useEffect(() => {
      if (!contextMenu) return;
      const dismiss = () => setContextMenu(null);
      window.addEventListener("mousedown", dismiss);
      window.addEventListener("blur", dismiss);
      const scroller = scrollRef.current;
      scroller?.addEventListener("scroll", dismiss);
      return () => {
        window.removeEventListener("mousedown", dismiss);
        window.removeEventListener("blur", dismiss);
        scroller?.removeEventListener("scroll", dismiss);
      };
    }, [contextMenu]);

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

    // Render-helper: a single data row, given its display index.
    // Used both inside the virtualized scrollable area and the frozen band.
    function renderRow(
      displayIdx: number,
      opts: { absolute: boolean; topPx?: number; key: string | number },
    ) {
      const dataRow = dataRowFor(displayIdx);
      const row = cache.get(dataRow);
      const isHit = searchHitRows.has(dataRow);
      const isSelectedRow = activeCell != null && activeCell.row === dataRow;
      const isFrozenRow = displayIdx < effectiveFrozenRows;
      const thisRowHeight = heightFor(dataRow);
      const baseStyle: React.CSSProperties = opts.absolute
        ? {
            height: thisRowHeight,
            transform: `translateY(${opts.topPx}px)`,
            width: totalWidth,
            position: "absolute",
            top: 0,
            left: 0,
          }
        : {
            height: thisRowHeight,
            width: totalWidth,
            position: "relative",
          };
      return (
        <div
          key={opts.key}
          className={`grid-row ${isHit ? "hit" : ""} ${
            isSelectedRow ? "row-selected" : ""
          } ${isFrozenRow ? "frozen-row" : ""}`}
          role="row"
          data-testid={`row-${displayIdx}`}
          style={baseStyle}
        >
          <div
            className={`row-index-cell ${effectiveFrozenColumns > 0 || effectiveFrozenRows > 0 ? "sticky-left" : ""} ${isFrozenRow ? "frozen" : ""}`}
            style={{
              height: thisRowHeight,
              ...(effectiveFrozenColumns > 0 || effectiveFrozenRows > 0
                ? { position: "sticky", left: 0, zIndex: isFrozenRow ? 4 : 2 }
                : null),
            }}
            onContextMenu={(e) => onRowIndexContextMenu(e, displayIdx)}
            data-testid={`row-index-${displayIdx}`}
            title="Right-click for row options · drag bottom edge to resize"
          >
            {dataRow + 1}
            <div
              className={`row-resize-handle ${resizingRow === dataRow ? "dragging" : ""}`}
              onMouseDown={(e) => startRowResize(e, dataRow)}
              onDoubleClick={(e) => {
                e.stopPropagation();
                resetRowHeight(dataRow);
              }}
              title="Drag to resize row, double-click to reset"
              data-testid={`row-resize-${dataRow}`}
            />
          </div>
          {visibleColumns.map((col, visibleColIdx) => {
            const value = row ? (row[col.index] ?? "") : "";
            const numeric = numericKinds.has(col.kind);
            const isActive =
              activeCell != null &&
              activeCell.row === dataRow &&
              activeCell.col === col.index;
            const isEditingThis =
              editingCell != null &&
              editingCell.row === dataRow &&
              editingCell.col === col.index;
            const isFrozenCol = visibleColIdx < effectiveFrozenColumns;
            const stickyStyle: React.CSSProperties = isFrozenCol
              ? {
                  position: "sticky",
                  left: frozenColLefts[visibleColIdx],
                  zIndex: isFrozenRow ? 4 : 2,
                }
              : {};
            return (
              <div
                key={col.index}
                className={`grid-cell ${numeric ? "numeric" : ""} ${
                  row ? "" : "loading"
                } ${isActive ? "cell-active" : ""} ${isFrozenCol ? "frozen-col" : ""} ${isFrozenRow ? "frozen" : ""}`}
                role="cell"
                style={{
                  width: widthFor(col.index),
                  height: thisRowHeight,
                  ...stickyStyle,
                }}
                title={value}
                onClick={(e) => onCellClick(e, dataRow, col.index)}
                onDoubleClick={(e) => onCellDoubleClick(e, dataRow, col.index)}
                data-testid={`cell-${dataRow}-${col.index}`}
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
                    data-testid={`cell-edit-${dataRow}-${col.index}`}
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
    }

    const frozenRowTops = useMemo(() => {
      const tops: number[] = [];
      let acc = 0;
      for (let i = 0; i < effectiveFrozenRows; i++) {
        tops.push(acc);
        acc += heightFor(dataRowFor(i));
      }
      return { tops, total: acc };
    }, [effectiveFrozenRows, dataRowFor, heightFor]);
    const frozenRowsHeight = frozenRowTops.total;
    const totalHeight =
      rowVirtualizer.getTotalSize() + HEADER_HEIGHT + frozenRowsHeight;

    const hasHidden = hiddenRows.size > 0 || hiddenColumns.size > 0;

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
            height: totalHeight,
          }}
        >
          {/* --- Header row --- */}
          <div
            className="grid-header-row"
            style={{ width: totalWidth, height: HEADER_HEIGHT }}
            role="row"
          >
            <div
              className={`row-index-header ${effectiveFrozenColumns > 0 ? "sticky-left" : ""}`}
              style={{
                height: HEADER_HEIGHT,
                ...(effectiveFrozenColumns > 0
                  ? { position: "sticky", left: 0, zIndex: 6 }
                  : null),
              }}
              aria-label="Row number"
              title={
                hasHidden
                  ? `${hiddenRows.size} hidden row(s), ${hiddenColumns.size} hidden column(s) — click to show all`
                  : "#"
              }
              onClick={() => {
                if (hasHidden) showAllHidden();
              }}
              data-testid="row-index-header"
            >
              {hasHidden ? (
                <span className="hidden-pill" aria-label="Hidden items">
                  {hiddenRows.size + hiddenColumns.size}
                </span>
              ) : (
                "#"
              )}
            </div>
            {visibleColumns.map((col, visibleColIdx) => {
              const sortLabel = sortLabelFor(col.index);
              const isSelected = selectedColumn === col.index;
              const isFrozenCol = visibleColIdx < effectiveFrozenColumns;
              const stickyStyle: React.CSSProperties = isFrozenCol
                ? {
                    position: "sticky",
                    left: frozenColLefts[visibleColIdx],
                    zIndex: 5,
                  }
                : {};
              return (
                <div
                  key={col.index}
                  className={`grid-header-cell ${isFrozenCol ? "frozen-col" : ""}`}
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
                    ...stickyStyle,
                  }}
                  onClick={(e) => onHeaderClick(e, col)}
                  onContextMenu={(e) => onHeaderContextMenu(e, col)}
                  data-testid={`header-${col.index}`}
                  title="Right-click for column options"
                >
                  <span className={`kind ${col.kind}`}>{col.kind.slice(0, 3)}</span>
                  <span className="name">{col.name}</span>
                  <span className="sort">{sortLabel}</span>
                  <div
                    className={`resize-handle ${resizing === col.index ? "dragging" : ""}`}
                    onMouseDown={(e) => startResize(e, col.index)}
                    onDoubleClick={(e) => {
                      e.stopPropagation();
                      autoSizeColumn(col.index);
                    }}
                    title="Drag to resize, double-click to auto-size"
                    data-testid={`resize-${col.index}`}
                  />
                </div>
              );
            })}
          </div>

          {/* --- Frozen rows band --- */}
          {effectiveFrozenRows > 0 && (
            <div
              className="grid-frozen-band"
              style={{
                position: "sticky",
                top: HEADER_HEIGHT,
                zIndex: 3,
                width: totalWidth,
                height: frozenRowsHeight,
                marginLeft: 0,
              }}
              data-testid="frozen-band"
            >
              {Array.from({ length: effectiveFrozenRows }).map((_, i) =>
                renderRow(i, {
                  absolute: true,
                  topPx: frozenRowTops.tops[i],
                  key: `frozen-${i}`,
                }),
              )}
            </div>
          )}

          {/* --- Virtualized rows (scrolling region) --- */}
          {virtualItems.map((virtualRow) => {
            // Skip rows that are already in the frozen band — they'd render
            // twice and cause a brief overlap as you scroll near the top.
            if (virtualRow.index < effectiveFrozenRows) return null;
            return renderRow(virtualRow.index, {
              absolute: true,
              topPx: virtualRow.start + HEADER_HEIGHT + frozenRowsHeight,
              key: String(virtualRow.key),
            });
          })}

          {/* --- Sticky shadow under the frozen-cols column band --- */}
          {effectiveFrozenColumns > 0 && (
            <div
              className="frozen-cols-shadow"
              style={{
                position: "sticky",
                top: HEADER_HEIGHT + frozenRowsHeight,
                left: frozenColumnsRightEdge,
                width: 6,
                height: 0,
                zIndex: 1,
                pointerEvents: "none",
              }}
              aria-hidden
            />
          )}
        </div>

        {/* --- Context menu --- */}
        {contextMenu && (
          <ContextMenu
            state={contextMenu}
            columns={columns}
            visibleColumns={visibleColumns}
            visibleRowIndices={visibleRowIndices}
            rowCount={rowCount}
            displayRowCount={displayRowCount}
            frozenRows={frozenRows}
            frozenColumns={frozenColumns}
            hasHidden={hasHidden}
            hasDeleteColumn={!!onDeleteColumn}
            onClose={() => setContextMenu(null)}
            actions={{
              autoSize: autoSizeColumn,
              autoSizeAll: autoSizeAllColumns,
              hideColumn,
              hideRow,
              showAll: showAllHidden,
              freezeRowsThrough: (displayIdx) => setFrozenRows(displayIdx + 1),
              freezeColumnsThrough: (visibleColIdx) =>
                setFrozenColumns(visibleColIdx + 1),
              unfreezeRows: () => setFrozenRowsState(0),
              unfreezeColumns: () => setFrozenColumnsState(0),
              deleteRow: (dataRow) => requestDeleteRows([dataRow]),
              deleteColumn: requestDeleteColumn,
              sortAsc: (col) =>
                onSortChange([{ column: col, direction: "asc" }]),
              sortDesc: (col) =>
                onSortChange([{ column: col, direction: "desc" }]),
              clearSort: () => onSortChange([]),
            }}
          />
        )}

        {/* --- Confirm modal --- */}
        {confirm && (
          <ConfirmModal
            state={confirm}
            onCancel={() => setConfirm(null)}
            onConfirm={() => {
              const action = confirm.onConfirm;
              setConfirm(null);
              action();
            }}
          />
        )}
      </div>
    );
  },
);

// ---------------------------------------------------------------------------
// Subcomponents
// ---------------------------------------------------------------------------

interface ContextMenuActions {
  autoSize: (columnIndex: number) => void;
  autoSizeAll: () => void;
  hideColumn: (columnIndex: number) => void;
  hideRow: (rowIndex: number) => void;
  showAll: () => void;
  freezeRowsThrough: (displayRowIdx: number) => void;
  freezeColumnsThrough: (visibleColIdx: number) => void;
  unfreezeRows: () => void;
  unfreezeColumns: () => void;
  deleteRow: (dataRowIndex: number) => void;
  deleteColumn: (columnIndex: number) => void;
  sortAsc: (columnIndex: number) => void;
  sortDesc: (columnIndex: number) => void;
  clearSort: () => void;
}

interface ContextMenuProps {
  state: ContextMenuState;
  columns: ColumnMeta[];
  visibleColumns: ColumnMeta[];
  visibleRowIndices: number[] | null;
  rowCount: number;
  displayRowCount: number;
  frozenRows: number;
  frozenColumns: number;
  hasHidden: boolean;
  hasDeleteColumn: boolean;
  onClose: () => void;
  actions: ContextMenuActions;
}

function ContextMenu(props: ContextMenuProps) {
  const {
    state,
    columns,
    visibleColumns,
    visibleRowIndices,
    rowCount,
    displayRowCount,
    frozenRows,
    frozenColumns,
    hasHidden,
    hasDeleteColumn,
    onClose,
    actions,
  } = props;
  // Pin the menu inside the viewport.
  const [coords, setCoords] = useState({ x: state.x, y: state.y });
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let x = state.x;
    let y = state.y;
    if (x + rect.width > vw - 8) x = Math.max(8, vw - rect.width - 8);
    if (y + rect.height > vh - 8) y = Math.max(8, vh - rect.height - 8);
    if (x !== coords.x || y !== coords.y) setCoords({ x, y });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.x, state.y]);

  // Stop propagation so the global mousedown dismiss handler doesn't fire.
  const stop = (e: React.MouseEvent) => e.stopPropagation();

  const items: React.ReactNode[] = [];

  if (state.kind === "header") {
    const colIndex = state.index;
    const visibleColIdx = visibleColumns.findIndex((c) => c.index === colIndex);
    const col = columns.find((c) => c.index === colIndex);
    items.push(
      <div className="ctxmenu-section" key="hdr">
        <div className="ctxmenu-title">{col?.name ?? `Column ${colIndex + 1}`}</div>
      </div>,
      <button
        key="sort-asc"
        className="ctxmenu-item"
        onClick={() => {
          actions.sortAsc(colIndex);
          onClose();
        }}
      >
        Sort ascending
      </button>,
      <button
        key="sort-desc"
        className="ctxmenu-item"
        onClick={() => {
          actions.sortDesc(colIndex);
          onClose();
        }}
      >
        Sort descending
      </button>,
      <button
        key="clear-sort"
        className="ctxmenu-item"
        onClick={() => {
          actions.clearSort();
          onClose();
        }}
      >
        Clear sort
      </button>,
      <div className="ctxmenu-divider" key="d1" />,
      <button
        key="autosize"
        className="ctxmenu-item"
        onClick={() => {
          actions.autoSize(colIndex);
          onClose();
        }}
      >
        Auto-size column
      </button>,
      <button
        key="autosize-all"
        className="ctxmenu-item"
        onClick={() => {
          actions.autoSizeAll();
          onClose();
        }}
      >
        Auto-size all columns
      </button>,
      <div className="ctxmenu-divider" key="d2" />,
    );
    if (visibleColIdx >= 0) {
      items.push(
        <button
          key="freeze-thru"
          className="ctxmenu-item"
          onClick={() => {
            actions.freezeColumnsThrough(visibleColIdx);
            onClose();
          }}
        >
          Freeze columns through here
          <kbd className="ctxmenu-kbd">{visibleColIdx + 1}</kbd>
        </button>,
      );
    }
    if (frozenColumns > 0) {
      items.push(
        <button
          key="unfreeze-cols"
          className="ctxmenu-item"
          onClick={() => {
            actions.unfreezeColumns();
            onClose();
          }}
        >
          Unfreeze columns
        </button>,
      );
    }
    items.push(
      <div className="ctxmenu-divider" key="d3" />,
      <button
        key="hide-col"
        className="ctxmenu-item"
        disabled={visibleColumns.length <= 1}
        onClick={() => {
          actions.hideColumn(colIndex);
          onClose();
        }}
      >
        Hide column
      </button>,
    );
    if (hasHidden) {
      items.push(
        <button
          key="show-all"
          className="ctxmenu-item"
          onClick={() => {
            actions.showAll();
            onClose();
          }}
        >
          Show all hidden
        </button>,
      );
    }
    if (hasDeleteColumn) {
      items.push(
        <div className="ctxmenu-divider" key="d4" />,
        <button
          key="delete-col"
          className="ctxmenu-item destructive"
          disabled={visibleColumns.length <= 1}
          onClick={() => {
            actions.deleteColumn(colIndex);
            onClose();
          }}
        >
          Delete column…
        </button>,
      );
    }
  } else {
    const dataRow = state.index;
    const displayIdx = visibleRowIndices
      ? visibleRowIndices.indexOf(dataRow)
      : dataRow;
    items.push(
      <div className="ctxmenu-section" key="hdr">
        <div className="ctxmenu-title">Row {dataRow + 1}</div>
      </div>,
    );
    if (displayIdx >= 0) {
      items.push(
        <button
          key="freeze-rows"
          className="ctxmenu-item"
          onClick={() => {
            actions.freezeRowsThrough(displayIdx);
            onClose();
          }}
        >
          Freeze rows through here
          <kbd className="ctxmenu-kbd">{displayIdx + 1}</kbd>
        </button>,
      );
    }
    if (frozenRows > 0) {
      items.push(
        <button
          key="unfreeze-rows"
          className="ctxmenu-item"
          onClick={() => {
            actions.unfreezeRows();
            onClose();
          }}
        >
          Unfreeze rows
        </button>,
      );
    }
    items.push(
      <div className="ctxmenu-divider" key="d1" />,
      <button
        key="hide-row"
        className="ctxmenu-item"
        disabled={displayRowCount <= 1}
        onClick={() => {
          actions.hideRow(dataRow);
          onClose();
        }}
      >
        Hide row
      </button>,
    );
    if (hasHidden) {
      items.push(
        <button
          key="show-all"
          className="ctxmenu-item"
          onClick={() => {
            actions.showAll();
            onClose();
          }}
        >
          Show all hidden
        </button>,
      );
    }
    items.push(
      <div className="ctxmenu-divider" key="d2" />,
      <button
        key="delete-row"
        className="ctxmenu-item destructive"
        disabled={rowCount <= 1}
        onClick={() => {
          actions.deleteRow(dataRow);
          onClose();
        }}
      >
        Delete row…
      </button>,
    );
  }

  return (
    <div
      ref={ref}
      className="ctxmenu"
      style={{ position: "fixed", left: coords.x, top: coords.y }}
      onMouseDown={stop}
      onClick={stop}
      role="menu"
      data-testid="ctxmenu"
    >
      {items}
    </div>
  );
}

interface ConfirmModalProps {
  state: ConfirmState;
  onCancel: () => void;
  onConfirm: () => void;
}

function ConfirmModal({ state, onCancel, onConfirm }: ConfirmModalProps) {
  const confirmRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    confirmRef.current?.focus();
  }, []);
  return (
    <div
      className="confirm-backdrop"
      role="dialog"
      aria-modal="true"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      data-testid="confirm-modal"
    >
      <div className="confirm-modal" onMouseDown={(e) => e.stopPropagation()}>
        <p className="confirm-message">{state.message}</p>
        {state.detail && <p className="confirm-detail">{state.detail}</p>}
        <div className="confirm-buttons">
          <button onClick={onCancel} data-testid="confirm-cancel">
            Cancel
          </button>
          <button
            ref={confirmRef}
            className={state.destructive ? "danger" : "primary"}
            onClick={onConfirm}
            data-testid="confirm-ok"
          >
            {state.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
