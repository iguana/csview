import { useCallback, useEffect, useState } from "react";
import {
  applyPalette,
  DEFAULT_PALETTE,
  isPaletteId,
  type PaletteId,
} from "./palettes";

export type ThemeMode = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const MODE_KEY = "csview.theme";
const PALETTE_KEY = "csview.palette";

export function readStoredMode(): ThemeMode {
  try {
    const raw = localStorage.getItem(MODE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    // localStorage may be unavailable (private mode, tests)
  }
  return "system";
}

export function readStoredPalette(): PaletteId {
  try {
    const raw = localStorage.getItem(PALETTE_KEY);
    if (isPaletteId(raw)) return raw;
  } catch {
    // ignore
  }
  return DEFAULT_PALETTE;
}

export function resolveTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === "light" || mode === "dark") return mode;
  if (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: light)").matches
  ) {
    return "light";
  }
  return "dark";
}

export function nextMode(mode: ThemeMode): ThemeMode {
  if (mode === "system") return "light";
  if (mode === "light") return "dark";
  return "system";
}

export function modeLabel(mode: ThemeMode): string {
  if (mode === "light") return "Light";
  if (mode === "dark") return "Dark";
  return "Auto";
}

export function modeIcon(mode: ThemeMode): string {
  if (mode === "light") return "☀";
  if (mode === "dark") return "☾";
  return "◐";
}

export function useTheme(): {
  mode: ThemeMode;
  resolved: ResolvedTheme;
  palette: PaletteId;
  setMode: (m: ThemeMode) => void;
  setPalette: (p: PaletteId) => void;
  cycle: () => void;
} {
  const [mode, setModeState] = useState<ThemeMode>(() => readStoredMode());
  const [palette, setPaletteState] = useState<PaletteId>(() => readStoredPalette());
  const [resolved, setResolved] = useState<ResolvedTheme>(() =>
    resolveTheme(readStoredMode()),
  );

  const setMode = useCallback((m: ThemeMode) => {
    try {
      localStorage.setItem(MODE_KEY, m);
    } catch {
      // ignore
    }
    setModeState(m);
  }, []);

  const setPalette = useCallback((p: PaletteId) => {
    try {
      localStorage.setItem(PALETTE_KEY, p);
    } catch {
      // ignore
    }
    setPaletteState(p);
  }, []);

  const cycle = useCallback(() => setMode(nextMode(mode)), [mode, setMode]);

  useEffect(() => {
    const next = resolveTheme(mode);
    setResolved(next);
    applyPalette(palette, next);
    if (mode !== "system") return;
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = () => {
      const r = resolveTheme("system");
      setResolved(r);
      applyPalette(palette, r);
    };
    if (mq.addEventListener) mq.addEventListener("change", onChange);
    else if ("addListener" in mq) (mq as MediaQueryList).addListener(onChange);
    return () => {
      if (mq.removeEventListener) mq.removeEventListener("change", onChange);
      else if ("removeListener" in mq)
        (mq as MediaQueryList).removeListener(onChange);
    };
  }, [mode, palette]);

  return { mode, resolved, palette, setMode, setPalette, cycle };
}
