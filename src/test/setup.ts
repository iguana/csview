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

// Stub the Tauri invoke system so components can render in jsdom.
// Tests that need behavior can override `window.__TAURI_INVOKE__`.
(globalThis as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: vi.fn(async () => ({})),
  transformCallback: (cb: unknown) => cb,
  metadata: { windows: [] },
};
