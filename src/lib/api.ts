import { invoke } from "@tauri-apps/api/core";
import type {
  ColumnStats,
  CsvMetadata,
  SearchHit,
  SortKey,
} from "./types";

export const api = {
  openCsv(path: string, forceHeader?: boolean) {
    return invoke<CsvMetadata>("open_csv", { path, forceHeader });
  },
  readRange(fileId: string, start: number, end: number) {
    return invoke<string[][]>("read_range", { fileId, start, end });
  },
  search(fileId: string, query: string, limit: number) {
    return invoke<SearchHit[]>("search_csv", { fileId, query, limit });
  },
  computeStats(fileId: string, column: number) {
    return invoke<ColumnStats>("compute_stats", { fileId, column });
  },
  sort(fileId: string, keys: SortKey[]) {
    return invoke<void>("sort_csv", { fileId, keys });
  },
  close(fileId: string) {
    return invoke<void>("close_csv", { fileId });
  },
  reloadWithHeader(fileId: string, hasHeader: boolean) {
    return invoke<CsvMetadata>("reload_with_header", { fileId, hasHeader });
  },
  updateCell(fileId: string, row: number, column: number, value: string) {
    return invoke<CsvMetadata>("update_cell", { fileId, row, column, value });
  },
  insertRow(fileId: string, at: number | null, values: string[] | null) {
    return invoke<CsvMetadata>("insert_row", { fileId, at, values });
  },
  deleteRows(fileId: string, rows: number[]) {
    return invoke<CsvMetadata>("delete_rows", { fileId, rows });
  },
  deleteColumn(fileId: string, column: number) {
    return invoke<{
      metadata: CsvMetadata;
      removed_name: string;
      removed_values: string[];
    }>("delete_column", { fileId, column });
  },
  insertColumn(
    fileId: string,
    at: number,
    name: string,
    values: string[],
  ) {
    return invoke<CsvMetadata>("insert_column", { fileId, at, name, values });
  },
  save(fileId: string) {
    return invoke<CsvMetadata>("save_csv", { fileId });
  },
  saveAs(fileId: string, path: string) {
    return invoke<CsvMetadata>("save_csv_as", { fileId, path });
  },
  openInNewWindow(path: string) {
    return invoke<void>("open_in_new_window", { path });
  },
  newWindow() {
    return invoke<void>("new_window");
  },
};
