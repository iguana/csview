/**
 * Palette registry — each palette has a dark and light variant as CSS custom
 * property maps. The keys here match the CSS var names (without the `--`
 * prefix). `applyPalette` writes them to :root so a single switch recolors
 * the whole app.
 */

export type PaletteId =
  | "parchment"
  | "noir"
  | "solarized"
  | "ocean"
  | "forest"
  | "graphite";

export interface PaletteVars {
  bg: string;
  "bg-elevated": string;
  "bg-grid": string;
  "bg-row-alt": string;
  "bg-row-hover": string;
  "bg-row-selected": string;
  border: string;
  "border-strong": string;
  text: string;
  "text-muted": string;
  "text-dim": string;
  accent: string;
  "accent-strong": string;
  success: string;
  warning: string;
  danger: string;
  "kind-integer": string;
  "kind-float": string;
  "kind-boolean": string;
  "kind-date": string;
  "kind-string": string;
  "kind-empty": string;
  "badge-text": string;
  "titlebar-top": string;
  "titlebar-bottom": string;
  "logo-from": string;
  "logo-to": string;
  "logo-text": string;
  hl: string;
  "hl-row": string;
  "scroll-thumb": string;
  "scroll-thumb-hover": string;
}

export interface Palette {
  id: PaletteId;
  name: string;
  description: string;
  dark: PaletteVars;
  light: PaletteVars;
}

// --- Parchment & Ink (default) ---------------------------------------------
const parchment: Palette = {
  id: "parchment",
  name: "Parchment",
  description: "Warm editorial — rust accent, earthy badges",
  dark: {
    bg: "#1e1c16",
    "bg-elevated": "#272520",
    "bg-grid": "#1a1812",
    "bg-row-alt": "#23211a",
    "bg-row-hover": "#302d24",
    "bg-row-selected": "#3d3729",
    border: "#342f23",
    "border-strong": "#4a4535",
    text: "#e9e1ce",
    "text-muted": "#a49980",
    "text-dim": "#6d6553",
    accent: "#e0995b",
    "accent-strong": "#c97f3d",
    success: "#a7c07b",
    warning: "#e6b457",
    danger: "#d6846c",
    "kind-integer": "#d4a659",
    "kind-float": "#e09366",
    "kind-boolean": "#b2c470",
    "kind-date": "#7ab386",
    "kind-string": "#8eb0d0",
    "kind-empty": "#9d9481",
    "badge-text": "#1e1c16",
    "titlebar-top": "#272520",
    "titlebar-bottom": "#1e1c16",
    "logo-from": "#e6a868",
    "logo-to": "#b26122",
    "logo-text": "#1e1c16",
    hl: "rgba(224, 153, 91, 0.28)",
    "hl-row": "rgba(224, 153, 91, 0.14)",
    "scroll-thumb": "rgba(164, 153, 128, 0.26)",
    "scroll-thumb-hover": "rgba(164, 153, 128, 0.44)",
  },
  light: {
    bg: "#f4ede1",
    "bg-elevated": "#ebe2d1",
    "bg-grid": "#f7f1e5",
    "bg-row-alt": "#efe7d3",
    "bg-row-hover": "#e3d7bc",
    "bg-row-selected": "#e0cc96",
    border: "#d8ccb0",
    "border-strong": "#b6a680",
    text: "#2e271a",
    "text-muted": "#6b5e47",
    "text-dim": "#9a8b6f",
    accent: "#9c4a1f",
    "accent-strong": "#7a3914",
    success: "#5c7a32",
    warning: "#b07611",
    danger: "#a13822",
    "kind-integer": "#8a6320",
    "kind-float": "#b06226",
    "kind-boolean": "#6b7d28",
    "kind-date": "#3d7a52",
    "kind-string": "#3b5a7a",
    "kind-empty": "#857966",
    "badge-text": "#f7f1e5",
    "titlebar-top": "#ece2cc",
    "titlebar-bottom": "#e2d6ba",
    "logo-from": "#c17332",
    "logo-to": "#7a3914",
    "logo-text": "#f7f1e5",
    hl: "rgba(156, 74, 31, 0.18)",
    "hl-row": "rgba(156, 74, 31, 0.08)",
    "scroll-thumb": "rgba(46, 39, 26, 0.22)",
    "scroll-thumb-hover": "rgba(46, 39, 26, 0.4)",
  },
};

