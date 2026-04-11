import { describe, it, expect, vi } from "vitest";
import { RangeCache } from "./rangeCache";

describe("RangeCache", () => {
  const makePage = (start: number, end: number): string[][] =>
    Array.from({ length: end - start }, (_, i) => [String(start + i)]);

  it("fetches pages on demand and serves cached rows", async () => {
    const fetcher = vi.fn(async (start: number, end: number) => makePage(start, end));
    const cache = new RangeCache(fetcher, { pageSize: 100 });

    await cache.ensure(0, 50);
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(cache.get(0)).toEqual(["0"]);
    expect(cache.get(49)).toEqual(["49"]);

    // Row 100 isn't loaded yet.
    expect(cache.get(100)).toBeUndefined();

    await cache.ensure(100, 120);
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(cache.get(100)).toEqual(["100"]);
  });

  it("does not re-fetch a page already in memory", async () => {
    const fetcher = vi.fn(async (start: number, end: number) => makePage(start, end));
    const cache = new RangeCache(fetcher, { pageSize: 50 });

    await cache.ensure(0, 10);
    await cache.ensure(0, 20);
    await cache.ensure(10, 30);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("spans requests over multiple pages", async () => {
    const fetcher = vi.fn(async (start: number, end: number) => makePage(start, end));
    const cache = new RangeCache(fetcher, { pageSize: 10 });
    await cache.ensure(5, 25);
    expect(fetcher).toHaveBeenCalledTimes(3); // pages 0, 1, 2
  });

  it("invalidate drops all pages and bumps generation", async () => {
    const fetcher = vi.fn(async (start: number, end: number) => makePage(start, end));
    const cache = new RangeCache(fetcher, { pageSize: 10 });
    await cache.ensure(0, 10);
    expect(cache.get(0)).toBeDefined();
    cache.invalidate();
    expect(cache.get(0)).toBeUndefined();
    await cache.ensure(0, 10);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("evicts oldest pages beyond maxPages", async () => {
    const fetcher = vi.fn(async (start: number, end: number) => makePage(start, end));
    const cache = new RangeCache(fetcher, { pageSize: 10, maxPages: 3 });
    await cache.ensure(0, 10);
    await cache.ensure(10, 20);
    await cache.ensure(20, 30);
    await cache.ensure(30, 40);
    expect(cache.get(0)).toBeUndefined(); // evicted
    expect(cache.get(30)).toBeDefined();
  });

  it("deduplicates concurrent fetches for the same page", async () => {
    let resolveFn: (v: string[][]) => void = () => {};
    const fetcher = vi.fn(
      () =>
        new Promise<string[][]>((r) => {
          resolveFn = r;
        }),
    );
    const cache = new RangeCache(fetcher, { pageSize: 10 });
    const a = cache.ensure(0, 10);
    const b = cache.ensure(0, 10);
    expect(fetcher).toHaveBeenCalledTimes(1);
    resolveFn(makePage(0, 10));
    await Promise.all([a, b]);
    expect(cache.get(5)).toEqual(["5"]);
  });
});
