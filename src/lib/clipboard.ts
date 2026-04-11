/**
 * TSV serialization for clipboard interop — compatible with Excel, Numbers,
 * Google Sheets. Rows separated by \n, cells by \t. Fields with tab, newline,
 * or quote get double-quoted with internal quotes escaped.
 */

export function encodeTsv(rows: string[][]): string {
  return rows
    .map((row) => row.map(encodeCell).join("\t"))
    .join("\n");
}

function encodeCell(value: string): string {
  if (value.includes("\t") || value.includes("\n") || value.includes('"')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

/**
 * Parse a clipboard string back into a 2D array of strings. Accepts TSV or
 * single-cell plain text. Handles RFC-4180-ish quoted fields with doubled
 * quotes for escaping.
 */
export function decodeTsv(text: string): string[][] {
  // Strip trailing newline — many clipboards include one.
  const trimmed = text.replace(/\r\n/g, "\n").replace(/\n$/, "");
  if (trimmed === "") return [[""]];

  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let inQuotes = false;

  for (let i = 0; i < trimmed.length; i++) {
    const c = trimmed[i];
    if (inQuotes) {
      if (c === '"') {
        if (trimmed[i + 1] === '"') {
          cell += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        cell += c;
      }
      continue;
    }
    if (c === '"' && cell === "") {
      inQuotes = true;
      continue;
    }
    if (c === "\t") {
      row.push(cell);
      cell = "";
      continue;
    }
    if (c === "\n") {
      row.push(cell);
      rows.push(row);
      row = [];
      cell = "";
      continue;
    }
    cell += c;
  }
  row.push(cell);
  rows.push(row);
  return rows;
}
