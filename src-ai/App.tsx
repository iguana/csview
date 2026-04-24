import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  readText as readClipboardText,
  writeText as writeClipboardText,
} from "@tauri-apps/plugin-clipboard-manager";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Shared from base csview
import { useTheme } from "../src/lib/theme";
import { RangeCache } from "../src/lib/rangeCache";
import { HistoryStack } from "../src/lib/history";
import { decodeTsv, encodeTsv } from "../src/lib/clipboard";
import { basename, formatBytes, formatCount } from "../src/lib/format";
import { isPaletteId } from "../src/lib/palettes";
import { DataGrid, type CellCoord, type DataGridHandle } from "../src/components/DataGrid";
import { ThemeMenu } from "../src/components/ThemeMenu";

// AI-specific
import { csvApi, aiApi } from "./lib/api-ai";
import type { AccountStatus, FileInfo, TransformResult } from "./lib/types-ai";
import { AISidebar, type AiTab } from "./components/AISidebar";

const AI_APP_NAME = "csviewai";
const ROW_HEIGHT_DEFAULT = 28;

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}

function SaveIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M3 2h9l2 2v10a.5.5 0 0 1-.5.5h-11A.5.5 0 0 1 2 14V2.5A.5.5 0 0 1 2.5 2H3z" />
      <path d="M4 2v4h7V2" />
      <rect x="4" y="9" width="8" height="5" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// App component
// ---------------------------------------------------------------------------

