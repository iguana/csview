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
import { api } from "./lib/api";
import { APP_NAME, APP_TAGLINE } from "./lib/config";
import { DataGrid, type CellCoord, type DataGridHandle } from "./components/DataGrid";
import { StatsPanel } from "./components/StatsPanel";
import { RowPanel } from "./components/RowPanel";
import { ThemeMenu } from "./components/ThemeMenu";
import { RangeCache } from "./lib/rangeCache";
import { HistoryStack } from "./lib/history";
import { decodeTsv, encodeTsv } from "./lib/clipboard";
import type {
  ColumnStats,
  CsvMetadata,
  SearchHit,
  SortKey,
} from "./lib/types";
import { basename, formatBytes, formatCount } from "./lib/format";
import { useTheme } from "./lib/theme";

const ROW_HEIGHT_DEFAULT = 28;
const SEARCH_LIMIT = 500;

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

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

export function App() {
  const theme = useTheme();
  const [metadata, setMetadata] = useState<CsvMetadata | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [sortKeys, setSortKeys] = useState<SortKey[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchHits, setSearchHits] = useState<SearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [selectedColumn, setSelectedColumn] = useState<number | null>(null);
  const [selectedRow, setSelectedRow] = useState<number | null>(null);
  const [selectedRowValues, setSelectedRowValues] = useState<string[] | undefined>(
    undefined,
  );
  const [sidebarMode, setSidebarMode] = useState<"row" | "column" | "none">(
    "none",
  );
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(320);
  const [activeCell, setActiveCell] = useState<CellCoord | null>(null);
  const [stats, setStats] = useState<ColumnStats | null>(null);
  const [statsLoading, setStatsLoading] = useState(false);
  const [rowHeight, setRowHeight] = useState(ROW_HEIGHT_DEFAULT);
  const [cacheVersion, setCacheVersion] = useState(0);
  const [jumpToRow, setJumpToRow] = useState<number | null>(null);
  const [historyTick, setHistoryTick] = useState(0);

  const cacheRef = useRef<RangeCache | null>(null);
  const gridRef = useRef<DataGridHandle>(null);
  const historyRef = useRef(new HistoryStack(500));
  const metadataRef = useRef<CsvMetadata | null>(null);

  useEffect(() => {
    metadataRef.current = metadata;
  }, [metadata]);

  const bumpHistory = useCallback(() => setHistoryTick((t) => t + 1), []);

  const ensureCache = useCallback((fileId: string): RangeCache => {
    const cache = new RangeCache((start, end) => api.readRange(fileId, start, end), {
      pageSize: 200,
      maxPages: 128,
    });
    cacheRef.current = cache;
    return cache;
  }, []);

  const cache = cacheRef.current;

  // --- Window title sync ---
  useEffect(() => {
    (async () => {
      const win = getCurrentWindow();
      if (!metadata) {
        await win.setTitle(APP_NAME);
        return;
      }
      const dirtyMark = metadata.dirty ? "● " : "";
      await win.setTitle(`${dirtyMark}${basename(metadata.path)} — ${APP_NAME}`);
    })().catch(() => {
      // setTitle may fail in tests without a real window; ignore.
    });
  }, [metadata]);

  const openFile = useCallback(
    async (path: string, forceHeader?: boolean) => {
      setError(null);
      setLoading(true);
      try {
        const meta = await api.openCsv(path, forceHeader);
        setMetadata(meta);
        setSortKeys([]);
        setSearchQuery("");
        setSearchHits([]);
        setSelectedColumn(null);
        setSelectedRow(null);
        setSelectedRowValues(undefined);
        setSidebarMode("none");
        setStats(null);
        setActiveCell(null);
        historyRef.current.clear();
        bumpHistory();
        const c = ensureCache(meta.file_id);
        if (meta.sample.length > 0) {
          await c.ensure(0, Math.min(meta.sample.length, c.pageSize));
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

  // --- File open dialog ---
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
      // If we already have a file open, open in a new window per macOS norms.
      if (metadata) {
        await api.openInNewWindow(selection);
      } else {
        await openFile(selection);
      }
    } catch (e) {
      setError(errMsg(e));
    }
  }, [openFile, metadata]);

  const onNewWindow = useCallback(async () => {
    try {
      await api.newWindow();
    } catch (e) {
      setError(errMsg(e));
    }
  }, []);

  // --- Tauri event listeners (one-shot mount) ---
  useEffect(() => {
    const offFile = listen<string>("cli-open-file", async (event) => {
      if (event.payload) await openFile(event.payload);
    });
    const offDemo = listen<{
      sort?: SortKey[];
      selectColumn?: number;
      selectRow?: number;
      activeCell?: { row: number; col: number };
      sidebarCollapsed?: boolean;
      search?: string;
      palette?: string;
    }>("csview-demo", (event) => {
      setTimeout(() => {
        if (event.payload.sort) setSortKeys(event.payload.sort);
        if (event.payload.selectColumn != null) {
          setSelectedColumn(event.payload.selectColumn);
          setSidebarMode("column");
        }
        if (event.payload.selectRow != null) {
          setSelectedRow(event.payload.selectRow);
          setSidebarMode("row");
        }
        if (event.payload.activeCell) setActiveCell(event.payload.activeCell);
        if (event.payload.sidebarCollapsed != null)
          setSidebarCollapsed(event.payload.sidebarCollapsed);
        if (event.payload.search != null) setSearchQuery(event.payload.search);
        if (event.payload.palette) {
          // Any palette id; ignore if unrecognized.
          import("./lib/palettes").then(({ isPaletteId }) => {
            if (isPaletteId(event.payload.palette)) {
              theme.setPalette(event.payload.palette);
            }
          });
        }
      }, 200);
    });
    const offTheme = listen<string>("csview-theme", (event) => {
      const v = event.payload;
      if (v === "light" || v === "dark" || v === "system") {
        theme.setMode(v);
      }
    });
    return () => {
      void offFile.then((fn) => fn());
      void offDemo.then((fn) => fn());
      void offTheme.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openFile]);

  // --- Apply sort ---
  useEffect(() => {
    if (!metadata) return;
    let cancelled = false;
    (async () => {
      try {
        await api.sort(metadata.file_id, sortKeys);
        if (cancelled) return;
        cacheRef.current?.invalidate();
        setCacheVersion((v) => v + 1);
      } catch (e) {
        if (!cancelled) setError(errMsg(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sortKeys, metadata]);

  // --- Debounced search ---
  useEffect(() => {
    if (!metadata) return;
    if (!searchQuery.trim()) {
      setSearchHits([]);
      return;
    }
    setSearching(true);
    const handle = setTimeout(async () => {
      try {
        const hits = await api.search(
          metadata.file_id,
          searchQuery.trim(),
          SEARCH_LIMIT,
        );
        setSearchHits(hits);
        if (hits.length > 0) setJumpToRow(hits[0].row);
      } catch (e) {
        setError(errMsg(e));
      } finally {
        setSearching(false);
      }
    }, 180);
    return () => clearTimeout(handle);
  }, [searchQuery, metadata]);

  // --- Column stats ---
  useEffect(() => {
    if (!metadata || selectedColumn == null) return;
    let cancelled = false;
    setStatsLoading(true);
    setStats(null);
    (async () => {
      try {
        const s = await api.computeStats(metadata.file_id, selectedColumn);
        if (!cancelled) setStats(s);
      } catch (e) {
        if (!cancelled) setError(errMsg(e));
      } finally {
        if (!cancelled) setStatsLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedColumn, metadata]);

  // --- Row values for the panel ---
  useEffect(() => {
    if (!metadata || selectedRow == null) {
      setSelectedRowValues(undefined);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const rows = await api.readRange(
          metadata.file_id,
          selectedRow,
          selectedRow + 1,
        );
        if (!cancelled) setSelectedRowValues(rows[0]);
      } catch (e) {
        if (!cancelled) setError(errMsg(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedRow, metadata, cacheVersion]);

  // --- Mirror active cell → row panel (so the sidebar stays in sync) ---
  useEffect(() => {
    if (!activeCell) return;
    setSelectedRow(activeCell.row);
    if (sidebarMode === "none" || sidebarMode === "row") setSidebarMode("row");
  }, [activeCell, sidebarMode]);

  const searchHitRows = useMemo(() => {
    const set = new Set<number>();
    for (const h of searchHits) set.add(h.row);
    return set;
  }, [searchHits]);

  const onToggleHeader = useCallback(async () => {
    if (!metadata) return;
    try {
      const meta = await api.reloadWithHeader(
        metadata.file_id,
        !metadata.has_header,
      );
      setMetadata(meta);
      setSortKeys([]);
      setSelectedColumn(null);
      setStats(null);
      cacheRef.current?.invalidate();
      setCacheVersion((v) => v + 1);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [metadata]);

  const columnForSelection = useMemo(() => {
    if (!metadata || selectedColumn == null) return null;
    return metadata.columns[selectedColumn] ?? null;
  }, [metadata, selectedColumn]);

  const onColumnClick = useCallback((col: number) => {
    setSelectedColumn(col);
    setSidebarMode("column");
  }, []);

  const onCloseSidebar = useCallback(() => {
    setSidebarCollapsed(true);
  }, []);

  // Drag-to-resize the sidebar. The handle captures mousemove on the window
  // so the pointer doesn't lose the drag if it leaves the divider.
  const startSidebarResize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startWidth = sidebarWidth;
      const onMove = (ev: MouseEvent) => {
        const delta = startX - ev.clientX;
        const next = Math.max(220, Math.min(720, startWidth + delta));
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

  // --- Editing ---
  const readCell = useCallback(
    async (row: number, col: number): Promise<string> => {
      if (!metadataRef.current) return "";
      const c = cacheRef.current;
      const cached = c?.get(row);
      if (cached) return cached[col] ?? "";
      const rows = await api.readRange(
        metadataRef.current.file_id,
        row,
        row + 1,
      );
      return rows[0]?.[col] ?? "";
    },
    [],
  );

  const applyCellUpdate = useCallback(
    async (row: number, col: number, value: string): Promise<void> => {
      const meta = metadataRef.current;
      if (!meta) return;
      const next = await api.updateCell(meta.file_id, row, col, value);
      setMetadata(next);
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
        const entry = {
          label: "Edit cell",
          undo: async () => {
            await applyCellUpdate(row, col, previous);
          },
          redo: async () => {
            await applyCellUpdate(row, col, value);
          },
        };
        historyRef.current.push(entry);
        bumpHistory();
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [readCell, applyCellUpdate, bumpHistory],
  );

  const onDeleteRows = useCallback(
    async (rows: number[]) => {
      const meta = metadataRef.current;
      if (!meta) return;
      try {
        // Snapshot values so we can restore on undo.
        const snapshots: { row: number; values: string[] }[] = [];
        for (const r of rows) {
          const vs = await api.readRange(meta.file_id, r, r + 1);
          if (vs[0]) snapshots.push({ row: r, values: vs[0] });
        }
        const next = await api.deleteRows(meta.file_id, rows);
        setMetadata(next);
        cacheRef.current?.invalidate();
        setCacheVersion((v) => v + 1);
        const entry = {
          label: `Delete ${rows.length} row(s)`,
          undo: async () => {
            for (const s of snapshots) {
              const m = await api.insertRow(meta.file_id, s.row, s.values);
              setMetadata(m);
            }
            cacheRef.current?.invalidate();
            setCacheVersion((v) => v + 1);
          },
          redo: async () => {
            const m = await api.deleteRows(meta.file_id, rows);
            setMetadata(m);
            cacheRef.current?.invalidate();
            setCacheVersion((v) => v + 1);
          },
        };
        historyRef.current.push(entry);
        bumpHistory();
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [bumpHistory],
  );

  const onInsertRow = useCallback(async () => {
    const meta = metadataRef.current;
    if (!meta) return;
    const at = activeCell?.row ?? meta.row_count;
    try {
      const next = await api.insertRow(meta.file_id, at, null);
      setMetadata(next);
      cacheRef.current?.invalidate();
      setCacheVersion((v) => v + 1);
      const entry = {
        label: "Insert row",
        undo: async () => {
          const m = await api.deleteRows(meta.file_id, [at]);
          setMetadata(m);
          cacheRef.current?.invalidate();
          setCacheVersion((v) => v + 1);
        },
        redo: async () => {
          const m = await api.insertRow(meta.file_id, at, null);
          setMetadata(m);
          cacheRef.current?.invalidate();
          setCacheVersion((v) => v + 1);
        },
      };
      historyRef.current.push(entry);
      bumpHistory();
    } catch (e) {
      setError(errMsg(e));
    }
  }, [activeCell, bumpHistory]);

  // --- Save / Save As ---
  const doSave = useCallback(async () => {
    const meta = metadataRef.current;
    if (!meta) return;
    try {
      const next = await api.save(meta.file_id);
      setMetadata(next);
    } catch (e) {
      setError(errMsg(e));
    }
  }, []);

  const doSaveAs = useCallback(async () => {
    const meta = metadataRef.current;
    if (!meta) return;
    try {
      const target = await saveDialog({
        title: "Save CSV as",
        defaultPath: meta.path,
        filters: [
          { name: "CSV", extensions: ["csv"] },
          { name: "TSV", extensions: ["tsv"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (!target) return;
      const next = await api.saveAs(meta.file_id, target);
      setMetadata(next);
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
    try {
      const text = (await readClipboardText()) ?? "";
      const grid = decodeTsv(text);
      if (grid.length === 0) return;
      // Apply as a rectangle anchored at activeCell.
      const changes: { row: number; col: number; value: string; prev: string }[] = [];
      for (let dr = 0; dr < grid.length; dr++) {
        for (let dc = 0; dc < grid[dr].length; dc++) {
          const r = activeCell.row + dr;
          const c = activeCell.col + dc;
          if (!metadataRef.current) continue;
          if (r >= metadataRef.current.row_count) continue;
          if (c >= metadataRef.current.column_count) continue;
          const prev = await readCell(r, c);
          changes.push({ row: r, col: c, value: grid[dr][dc], prev });
        }
      }
      for (const ch of changes) {
        await applyCellUpdate(ch.row, ch.col, ch.value);
      }
      const entry = {
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
      };
      historyRef.current.push(entry);
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
  // All menu items with custom IDs (new_window, open, save, save_as, undo,
  // redo, toggle_sidebar, insert_row, delete_row, find, clear_sort,
  // toggle_header, row_height.*, theme.*, palette.*) dispatch here.
  useEffect(() => {
    const off = listen<string>("menu-action", (event) => {
      const id = event.payload;
      switch (id) {
        case "new_window":
          void onNewWindow();
          return;
        case "open":
          void onOpenClick();
          return;
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
        case "toggle_sidebar":
          setSidebarCollapsed((v) => !v);
          return;
        case "toggle_header":
          void onToggleHeader();
          return;
        case "clear_sort":
          setSortKeys([]);
          return;
        case "insert_row":
          void onInsertRow();
          return;
        case "delete_row":
          if (activeCell) void onDeleteRows([activeCell.row]);
          return;
        case "find":
          // Focus the search input in the toolbar.
          {
            const el = document.querySelector<HTMLInputElement>(
              '[data-testid="search-input"]',
            );
            el?.focus();
            el?.select();
          }
          return;
        case "row_height.compact":
          setRowHeight(22);
          return;
        case "row_height.normal":
          setRowHeight(28);
          return;
        case "row_height.tall":
          setRowHeight(40);
          return;
        case "theme.system":
          theme.setMode("system");
          return;
        case "theme.light":
          theme.setMode("light");
          return;
        case "theme.dark":
          theme.setMode("dark");
          return;
      }
      if (id.startsWith("palette.")) {
        const palette = id.slice("palette.".length);
        import("./lib/palettes").then(({ isPaletteId }) => {
          if (isPaletteId(palette)) theme.setPalette(palette);
        });
      }
    });
    return () => {
      void off.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    onNewWindow,
    onOpenClick,
    doSave,
    doSaveAs,
    doUndo,
    doRedo,
    onToggleHeader,
    onInsertRow,
    onDeleteRows,
    activeCell,
  ]);

  // --- Global keyboard fallback ---
  // Menu accelerators handle most shortcuts on macOS, but we keep a small
  // fallback for keys the menu doesn't own (and for dev builds where the
  // menu isn't attached to the window focus tree yet).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (!meta) return;
      const key = e.key.toLowerCase();
      if (key === "b") {
        e.preventDefault();
        setSidebarCollapsed((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const history = historyRef.current;
  const canUndo = history.undoable;
  const canRedo = history.redoable;
  // Touch the tick to keep this derivation reactive:
  void historyTick;

  return (
    <div className="app" data-testid="app">
      <div className="titlebar" data-testid="titlebar">
        <div className="titlebar-title">
          <span className="app-name">{APP_NAME}</span>
          {metadata && (
            <>
              <span className="title-sep" aria-hidden>
                ·
              </span>
              <span className="filename">
                {metadata.dirty && (
                  <span className="dirty-dot" aria-label="Unsaved changes">
                    ●
                  </span>
                )}
                {basename(metadata.path)}
              </span>
            </>
          )}
        </div>
        {metadata && (
          <div className="titlebar-stats" data-testid="titlebar-stats">
            <span className="stat-pill">
              <span className="stat-num">{formatCount(metadata.row_count)}</span>
              <span className="stat-unit">rows</span>
            </span>
            <span className="stat-pill">
              <span className="stat-num">{metadata.column_count}</span>
              <span className="stat-unit">cols</span>
            </span>
            <span className="stat-pill">
              <span className="stat-num">{formatBytes(metadata.size_bytes)}</span>
            </span>
            <span
              className={`stat-pill ${metadata.fully_loaded ? "ok" : "paged"}`}
              title={
                metadata.fully_loaded
                  ? "Entire file loaded in memory"
                  : "Large file — reading pages on demand"
              }
            >
              <span className="stat-num">
                {metadata.fully_loaded ? "Loaded" : "Paged"}
              </span>
            </span>
          </div>
        )}
        <div className="spacer" />
        {metadata && (
          <>
            <button
              className={`iconbtn save-btn ${metadata.dirty ? "primary" : ""}`}
              onClick={doSave}
              disabled={!metadata.dirty}
              title={
                metadata.dirty
                  ? "Save (⌘S)"
                  : "Save — no unsaved changes"
              }
              aria-label="Save"
              data-testid="titlebar-save"
            >
              <SaveIcon />
              Save
            </button>
            <button
              className="iconbtn"
              onClick={doSaveAs}
              title="Save As… (⇧⌘S)"
              aria-label="Save As"
              data-testid="titlebar-save-as"
            >
              Save As…
            </button>
          </>
        )}
        <ThemeMenu theme={theme} />
        <button
          className="iconbtn"
          onClick={() => setSidebarCollapsed((v) => !v)}
          title={sidebarCollapsed ? "Show sidebar (⌘B)" : "Hide sidebar (⌘B)"}
          aria-label="Toggle sidebar"
          data-testid="sidebar-toggle"
        >
          {sidebarCollapsed ? "⇤" : "⇥"}
        </button>
        <button onClick={onOpenClick} disabled={loading} title="Open CSV… (⌘O)">
          Open…
        </button>
      </div>

      {metadata && (
        <div className="toolbar" data-testid="toolbar">
          <div className="search">
            <span className="search-icon">⌕</span>
            <input
              type="search"
              placeholder="Search cells…"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              aria-label="Search"
              data-testid="search-input"
            />
          </div>
          {searchQuery && (
            <div className="meta">
              {searching
                ? "Searching…"
                : `${formatCount(searchHits.length)} match${
                    searchHits.length === 1 ? "" : "es"
                  }`}
            </div>
          )}
          <label className="toggle">
            <input
              type="checkbox"
              checked={metadata.has_header}
              onChange={onToggleHeader}
              data-testid="header-toggle"
            />
            First row is header
          </label>
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
            onClick={doUndo}
            disabled={!canUndo}
            title="Undo (⌘Z)"
            data-testid="undo-btn"
          >
            ↶ Undo
          </button>
          <button
            onClick={doRedo}
            disabled={!canRedo}
            title="Redo (⇧⌘Z)"
            data-testid="redo-btn"
          >
            ↷ Redo
          </button>
          <button onClick={onInsertRow} title="Add a new row at the active position">
            + Row
          </button>
          <button
            onClick={doSave}
            disabled={!metadata.dirty}
            className={metadata.dirty ? "primary" : ""}
            title="Save (⌘S)"
            data-testid="save-btn"
          >
            Save
          </button>
          <button onClick={doSaveAs} title="Save As… (⇧⌘S)">
            Save As…
          </button>
          {sortKeys.length > 0 && (
            <button onClick={() => setSortKeys([])}>Clear sort</button>
          )}
        </div>
      )}

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

      <div className="main">
        {!metadata ? (
          <div className="welcome">
            <div className="logo">{APP_NAME.slice(0, 1).toUpperCase()}</div>
            <h1>{APP_NAME}</h1>
            <p>{APP_TAGLINE}. Open a file to see a spreadsheet-style grid with sorting, search, editing, and column statistics. Large files stream from disk so nothing blocks.</p>
            <button className="primary" onClick={onOpenClick}>
              Open CSV…
            </button>
            <p className="welcome-hint">
              Or launch from the terminal: <code>{APP_NAME} path/to/file.csv</code>
            </p>
            <div className="welcome-shortcuts">
              <span><kbd>⌘O</kbd> Open</span>
              <span><kbd>⌘N</kbd> New window</span>
            </div>
          </div>
        ) : (
          <div className="grid-container">
            {cache && (
              <DataGrid
                ref={gridRef}
                columns={metadata.columns}
                rowCount={metadata.row_count}
                sortKeys={sortKeys}
                onSortChange={setSortKeys}
                cache={cache}
                cacheVersion={cacheVersion}
                searchHitRows={searchHitRows}
                highlightQuery={searchQuery.trim()}
                onSelectColumn={onColumnClick}
                selectedColumn={selectedColumn}
                activeCell={activeCell}
                onActiveCellChange={setActiveCell}
                onCellCommit={onCellCommit}
                onCopy={onCopy}
                onCut={onCut}
                onPaste={onPaste}
                onDeleteRows={onDeleteRows}
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
                        `${metadata.columns[k.column]?.name ?? k.column} ${
                          k.direction === "asc" ? "↑" : "↓"
                        }`,
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
                    {metadata.columns[activeCell.col]
                      ? ` (${metadata.columns[activeCell.col].name})`
                      : ""}
                  </span>
                </>
              )}
              <div className="spacer" style={{ flex: 1 }} />
              {searchHits.length > 0 && (
                <span>
                  Found {formatCount(searchHits.length)} matches in{" "}
                  {searchHitRows.size} row{searchHitRows.size === 1 ? "" : "s"}
                </span>
              )}
            </div>
          </div>
        )}
        {metadata && !sidebarCollapsed && (
          <>
            <div
              className="sidebar-resizer"
              onMouseDown={startSidebarResize}
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize sidebar"
              title="Drag to resize sidebar"
              data-testid="sidebar-resizer"
            />
            <div
              className="sidebar-wrap"
              style={{ width: sidebarWidth }}
              data-testid="sidebar-wrap"
            >
              {sidebarMode === "row" && selectedRow != null ? (
                <RowPanel
                  rowIndex={selectedRow}
                  totalRows={metadata.row_count}
                  columns={metadata.columns}
                  values={selectedRowValues}
                  onClose={onCloseSidebar}
                />
              ) : (
                <StatsPanel
                  column={columnForSelection}
                  stats={stats}
                  loading={statsLoading}
                  totalRows={metadata.row_count}
                  onClose={onCloseSidebar}
                />
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
