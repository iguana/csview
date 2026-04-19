//! Join engine for combining two CSV datasets.
//!
//! Supports `Inner`, `Left`, `Right`, and `Full` outer joins on a single
//! key column from each side, with optional fuzzy (case-insensitive,
//! whitespace-trimmed) matching.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The kind of join to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// Specification for a join operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinSpec {
    pub join_type: JoinType,
    /// Column index in the left dataset used as the join key.
    pub left_key: usize,
    /// Column index in the right dataset used as the join key.
    pub right_key: usize,
    /// When `true`, keys are normalised (lowercased + trimmed) before matching.
    pub fuzzy: bool,
}

/// A discrepancy between two rows that matched on the key column but differ
/// elsewhere — useful for reconciliation workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinMismatch {
    /// Left-side row index (0-based, excluding header).
    pub left_row: usize,
    /// Right-side row index (0-based, excluding header).
    pub right_row: usize,
    /// The shared key value.
    pub key: String,
    /// Column index in the merged output where values differ.
    pub column: usize,
    pub left_value: String,
    pub right_value: String,
}

/// The result of a join operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResult {
    /// Merged header row (left headers + right non-key headers).
    pub headers: Vec<String>,
    /// Merged data rows.
    pub rows: Vec<Vec<String>>,
    /// Number of rows that matched on the key in both datasets.
    pub matched: usize,
    /// Number of left-only rows (unmatched left rows, zero for `Inner`/`Right`).
    pub left_only: usize,
    /// Number of right-only rows (unmatched right rows, zero for `Inner`/`Left`).
    pub right_only: usize,
    /// Value mismatches found in matched rows (non-key columns).
    pub mismatches: Vec<JoinMismatch>,
}

/// Errors that can arise during a join.
#[derive(Debug, Error)]
pub enum JoinError {
    #[error("left key column {0} out of range (left has {1} columns)")]
    LeftKeyOutOfRange(usize, usize),
    #[error("right key column {0} out of range (right has {1} columns)")]
    RightKeyOutOfRange(usize, usize),
}

pub type Result<T> = std::result::Result<T, JoinError>;

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

/// Normalise a key string according to the fuzzy flag.
#[inline]
fn normalise(s: &str, fuzzy: bool) -> String {
    if fuzzy {
        s.trim().to_lowercase()
    } else {
        s.to_string()
    }
}