// --- Noir — near-black with one punchy accent ------------------------------
const noir: Palette = {
  id: "noir",
  name: "Noir",
  description: "Near-black & paper, saffron accent",
  dark: {
    bg: "#0f0f11",
    "bg-elevated": "#17171a",
    "bg-grid": "#0b0b0e",
    "bg-row-alt": "#131316",
    "bg-row-hover": "#1e1e22",
    "bg-row-selected": "#2a2a30",
    border: "#212125",
    "border-strong": "#333339",
    text: "#e4e4e7",
    "text-muted": "#8b8b92",
    "text-dim": "#5a5a60",
    accent: "#f4b942",
    "accent-strong": "#d99a1e",
    success: "#a3d081",
    warning: "#f4b942",
    danger: "#e87770",
    "kind-integer": "#f4b942",
    "kind-float": "#f49a56",
    "kind-boolean": "#c4b9f0",
    "kind-date": "#88d3a6",
    "kind-string": "#8fb8e0",
    "kind-empty": "#7a7a82",
    "badge-text": "#0f0f11",
    "titlebar-top": "#18181b",
    "titlebar-bottom": "#0f0f11",
    "logo-from": "#f4b942",
    "logo-to": "#c68418",
    "logo-text": "#0f0f11",
    hl: "rgba(244, 185, 66, 0.26)",
    "hl-row": "rgba(244, 185, 66, 0.12)",
    "scroll-thumb": "rgba(180, 180, 190, 0.22)",
    "scroll-thumb-hover": "rgba(180, 180, 190, 0.4)",
  },
  light: {
    bg: "#fafaf9",
    "bg-elevated": "#f0efed",
    "bg-grid": "#ffffff",
    "bg-row-alt": "#f5f4f2",
    "bg-row-hover": "#e8e6e2",
    "bg-row-selected": "#e0dcd2",
    border: "#e2dfd8",
    "border-strong": "#c4beb0",
    text: "#1a1a1c",
    "text-muted": "#5f5f68",
    "text-dim": "#95958f",
    accent: "#b07300",
    "accent-strong": "#8c5a00",
    success: "#4a7a28",
    warning: "#a66a00",
    danger: "#8d2f26",
    "kind-integer": "#8c5a00",
    "kind-float": "#a65626",
    "kind-boolean": "#5b4aa0",
    "kind-date": "#356a3f",
    "kind-string": "#2e5a8a",
    "kind-empty": "#78766c",
    "badge-text": "#fafaf9",
    "titlebar-top": "#f4f2ed",
    "titlebar-bottom": "#ebe8e0",
    "logo-from": "#d99a1e",
    "logo-to": "#8c5a00",
    "logo-text": "#fafaf9",
    hl: "rgba(176, 115, 0, 0.2)",
    "hl-row": "rgba(176, 115, 0, 0.08)",
    "scroll-thumb": "rgba(26, 26, 28, 0.22)",
    "scroll-thumb-hover": "rgba(26, 26, 28, 0.4)",
  },
};

