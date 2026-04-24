import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Polyfill ResizeObserver for @tanstack/react-virtual
class ResizeObserverMock {
  constructor(_cb: unknown) {}
  observe() {}
  unobserve() {}
  disconnect() {}
}
(globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver =
  ResizeObserverMock;

// Polyfill scrollTo on HTMLElement used by the virtualizer
if (!HTMLElement.prototype.scrollTo) {
  HTMLElement.prototype.scrollTo = function () {};
}

// Polyfill matchMedia for the theme hook. Tests can override via
// `window.matchMedia = vi.fn(...)`.
if (typeof window.matchMedia === "undefined") {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    configurable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

// Polyfill layout so recharts' ResponsiveContainer thinks it has space.
// jsdom returns 0×0 for everything, which makes recharts skip rendering
// the entire chart. Lying here lets ChartView tests verify SVG output.
if (typeof Element !== "undefined") {
  const originalGetBCR = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function () {
    const rect = originalGetBCR.call(this);
    if (rect.width === 0 && rect.height === 0) {
      return {
        x: 0,
        y: 0,
        top: 0,
        left: 0,
        right: 600,
        bottom: 300,
        width: 600,
        height: 300,
        toJSON: () => ({}),
      } as DOMRect;
    }
    return rect;
  };
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
    configurable: true,
    get: () => 600,
  });
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get: () => 300,
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get: () => 600,
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get: () => 300,
  });
}

// Stub the Tauri invoke system so components can render in jsdom.
// Tests that need behavior can override `window.__TAURI_INVOKE__`.
(globalThis as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: vi.fn(async () => ({})),
  transformCallback: (cb: unknown) => cb,
  metadata: { windows: [] },
};