/// Join two datasets according to `spec`.
///
/// `left_headers` / `right_headers` are the header rows.
/// `left_rows` / `right_rows` are the data rows (headers excluded).
pub fn join_datasets(
    left_headers: &[String],
    left_rows: &[Vec<String>],
    right_headers: &[String],
    right_rows: &[Vec<String>],
    spec: &JoinSpec,
) -> Result<JoinResult> {
    let lw = left_headers.len();
    let rw = right_headers.len();

    if spec.left_key >= lw {
        return Err(JoinError::LeftKeyOutOfRange(spec.left_key, lw));
    }
    if spec.right_key >= rw {
        return Err(JoinError::RightKeyOutOfRange(spec.right_key, rw));
    }

    // Columns from the right side that will be included in the output
    // (all right columns except the key, which is already present from the left)
    let right_extra_cols: Vec<usize> = (0..rw).filter(|&c| c != spec.right_key).collect();

    // Build merged headers
    let mut merged_headers = left_headers.to_vec();
    for &rc in &right_extra_cols {
        merged_headers.push(right_headers[rc].clone());
    }

    // Index right rows by their (normalised) key for fast lookup
    // key -> Vec<right_row_index>
    let mut right_index: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (ri, rrow) in right_rows.iter().enumerate() {
        let key = normalise(
            rrow.get(spec.right_key).map(String::as_str).unwrap_or(""),
            spec.fuzzy,
        );
        right_index.entry(key).or_default().push(ri);
    }

    let mut result_rows: Vec<Vec<String>> = Vec::new();
    let mut mismatches: Vec<JoinMismatch> = Vec::new();
    let mut matched = 0usize;
    let mut left_only = 0usize;
    let mut right_matched: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Iterate left rows
    for (li, lrow) in left_rows.iter().enumerate() {
        let lkey_raw = lrow.get(spec.left_key).map(String::as_str).unwrap_or("");
        let lkey = normalise(lkey_raw, spec.fuzzy);

        if let Some(right_indices) = right_index.get(&lkey) {
            for &ri in right_indices {
                matched += 1;
                right_matched.insert(ri);

                let rrow = &right_rows[ri];

                // Build merged row: all left columns first
                let mut merged: Vec<String> = lrow.clone();
                for &rc in &right_extra_cols {
                    merged.push(rrow.get(rc).cloned().unwrap_or_default());
                }

                // Record mismatches for non-key columns that appear in both sides
                for &rc in &right_extra_cols {
                    // Find corresponding left column by header name
                    let right_col_name = &right_headers[rc];
                    if let Some(lc) = left_headers.iter().position(|h| h == right_col_name) {
                        let lval = lrow.get(lc).map(String::as_str).unwrap_or("");
                        let rval = rrow.get(rc).map(String::as_str).unwrap_or("");
                        if lval != rval {
                            mismatches.push(JoinMismatch {
                                left_row: li,
                                right_row: ri,
                                key: lkey_raw.to_string(),
                                column: lc,
                                left_value: lval.to_string(),
                                right_value: rval.to_string(),
                            });
                        }
                    }
                }

                result_rows.push(merged);
            }
        } else {
            // Left row has no match
            match spec.join_type {
                JoinType::Left | JoinType::Full => {
                    left_only += 1;
                    let mut merged: Vec<String> = lrow.clone();
                    for _ in &right_extra_cols {
                        merged.push(String::new());
                    }
                    result_rows.push(merged);
                }
                JoinType::Inner | JoinType::Right => {
                    // Discard unmatched left rows
                }
            }
        }
    }

    // Handle unmatched right rows
    let mut right_only = 0usize;
    if matches!(spec.join_type, JoinType::Right | JoinType::Full) {
        for (ri, rrow) in right_rows.iter().enumerate() {
            if right_matched.contains(&ri) {
                continue;
            }
            right_only += 1;
            let mut merged: Vec<String> = vec![String::new(); lw];
            // Fill the right key into the left key column
            merged[spec.left_key] =
                rrow.get(spec.right_key).cloned().unwrap_or_default();
            // Fill the remaining right columns
            for &rc in &right_extra_cols {
                // The position in merged = lw + index of rc in right_extra_cols
                let pos = lw + right_extra_cols.iter().position(|&c| c == rc).unwrap();
                if pos < merged.len() {
                    merged[pos] = rrow.get(rc).cloned().unwrap_or_default();
                } else {
                    merged.push(rrow.get(rc).cloned().unwrap_or_default());
                }
            }
            result_rows.push(merged);
        }
    }

    Ok(JoinResult {
        headers: merged_headers,
        rows: result_rows,
        matched,
        left_only,
        right_only,
        mismatches,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn sr(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter().map(|r| r.iter().map(|x| x.to_string()).collect()).collect()
    }

    fn basic_spec(jt: JoinType) -> JoinSpec {
        JoinSpec { join_type: jt, left_key: 0, right_key: 0, fuzzy: false }
    }

    // Left: id, name
    // Right: id, dept
    fn datasets() -> (Vec<String>, Vec<Vec<String>>, Vec<String>, Vec<Vec<String>>) {
        let lh = sh(&["id", "name"]);
        let lr = sr(&[&["1", "Alice"], &["2", "Bob"], &["3", "Carol"]]);
        let rh = sh(&["id", "dept"]);
        let rr = sr(&[&["1", "Engineering"], &["2", "HR"], &["4", "Marketing"]]);
        (lh, lr, rh, rr)
    }

    #[test]
    fn test_inner_join() {
        let (lh, lr, rh, rr) = datasets();
        let spec = basic_spec(JoinType::Inner);
        let result = join_datasets(&lh, &lr, &rh, &rr, &spec).unwrap();
        // IDs 1 and 2 match; ID 3 (left only) and 4 (right only) excluded
        assert_eq!(result.matched, 2);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.left_only, 0);
        assert_eq!(result.right_only, 0);
    }

    #[test]
    fn test_left_join() {
        let (lh, lr, rh, rr) = datasets();
        let spec = basic_spec(JoinType::Left);
        let result = join_datasets(&lh, &lr, &rh, &rr, &spec).unwrap();
        // IDs 1, 2, 3 — Carol has no match so gets empty dept
        assert_eq!(result.rows.len(), 3);
        assert_eq!(result.left_only, 1);
        assert_eq!(result.right_only, 0);
        // Carol's dept column should be empty
        let carol = result.rows.iter().find(|r| r[1] == "Carol").unwrap();
        assert_eq!(carol[2], "");
    }

    #[test]
    fn test_full_join() {
        let (lh, lr, rh, rr) = datasets();
        let spec = basic_spec(JoinType::Full);
        let result = join_datasets(&lh, &lr, &rh, &rr, &spec).unwrap();
        // 2 matched + 1 left-only (Carol) + 1 right-only (Marketing)
        assert_eq!(result.rows.len(), 4);
        assert_eq!(result.matched, 2);
        assert_eq!(result.left_only, 1);
        assert_eq!(result.right_only, 1);
    }

    #[test]
    fn test_join_no_matches() {
        let lh = sh(&["id", "val"]);
        let lr = sr(&[&["A", "1"]]);
        let rh = sh(&["id", "other"]);
        let rr = sr(&[&["Z", "9"]]);
        let spec = basic_spec(JoinType::Inner);
        let result = join_datasets(&lh, &lr, &rh, &rr, &spec).unwrap();
        assert!(result.rows.is_empty());
        assert_eq!(result.matched, 0);
    }

    #[test]
    fn test_join_duplicate_keys() {
        // Right has two rows with key "1" — both should appear in inner join
        let lh = sh(&["id", "name"]);
        let lr = sr(&[&["1", "Alice"]]);
        let rh = sh(&["id", "score"]);
        let rr = sr(&[&["1", "90"], &["1", "95"]]);
        let spec = basic_spec(JoinType::Inner);
        let result = join_datasets(&lh, &lr, &rh, &rr, &spec).unwrap();
        assert_eq!(result.matched, 2);
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn test_join_out_of_range_key() {
        let lh = sh(&["id"]);
        let lr: Vec<Vec<String>> = vec![];
        let rh = sh(&["id"]);
        let rr: Vec<Vec<String>> = vec![];
        let spec = JoinSpec { join_type: JoinType::Inner, left_key: 5, right_key: 0, fuzzy: false };
        assert!(join_datasets(&lh, &lr, &rh, &rr, &spec).is_err());
    }
}