// --- Solarized-esque (original-adjacent, not trademark) --------------------
const solarized: Palette = {
  id: "solarized",
  name: "Solarized-ish",
  description: "Balanced yellows & teals on cream or midnight",
  dark: {
    bg: "#002b36",
    "bg-elevated": "#073642",
    "bg-grid": "#01242e",
    "bg-row-alt": "#042e39",
    "bg-row-hover": "#0b4350",
    "bg-row-selected": "#125063",
    border: "#0d3b49",
    "border-strong": "#1b5669",
    text: "#e6ded1",
    "text-muted": "#93a1a1",
    "text-dim": "#586e75",
    accent: "#b58900",
    "accent-strong": "#906d00",
    success: "#859900",
    warning: "#cb4b16",
    danger: "#dc322f",
    "kind-integer": "#b58900",
    "kind-float": "#cb4b16",
    "kind-boolean": "#d33682",
    "kind-date": "#859900",
    "kind-string": "#268bd2",
    "kind-empty": "#93a1a1",
    "badge-text": "#002b36",
    "titlebar-top": "#073642",
    "titlebar-bottom": "#002b36",
    "logo-from": "#b58900",
    "logo-to": "#cb4b16",
    "logo-text": "#002b36",
    hl: "rgba(181, 137, 0, 0.28)",
    "hl-row": "rgba(181, 137, 0, 0.14)",
    "scroll-thumb": "rgba(147, 161, 161, 0.26)",
    "scroll-thumb-hover": "rgba(147, 161, 161, 0.44)",
  },
  light: {
    bg: "#fdf6e3",
    "bg-elevated": "#eee8d5",
    "bg-grid": "#fefaea",
    "bg-row-alt": "#f5eed9",
    "bg-row-hover": "#e6dfc8",
    "bg-row-selected": "#dbd2b4",
    border: "#e2dbc0",
    "border-strong": "#bfb79a",
    text: "#586e75",
    "text-muted": "#7e8a8e",
    "text-dim": "#a7afa9",
    accent: "#b58900",
    "accent-strong": "#906d00",
    success: "#859900",
    warning: "#cb4b16",
    danger: "#dc322f",
    "kind-integer": "#b58900",
    "kind-float": "#cb4b16",
    "kind-boolean": "#d33682",
    "kind-date": "#859900",
    "kind-string": "#268bd2",
    "kind-empty": "#93a1a1",
    "badge-text": "#fdf6e3",
    "titlebar-top": "#f5eed6",
    "titlebar-bottom": "#ebe4cc",
    "logo-from": "#b58900",
    "logo-to": "#906d00",
    "logo-text": "#fdf6e3",
    hl: "rgba(181, 137, 0, 0.22)",
    "hl-row": "rgba(181, 137, 0, 0.1)",
    "scroll-thumb": "rgba(88, 110, 117, 0.26)",
    "scroll-thumb-hover": "rgba(88, 110, 117, 0.44)",
  },
};

// --- Ocean — cool blues and teals ------------------------------------------
const ocean: Palette = {
  id: "ocean",
  name: "Ocean",
  description: "Cool seafoam, navy, and teal",
  dark: {
    bg: "#0e1b24",
    "bg-elevated": "#16262f",
    "bg-grid": "#0a1720",
    "bg-row-alt": "#112029",
    "bg-row-hover": "#1a2e3a",
    "bg-row-selected": "#264354",
    border: "#1d2f3a",
    "border-strong": "#2c4656",
    text: "#d8e4ed",
    "text-muted": "#8ca4b3",
    "text-dim": "#56707d",
    accent: "#5fc4b8",
    "accent-strong": "#3ea99d",
    success: "#88d4a0",
    warning: "#ecc46a",
    danger: "#e27978",
    "kind-integer": "#7dd3c6",
    "kind-float": "#68b8d6",
    "kind-boolean": "#a997d9",
    "kind-date": "#8fd49e",
    "kind-string": "#7bb3e3",
    "kind-empty": "#7d97a6",
    "badge-text": "#0e1b24",
    "titlebar-top": "#162630",
    "titlebar-bottom": "#0e1b24",
    "logo-from": "#5fc4b8",
    "logo-to": "#2e7a87",
    "logo-text": "#0e1b24",
    hl: "rgba(95, 196, 184, 0.26)",
    "hl-row": "rgba(95, 196, 184, 0.12)",
    "scroll-thumb": "rgba(140, 164, 179, 0.24)",
    "scroll-thumb-hover": "rgba(140, 164, 179, 0.42)",
  },
  light: {
    bg: "#eaf2f7",
    "bg-elevated": "#dde9f0",
    "bg-grid": "#f2f7fa",
    "bg-row-alt": "#e4eef4",
    "bg-row-hover": "#d3e1ea",
    "bg-row-selected": "#bfd2df",
    border: "#cddbe3",
    "border-strong": "#a6bcc8",
    text: "#1c3442",
    "text-muted": "#55707f",
    "text-dim": "#899ea9",
    accent: "#0f766e",
    "accent-strong": "#0b5d57",
    success: "#3b7a5a",
    warning: "#a06812",
    danger: "#9a3c36",
    "kind-integer": "#0f766e",
    "kind-float": "#2a6a9a",
    "kind-boolean": "#6a4fa6",
    "kind-date": "#3b7a5a",
    "kind-string": "#2a6a9a",
    "kind-empty": "#728491",
    "badge-text": "#f2f7fa",
    "titlebar-top": "#e0ecf3",
    "titlebar-bottom": "#d4e3ec",
    "logo-from": "#2ea498",
    "logo-to": "#0b5d57",
    "logo-text": "#f2f7fa",
    hl: "rgba(15, 118, 110, 0.2)",
    "hl-row": "rgba(15, 118, 110, 0.08)",
    "scroll-thumb": "rgba(28, 52, 66, 0.24)",
    "scroll-thumb-hover": "rgba(28, 52, 66, 0.42)",
  },
};

