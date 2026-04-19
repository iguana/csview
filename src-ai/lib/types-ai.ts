export interface FileInfo {
  file_id: string;
  path: string;
  row_count: number;
  columns: SchemaColumn[];
  table_name: string;
}

export interface SchemaColumn {
  index: number;
  name: string;
  original_name: string;
  kind: string;
  nullable_pct: number;
  unique_count: number;
  sample_values: string[];
}

export interface SchemaContext {
  table_name: string;
  columns: SchemaColumn[];
  row_count: number;
  sample_rows: string[][];
}

export interface QueryResult {
  columns: string[];
  rows: (string | number | boolean | null)[][];
  row_count: number;
  sql: string;
}

export interface AccountStatus {
  has_key: boolean;
  model: string;
}

export interface NlQueryResult {
  sql: string;
  explanation: string;
  result: QueryResult;
}

export interface ProfileReport {
  id: string;
  markdown: string;
  generated_at: string;
}

export interface TransformResult {
  expression: string;
  column_name: string;
  preview: string[];
}

export interface AnomalyReport {
  anomalies: Anomaly[];
  narrative: string;
}

export interface Anomaly {
  row: number;
  column: string;
  value: string;
  z_score: number;
  reason: string;
}

export interface GroupByReport {
  sql: string;
  result: QueryResult;
  narrative: string;
}

export interface QualityReport {
  issues: QualityIssue[];
  narrative: string;
  summary: Record<string, number>;
}

export interface QualityIssue {
  row: number;
  column: string;
  issue_type: string;
  value: string;
  suggestion: string | null;
}

export interface ChatSession {
  id: string;
  title: string | null;
  created_at: string;
}

export interface ChatResponse {
  content: string;
  sql_executed: string | null;
  query_result: QueryResult | null;
}

export interface Report {
  id: string;
  title: string;
  markdown: string;
  generated_at: string;
}

export interface JoinSuggestion {
  left_key: string;
  right_key: string;
  join_type: string;
  explanation: string;
}

export interface ComplianceReport {
  issues: ComplianceIssue[];
  narrative: string;
}

export interface ComplianceIssue {
  column: string;
  pii_type: string;
  count: number;
  sample_values: string[];
}

export interface ForecastReport {
  slope: number;
  intercept: number;
  r_squared: number;
  narrative: string;
  predictions: [number, number][];
}
