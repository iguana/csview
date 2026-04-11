import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import {
  useTheme,
  resolveTheme,
  nextMode,
  modeLabel,
  modeIcon,
  readStoredMode,
} from "./theme";
import { PALETTES } from "./palettes";

function mockMatchMedia(prefersLight: boolean) {
  const listeners: Array<(e: MediaQueryListEvent) => void> = [];
  const mq = {
    matches: prefersLight,
    media: "(prefers-color-scheme: light)",
    onchange: null,
    addEventListener: (_event: string, cb: (e: MediaQueryListEvent) => void) => {
      listeners.push(cb);
    },
    removeEventListener: (
      _event: string,
      cb: (e: MediaQueryListEvent) => void,
    ) => {
      const i = listeners.indexOf(cb);
      if (i >= 0) listeners.splice(i, 1);
    },
    dispatchEvent: () => false,
    addListener: () => {},
    removeListener: () => {},
  };
  window.matchMedia = vi.fn().mockReturnValue(mq);
  return {
    mq,
    fire(matches: boolean) {
      mq.matches = matches;
      for (const cb of listeners) {
        cb({ matches } as unknown as MediaQueryListEvent);
      }
    },
  };
}

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.removeAttribute("data-palette");
  // Clear any CSS vars set by a previous test run.
  document.documentElement.removeAttribute("style");
  mockMatchMedia(false);
});

describe("nextMode", () => {
  it("cycles system → light → dark → system", () => {
    expect(nextMode("system")).toBe("light");
    expect(nextMode("light")).toBe("dark");
    expect(nextMode("dark")).toBe("system");
  });
});

describe("modeLabel & modeIcon", () => {
  it("returns correct labels", () => {
    expect(modeLabel("system")).toBe("Auto");
    expect(modeLabel("light")).toBe("Light");
    expect(modeLabel("dark")).toBe("Dark");
  });
  it("returns icons per mode", () => {
    expect(modeIcon("system")).toBe("◐");
    expect(modeIcon("light")).toBe("☀");
    expect(modeIcon("dark")).toBe("☾");
  });
});

describe("resolveTheme", () => {
  it("passes through explicit modes", () => {
    expect(resolveTheme("light")).toBe("light");
    expect(resolveTheme("dark")).toBe("dark");
  });
  it("resolves system to light when prefers-color-scheme is light", () => {
    mockMatchMedia(true);
    expect(resolveTheme("system")).toBe("light");
  });
  it("resolves system to dark when prefers-color-scheme is not light", () => {
    mockMatchMedia(false);
    expect(resolveTheme("system")).toBe("dark");
  });
});

describe("readStoredMode", () => {
  it("defaults to system when no value is stored", () => {
    expect(readStoredMode()).toBe("system");
  });
  it("returns stored value", () => {
    localStorage.setItem("csview.theme", "light");
    expect(readStoredMode()).toBe("light");
  });
  it("ignores invalid stored values", () => {
    localStorage.setItem("csview.theme", "purple");
    expect(readStoredMode()).toBe("system");
  });
});

describe("useTheme palette application", () => {
  it("writes palette CSS variables to :root when the hook runs", () => {
    mockMatchMedia(false);
    renderHook(() => useTheme());
    const bg = document.documentElement.style.getPropertyValue("--bg");
    expect(bg).toBeTruthy();
    // Should match one of the registered palette backgrounds.
    const allBgs = Object.values(PALETTES).flatMap((p) => [p.dark.bg, p.light.bg]);
    expect(allBgs).toContain(bg);
  });
  it("setPalette changes the applied --accent variable", () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    const before = document.documentElement.style.getPropertyValue("--accent");
    act(() => result.current.setPalette("ocean"));
    const after = document.documentElement.style.getPropertyValue("--accent");
    expect(after).not.toBe(before);
    // Ocean dark accent
    expect(after).toBe(PALETTES.ocean.dark.accent);
  });
});

describe("useTheme", () => {
  it("defaults to system mode and resolves based on matchMedia", () => {
    mockMatchMedia(true); // prefers light
    const { result } = renderHook(() => useTheme());
    expect(result.current.mode).toBe("system");
    expect(result.current.resolved).toBe("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("resolves to dark when system prefers dark", () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    expect(result.current.resolved).toBe("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("setMode persists to localStorage and updates resolved", () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    act(() => result.current.setMode("light"));
    expect(result.current.mode).toBe("light");
    expect(result.current.resolved).toBe("light");
    expect(localStorage.getItem("csview.theme")).toBe("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("cycle advances through all three modes", () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    expect(result.current.mode).toBe("system");
    act(() => result.current.cycle());
    expect(result.current.mode).toBe("light");
    act(() => result.current.cycle());
    expect(result.current.mode).toBe("dark");
    act(() => result.current.cycle());
    expect(result.current.mode).toBe("system");
  });

  it("reacts to OS preference changes when in system mode", () => {
    const mm = mockMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    expect(result.current.resolved).toBe("dark");
    act(() => mm.fire(true));
    expect(result.current.resolved).toBe("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("does not react to OS preference when an explicit mode is set", () => {
    const mm = mockMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    act(() => result.current.setMode("dark"));
    act(() => mm.fire(true));
    expect(result.current.resolved).toBe("dark");
  });

  it("hydrates from localStorage", () => {
    localStorage.setItem("csview.theme", "light");
    mockMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    expect(result.current.mode).toBe("light");
    expect(result.current.resolved).toBe("light");
  });
});
