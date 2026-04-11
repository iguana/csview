/**
 * A small undo/redo ring. Each entry is an opaque `do` and `undo` function —
 * the caller is responsible for issuing the backend mutation. Pushing after
 * an undo truncates the redo branch (the canonical branch behavior most apps
 * get wrong).
 */

export interface HistoryEntry {
  label: string;
  undo: () => Promise<void> | void;
  redo: () => Promise<void> | void;
}

export class HistoryStack {
  private entries: HistoryEntry[] = [];
  private cursor = 0; // number of committed entries (cursor points to next redo)
  public readonly limit: number;

  constructor(limit = 200) {
    this.limit = limit;
  }

  get size(): number {
    return this.entries.length;
  }

  get undoable(): boolean {
    return this.cursor > 0;
  }

  get redoable(): boolean {
    return this.cursor < this.entries.length;
  }

  peekUndo(): HistoryEntry | undefined {
    return this.cursor > 0 ? this.entries[this.cursor - 1] : undefined;
  }

  peekRedo(): HistoryEntry | undefined {
    return this.cursor < this.entries.length
      ? this.entries[this.cursor]
      : undefined;
  }

  push(entry: HistoryEntry): void {
    // Drop any redo branch after the current cursor.
    this.entries.length = this.cursor;
    this.entries.push(entry);
    this.cursor = this.entries.length;
    // Cap the oldest history to keep memory bounded.
    while (this.entries.length > this.limit) {
      this.entries.shift();
      this.cursor--;
    }
  }

  async undo(): Promise<HistoryEntry | null> {
    if (!this.undoable) return null;
    this.cursor--;
    const entry = this.entries[this.cursor];
    await entry.undo();
    return entry;
  }

  async redo(): Promise<HistoryEntry | null> {
    if (!this.redoable) return null;
    const entry = this.entries[this.cursor];
    this.cursor++;
    await entry.redo();
    return entry;
  }

  clear(): void {
    this.entries = [];
    this.cursor = 0;
  }
}
