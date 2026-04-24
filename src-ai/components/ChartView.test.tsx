// Visual integrity tests for ChartView. We can't render real SVG geometry
// in jsdom (it doesn't lay out), but we CAN check that recharts emitted
// the expected DOM elements (svg + chart-specific role) and that our
// per-cell colour resolver actually produced opaque hex strings — the
// regression that left the pie chart invisible in production.

import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { ChartView } from "./ChartView";
import type { ChartData } from "../lib/types-ai";

function pieData(): ChartData {
  return {
    spec: {
      chartType: "pie",
      title: "Distribution of Event Types",
      annotation: "Pie chart of event-type counts.",
      xColumn: "type",
      aggregation: "count",
    },
    sql: 'SELECT "type" AS x, COUNT(*) AS y FROM data GROUP BY "type"',
    rows: [
      { x: "click", y: 42 },
      { x: "view", y: 28 },
      { x: "purchase", y: 7 },
    ],
    series: [],
    xLabel: "type",
    yLabel: "Count",
  };
}

function barData(): ChartData {
  return {
    spec: {
      chartType: "bar",
      title: "Avg salary by dept",
      annotation: "Bar chart of average salary per department.",
      xColumn: "department",
      yColumn: "salary",
      aggregation: "avg",
    },
    sql: "SELECT department, AVG(salary) FROM data GROUP BY department",
    rows: [
      { x: "Engineering", y: 175000 },
      { x: "Design", y: 140000 },
      { x: "Data", y: 162000 },
    ],
    series: [],
    xLabel: "department",
    yLabel: "Avg salary",
  };
}

describe("ChartView", () => {
  it("renders the title + annotation surroundings for a pie chart", () => {
    render(<ChartView data={pieData()} />);
    expect(screen.getByText("Distribution of Event Types")).toBeInTheDocument();
    // x and y labels appear in the header pill row.
    expect(screen.getByText("type")).toBeInTheDocument();
    expect(screen.getByText("Count")).toBeInTheDocument();
  });

  it("emits an svg with at least 3 pie slice sectors", () => {
    const { container } = render(<ChartView data={pieData()} />);
    const svg = container.querySelector("svg");
    expect(svg, "Pie chart should mount an SVG").toBeTruthy();
    const sectors = container.querySelectorAll("g.recharts-pie-sector");
    expect(sectors.length, "expected one sector per data row").toBe(3);
  });

  it("each pie slice has a non-empty, non-CSS-variable fill (so it actually shows)", () => {
    // Production regression: <Cell fill="var(--accent)" /> renders as
    // an SVG fill attribute that browsers don't resolve, leaving every
    // slice transparent. After the fix, fill should be a literal colour
    // (hex / rgb / named).
    const { container } = render(<ChartView data={pieData()} />);
    const cells = container.querySelectorAll("path.recharts-pie-sector, path.recharts-sector, path.recharts-pie path");
    // Fall back to all paths if the class names changed across recharts versions.
    const candidates = cells.length > 0 ? cells : container.querySelectorAll("path");
    let anySliceRendered = false;
    for (const el of Array.from(candidates)) {
      const fill = (el.getAttribute("fill") ?? "").trim();
      if (!fill || fill === "none") continue;
      // The bug looked like fill="var(--accent)" — assert we never ship that.
      expect(
        fill.startsWith("var("),
        `pie slice fill is a CSS variable reference, won't render: ${fill}`,
      ).toBe(false);
      anySliceRendered = true;
    }
    expect(anySliceRendered, "at least one slice should have a paintable fill").toBe(
      true,
    );
  });

  it("bar chart renders bars without CSS-variable fills either", () => {
    const { container } = render(<ChartView data={barData()} />);
    // Recharts renders Bars (with corner radius) as `<path>` elements
    // inside `<g class="recharts-bar-rectangle">`. With 3 data rows
    // we expect 3 such bars.
    const bars = container.querySelectorAll("g.recharts-bar-rectangle");
    expect(bars.length, "expected one bar per data row").toBe(3);
    for (const g of Array.from(bars)) {
      const path = g.querySelector("path");
      expect(path, "each bar group should hold a path").toBeTruthy();
      const fill = (path!.getAttribute("fill") ?? "").trim();
      expect(
        fill.startsWith("var("),
        `bar fill is a CSS variable reference: ${fill}`,
      ).toBe(false);
      expect(fill.length, "bar should have a paintable fill").toBeGreaterThan(0);
    }
  });

  it("includes the SQL audit details element", () => {
    render(<ChartView data={pieData()} />);
    const sqlSection = screen.getByText(/SQL/i, { selector: "summary" });
    expect(sqlSection).toBeInTheDocument();
    // Open the details and confirm the SQL text is there.
    const card = sqlSection.closest(".chart-card") as HTMLElement;
    expect(within(card).getByText(/SELECT "type"/)).toBeInTheDocument();
  });
});
