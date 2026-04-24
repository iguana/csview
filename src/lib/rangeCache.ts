/**
 * Caches CSV row ranges fetched from the backend in fixed-size pages.
 * The grid asks for any row, the cache answers from memory when possible
 * and batches fetches per page. Pages are dropped when their generation
 * is bumped (e.g. after sort changes).
 */
export type PageFetcher = (start: number, end: number) => Promise<string[][]>;

export interface RangeCacheOptions {
  pageSize?: number;
  maxPages?: number;
}

export class RangeCache {
  private pages = new Map<number, string[][]>();
  private pending = new Map<number, Promise<string[][]>>();
  private order: number[] = [];
  private generation = 0;
  public readonly pageSize: number;
  public readonly maxPages: number;

  constructor(
    private fetcher: PageFetcher,
    { pageSize = 200, maxPages = 64 }: RangeCacheOptions = {},
  ) {
    this.pageSize = pageSize;
    this.maxPages = maxPages;
  }

  invalidate(): void {
    this.generation++;
    this.pages.clear();
    this.pending.clear();
    this.order = [];
  }

  get(row: number): string[] | undefined {
    const pageIdx = Math.floor(row / this.pageSize);
    const page = this.pages.get(pageIdx);
    if (!page) return undefined;
    return page[row - pageIdx * this.pageSize];
  }

  /** Iterate every row currently held in cache, in arbitrary page order. */
  *loadedRows(): IterableIterator<{ index: number; row: string[] }> {
    for (const [pageIdx, page] of this.pages) {
      const start = pageIdx * this.pageSize;
      for (let i = 0; i < page.length; i++) {
        yield { index: start + i, row: page[i] };
      }
    }
  }

  async ensure(startRow: number, endRow: number): Promise<void> {
    const firstPage = Math.floor(startRow / this.pageSize);
    const lastPage = Math.floor(Math.max(startRow, endRow - 1) / this.pageSize);
    const promises: Promise<unknown>[] = [];
    for (let p = firstPage; p <= lastPage; p++) {
      if (!this.pages.has(p) && !this.pending.has(p)) {
        promises.push(this.fetchPage(p));
      }
    }
    await Promise.all(promises);
  }

  private async fetchPage(pageIdx: number): Promise<string[][]> {
    const start = pageIdx * this.pageSize;
    const end = start + this.pageSize;
    const gen = this.generation;
    const promise = this.fetcher(start, end).then((rows) => {
      if (gen !== this.generation) return rows;
      this.pages.set(pageIdx, rows);
      this.order.push(pageIdx);
      this.evictIfNeeded();
      this.pending.delete(pageIdx);
      return rows;
    });
    this.pending.set(pageIdx, promise);
    return promise;
  }

  private evictIfNeeded(): void {
    while (this.order.length > this.maxPages) {
      const oldest = this.order.shift();
      if (oldest != null) this.pages.delete(oldest);
    }
  }
}
