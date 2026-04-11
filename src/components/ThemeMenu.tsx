import { useEffect, useRef, useState } from "react";
import { modeIcon, modeLabel, type ThemeMode, type ResolvedTheme } from "../lib/theme";
import { PALETTE_LIST, type PaletteId } from "../lib/palettes";

export interface ThemeMenuProps {
  theme: {
    mode: ThemeMode;
    resolved: ResolvedTheme;
    palette: PaletteId;
    setMode: (m: ThemeMode) => void;
    setPalette: (p: PaletteId) => void;
  };
}

/**
 * Compact dropdown for theme mode (system/light/dark) + palette picker. Lives
 * in the titlebar. Opens on click, closes on outside-click or Escape.
 */
export function ThemeMenu({ theme }: ThemeMenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!ref.current) return;
      if (!ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onEsc);
    };
  }, [open]);

  return (
    <div className="theme-menu" ref={ref}>
      <button
        className="theme-toggle"
        onClick={() => setOpen((v) => !v)}
        title={`Theme: ${modeLabel(theme.mode)}${
          theme.mode === "system" ? ` (using ${theme.resolved})` : ""
        }. Palette: ${theme.palette}. Click to change.`}
        aria-haspopup="menu"
        aria-expanded={open}
        data-testid="theme-toggle"
      >
        <span className="icon" aria-hidden>
          {modeIcon(theme.mode)}
        </span>
        <span className="theme-toggle-label">
          {modeLabel(theme.mode)} · {theme.palette}
        </span>
        <span className="theme-caret" aria-hidden>▾</span>
      </button>
      {open && (
        <div className="theme-dropdown" role="menu" data-testid="theme-dropdown">
          <div className="theme-section-label">Appearance</div>
          <div className="theme-mode-row">
            {(["system", "light", "dark"] as ThemeMode[]).map((m) => (
              <button
                key={m}
                className={`theme-mode-btn ${theme.mode === m ? "active" : ""}`}
                onClick={() => theme.setMode(m)}
                data-testid={`theme-mode-${m}`}
              >
                <span aria-hidden>{modeIcon(m)}</span>
                {modeLabel(m)}
              </button>
            ))}
          </div>
          <div className="theme-section-label">Palette</div>
          <div className="theme-palette-list">
            {PALETTE_LIST.map((p) => {
              const active = theme.palette === p.id;
              const sample = p[theme.resolved];
              return (
                <button
                  key={p.id}
                  className={`palette-row ${active ? "active" : ""}`}
                  onClick={() => {
                    theme.setPalette(p.id);
                    setOpen(false);
                  }}
                  data-testid={`palette-${p.id}`}
                >
                  <span
                    className="palette-swatch"
                    style={{
                      background: sample.bg,
                      borderColor: sample.border,
                    }}
                  >
                    <span
                      className="palette-swatch-bar"
                      style={{ background: sample.accent }}
                    />
                    <span
                      className="palette-swatch-chip"
                      style={{ background: sample["kind-string"] }}
                    />
                    <span
                      className="palette-swatch-chip"
                      style={{ background: sample["kind-integer"] }}
                    />
                  </span>
                  <span className="palette-meta">
                    <span className="palette-name">{p.name}</span>
                    <span className="palette-desc">{p.description}</span>
                  </span>
                  {active && <span className="palette-check" aria-hidden>✓</span>}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
