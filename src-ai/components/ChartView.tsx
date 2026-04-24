// Renders a ChartData payload from the make_chart tool.
//
// Recharts is used for everything bar treemap+horizontal_bar (which both
// have direct equivalents). The colour palette is pulled from the app's
// CSS custom properties so charts pick up whatever theme the user picked.

import { useEffect, useMemo, useState } from "react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Legend,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Scatter,
  ScatterChart,
  Tooltip,
  Treemap,
  XAxis,
  YAxis,
} from "recharts";
import type { ChartData } from "../lib/types-ai";

interface ChartViewProps {
  data: ChartData;
}

// Palette is keyed off the app's CSS custom properties so charts adopt the
// active theme. SVG `fill` attributes only resolve `var(...)` reliably in
// some recharts subcomponents (Bar fills work; Cell fills inside Pie /
// Treemap do not). To keep every chart kind consistent, we resolve the
// CSS-variable references to actual computed colour strings on mount —
// see `useResolvedPalette` below.
const COLOR_VAR_NAMES = [
  "--accent",
  "--kind-string",
  "--kind-integer",
  "--success",
  "--kind-date",
  "--kind-float",
  "--warning",
  "--kind-boolean",
  "--accent-strong",
  "--text-muted",
] as const;

// Fallback hex ramp used when getComputedStyle can't read the variables
// (jsdom in tests, or running before the theme palette has been applied).
const FALLBACK_RAMP = [
  "#e0995b",
  "#8eb0d0",
  "#d4a659",
  "#a7c07b",
  "#7ab386",
  "#e09366",
  "#e6b457",
  "#b2c470",
  "#c97f3d",
  "#a49980",
];

function useResolvedPalette(): string[] {
  const [palette, setPalette] = useState<string[]>(FALLBACK_RAMP);
  useEffect(() => {
    const root = document.documentElement;
    const cs = window.getComputedStyle(root);
    const resolved = COLOR_VAR_NAMES.map((name, i) => {
      const v = cs.getPropertyValue(name).trim();
      return v || FALLBACK_RAMP[i];
    });
    setPalette(resolved);
    // Re-resolve when the palette / theme attribute changes — the rest of
    // the app sets `data-theme` and inline-CSS-vars via applyPalette().
    const observer = new MutationObserver(() => {
      const cs2 = window.getComputedStyle(root);
      setPalette(
        COLOR_VAR_NAMES.map((name, i) => {
          const v = cs2.getPropertyValue(name).trim();
          return v || FALLBACK_RAMP[i];
        }),
      );
    });
    observer.observe(root, {
      attributes: true,
      attributeFilter: ["style", "data-theme"],
    });
    return () => observer.disconnect();
  }, []);
  return palette;
}

const BASE_HEIGHT = 280;

