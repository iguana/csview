/**
 * Tests that every invoke() call in api-ai.ts sends the correct parameter keys
 * that Tauri 2 expects. Tauri auto-converts JS camelCase → Rust snake_case,
 * so the JS must use camelCase for multi-word params (fileId, sessionId, etc).
 *
 * These tests catch mismatches that cause runtime "missing required key" errors.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const mockInvoke = vi.fn(async (..._args: unknown[]) => ({}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import { aiApi, csvApi } from "./api-ai";

function getArgs(): Record<string, unknown> {
  return (mockInvoke.mock.calls[0] as unknown[])[1] as Record<string, unknown>;
}

function getCmd(): string {
  return (mockInvoke.mock.calls[0] as unknown[])[0] as string;
}

beforeEach(() => {
  mockInvoke.mockClear();
  mockInvoke.mockResolvedValue({});
});

// ---- AI commands ----

describe("aiApi parameter keys match Rust (camelCase for Tauri auto-convert)", () => {
  it("setApiKey sends provider, key, model", async () => {
    await aiApi.setApiKey("openai", "sk-test", "gpt-4.1");
    expect(getCmd()).toBe("set_api_key");
    expect(getArgs()).toEqual({ provider: "openai", key: "sk-test", model: "gpt-4.1" });
  });

  it("nlQuery sends fileId, query", async () => {
    await aiApi.nlQuery("fid", "show engineers");
    expect(getCmd()).toBe("nl_query");
    expect(getArgs()).toHaveProperty("fileId", "fid");
    expect(getArgs()).toHaveProperty("query", "show engineers");
  });

  it("generateProfile sends fileId", async () => {
    await aiApi.generateProfile("fid");
    expect(getArgs()).toHaveProperty("fileId", "fid");
  });

  it("nlTransform sends fileId, query", async () => {
    await aiApi.nlTransform("fid", "uppercase name");
    expect(getArgs()).toHaveProperty("fileId", "fid");
    expect(getArgs()).toHaveProperty("query");
  });

  it("detectAnomalies sends fileId, columns", async () => {
    await aiApi.detectAnomalies("fid", ["salary"]);
    expect(getArgs()).toHaveProperty("fileId", "fid");
    expect(getArgs()).toHaveProperty("columns", ["salary"]);
  });

  it("smartGroup sends fileId, query", async () => {
    await aiApi.smartGroup("fid", "avg salary by dept");
    expect(getArgs()).toHaveProperty("fileId");
  });

  it("auditQuality sends fileId", async () => {
    await aiApi.auditQuality("fid");
    expect(getArgs()).toHaveProperty("fileId");
  });

  it("chatMessage sends fileId, sessionId, message", async () => {
    await aiApi.chatMessage("fid", "sid", "hello");
    expect(getCmd()).toBe("chat_message");
    expect(getArgs()).toHaveProperty("fileId", "fid");
    expect(getArgs()).toHaveProperty("sessionId", "sid");
    expect(getArgs()).toHaveProperty("message", "hello");
  });

  it("newChatSession sends fileId", async () => {
    await aiApi.newChatSession("fid");
    expect(getCmd()).toBe("new_chat_session");
    expect(getArgs()).toHaveProperty("fileId", "fid");
  });

  it("generateReport sends fileId, request", async () => {
    await aiApi.generateReport("fid", "summary");
    expect(getArgs()).toHaveProperty("fileId");
    expect(getArgs()).toHaveProperty("request");
  });

  it("suggestJoin sends leftId, rightId", async () => {
    await aiApi.suggestJoin("lid", "rid");
    expect(getArgs()).toHaveProperty("leftId", "lid");
    expect(getArgs()).toHaveProperty("rightId", "rid");
  });

  it("executeJoin sends leftId, rightId, leftKey, rightKey, joinType", async () => {
    await aiApi.executeJoin("lid", "rid", "email", "email", "inner");
    expect(getArgs()).toHaveProperty("leftId");
    expect(getArgs()).toHaveProperty("rightId");
    expect(getArgs()).toHaveProperty("leftKey");
    expect(getArgs()).toHaveProperty("rightKey");
    expect(getArgs()).toHaveProperty("joinType");
  });

  it("complianceScan sends fileId", async () => {
    await aiApi.complianceScan("fid");
    expect(getArgs()).toHaveProperty("fileId");
  });

  it("forecast sends fileId, xCol, yCol", async () => {
    await aiApi.forecast("fid", "date", "revenue");
    expect(getArgs()).toHaveProperty("fileId");
    expect(getArgs()).toHaveProperty("xCol", "date");
    expect(getArgs()).toHaveProperty("yCol", "revenue");
  });
});

// ---- CSV commands ----

describe("csvApi parameter keys match Rust (camelCase)", () => {
  it("openCsv sends path", async () => {
    await csvApi.openCsv("/tmp/test.csv");
    expect(getCmd()).toBe("open_csv");
    expect(getArgs()).toHaveProperty("path");
  });

  it("queryData sends fileId, sql", async () => {
    await csvApi.queryData("fid", "SELECT * FROM data");
    expect(getArgs()).toHaveProperty("fileId");
    expect(getArgs()).toHaveProperty("sql");
  });

  it("readRange sends fileId, offset, limit, orderBy", async () => {
    await csvApi.readRange("fid", 0, 100, "salary DESC");
    expect(getArgs()).toHaveProperty("fileId");
    expect(getArgs()).toHaveProperty("offset", 0);
    expect(getArgs()).toHaveProperty("limit", 100);
    expect(getArgs()).toHaveProperty("orderBy", "salary DESC");
  });

  it("readRange without orderBy sends null", async () => {
    await csvApi.readRange("fid", 0, 100);
    expect(getArgs()).toHaveProperty("orderBy", null);
  });

  it("updateCell sends fileId, rowid, column, value", async () => {
    await csvApi.updateCell("fid", 1, "name", "Alice");
    expect(getArgs()).toEqual({ fileId: "fid", rowid: 1, column: "name", value: "Alice" });
  });

  it("insertRow sends fileId, values", async () => {
    await csvApi.insertRow("fid", { name: "Test" });
    expect(getArgs()).toHaveProperty("fileId");
    expect(getArgs()).toHaveProperty("values");
  });

  it("deleteRows sends fileId, rowids", async () => {
    await csvApi.deleteRows("fid", [1, 2, 3]);
    expect(getArgs()).toHaveProperty("fileId");
    expect(getArgs()).toHaveProperty("rowids", [1, 2, 3]);
  });

  it("saveCsv sends fileId", async () => {
    await csvApi.saveCsv("fid");
    expect(getArgs()).toHaveProperty("fileId");
  });

  it("saveCsvAs sends fileId, path", async () => {
    await csvApi.saveCsvAs("fid", "/tmp/out.csv");
    expect(getArgs()).toHaveProperty("fileId");
    expect(getArgs()).toHaveProperty("path");
  });

  it("getSchema sends fileId", async () => {
    await csvApi.getSchema("fid");
    expect(getArgs()).toHaveProperty("fileId");
  });

  it("closeFile sends fileId", async () => {
    await csvApi.closeFile("fid");
    expect(getArgs()).toHaveProperty("fileId");
  });
});