export function App() {
  const theme = useTheme();

  // --- File / data state ---
  const [fileInfo, setFileInfo] = useState<FileInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [sortKey, setSortKey] = useState<string | null>(null);
  const [rowHeight, setRowHeight] = useState(ROW_HEIGHT_DEFAULT);
  const [cacheVersion, setCacheVersion] = useState(0);
  const [jumpToRow, setJumpToRow] = useState<number | null>(null);
  const [activeCell, setActiveCell] = useState<CellCoord | null>(null);
  const [historyTick, setHistoryTick] = useState(0);

  // --- Sidebar state ---
  const [sidebarWidth, setSidebarWidth] = useState(380);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  // --- AI state ---
  const [aiSidebarTab, setAiSidebarTab] = useState<AiTab>("chat");
  const [apiKeySet, setApiKeySet] = useState(false);
  const [isAiProcessing, setIsAiProcessing] = useState(false);
  const [aiQuery, setAiQuery] = useState("");

  // Refs
  const cacheRef = useRef<RangeCache | null>(null);
  const gridRef = useRef<DataGridHandle>(null);
  const historyRef = useRef(new HistoryStack(500));
  const fileInfoRef = useRef<FileInfo | null>(null);

  useEffect(() => {
    fileInfoRef.current = fileInfo;
  }, [fileInfo]);

  const bumpHistory = useCallback(() => setHistoryTick((t) => t + 1), []);

  // Check for existing API key on mount
  useEffect(() => {
    aiApi.getAccountStatus().then((s) => {
      setApiKeySet(s.hasApiKey);
    }).catch(() => {
      // backend may not be running in dev
    });
  }, []);

  // --- Cache management ---
  const ensureCache = useCallback((fileId: string): RangeCache => {
    const cache = new RangeCache(
      async (start, end) => {
        const result = await csvApi.readRange(fileId, start, end - start);
        // csvApi returns QueryResult; rows are arrays of mixed types — convert to strings
        return result.rows.map((row) =>
          row.map((cell) => (cell == null ? "" : String(cell))),
        );
      },
      { pageSize: 200, maxPages: 128 },
    );
    cacheRef.current = cache;
    return cache;
  }, []);

  const cache = cacheRef.current;

  // --- Window title ---
  useEffect(() => {
    (async () => {
      const win = getCurrentWindow();
      if (!fileInfo) {
        await win.setTitle(AI_APP_NAME);
        return;
      }
      await win.setTitle(`${basename(fileInfo.path)} — ${AI_APP_NAME}`);
    })().catch(() => {
      // setTitle may fail in tests without a real window
    });
  }, [fileInfo]);

  // --- Open file ---
  const openFile = useCallback(
    async (path: string) => {
      setError(null);
      setLoading(true);
      try {
        const info = await csvApi.openCsv(path);
        setFileInfo(info);
        setSortKey(null);
        setActiveCell(null);
        historyRef.current.clear();
        bumpHistory();
        const c = ensureCache(info.fileId);
        const initialCount = Math.min(info.rowCount, c.pageSize);
        if (initialCount > 0) {
          await c.ensure(0, initialCount);
        }
        setCacheVersion((v) => v + 1);
      } catch (e) {
        setError(errMsg(e));
      } finally {
        setLoading(false);
      }
    },
    [ensureCache, bumpHistory],
  );

  const onOpenClick = useCallback(async () => {
    try {
      const selection = await openDialog({
        multiple: false,
        filters: [
          { name: "CSV / TSV", extensions: ["csv", "tsv", "txt"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof selection !== "string") return;
      await openFile(selection);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [openFile]);

  // --- Tauri event listeners ---
  useEffect(() => {
    const offFile = listen<string>("cli-open-file", async (event) => {
      if (event.payload) await openFile(event.payload);
    });
    const offTheme = listen<string>("csview-theme", (event) => {
      const v = event.payload;
      if (v === "light" || v === "dark" || v === "system") {
        theme.setMode(v);
      }
    });
    const offDemo = listen<{
      sort?: string;
      palette?: string;
    }>("csviewai-demo", (event) => {
      setTimeout(() => {
        if (event.payload.sort) setSortKey(event.payload.sort);
        if (event.payload.palette && isPaletteId(event.payload.palette)) {
          theme.setPalette(event.payload.palette);
        }
      }, 200);
    });
    return () => {
      void offFile.then((fn) => fn());
      void offTheme.then((fn) => fn());
      void offDemo.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openFile]);

  // --- Build ColumnMeta for DataGrid from FileInfo ---
  const columns = useMemo(() => {
    if (!fileInfo) return [];
    return fileInfo.columns.map((col) => ({
      index: col.index,
      name: col.name,
      kind: col.kind as import("../src/lib/types").ColumnKind,
    }));
  }, [fileInfo]);

  // --- Reading cells (for editing) ---
  const readCell = useCallback(async (row: number, col: number): Promise<string> => {
    const info = fileInfoRef.current;
    if (!info) return "";
    const c = cacheRef.current;
    const cached = c?.get(row);
    if (cached) return cached[col] ?? "";
    const result = await csvApi.readRange(info.fileId, row, 1);
    return result.rows[0]?.[col] != null ? String(result.rows[0][col]) : "";
  }, []);

  // --- Cell commit ---
  const applyCellUpdate = useCallback(
    async (row: number, col: number, value: string): Promise<void> => {
      const info = fileInfoRef.current;
      if (!info) return;
      const colName = info.columns[col]?.name ?? String(col);
      // rowid: use row index as rowid (1-based in SQLite)
      await csvApi.updateCell(info.fileId, row + 1, colName, value);
      cacheRef.current?.invalidate();
      setCacheVersion((v) => v + 1);
    },
    [],
  );

  const onCellCommit = useCallback(
    async (row: number, col: number, value: string) => {
      try {
        const previous = await readCell(row, col);
        if (previous === value) return;
        await applyCellUpdate(row, col, value);
        historyRef.current.push({
          label: "Edit cell",
          undo: async () => { await applyCellUpdate(row, col, previous); },
          redo: async () => { await applyCellUpdate(row, col, value); },
        });
        bumpHistory();
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [readCell, applyCellUpdate, bumpHistory],
  );

  // --- Delete rows ---
  const onDeleteRows = useCallback(
    async (rows: number[]) => {
      const info = fileInfoRef.current;
      if (!info) return;
      try {
        const rowids = rows.map((r) => r + 1); // 1-based
        await csvApi.deleteRows(info.fileId, rowids);
        cacheRef.current?.invalidate();
        setCacheVersion((v) => v + 1);
        const updatedInfo = await csvApi.openCsv(info.path);
        setFileInfo(updatedInfo);
        historyRef.current.push({
          label: `Delete ${rows.length} row(s)`,
          undo: async () => {
            // Re-open file to pick up restored state (undo not supported for deletes yet)
            const refreshed = await csvApi.openCsv(info.path);
            setFileInfo(refreshed);
            cacheRef.current?.invalidate();
            setCacheVersion((v) => v + 1);
          },
          redo: async () => {
            await csvApi.deleteRows(info.fileId, rowids);
            cacheRef.current?.invalidate();
            setCacheVersion((v) => v + 1);
          },
        });
        bumpHistory();
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [bumpHistory],
  );

  // --- Delete column ---
  const onDeleteColumn = useCallback(
    async (column: number) => {
      const info = fileInfoRef.current;
      if (!info) return;
      try {
        await csvApi.deleteColumn(info.fileId, column);
        // Re-open to refresh the schema (column list is part of FileInfo).
        const refreshed = await csvApi.openCsv(info.path);
        setFileInfo(refreshed);
        cacheRef.current?.invalidate();
        setCacheVersion((v) => v + 1);
        setActiveCell((c) =>
          c && c.col >= refreshed.columns.length
            ? { row: c.row, col: Math.max(0, refreshed.columns.length - 1) }
            : c,
        );
        // SQLite-backed undo just re-opens the file (matches delete-rows
        // semantics). Push a minimal history entry so users see the change.
        historyRef.current.push({
          label: `Delete column`,
          undo: async () => {
            const r = await csvApi.openCsv(info.path);
            setFileInfo(r);
            cacheRef.current?.invalidate();
            setCacheVersion((v) => v + 1);
          },
          redo: async () => {
            await csvApi.deleteColumn(info.fileId, column);
            const r = await csvApi.openCsv(info.path);
            setFileInfo(r);
            cacheRef.current?.invalidate();
            setCacheVersion((v) => v + 1);
          },
        });
        bumpHistory();
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [bumpHistory],
  );

  // --- Save ---
  const doSave = useCallback(async () => {
    const info = fileInfoRef.current;
    if (!info) return;
    try {
      await csvApi.saveCsv(info.fileId);
    } catch (e) {
      setError(errMsg(e));
    }
  }, []);

  const doSaveAs = useCallback(async () => {
    const info = fileInfoRef.current;
    if (!info) return;
    try {
      const target = await saveDialog({
        title: "Save CSV as",
        defaultPath: info.path,
        filters: [
          { name: "CSV", extensions: ["csv"] },
          { name: "TSV", extensions: ["tsv"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (!target) return;
      await csvApi.saveCsvAs(info.fileId, target);
    } catch (e) {
      setError(errMsg(e));
    }
  }, []);

  // --- Copy / Cut / Paste ---
  const onCopy = useCallback(async (cells: CellCoord[]) => {
    if (cells.length === 0) return;
    try {
      const values = await Promise.all(
        cells.map(async (c) => [await readCell(c.row, c.col)]),
      );
      await writeClipboardText(encodeTsv(values));
    } catch (e) {
      setError(errMsg(e));
    }
  }, [readCell]);

  const onCut = useCallback(
    async (cells: CellCoord[]) => {
      if (cells.length === 0) return;
      try {
        const values = await Promise.all(
          cells.map(async (c) => [await readCell(c.row, c.col)]),
        );
        await writeClipboardText(encodeTsv(values));
        for (const c of cells) {
          await onCellCommit(c.row, c.col, "");
        }
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [readCell, onCellCommit],
  );

  const onPaste = useCallback(async () => {
    if (!activeCell) return;
    const info = fileInfoRef.current;
    if (!info) return;
    try {
      const text = (await readClipboardText()) ?? "";
      const grid = decodeTsv(text);
      if (grid.length === 0) return;
      const changes: { row: number; col: number; value: string; prev: string }[] = [];
      for (let dr = 0; dr < grid.length; dr++) {
        for (let dc = 0; dc < grid[dr].length; dc++) {
          const r = activeCell.row + dr;
          const c = activeCell.col + dc;
          if (r >= info.rowCount || c >= info.columns.length) continue;
          const prev = await readCell(r, c);
          changes.push({ row: r, col: c, value: grid[dr][dc], prev });
        }
      }
      for (const ch of changes) {
        await applyCellUpdate(ch.row, ch.col, ch.value);
      }
      historyRef.current.push({
        label: "Paste",
        undo: async () => {
          for (const ch of changes) {
            await applyCellUpdate(ch.row, ch.col, ch.prev);
          }
        },
        redo: async () => {
          for (const ch of changes) {
            await applyCellUpdate(ch.row, ch.col, ch.value);
          }
        },
      });
      bumpHistory();
    } catch (e) {
      setError(errMsg(e));
    }
  }, [activeCell, readCell, applyCellUpdate, bumpHistory]);

  // --- Undo / Redo ---
  const doUndo = useCallback(async () => {
    await historyRef.current.undo();
    bumpHistory();
  }, [bumpHistory]);
  const doRedo = useCallback(async () => {
    await historyRef.current.redo();
    bumpHistory();
  }, [bumpHistory]);

  // --- Menu events from the native menu bar ---
  // Display-affecting actions (freeze, hide, autosize) dispatch into the
  // grid imperatively via gridRef. Backend-affecting actions (delete row /
  // column) route through the App so the modal + history stack get involved.
  useEffect(() => {
    const off = listen<string>("menu-action", (event) => {
      const id = event.payload;
      switch (id) {
        case "save":
          void doSave();
          return;
        case "save_as":
          void doSaveAs();
          return;
        case "undo":
          void doUndo();
          return;
        case "redo":
          void doRedo();
          return;
        case "delete_row":
          if (activeCell) gridRef.current?.requestDeleteRows([activeCell.row]);
          return;
        case "delete_column":
          if (activeCell != null)
            gridRef.current?.requestDeleteColumn(activeCell.col);
          return;
        case "freeze_rows_to_cursor":
          if (activeCell != null)
            gridRef.current?.setFrozenRows(activeCell.row + 1);
          return;
        case "freeze_columns_to_cursor":
          if (activeCell != null)
            gridRef.current?.setFrozenColumns(activeCell.col + 1);
          return;
        case "unfreeze_all":
          gridRef.current?.unfreezeAll();
          return;
        case "hide_row":
          if (activeCell != null) gridRef.current?.hideRow(activeCell.row);
          return;
        case "hide_column":
          if (activeCell != null) gridRef.current?.hideColumn(activeCell.col);
          return;
        case "show_all_hidden":
          gridRef.current?.showAllHidden();
          return;
        case "autosize_column":
          if (activeCell != null)
            gridRef.current?.autoSizeColumn(activeCell.col);
          return;
        case "autosize_all_columns":
          gridRef.current?.autoSizeAllColumns();
          return;
      }
    });
    return () => {
      void off.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doSave, doSaveAs, doUndo, doRedo, activeCell]);

  // --- Sidebar resize ---
  const startSidebarResize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startWidth = sidebarWidth;
      const onMove = (ev: MouseEvent) => {
        const delta = startX - ev.clientX;
        const next = Math.max(280, Math.min(600, startWidth + delta));
        setSidebarWidth(next);
      };
      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [sidebarWidth],
  );

  // --- AI event handlers ---
  const handleAccountStatus = useCallback((status: AccountStatus) => {
    setApiKeySet(status.hasApiKey);
  }, []);

  const handleJumpToRow = useCallback((row: number) => {
    setJumpToRow(row);
    // Clear jump after one tick so re-using the same row re-triggers
    setTimeout(() => setJumpToRow(null), 100);
  }, []);

  const handleApplyFilter = useCallback((_sql: string) => {
    // In the AI app the filter is applied by re-running query_data.
    // Switching to query tab lets user see results there.
    setAiSidebarTab("query");
  }, []);

  const handleTransformApply = useCallback(
    async (result: TransformResult, columnName: string) => {
      const info = fileInfoRef.current;
      if (!info) return;
      setIsAiProcessing(true);
      try {
        // Execute the transform as a SQL query to add the derived column
        await csvApi.queryData(
          info.fileId,
          `ALTER TABLE "${info.tableName}" ADD COLUMN "${columnName}" TEXT GENERATED ALWAYS AS (${result.expression})`,
        );
        // Refresh file info
        const refreshed = await csvApi.openCsv(info.path);
        setFileInfo(refreshed);
        cacheRef.current?.invalidate();
        setCacheVersion((v) => v + 1);
      } catch (e) {
        setError(errMsg(e));
      } finally {
        setIsAiProcessing(false);
      }
    },
    [],
  );

  const handleJoinComplete = useCallback((result: FileInfo) => {
    setFileInfo(result);
    const c = ensureCache(result.fileId);
    void c.ensure(0, Math.min(result.rowCount, c.pageSize));
    setCacheVersion((v) => v + 1);
  }, [ensureCache]);

  // --- AI top bar quick query ---
  const handleAiQuerySubmit = useCallback(() => {
    if (!aiQuery.trim()) return;
    setAiSidebarTab("query");
    if (!sidebarCollapsed) return;
    setSidebarCollapsed(false);
  }, [aiQuery, sidebarCollapsed]);

  // --- Global keyboard shortcuts ---
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (!meta) return;
      const key = e.key.toLowerCase();
      if (key === "b") {
        e.preventDefault();
        setSidebarCollapsed((v) => !v);
      }
      if (key === "z") {
        e.preventDefault();
        if (e.shiftKey) void doRedo();
        else void doUndo();
      }
      if (key === "s") {
        e.preventDefault();
        void doSave();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [doUndo, doRedo, doSave]);

  const history = historyRef.current;
  const canUndo = history.undoable;
  const canRedo = history.redoable;
  void historyTick;

  // ---------------------------------------------------------------------------
  // SortKeys adapter: DataGrid expects SortKey[] with column indices.
  // The AI backend uses column name strings.
  // We handle a single sort for now; multi-sort can be added later.
  // ---------------------------------------------------------------------------
  const sortKeys = useMemo(() => {
    if (!sortKey || !fileInfo) return [];
    const dir = sortKey.startsWith("-") ? "desc" : "asc";
    const colName = sortKey.startsWith("-") ? sortKey.slice(1) : sortKey;
    const colIdx = fileInfo.columns.findIndex((c) => c.name === colName);
    if (colIdx < 0) return [];
    return [{ column: colIdx, direction: dir as "asc" | "desc" }];
  }, [sortKey, fileInfo]);

  const onSortChange = useCallback(
    (keys: import("../src/lib/types").SortKey[]) => {
      if (!fileInfo) return;
      if (keys.length === 0) {
        setSortKey(null);
        cacheRef.current?.invalidate();
        setCacheVersion((v) => v + 1);
        return;
      }
      const k = keys[keys.length - 1];
      const colName = fileInfo.columns[k.column]?.name ?? "";
      const newKey = k.direction === "desc" ? `-${colName}` : colName;
      setSortKey(newKey);
      // Re-sort via query — refresh cache after sort
      const orderBy = `"${colName}" ${k.direction.toUpperCase()}`;
      csvApi.readRange(fileInfo.fileId, 0, 0, orderBy).then(() => {
        cacheRef.current?.invalidate();
        setCacheVersion((v) => v + 1);
      }).catch((e: unknown) => setError(errMsg(e)));
    },
    [fileInfo],
  );

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div className="app" data-testid="app">
      {/* ------------------------------------------------------------------ */}
      {/* Titlebar                                                            */}
      {/* ------------------------------------------------------------------ */}
      <div className="titlebar" data-testid="titlebar">
        <div className="titlebar-title">
          <span className="app-name">{AI_APP_NAME}</span>
          <span className="ai-badge">AI</span>
          {fileInfo && (
            <>
              <span className="title-sep" aria-hidden>·</span>
              <span className="filename">{basename(fileInfo.path)}</span>
            </>
          )}
        </div>
        {fileInfo && (
          <div className="titlebar-stats" data-testid="titlebar-stats">
            <span className="stat-pill">
              <span className="stat-num">{formatCount(fileInfo.rowCount)}</span>
              <span className="stat-unit">rows</span>
            </span>
            <span className="stat-pill">
              <span className="stat-num">{fileInfo.columns.length}</span>
              <span className="stat-unit">cols</span>
            </span>
          </div>
        )}
        <div className="spacer" />
        {fileInfo && (
          <>
            <button
              className="iconbtn save-btn"
              onClick={() => void doSave()}
              title="Save (⌘S)"
              aria-label="Save"
              data-testid="titlebar-save"
            >
              <SaveIcon />
              Save
            </button>
            <button
              className="iconbtn"
              onClick={() => void doSaveAs()}
              title="Save As…"
              aria-label="Save As"
            >
              Save As…
            </button>
          </>
        )}
        <ThemeMenu theme={theme} />
        <button
          className="iconbtn"
          onClick={() => setSidebarCollapsed((v) => !v)}
          title={sidebarCollapsed ? "Show AI sidebar (⌘B)" : "Hide AI sidebar (⌘B)"}
          aria-label="Toggle AI sidebar"
        >
          {sidebarCollapsed ? "⇤" : "⇥"}
        </button>
        <button onClick={() => void onOpenClick()} disabled={loading} title="Open CSV… (⌘O)">
          Open…
        </button>
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Toolbar                                                             */}
      {/* ------------------------------------------------------------------ */}
      {fileInfo && (
        <div className="toolbar" data-testid="toolbar">
          <label className="toggle">
            Row height
            <input
              type="range"
              min={20}
              max={56}
              step={2}
              value={rowHeight}
              onChange={(e) => setRowHeight(Number(e.target.value))}
              aria-label="Row height"
            />
            <span style={{ color: "var(--text)" }}>{rowHeight}px</span>
          </label>
          <div className="spacer" />
          <button
            onClick={() => void doUndo()}
            disabled={!canUndo}
            title="Undo (⌘Z)"
            data-testid="undo-btn"
          >
            ↶ Undo
          </button>
          <button
            onClick={() => void doRedo()}
            disabled={!canRedo}
            title="Redo (⇧⌘Z)"
            data-testid="redo-btn"
          >
            ↷ Redo
          </button>
          {sortKeys.length > 0 && (
            <button onClick={() => { setSortKey(null); cacheRef.current?.invalidate(); setCacheVersion((v) => v + 1); }}>
              Clear sort
            </button>
          )}
          {/* AI quick-query bar */}
          <div className="search ai-query-input">
            <span className="ai-query-icon">✦</span>
            <input
              type="text"
              placeholder="Ask AI about this data…"
              value={aiQuery}
              onChange={(e) => setAiQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleAiQuerySubmit();
              }}
              aria-label="AI query"
              data-testid="ai-query-input"
            />
          </div>
          <button
            className={apiKeySet ? "primary" : ""}
            onClick={() => {
              handleAiQuerySubmit();
              if (!apiKeySet) setAiSidebarTab("settings");
              if (sidebarCollapsed) setSidebarCollapsed(false);
            }}
            disabled={isAiProcessing}
            title={apiKeySet ? "Run AI query" : "Configure API key to use AI features"}
          >
            {isAiProcessing ? "…" : "Ask AI"}
          </button>
        </div>
      )}

      {/* ------------------------------------------------------------------ */}
      {/* Error banner                                                        */}
      {/* ------------------------------------------------------------------ */}
      {error && (
        <div className="error-banner">
          {error}
          <button
            className="error-dismiss"
            onClick={() => setError(null)}
            aria-label="Dismiss error"
          >
            ×
          </button>
        </div>
      )}

      {/* ------------------------------------------------------------------ */}
      {/* Main content                                                        */}
      {/* ------------------------------------------------------------------ */}
      <div className="main">
        {!fileInfo ? (
          <div className="welcome">
            <div className="logo">
              {AI_APP_NAME.slice(0, 1).toUpperCase()}
            </div>
            <h1>{AI_APP_NAME}</h1>
            <p>
              The AI-powered CSV viewer. Open any CSV and use natural language to
              query, transform, and analyse your data with Claude.
            </p>
            <button className="primary" onClick={() => void onOpenClick()}>
              Open CSV…
            </button>
            {!apiKeySet && (
              <p style={{ fontSize: "12px", color: "var(--text-dim)", marginTop: 8 }}>
                Set your Anthropic API key in Settings to enable AI features.
              </p>
            )}
            <div className="welcome-shortcuts">
              <span><kbd>⌘O</kbd> Open</span>
              <span><kbd>⌘B</kbd> Toggle AI sidebar</span>
            </div>
          </div>
        ) : (
          <div className="grid-container">
            {cache && (
              <DataGrid
                ref={gridRef}
                columns={columns}
                rowCount={fileInfo.rowCount}
                sortKeys={sortKeys}
                onSortChange={onSortChange}
                cache={cache}
                cacheVersion={cacheVersion}
                searchHitRows={new Set<number>()}
                highlightQuery=""
                onSelectColumn={() => undefined}
                selectedColumn={null}
                activeCell={activeCell}
                onActiveCellChange={setActiveCell}
                onCellCommit={(row, col, value) => void onCellCommit(row, col, value)}
                onCopy={(cells) => void onCopy(cells)}
                onCut={(cells) => void onCut(cells)}
                onPaste={() => void onPaste()}
                onDeleteRows={(rows) => void onDeleteRows(rows)}
                onDeleteColumn={(col) => void onDeleteColumn(col)}
                rowHeight={rowHeight}
                jumpToRow={jumpToRow}
              />
            )}
            <div className="statusbar" data-testid="statusbar">
              {sortKeys.length > 0 ? (
                <span>
                  Sorted by{" "}
                  {sortKeys
                    .map(
                      (k) =>
                        `${fileInfo.columns[k.column]?.name ?? k.column} ${k.direction === "asc" ? "↑" : "↓"}`,
                    )
                    .join(", ")}
                </span>
              ) : (
                <span>Unsorted</span>
              )}
              {activeCell != null && (
                <>
                  <span className="divider" />
                  <span>
                    Cell R{activeCell.row + 1}, C{activeCell.col + 1}
                    {fileInfo.columns[activeCell.col]
                      ? ` (${fileInfo.columns[activeCell.col].name})`
                      : ""}
                  </span>
                </>
              )}
              <div className="spacer" style={{ flex: 1 }} />
              <span>
                {formatBytes(0)} ·{" "}
                {fileInfo.rowCount.toLocaleString()} rows ·{" "}
                {fileInfo.columns.length} cols
              </span>
            </div>
          </div>
        )}

        {/* AI Sidebar */}
        {!sidebarCollapsed && (
          <>
            <div
              className="sidebar-resizer"
              onMouseDown={startSidebarResize}
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize AI sidebar"
              title="Drag to resize AI sidebar"
            />
            <div
              className="ai-sidebar"
              style={{ width: sidebarWidth }}
              data-testid="ai-sidebar"
            >
              <AISidebar
                activeTab={aiSidebarTab}
                onTabChange={setAiSidebarTab}
                fileInfo={fileInfo}
                apiKeySet={apiKeySet}
                isProcessing={isAiProcessing}
                onProcessing={setIsAiProcessing}
                onStatusChange={handleAccountStatus}
                onJumpToRow={handleJumpToRow}
                onApplyFilter={handleApplyFilter}
                onTransformApply={(result, colName) => void handleTransformApply(result, colName)}
                onJoinComplete={handleJoinComplete}
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
