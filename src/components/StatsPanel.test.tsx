import { describe, it, expect, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { beforeEach } from "vitest";
import { StatsPanel } from "./StatsPanel";

beforeEach(() => cleanup());

describe("StatsPanel", () => {
  it("shows prompt when no column is selected", () => {
    render(
      <StatsPanel column={null} stats={null} loading={false} totalRows={10} />,
    );
    expect(
      screen.getByText(/click a column header/i),
    ).toBeInTheDocument();
  });

  it("renders numeric stats with min/max/mean/sum", () => {
    render(
      <StatsPanel
        column={{ index: 0, name: "age", kind: "integer" }}
        stats={{
          column: 0,
          count: 5,
          empty: 0,
          unique: 5,
          numeric_count: 5,
          min: 1,
          max: 9,
          mean: 5,
          sum: 25,
          shortest: "1",
          longest: "9",
          top_values: [
            ["1", 1],
            ["5", 1],
          ],
        }}
        loading={false}
        totalRows={5}
      />,
    );
    expect(screen.getByText("age")).toBeInTheDocument();
    expect(screen.getByText("Min")).toBeInTheDocument();
    expect(screen.getByText("Max")).toBeInTheDocument();
    expect(screen.getByText("Mean")).toBeInTheDocument();
    expect(screen.getByText("Sum")).toBeInTheDocument();
    expect(screen.getByText("25")).toBeInTheDocument();
  });

  it("renders top values", () => {
    render(
      <StatsPanel
        column={{ index: 0, name: "tag", kind: "string" }}
        stats={{
          column: 0,
          count: 6,
          empty: 0,
          unique: 3,
          numeric_count: 0,
          min: null,
          max: null,
          mean: null,
          sum: null,
          shortest: "cat",
          longest: "alpaca",
          top_values: [
            ["apple", 3],
            ["banana", 2],
            ["cherry", 1],
          ],
        }}
        loading={false}
        totalRows={6}
      />,
    );
    expect(screen.getByText("apple")).toBeInTheDocument();
    expect(screen.getByText("banana")).toBeInTheDocument();
    expect(screen.getByText("cherry")).toBeInTheDocument();
  });

  it("invokes onClose when the × button is clicked", () => {
    const onClose = vi.fn();
    render(
      <StatsPanel
        column={null}
        stats={null}
        loading={false}
        totalRows={10}
        onClose={onClose}
      />,
    );
    fireEvent.click(screen.getByLabelText(/hide sidebar/i));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("renders empty placeholder for empty value in top values", () => {
    render(
      <StatsPanel
        column={{ index: 0, name: "x", kind: "string" }}
        stats={{
          column: 0,
          count: 3,
          empty: 2,
          unique: 2,
          numeric_count: 0,
          min: null,
          max: null,
          mean: null,
          sum: null,
          shortest: null,
          longest: null,
          top_values: [
            ["", 2],
            ["hi", 1],
          ],
        }}
        loading={false}
        totalRows={3}
      />,
    );
    expect(screen.getByText("(empty)")).toBeInTheDocument();
  });
});
