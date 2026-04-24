/**
 * TypeScript interfaces matching Rust response structs from csviewai.
 *
 * IMPORTANT: Tauri 2 serializes Rust snake_case fields as camelCase in JS.
 * So Rust `file_id: String` arrives as `fileId` in TS.
 * All interfaces here must use camelCase field names.
 */

// --- CSV commands ---

export interface FileInfo {
  fileId: string;
  path: string;
  rowCount: number;
  columns: SchemaColumn[];
  tableName: string;
}

export interface SchemaColumn {
  index: number;
  name: string;
  originalName: string;
  kind: string;
  nullablePct: number;
  uniqueCount: number;
  sampleValues: string[];
}

export interface SchemaContext {
  tableName: string;
  columns: SchemaColumn[];
  rowCount: number;
  sampleRows: string[][];
}

export interface QueryResult {
  columns: string[];
  rows: (string | number | boolean | null)[][];
  rowCount: number;
  sql: string;
}

// --- Account ---

export interface AccountStatus {
  hasApiKey: boolean;
  provider: string;
  model: string;
  availableModels: AvailableModel[];
}

export interface AvailableModel {
  id: string;
  name: string;
  provider: string;
  tier: string;
  description: string;
}

// --- Feature 1: NL Query ---

export interface NlQueryResult {
  whereClause: string;
  sql: string;
  columns: string[];
  rows: (string | number | boolean | null)[][];
  rowCount: number;
}

// --- Feature 2: Profile ---

export interface ProfileReport {
  markdown: string;
  stats: unknown;
}

// --- Feature 3: Transform ---

export interface TransformResult {
  expression: string;
  columnName: string;
  rowsUpdated: number;
}

// --- Feature 4: Anomaly ---

export interface AnomalyReport {
  markdown: string;
  anomalies: AnomalyResult[];
}

export interface AnomalyResult {
  row: number;
  column: string;
  value: string;
  zScore: number;
  iqrFlag: boolean;
  reason: string;
}

// --- Feature 5: Grouping ---

export interface GroupByReport {
  markdown: string;
  sql: string;
  columns: string[];
  rows: (string | number | boolean | null)[][];
}

// --- Feature 6: Quality ---

export interface QualityReport {
  markdown: string;
  issues: QualityIssueSer[];
}

export interface QualityIssueSer {
  row: number;
  column: string;
  issueType: string;
  value: string;
  suggestion: string | null;
}

// --- Feature 7: Chat ---

export interface ChatSession {
  sessionId: string;
  fileId: string;
  createdAt: string;
}

export interface ChatResponse {
  sessionId: string;
  message: string;
  role: string;
}

// --- Feature 8: Report ---

export interface Report {
  reportId: string;
  title: string;
  markdown: string;
}

// --- Feature 9: Join ---

export interface JoinSuggestion {
  leftKey: string;
  rightKey: string;
  joinType: string;
  explanation: string;
}

// --- Feature 10: Compliance ---

export interface ComplianceReport {
  markdown: string;
  piiColumns: PiiColumn[];
}

export interface PiiColumn {
  columnName: string;
  piiKind: string;
  sampleValues: string[];
}

// --- Feature 11: Forecast ---

export interface ForecastReport {
  markdown: string;
  slope: number;
  intercept: number;
  rSquared: number;
  n: number;
  xCol: string;
  yCol: string;
}
