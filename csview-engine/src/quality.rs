//! Data quality auditing for CSV columns.
//!
//! Detects common data issues: mixed casing, leading/trailing whitespace,
//! inconsistent date formats, possible duplicates, PII patterns, type
//! mismatches, and statistical outliers.

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::engine::{ColumnKind, ColumnMeta};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Category of PII (Personally Identifiable Information) detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiKind {
    Email,
    Phone,
    Ssn,
    CreditCard,
    IpAddress,
}

/// The kind of quality issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueType {
    MixedCase,
    LeadingTrailingWhitespace,
    InconsistentDateFormat,
    PossibleDuplicate,
    PossiblePii(PiiKind),
    TypeMismatch,
    Outlier,
}

/// A single quality issue found in a cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    /// Data row index (0-based, header excluded).
    pub row: usize,
    /// Column index.
    pub column: usize,
    pub issue_type: IssueType,
    /// The raw cell value.
    pub value: String,
    /// Optional corrective suggestion.
    pub suggestion: Option<String>,
}

// ---------------------------------------------------------------------------
// PII patterns
// ---------------------------------------------------------------------------

/// Attempt to classify `value` as a specific PII kind.
///
/// Returns `None` when no pattern matches.
pub fn detect_pii(value: &str) -> Option<PiiKind> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }

    // Email — simple RFC-ish pattern
    if Regex::new(r"(?i)^[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}$")
        .unwrap()
        .is_match(v)
    {
        return Some(PiiKind::Email);
    }

    // US Social Security Number: XXX-XX-XXXX or XXXXXXXXX
    if Regex::new(r"^\d{3}-\d{2}-\d{4}$").unwrap().is_match(v)
        || Regex::new(r"^\d{9}$").unwrap().is_match(v)
    {
        return Some(PiiKind::Ssn);
    }

    // Credit card: 13-19 digits, possibly separated by spaces or dashes
    if Regex::new(r"^\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{1,7}$")
        .unwrap()
        .is_match(v)
    {
        // Reject values that look like plain integers with fewer than 13 total digits
        let digits: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 13 {
            return Some(PiiKind::CreditCard);
        }
    }

    // Phone: various North American / international formats
    if Regex::new(r"^[\+]?[(]?[0-9]{1,4}[)]?[-\s\.]?[(]?[0-9]{1,3}[)]?[-\s\.]?[0-9]{3,4}[-\s\.]?[0-9]{3,4}$")
        .unwrap()
        .is_match(v)
    {
        let digits: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 7 {
            return Some(PiiKind::Phone);
        }
    }

    // IPv4
    if Regex::new(
        r"^((25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(25[0-5]|2[0-4]\d|[01]?\d\d?)$",
    )
    .unwrap()
    .is_match(v)
    {
        return Some(PiiKind::IpAddress);
    }

    None
}

// ---------------------------------------------------------------------------
// Date format detection helpers
// ---------------------------------------------------------------------------

/// Returns `true` when `value` can be parsed as a date-like token.
fn looks_like_date(value: &str) -> bool {
    let v = value.trim();
    // Accept ISO (2024-01-15), US (01/15/2024), European (15.01.2024)
    Regex::new(r"^\d{1,4}[-/.]\d{1,2}[-/.]\d{1,4}$")
        .unwrap()
        .is_match(v)
}

/// Extract the date separator character (`-`, `/`, `.`) from a date-like string.
fn date_separator(value: &str) -> Option<char> {
    value.trim().chars().find(|c| *c == '-' || *c == '/' || *c == '.')
}

// ---------------------------------------------------------------------------
// Column audit
// ---------------------------------------------------------------------------