export function ChartView({ data }: ChartViewProps) {
  const { spec, rows, series, xLabel, yLabel } = data;
  const kind = spec.chartType;
  const palette = useResolvedPalette();
  const colorAt = useMemo(
    () => (i: number) => palette[i % palette.length],
    [palette],
  );

  // Common axis / grid / tooltip styling.
  const axisStyle = {
    stroke: "var(--text-dim)",
    style: { fontSize: 11 },
  } as const;
  const tooltipStyle = {
    contentStyle: {
      background: "var(--bg-elevated)",
      border: "1px solid var(--border-strong)",
      borderRadius: 8,
      fontSize: 12,
      color: "var(--text)",
    },
    cursor: { fill: "color-mix(in srgb, var(--accent) 12%, transparent)" },
  } as const;

  let chartNode: React.ReactNode;

  if (kind === "bar" || kind === "horizontal_bar") {
    const horizontal = kind === "horizontal_bar";
    chartNode = (
      <BarChart data={rows} layout={horizontal ? "vertical" : "horizontal"}>
        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" />
        {horizontal ? (
          <>
            <XAxis type="number" {...axisStyle} />
            <YAxis dataKey="x" type="category" width={120} {...axisStyle} />
          </>
        ) : (
          <>
            <XAxis dataKey="x" {...axisStyle} />
            <YAxis {...axisStyle} />
          </>
        )}
        <Tooltip {...tooltipStyle} />
        <Bar dataKey="y" fill={colorAt(0)} radius={[6, 6, 0, 0]} />
      </BarChart>
    );
  } else if (kind === "stacked_bar" || kind === "grouped_bar") {
    const stacked = kind === "stacked_bar";
    chartNode = (
      <BarChart data={rows}>
        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" />
        <XAxis dataKey="x" {...axisStyle} />
        <YAxis {...axisStyle} />
        <Tooltip {...tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 11, color: "var(--text-muted)" }} />
        {series.map((s, i) => (
          <Bar
            key={s}
            dataKey={s}
            stackId={stacked ? "a" : undefined}
            fill={colorAt(i)}
            radius={stacked ? 0 : [4, 4, 0, 0]}
          />
        ))}
      </BarChart>
    );
  } else if (kind === "line") {
    chartNode = (
      <LineChart data={rows}>
        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" />
        <XAxis dataKey="x" {...axisStyle} />
        <YAxis {...axisStyle} />
        <Tooltip {...tooltipStyle} />
        {series.length > 0 && (
          <Legend wrapperStyle={{ fontSize: 11, color: "var(--text-muted)" }} />
        )}
        {series.length > 0 ? (
          series.map((s, i) => (
            <Line
              key={s}
              type="monotone"
              dataKey={s}
              stroke={colorAt(i)}
              strokeWidth={2}
              dot={{ r: 3 }}
              activeDot={{ r: 5 }}
            />
          ))
        ) : (
          <Line
            type="monotone"
            dataKey="y"
            stroke={colorAt(0)}
            strokeWidth={2}
            dot={{ r: 3 }}
            activeDot={{ r: 5 }}
          />
        )}
      </LineChart>
    );
  } else if (kind === "area") {
    chartNode = (
      <AreaChart data={rows}>
        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" />
        <XAxis dataKey="x" {...axisStyle} />
        <YAxis {...axisStyle} />
        <Tooltip {...tooltipStyle} />
        {series.length > 0 ? (
          <>
            <Legend wrapperStyle={{ fontSize: 11, color: "var(--text-muted)" }} />
            {series.map((s, i) => (
              <Area
                key={s}
                type="monotone"
                dataKey={s}
                stroke={colorAt(i)}
                fill={colorAt(i)}
                fillOpacity={0.3}
                strokeWidth={2}
              />
            ))}
          </>
        ) : (
          <Area
            type="monotone"
            dataKey="y"
            stroke={colorAt(0)}
            fill={colorAt(0)}
            fillOpacity={0.3}
            strokeWidth={2}
          />
        )}
      </AreaChart>
    );
  } else if (kind === "pie" || kind === "donut") {
    chartNode = (
      <PieChart>
        <Pie
          data={rows}
          dataKey="y"
          nameKey="x"
          innerRadius={kind === "donut" ? "55%" : 0}
          outerRadius="78%"
          paddingAngle={2}
          stroke="var(--bg-elevated)"
          strokeWidth={2}
          label={(p: { name?: string; percent?: number }) =>
            `${p.name ?? ""} ${((p.percent ?? 0) * 100).toFixed(0)}%`
          }
        >
          {rows.map((_, i) => (
            <Cell key={i} fill={colorAt(i)} />
          ))}
        </Pie>
        <Tooltip {...tooltipStyle} />
      </PieChart>
    );
  } else if (kind === "scatter") {
    chartNode = (
      <ScatterChart>
        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" />
        <XAxis dataKey="x" type="number" {...axisStyle} />
        <YAxis dataKey="y" type="number" {...axisStyle} />
        <Tooltip {...tooltipStyle} />
        <Scatter data={rows} fill={colorAt(0)} />
      </ScatterChart>
    );
  } else if (kind === "histogram") {
    chartNode = (
      <BarChart data={rows}>
        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" />
        <XAxis dataKey="x" {...axisStyle} interval={0} angle={-25} textAnchor="end" height={60} />
        <YAxis {...axisStyle} />
        <Tooltip {...tooltipStyle} />
        <Bar dataKey="y" fill={colorAt(0)} radius={[4, 4, 0, 0]} />
      </BarChart>
    );
  } else if (kind === "treemap") {
    // Recharts Treemap wants a single nested data array with name/size keys.
    const treemapData = rows.map((r) => ({
      name: String(r.x ?? ""),
      size: Number(r.y ?? 0),
    }));
    // Closure captures the resolved palette so each cell gets a real
    // colour (CSS-variable strings won't render in SVG fill attrs here).
    const TreemapCell = (props: unknown) => {
      const p = props as {
        x: number;
        y: number;
        width: number;
        height: number;
        index: number;
        name: string;
      };
      if (p.width < 0 || p.height < 0) return null;
      return (
        <g>
          <rect
            x={p.x}
            y={p.y}
            width={p.width}
            height={p.height}
            style={{
              fill: colorAt(p.index ?? 0),
              stroke: "var(--bg-elevated)",
              strokeWidth: 2,
            }}
          />
          {p.width > 60 && p.height > 24 ? (
            <text
              x={p.x + 6}
              y={p.y + 16}
              fill="var(--badge-text)"
              fontSize={11}
              fontWeight={600}
            >
              {p.name}
            </text>
          ) : null}
        </g>
      );
    };
    chartNode = (
      <Treemap
        data={treemapData}
        dataKey="size"
        stroke="var(--bg-elevated)"
        fill={colorAt(0)}
        content={<TreemapCell />}
      />
    );
  } else {
    chartNode = (
      <div className="chart-fallback">Unsupported chart kind: {kind}</div>
    );
  }

  return (
    <div className="chart-card" data-testid={`chart-${kind}`}>
      <div className="chart-header">
        <div className="chart-title">{spec.title}</div>
        <div className="chart-axes">
          {xLabel && <span className="chart-axis-label">{xLabel}</span>}
          {yLabel && (
            <>
              <span className="chart-axis-sep">↦</span>
              <span className="chart-axis-label">{yLabel}</span>
            </>
          )}
        </div>
      </div>
      <div className="chart-body" style={{ height: BASE_HEIGHT }}>
        <ResponsiveContainer width="100%" height="100%">
          {chartNode as React.ReactElement}
        </ResponsiveContainer>
      </div>
      <details className="chart-sql">
        <summary>SQL</summary>
        <pre>{data.sql}</pre>
      </details>
    </div>
  );
}


