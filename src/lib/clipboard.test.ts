import { describe, it, expect } from "vitest";
import { encodeTsv, decodeTsv } from "./clipboard";

describe("encodeTsv", () => {
  it("joins cells with tabs and rows with newlines", () => {
    expect(encodeTsv([["a", "b"], ["c", "d"]])).toBe("a\tb\nc\td");
  });
  it("quotes cells containing tab, newline, or quotes", () => {
    expect(encodeTsv([["hi\there"]])).toBe('"hi\there"');
    expect(encodeTsv([['say "hi"']])).toBe('"say ""hi"""');
    expect(encodeTsv([["line1\nline2"]])).toBe('"line1\nline2"');
  });
  it("handles a single cell", () => {
    expect(encodeTsv([["hello"]])).toBe("hello");
  });
  it("roundtrips through decode", () => {
    const data = [
      ["plain", "with\ttab", 'with "quote"'],
      ["line\nbreak", "", "end"],
    ];
    expect(decodeTsv(encodeTsv(data))).toEqual(data);
  });
});

describe("decodeTsv", () => {
  it("parses a simple grid", () => {
    expect(decodeTsv("a\tb\nc\td")).toEqual([
      ["a", "b"],
      ["c", "d"],
    ]);
  });
  it("returns a single cell for plain text", () => {
    expect(decodeTsv("hello")).toEqual([["hello"]]);
  });
  it("returns an empty cell for an empty string", () => {
    expect(decodeTsv("")).toEqual([[""]]);
  });
  it("normalizes CRLF to LF", () => {
    expect(decodeTsv("a\r\nb")).toEqual([["a"], ["b"]]);
  });
  it("strips a trailing newline", () => {
    expect(decodeTsv("a\nb\n")).toEqual([["a"], ["b"]]);
  });
  it("parses quoted fields with internal tabs", () => {
    expect(decodeTsv('"a\tb"\tc')).toEqual([["a\tb", "c"]]);
  });
  it("parses quoted fields with internal newlines", () => {
    expect(decodeTsv('"line1\nline2"\tnext')).toEqual([["line1\nline2", "next"]]);
  });
  it("unescapes doubled quotes", () => {
    expect(decodeTsv('"say ""hi"""')).toEqual([['say "hi"']]);
  });
});