/// Audit all values in a column for quality issues.
///
/// `values` contains the raw string values for the column (header excluded).
/// `col_index` is the 0-based position of the column in the row.
pub fn audit_column(values: &[&str], column_meta: &ColumnMeta, col_index: usize) -> Vec<QualityIssue> {
    let mut issues: Vec<QualityIssue> = Vec::new();

    // Track seen values for duplicate detection
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // Track date separators seen so far (for inconsistency detection)
    let mut date_seps: std::collections::HashSet<char> = std::collections::HashSet::new();

    // Compute numeric mean/stddev for outlier detection (if column is numeric)
    let numeric_vals: Vec<f64> = values
        .iter()
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    let (num_mean, num_stddev) = if numeric_vals.len() >= 3 {
        let m = numeric_vals.iter().sum::<f64>() / numeric_vals.len() as f64;
        let v = numeric_vals.iter().map(|x| (x - m).powi(2)).sum::<f64>()
            / numeric_vals.len() as f64;
        (Some(m), Some(v.sqrt()))
    } else {
        (None, None)
    };

    for (row, &val) in values.iter().enumerate() {
        // ---- PII check ---------------------------------------------------
        if let Some(pii) = detect_pii(val) {
            issues.push(QualityIssue {
                row,
                column: col_index,
                issue_type: IssueType::PossiblePii(pii),
                value: val.to_string(),
                suggestion: None,
            });
        }

        // ---- Whitespace --------------------------------------------------
        if val != val.trim() {
            issues.push(QualityIssue {
                row,
                column: col_index,
                issue_type: IssueType::LeadingTrailingWhitespace,
                value: val.to_string(),
                suggestion: Some(val.trim().to_string()),
            });
        }

        // ---- Mixed case (only for string columns) -----------------------
        if column_meta.kind == ColumnKind::String
            && !val.trim().is_empty()
            && val.to_uppercase() != val.to_string()
            && val.to_lowercase() != val.to_string()
        {
            // Flag values that mix upper/lower but are not obviously proper-case
            // sentences. A simple heuristic: more than one uppercase char after
            // the first position indicates mixed casing.
            let interior_upper = val.trim().chars().skip(1).filter(|c| c.is_uppercase()).count();
            if interior_upper > 0 {
                issues.push(QualityIssue {
                    row,
                    column: col_index,
                    issue_type: IssueType::MixedCase,
                    value: val.to_string(),
                    suggestion: Some(val.to_uppercase()),
                });
            }
        }

        // ---- Type mismatch -----------------------------------------------
        let is_numeric = val.trim().parse::<f64>().is_ok();
        match column_meta.kind {
            ColumnKind::Integer | ColumnKind::Float => {
                if !val.trim().is_empty() && !is_numeric {
                    issues.push(QualityIssue {
                        row,
                        column: col_index,
                        issue_type: IssueType::TypeMismatch,
                        value: val.to_string(),
                        suggestion: None,
                    });
                }
            }
            _ => {}
        }

        // ---- Date format consistency ------------------------------------
        if looks_like_date(val) {
            if let Some(sep) = date_separator(val) {
                date_seps.insert(sep);
                if date_seps.len() > 1 {
                    // This row introduced a new separator — flag it
                    issues.push(QualityIssue {
                        row,
                        column: col_index,
                        issue_type: IssueType::InconsistentDateFormat,
                        value: val.to_string(),
                        suggestion: None,
                    });
                }
            }
        }

        // ---- Outlier (numeric columns) -----------------------------------
        if let (Some(mean), Some(std)) = (num_mean, num_stddev) {
            if std > 0.0 {
                if let Ok(v) = val.trim().parse::<f64>() {
                    let z = (v - mean) / std;
                    if z.abs() > 3.0 {
                        issues.push(QualityIssue {
                            row,
                            column: col_index,
                            issue_type: IssueType::Outlier,
                            value: val.to_string(),
                            suggestion: None,
                        });
                    }
                }
            }
        }

        // ---- Duplicates -------------------------------------------------
        let normalised = val.trim().to_lowercase();
        if !normalised.is_empty() {
            let count = seen.entry(normalised.clone()).or_insert(0);
            *count += 1;
            if *count == 2 {
                // Flag the second occurrence; first was clean
                issues.push(QualityIssue {
                    row,
                    column: col_index,
                    issue_type: IssueType::PossibleDuplicate,
                    value: val.to_string(),
                    suggestion: None,
                });
            }
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{ColumnKind, ColumnMeta};

    fn str_meta() -> ColumnMeta {
        ColumnMeta { index: 0, name: "col".into(), kind: ColumnKind::String }
    }

    fn int_meta() -> ColumnMeta {
        ColumnMeta { index: 0, name: "col".into(), kind: ColumnKind::Integer }
    }

    // ------------------------------------------------------------------
    // PII detection
    // ------------------------------------------------------------------

    #[test]
    fn test_detect_email() {
        assert_eq!(detect_pii("user@example.com"), Some(PiiKind::Email));
        assert_eq!(detect_pii("not-an-email"), None);
    }

    #[test]
    fn test_detect_phone() {
        assert_eq!(detect_pii("555-867-5309"), Some(PiiKind::Phone));
        assert_eq!(detect_pii("+1-800-555-1234"), Some(PiiKind::Phone));
    }

    #[test]
    fn test_detect_ssn() {
        assert_eq!(detect_pii("123-45-6789"), Some(PiiKind::Ssn));
    }

    #[test]
    fn test_detect_ip_address() {
        assert_eq!(detect_pii("192.168.1.1"), Some(PiiKind::IpAddress));
        assert_eq!(detect_pii("256.0.0.1"), None); // invalid IP
    }

    #[test]
    fn test_no_pii() {
        assert_eq!(detect_pii("Hello World"), None);
        assert_eq!(detect_pii(""), None);
    }

    // ------------------------------------------------------------------
    // Column audit
    // ------------------------------------------------------------------

    #[test]
    fn test_mixed_case() {
        let values = vec!["Hello", "WORLD", "mixedCase"];
        let issues = audit_column(&values, &str_meta(), 0);
        assert!(issues.iter().any(|i| i.issue_type == IssueType::MixedCase && i.value == "mixedCase"));
    }

    #[test]
    fn test_whitespace() {
        let values = vec!["  leading", "trailing   ", "clean"];
        let issues = audit_column(&values, &str_meta(), 0);
        let ws_issues: Vec<_> = issues
            .iter()
            .filter(|i| i.issue_type == IssueType::LeadingTrailingWhitespace)
            .collect();
        assert_eq!(ws_issues.len(), 2);
        // Suggestion should be the trimmed value
        assert_eq!(ws_issues[0].suggestion.as_deref(), Some("leading"));
    }

    #[test]
    fn test_type_mismatch() {
        let values = vec!["1", "2", "three", "4"];
        let issues = audit_column(&values, &int_meta(), 0);
        assert!(issues.iter().any(|i| i.issue_type == IssueType::TypeMismatch && i.value == "three"));
    }

    #[test]
    fn test_possible_duplicate() {
        let values = vec!["apple", "banana", "apple"];
        let issues = audit_column(&values, &str_meta(), 0);
        assert!(issues.iter().any(|i| i.issue_type == IssueType::PossibleDuplicate));
    }

    #[test]
    fn test_inconsistent_date_format() {
        let values = vec!["2024-01-15", "01/16/2024"];
        let issues = audit_column(&values, &str_meta(), 0);
        assert!(issues
            .iter()
            .any(|i| i.issue_type == IssueType::InconsistentDateFormat));
    }

    #[test]
    fn test_audit_email_detected() {
        let values = vec!["Alice", "bob@example.com"];
        let issues = audit_column(&values, &str_meta(), 0);
        assert!(issues
            .iter()
            .any(|i| matches!(&i.issue_type, IssueType::PossiblePii(PiiKind::Email))));
    }
}
