import { describe, it, expect, vi } from "vitest";
import { HistoryStack } from "./history";

function makeEntry(label: string, log: string[]) {
  return {
    label,
    undo: () => {
      log.push(`undo:${label}`);
    },
    redo: () => {
      log.push(`redo:${label}`);
    },
  };
}

describe("HistoryStack", () => {
  it("starts empty and not undoable/redoable", () => {
    const h = new HistoryStack();
    expect(h.size).toBe(0);
    expect(h.undoable).toBe(false);
    expect(h.redoable).toBe(false);
  });

  it("push sets undoable but not redoable", () => {
    const h = new HistoryStack();
    const log: string[] = [];
    h.push(makeEntry("a", log));
    expect(h.undoable).toBe(true);
    expect(h.redoable).toBe(false);
  });

  it("undo moves cursor back and calls undo fn", async () => {
    const h = new HistoryStack();
    const log: string[] = [];
    h.push(makeEntry("a", log));
    h.push(makeEntry("b", log));
    await h.undo();
    expect(log).toEqual(["undo:b"]);
    expect(h.undoable).toBe(true);
    expect(h.redoable).toBe(true);
  });

  it("redo reapplies the undone entry", async () => {
    const h = new HistoryStack();
    const log: string[] = [];
    h.push(makeEntry("a", log));
    await h.undo();
    await h.redo();
    expect(log).toEqual(["undo:a", "redo:a"]);
  });

  it("pushing after undo truncates the redo branch", async () => {
    const h = new HistoryStack();
    const log: string[] = [];
    h.push(makeEntry("a", log));
    h.push(makeEntry("b", log));
    h.push(makeEntry("c", log));
    await h.undo(); // undo c
    await h.undo(); // undo b
    h.push(makeEntry("d", log)); // truncates b, c from redo
    expect(h.redoable).toBe(false);
    expect(h.size).toBe(2); // a, d
  });

  it("undo and redo are no-ops at the boundaries", async () => {
    const h = new HistoryStack();
    expect(await h.undo()).toBeNull();
    expect(await h.redo()).toBeNull();
  });

  it("respects the entry limit by dropping oldest", () => {
    const h = new HistoryStack(3);
    const log: string[] = [];
    h.push(makeEntry("a", log));
    h.push(makeEntry("b", log));
    h.push(makeEntry("c", log));
    h.push(makeEntry("d", log));
    expect(h.size).toBe(3);
    expect(h.peekUndo()?.label).toBe("d");
  });

  it("awaits async undo/redo functions", async () => {
    const h = new HistoryStack();
    const undo = vi.fn().mockResolvedValue(undefined);
    const redo = vi.fn().mockResolvedValue(undefined);
    h.push({ label: "x", undo, redo });
    await h.undo();
    await h.redo();
    expect(undo).toHaveBeenCalled();
    expect(redo).toHaveBeenCalled();
  });

  it("clear wipes everything", () => {
    const h = new HistoryStack();
    const log: string[] = [];
    h.push(makeEntry("a", log));
    h.clear();
    expect(h.size).toBe(0);
    expect(h.undoable).toBe(false);
  });
});
