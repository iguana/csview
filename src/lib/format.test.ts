import { describe, it, expect } from "vitest";
import { formatBytes, formatNumber, formatCount, basename } from "./format";

describe("formatBytes", () => {
  it("formats bytes under 1KB", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
  });
  it("formats KB/MB/GB", () => {
    expect(formatBytes(1024)).toBe("1.00 KB");
    expect(formatBytes(1536)).toBe("1.50 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.00 MB");
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1.00 GB");
  });
  it("drops decimals at large scales", () => {
    expect(formatBytes(256 * 1024)).toBe("256 KB");
  });
});

describe("formatNumber", () => {
  it("handles nulls", () => {
    expect(formatNumber(null)).toBe("—");
    expect(formatNumber(undefined)).toBe("—");
    expect(formatNumber(Number.NaN)).toBe("—");
  });
  it("formats integers without trailing zeros", () => {
    expect(formatNumber(42)).toBe("42");
    expect(formatNumber(1_000_000)).toBe("1,000,000");
  });
  it("formats floats with limited digits", () => {
    expect(formatNumber(3.14159)).toBe("3.14");
  });
  it("uses exponential for very small or very large", () => {
    expect(formatNumber(0.0001)).toBe("1.00e-4");
    expect(formatNumber(5e12)).toBe("5.00e+12");
  });
});

describe("formatCount", () => {
  it("adds thousand separators", () => {
    expect(formatCount(12_345)).toBe("12,345");
  });
});

describe("basename", () => {
  it("extracts the file name", () => {
    expect(basename("/tmp/foo/bar.csv")).toBe("bar.csv");
    expect(basename("bar.csv")).toBe("bar.csv");
    expect(basename("C:\\Users\\me\\bar.csv")).toBe("bar.csv");
  });
});
