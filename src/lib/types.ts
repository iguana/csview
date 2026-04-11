export type ColumnKind =
  | "integer"
  | "float"
  | "boolean"
  | "date"
  | "string"
  | "empty";

export interface ColumnMeta {
  index: number;
  name: string;
  kind: ColumnKind;
}

export interface CsvMetadata {
  file_id: string;
  path: string;
  size_bytes: number;
  row_count: number;
  column_count: number;
  has_header: boolean;
  columns: ColumnMeta[];
  delimiter: number;
  sample: string[][];
  fully_loaded: boolean;
  dirty: boolean;
}

export type SortDirection = "asc" | "desc";

export interface SortKey {
  column: number;
  direction: SortDirection;
}

export interface ColumnStats {
  column: number;
  count: number;
  empty: number;
  unique: number;
  numeric_count: number;
  min: number | null;
  max: number | null;
  mean: number | null;
  sum: number | null;
  shortest: string | null;
  longest: string | null;
  top_values: [string, number][];
}

export interface SearchHit {
  row: number;
  column: number;
  preview: string;
}