// --- Forest — greens and moss ----------------------------------------------
const forest: Palette = {
  id: "forest",
  name: "Forest",
  description: "Mossy greens and autumn",
  dark: {
    bg: "#151c16",
    "bg-elevated": "#1d2a1e",
    "bg-grid": "#101710",
    "bg-row-alt": "#182018",
    "bg-row-hover": "#253328",
    "bg-row-selected": "#2f4232",
    border: "#243024",
    "border-strong": "#354636",
    text: "#e1e7d6",
    "text-muted": "#9bad90",
    "text-dim": "#606d5b",
    accent: "#9bcb74",
    "accent-strong": "#78ad50",
    success: "#9bcb74",
    warning: "#e6b050",
    danger: "#cf7562",
    "kind-integer": "#d0b85a",
    "kind-float": "#e09466",
    "kind-boolean": "#b5a8d8",
    "kind-date": "#9bcb74",
    "kind-string": "#7ea9cb",
    "kind-empty": "#8e9683",
    "badge-text": "#151c16",
    "titlebar-top": "#1d2a1e",
    "titlebar-bottom": "#151c16",
    "logo-from": "#9bcb74",
    "logo-to": "#476930",
    "logo-text": "#151c16",
    hl: "rgba(155, 203, 116, 0.24)",
    "hl-row": "rgba(155, 203, 116, 0.12)",
    "scroll-thumb": "rgba(155, 173, 144, 0.24)",
    "scroll-thumb-hover": "rgba(155, 173, 144, 0.44)",
  },
  light: {
    bg: "#edf1e4",
    "bg-elevated": "#e3e9d3",
    "bg-grid": "#f3f6e8",
    "bg-row-alt": "#e7ecd8",
    "bg-row-hover": "#d5dec1",
    "bg-row-selected": "#c6d3ab",
    border: "#d4dcc1",
    "border-strong": "#a8b68c",
    text: "#2a3522",
    "text-muted": "#5e6c4e",
    "text-dim": "#94a080",
    accent: "#4c7a2d",
    "accent-strong": "#365819",
    success: "#4c7a2d",
    warning: "#a16a12",
    danger: "#9c3e28",
    "kind-integer": "#7a5e1a",
    "kind-float": "#9c5a26",
    "kind-boolean": "#5e4a9e",
    "kind-date": "#4c7a2d",
    "kind-string": "#2a5a7e",
    "kind-empty": "#7c8268",
    "badge-text": "#f3f6e8",
    "titlebar-top": "#e6ecd4",
    "titlebar-bottom": "#d9e0c0",
    "logo-from": "#78ad50",
    "logo-to": "#365819",
    "logo-text": "#f3f6e8",
    hl: "rgba(76, 122, 45, 0.2)",
    "hl-row": "rgba(76, 122, 45, 0.08)",
    "scroll-thumb": "rgba(42, 53, 34, 0.22)",
    "scroll-thumb-hover": "rgba(42, 53, 34, 0.4)",
  },
};

