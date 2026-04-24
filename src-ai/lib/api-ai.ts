import { invoke } from "@tauri-apps/api/core";
import type {
  AccountStatus,
  AnomalyReport,
  AvailableModel,
  ChatResponse,
  ChatSession,
  ComplianceReport,
  FileInfo,
  ForecastReport,
  GroupByReport,
  JoinSuggestion,
  NlQueryResult,
  ProfileReport,
  QualityReport,
  QueryResult,
  Report,
  SchemaContext,
  TransformResult,
} from "./types-ai";

// ---------------------------------------------------------------------------
// AI feature commands
// Tauri 2 auto-converts JS camelCase → Rust snake_case, so we use camelCase.
// ---------------------------------------------------------------------------

export const aiApi = {
  // Account
  setApiKey(provider: string, key: string, model: string) {
    return invoke<void>("set_api_key", { provider, key, model });
  },
  getAccountStatus() {
    return invoke<AccountStatus>("get_account_status");
  },
  fetchProviderModels(provider: string, key: string) {
    return invoke<AvailableModel[]>("fetch_provider_models", { provider, key });
  },

  // Feature 1: NL Query
  nlQuery(fileId: string, query: string) {
    return invoke<NlQueryResult>("nl_query", { fileId, query });
  },

  // Feature 2: Profile
  generateProfile(fileId: string) {
    return invoke<ProfileReport>("generate_profile", { fileId });
  },

  // Feature 3: Transform
  nlTransform(fileId: string, query: string) {
    return invoke<TransformResult>("nl_transform", { fileId, query });
  },

  // Feature 4: Anomaly
  detectAnomalies(fileId: string, columns: string[]) {
    return invoke<AnomalyReport>("detect_anomalies", { fileId, columns });
  },

  // Feature 5: Grouping
  smartGroup(fileId: string, query: string) {
    return invoke<GroupByReport>("smart_group", { fileId, query });
  },

  // Feature 6: Quality
  auditQuality(fileId: string) {
    return invoke<QualityReport>("audit_quality", { fileId });
  },

  // Feature 7: Chat
  chatMessage(fileId: string, sessionId: string, message: string) {
    return invoke<ChatResponse>("chat_message", { fileId, sessionId, message });
  },
  newChatSession(fileId: string) {
    return invoke<ChatSession>("new_chat_session", { fileId });
  },

  // Feature 8: Report
  generateReport(fileId: string, request: string) {
    return invoke<Report>("generate_report", { fileId, request });
  },

  // Feature 9: Join
  suggestJoin(leftId: string, rightId: string) {
    return invoke<JoinSuggestion>("suggest_join", { leftId, rightId });
  },
  executeJoin(
    leftId: string,
    rightId: string,
    leftKey: string,
    rightKey: string,
    joinType: string,
  ) {
    return invoke<FileInfo>("execute_join", {
      leftId,
      rightId,
      leftKey,
      rightKey,
      joinType,
    });
  },

  // Feature 10: Compliance
  complianceScan(fileId: string) {
    return invoke<ComplianceReport>("compliance_scan", { fileId });
  },

  // Feature 11: Forecast
  forecast(fileId: string, xCol: string, yCol: string) {
    return invoke<ForecastReport>("forecast", { fileId, xCol, yCol });
  },
};

// ---------------------------------------------------------------------------
// CSV commands (SQLite-backed store)
// ---------------------------------------------------------------------------

export const csvApi = {
  openCsv(path: string) {
    return invoke<FileInfo>("open_csv", { path });
  },
  queryData(fileId: string, sql: string) {
    return invoke<QueryResult>("query_data", { fileId, sql });
  },
  readRange(fileId: string, offset: number, limit: number, orderBy?: string) {
    return invoke<QueryResult>("read_range", { fileId, offset, limit, orderBy: orderBy ?? null });
  },
  updateCell(fileId: string, rowid: number, column: string, value: string) {
    return invoke<void>("update_cell", { fileId, rowid, column, value });
  },
  insertRow(fileId: string, values: Record<string, string>) {
    return invoke<number>("insert_row", { fileId, values });
  },
  deleteRows(fileId: string, rowids: number[]) {
    return invoke<number>("delete_rows", { fileId, rowids });
  },
  deleteColumn(fileId: string, column: number) {
    return invoke<string>("delete_column", { fileId, column });
  },
  saveCsv(fileId: string) {
    return invoke<void>("save_csv", { fileId });
  },
  saveCsvAs(fileId: string, path: string) {
    return invoke<void>("save_csv_as", { fileId, path });
  },
  getSchema(fileId: string) {
    return invoke<SchemaContext>("get_schema", { fileId });
  },
  closeFile(fileId: string) {
    return invoke<void>("close_file", { fileId });
  },
  newWindow() {
    return invoke<void>("new_window");
  },
  openInNewWindow(path: string) {
    return invoke<void>("open_in_new_window", { path });
  },
};
