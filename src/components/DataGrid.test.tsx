import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  render,
  screen,
  fireEvent,
  within,
  cleanup,
} from "@testing-library/react";
import { DataGrid } from "./DataGrid";
import { RangeCache } from "../lib/rangeCache";
import type { ColumnMeta, SortKey } from "../lib/types";

function makeCache(rows: string[][]): RangeCache {
  return new RangeCache(async (start, end) => rows.slice(start, end), {
    pageSize: 50,
    maxPages: 4,
  });
}

function columns(
  names: { name: string; kind: ColumnMeta["kind"] }[],
): ColumnMeta[] {
  return names.map((n, i) => ({ index: i, name: n.name, kind: n.kind }));
}

type Props = Omit<Parameters<typeof DataGrid>[0], never>;
function baseProps(overrides: Partial<Props> = {}): Props {
  const cache = overrides.cache ?? makeCache([["1"]]);
  return {
    columns: columns([{ name: "n", kind: "integer" }]),
    rowCount: 1,
    sortKeys: [],
    onSortChange: () => {},
    cache,
    cacheVersion: 1,
    searchHitRows: new Set(),
    highlightQuery: "",
    onSelectColumn: () => {},
    selectedColumn: null,
    activeCell: null,
    onActiveCellChange: () => {},
    onCellCommit: () => {},
    onCopy: () => {},
    onCut: () => {},
    onPaste: () => {},
    onDeleteRows: () => {},
    rowHeight: 28,
    jumpToRow: null,
    ...overrides,
  };
}

beforeEach(() => {
  cleanup();
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get: () => 600,
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get: () => 1200,
  });
});

describe("DataGrid", () => {
  it("renders column headers with type badges", () => {
    render(
      <DataGrid
        {...baseProps({
          columns: columns([
            { name: "id", kind: "integer" },
            { name: "name", kind: "string" },
          ]),
        })}
      />,
    );
    expect(screen.getByText("id")).toBeInTheDocument();
    expect(screen.getByText("name")).toBeInTheDocument();
    const idHeader = screen.getByTestId("header-0");
    expect(within(idHeader).getByText("int")).toBeInTheDocument();
  });

  it("emits a sort key on header click", () => {
    const onSort = vi.fn();
    render(<DataGrid {...baseProps({ onSortChange: onSort })} />);
    fireEvent.click(screen.getByTestId("header-0"));
    expect(onSort).toHaveBeenCalledWith([{ column: 0, direction: "asc" }]);
  });

  it("cycles sort direction asc → desc on repeated click", () => {
    const onSort = vi.fn();
    const keys: SortKey[] = [{ column: 0, direction: "asc" }];
    render(
      <DataGrid {...baseProps({ sortKeys: keys, onSortChange: onSort })} />,
    );
    fireEvent.click(screen.getByTestId("header-0"));
    expect(onSort).toHaveBeenCalledWith([{ column: 0, direction: "desc" }]);
  });

  it("appends a second sort key when shift-clicking", () => {
    const onSort = vi.fn();
    const keys: SortKey[] = [{ column: 0, direction: "asc" }];
    render(
      <DataGrid
        {...baseProps({
          columns: columns([
            { name: "n", kind: "integer" },
            { name: "s", kind: "string" },
          ]),
          sortKeys: keys,
          onSortChange: onSort,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("header-1"), { shiftKey: true });
    expect(onSort).toHaveBeenCalledWith([
      { column: 0, direction: "asc" },
      { column: 1, direction: "asc" },
    ]);
  });

  it("invokes onSelectColumn when a header is clicked", () => {
    const onSelect = vi.fn();
    render(<DataGrid {...baseProps({ onSelectColumn: onSelect })} />);
    fireEvent.click(screen.getByTestId("header-0"));
    expect(onSelect).toHaveBeenCalledWith(0);
  });

  it("calls onCopy with the active cell when Cmd+C is pressed", () => {
    const onCopy = vi.fn();
    render(
      <DataGrid
        {...baseProps({ activeCell: { row: 0, col: 0 }, onCopy })}
      />,
    );
    const grid = screen.getByTestId("datagrid-scroll");
    fireEvent.keyDown(grid, { key: "c", metaKey: true });
    expect(onCopy).toHaveBeenCalledWith([{ row: 0, col: 0 }]);
  });

  it("calls onPaste when Cmd+V is pressed", () => {
    const onPaste = vi.fn();
    render(
      <DataGrid
        {...baseProps({ activeCell: { row: 0, col: 0 }, onPaste })}
      />,
    );
    fireEvent.keyDown(screen.getByTestId("datagrid-scroll"), {
      key: "v",
      metaKey: true,
    });
    expect(onPaste).toHaveBeenCalled();
  });

  it("arrow keys move the active cell", () => {
    const onChange = vi.fn();
    render(
      <DataGrid
        {...baseProps({
          columns: columns([
            { name: "a", kind: "string" },
            { name: "b", kind: "string" },
          ]),
          rowCount: 3,
          activeCell: { row: 1, col: 0 },
          onActiveCellChange: onChange,
        })}
      />,
    );
    const grid = screen.getByTestId("datagrid-scroll");
    fireEvent.keyDown(grid, { key: "ArrowRight" });
    expect(onChange).toHaveBeenCalledWith({ row: 1, col: 1 });
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    expect(onChange).toHaveBeenLastCalledWith({ row: 2, col: 0 });
  });

  it("Cmd+Backspace asks to delete the active row", () => {
    const onDelete = vi.fn();
    render(
      <DataGrid
        {...baseProps({
          activeCell: { row: 2, col: 0 },
          onDeleteRows: onDelete,
        })}
      />,
    );
    fireEvent.keyDown(screen.getByTestId("datagrid-scroll"), {
      key: "Backspace",
      metaKey: true,
    });
    expect(onDelete).toHaveBeenCalledWith([2]);
  });
});