// --- Graphite — neutral monochrome -----------------------------------------
const graphite: Palette = {
  id: "graphite",
  name: "Graphite",
  description: "Neutral monochrome with a sage accent",
  dark: {
    bg: "#1a1a1a",
    "bg-elevated": "#242424",
    "bg-grid": "#151515",
    "bg-row-alt": "#1e1e1e",
    "bg-row-hover": "#2c2c2c",
    "bg-row-selected": "#3a3a3a",
    border: "#2d2d2d",
    "border-strong": "#424242",
    text: "#e0e0e0",
    "text-muted": "#9a9a9a",
    "text-dim": "#606060",
    accent: "#7fb285",
    "accent-strong": "#5f9a67",
    success: "#7fb285",
    warning: "#d6a55a",
    danger: "#c9776c",
    "kind-integer": "#d6a55a",
    "kind-float": "#d68d5a",
    "kind-boolean": "#a99fd0",
    "kind-date": "#7fb285",
    "kind-string": "#8aa6c7",
    "kind-empty": "#8a8a8a",
    "badge-text": "#1a1a1a",
    "titlebar-top": "#262626",
    "titlebar-bottom": "#1a1a1a",
    "logo-from": "#7fb285",
    "logo-to": "#466a4b",
    "logo-text": "#1a1a1a",
    hl: "rgba(127, 178, 133, 0.24)",
    "hl-row": "rgba(127, 178, 133, 0.1)",
    "scroll-thumb": "rgba(200, 200, 200, 0.2)",
    "scroll-thumb-hover": "rgba(200, 200, 200, 0.38)",
  },
  light: {
    bg: "#f4f4f4",
    "bg-elevated": "#e9e9e9",
    "bg-grid": "#fbfbfb",
    "bg-row-alt": "#eeeeee",
    "bg-row-hover": "#e0e0e0",
    "bg-row-selected": "#cfcfcf",
    border: "#dcdcdc",
    "border-strong": "#b5b5b5",
    text: "#1f1f1f",
    "text-muted": "#5a5a5a",
    "text-dim": "#8a8a8a",
    accent: "#3f8144",
    "accent-strong": "#2c5f30",
    success: "#3f8144",
    warning: "#9a6a10",
    danger: "#8d3d34",
    "kind-integer": "#8a5a1c",
    "kind-float": "#9c5a2e",
    "kind-boolean": "#4e4192",
    "kind-date": "#3f8144",
    "kind-string": "#2e5a8c",
    "kind-empty": "#6a6a6a",
    "badge-text": "#fbfbfb",
    "titlebar-top": "#ededed",
    "titlebar-bottom": "#e2e2e2",
    "logo-from": "#5f9a67",
    "logo-to": "#2c5f30",
    "logo-text": "#fbfbfb",
    hl: "rgba(63, 129, 68, 0.2)",
    "hl-row": "rgba(63, 129, 68, 0.08)",
    "scroll-thumb": "rgba(31, 31, 31, 0.2)",
    "scroll-thumb-hover": "rgba(31, 31, 31, 0.38)",
  },
};

export const PALETTES: Record<PaletteId, Palette> = {
  parchment,
  noir,
  solarized,
  ocean,
  forest,
  graphite,
};

export const PALETTE_LIST: Palette[] = [
  parchment,
  noir,
  solarized,
  ocean,
  forest,
  graphite,
];

export const DEFAULT_PALETTE: PaletteId = "parchment";

export function isPaletteId(x: unknown): x is PaletteId {
  return typeof x === "string" && x in PALETTES;
}

/**
 * Apply a palette + theme variant to the root element. Replaces the previous
 * CSS-only approach — every variable is set explicitly as an inline style on
 * :root so there's no cascade ambiguity.
 */
export function applyPalette(
  paletteId: PaletteId,
  resolved: "light" | "dark",
): void {
  if (typeof document === "undefined") return;
  const palette = PALETTES[paletteId] ?? PALETTES[DEFAULT_PALETTE];
  const vars = palette[resolved];
  const root = document.documentElement;
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(`--${key}`, value);
  }
  root.setAttribute("data-theme", resolved);
  root.setAttribute("data-palette", paletteId);
  root.style.colorScheme = resolved;
}
