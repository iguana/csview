import { describe, it, expect, vi, beforeEach } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { RowPanel } from "./RowPanel";
import type { ColumnMeta } from "../lib/types";

const cols: ColumnMeta[] = [
  { index: 0, name: "id", kind: "integer" },
  { index: 1, name: "name", kind: "string" },
  { index: 2, name: "note", kind: "string" },
];

beforeEach(() => cleanup());

describe("RowPanel", () => {
  it("shows the row number as a 1-based index", () => {
    render(
      <RowPanel
        rowIndex={4}
        totalRows={10}
        columns={cols}
        values={["5", "Alice", ""]}
        onClose={() => {}}
      />,
    );
    expect(screen.getByText("Row 5")).toBeInTheDocument();
  });

  it("renders a label + value for each column", () => {
    const { container } = render(
      <RowPanel
        rowIndex={0}
        totalRows={3}
        columns={cols}
        values={["1", "Alice", "hello"]}
        onClose={() => {}}
      />,
    );
    const fields = container.querySelectorAll(".row-field");
    expect(fields).toHaveLength(3);
    expect(within(fields[0] as HTMLElement).getByText("id")).toBeInTheDocument();
    expect(within(fields[0] as HTMLElement).getByText("1")).toBeInTheDocument();
    expect(within(fields[1] as HTMLElement).getByText("name")).toBeInTheDocument();
    expect(within(fields[1] as HTMLElement).getByText("Alice")).toBeInTheDocument();
  });

  it("shows a dash placeholder for empty cells", () => {
    render(
      <RowPanel
        rowIndex={0}
        totalRows={1}
        columns={cols}
        values={["1", "Alice", ""]}
        onClose={() => {}}
      />,
    );
    expect(screen.getByText("—")).toBeInTheDocument();
  });

  it("shows a loading message when values are undefined", () => {
    render(
      <RowPanel
        rowIndex={0}
        totalRows={1}
        columns={cols}
        values={undefined}
        onClose={() => {}}
      />,
    );
    expect(screen.getByText(/loading row/i)).toBeInTheDocument();
  });

  it("calls onClose when the close button is clicked", () => {
    const onClose = vi.fn();
    render(
      <RowPanel
        rowIndex={0}
        totalRows={1}
        columns={cols}
        values={["1", "a", "b"]}
        onClose={onClose}
      />,
    );
    fireEvent.click(screen.getByLabelText(/deselect row/i));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("marks integer/float fields as numeric", () => {
    const { container } = render(
      <RowPanel
        rowIndex={0}
        totalRows={1}
        columns={[
          { index: 0, name: "n", kind: "integer" },
          { index: 1, name: "s", kind: "string" },
        ]}
        values={["42", "hello"]}
        onClose={() => {}}
      />,
    );
    const values = container.querySelectorAll(".row-field-value");
    expect(values[0].classList.contains("numeric")).toBe(true);
    expect(values[1].classList.contains("numeric")).toBe(false);
  });
});
