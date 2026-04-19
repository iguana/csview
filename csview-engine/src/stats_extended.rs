//! Extended statistical functions beyond the basic `ColumnStats`.
//!
//! Provides median, standard deviation, quartiles, IQR, skewness, Pearson
//! correlation, anomaly detection, and simple linear regression.

use serde::{Deserialize, Serialize};

use crate::engine::ColumnMeta;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Extended per-column statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedColumnStats {
    pub base_count: usize,
    pub median: Option<f64>,
    pub stddev: Option<f64>,
    pub variance: Option<f64>,
    pub q1: Option<f64>,
    pub q3: Option<f64>,
    pub iqr: Option<f64>,
    pub skewness: Option<f64>,
    /// Selected percentiles: \[(5, val), (25, val), (50, val), (75, val), (95, val)\]
    pub percentiles: Vec<(u8, f64)>,
}

/// Pearson correlation between two columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correlation {
    pub col_a: usize,
    pub col_b: usize,
    pub pearson: f64,
    pub n: usize,
}

/// A data point flagged as anomalous.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyResult {
    pub row: usize,
    pub column: usize,
    pub value: String,
    pub z_score: f64,
    pub iqr_flag: bool,
    pub reason: String,
}

/// Result of a simple linear regression (y = slope·x + intercept).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionResult {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
    pub n: usize,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Collect only the finite `f64` values from a string slice.
fn numeric_values(values: &[&str]) -> Vec<f64> {
    values
        .iter()
        .filter_map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok().filter(|f| f.is_finite())
            }
        })
        .collect()
}

/// Compute the value at percentile `p` (0..=100) of a *sorted* slice.
/// Uses the nearest-rank method.
fn percentile_sorted(sorted: &[f64], p: u8) -> f64 {
    debug_assert!(!sorted.is_empty());
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (p as f64 / 100.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    let frac = rank - lower as f64;
    sorted[lower] + frac * (sorted[upper] - sorted[lower])
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Compute extended statistics for a column given raw string values.
///
/// Non-numeric and empty strings are excluded from all numeric calculations.
pub fn extended_stats(values: &[&str]) -> ExtendedColumnStats {
    let base_count = values.len();
    let mut nums = numeric_values(values);

    if nums.is_empty() {
        return ExtendedColumnStats {
            base_count,
            median: None,
            stddev: None,
            variance: None,
            q1: None,
            q3: None,
            iqr: None,
            skewness: None,
            percentiles: Vec::new(),
        };
    }

    nums.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = nums.len() as f64;

    // Mean
    let mean: f64 = nums.iter().sum::<f64>() / n;

    // Variance / stddev
    let variance: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let stddev = variance.sqrt();

    // Percentiles
    let pcts: Vec<(u8, f64)> = [5u8, 25, 50, 75, 95]
        .iter()
        .map(|&p| (p, percentile_sorted(&nums, p)))
        .collect();

    let median = Some(pcts[2].1);
    let q1 = Some(pcts[1].1);
    let q3 = Some(pcts[3].1);
    let iqr = Some(pcts[3].1 - pcts[1].1);

    // Skewness (Fisher-Pearson standardised moment coefficient)
    let skewness = if stddev == 0.0 {
        None
    } else {
        let s: f64 = nums.iter().map(|x| ((x - mean) / stddev).powi(3)).sum::<f64>() / n;
        Some(s)
    };

    ExtendedColumnStats {
        base_count,
        median,
        stddev: Some(stddev),
        variance: Some(variance),
        q1,
        q3,
        iqr,
        skewness,
        percentiles: pcts,
    }
}

/// Compute Pearson's r between two equal-length slices of finite `f64` values.
///
/// Returns `f64::NAN` when standard deviation is zero for either series.
pub fn pearson_correlation(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len().min(ys.len());
    if n == 0 {
        return f64::NAN;
    }
    let n_f = n as f64;

    let mean_x: f64 = xs[..n].iter().sum::<f64>() / n_f;
    let mean_y: f64 = ys[..n].iter().sum::<f64>() / n_f;

    let mut cov = 0_f64;
    let mut var_x = 0_f64;
    let mut var_y = 0_f64;

    for i in 0..n {
        let dx = xs[i] - mean_x;
        let dy = ys[i] - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    let denom = (var_x * var_y).sqrt();
    if denom == 0.0 {
        f64::NAN
    } else {
        cov / denom
    }
}

/// Compute pairwise Pearson correlations for the selected column indices.
///
/// Only rows where both columns parse as finite `f64` are included in each
/// pair's calculation.
pub fn correlations(
    rows: &[Vec<String>],
    _columns: &[ColumnMeta],
    col_indices: &[usize],
) -> Vec<Correlation> {
    let mut result = Vec::new();

    for (i, &ca) in col_indices.iter().enumerate() {
        for &cb in &col_indices[i + 1..] {
            let mut xs: Vec<f64> = Vec::with_capacity(rows.len());
            let mut ys: Vec<f64> = Vec::with_capacity(rows.len());

            for row in rows {
                let ax = row.get(ca).and_then(|s| s.trim().parse::<f64>().ok());
                let bx = row.get(cb).and_then(|s| s.trim().parse::<f64>().ok());
                if let (Some(x), Some(y)) = (ax, bx) {
                    if x.is_finite() && y.is_finite() {
                        xs.push(x);
                        ys.push(y);
                    }
                }
            }

            let n = xs.len();
            result.push(Correlation {
                col_a: ca,
                col_b: cb,
                pearson: pearson_correlation(&xs, &ys),
                n,
            });
        }
    }

    result
}

/// Detect anomalous values using Z-score and IQR fences.
///
/// A row/column combination is flagged when:
/// - `|z_score| > z_threshold`, OR
/// - the value lies outside `[Q1 - 1.5·IQR, Q3 + 1.5·IQR]`
pub fn detect_anomalies(
    rows: &[Vec<String>],
    _columns: &[ColumnMeta],
    col_indices: &[usize],
    z_threshold: f64,
) -> Vec<AnomalyResult> {
    let mut results = Vec::new();

    for &col in col_indices {
        let raw: Vec<&str> = rows
            .iter()
            .map(|r| r.get(col).map(String::as_str).unwrap_or(""))
            .collect();

        let stats = extended_stats(&raw);
        let nums: Vec<Option<f64>> = raw
            .iter()
            .map(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    t.parse::<f64>().ok().filter(|f| f.is_finite())
                }
            })
            .collect();

        let (mean, stddev) = match (stats.stddev, stats.median) {
            _ => {
                // Recompute mean from the numeric values
                let valid: Vec<f64> = nums.iter().filter_map(|x| *x).collect();
                if valid.is_empty() {
                    continue;
                }
                let m = valid.iter().sum::<f64>() / valid.len() as f64;
                let var = valid.iter().map(|x| (x - m).powi(2)).sum::<f64>()
                    / valid.len() as f64;
                (m, var.sqrt())
            }
        };

        let q1 = stats.q1.unwrap_or(f64::NEG_INFINITY);
        let q3 = stats.q3.unwrap_or(f64::INFINITY);
        let iqr = stats.iqr.unwrap_or(0.0);
        let lower_fence = q1 - 1.5 * iqr;
        let upper_fence = q3 + 1.5 * iqr;

        for (row_idx, opt_val) in nums.iter().enumerate() {
            let Some(v) = opt_val else { continue };

            let z = if stddev == 0.0 { 0.0 } else { (v - mean) / stddev };
            let iqr_flag = *v < lower_fence || *v > upper_fence;
            let z_flag = z.abs() > z_threshold;

            if z_flag || iqr_flag {
                let mut reasons = Vec::new();
                if z_flag {
                    reasons.push(format!("z-score {z:.2} exceeds threshold {z_threshold}"));
                }
                if iqr_flag {
                    reasons.push(format!(
                        "value {v} outside IQR fence [{lower_fence:.2}, {upper_fence:.2}]"
                    ));
                }
                results.push(AnomalyResult {
                    row: row_idx,
                    column: col,
                    value: raw[row_idx].to_string(),
                    z_score: z,
                    iqr_flag,
                    reason: reasons.join("; "),
                });
            }
        }
    }

    results
}

/// Compute ordinary-least-squares linear regression (y = slope·x + intercept).
///
/// Both slices are consumed together; pairs where either value is non-finite
/// are excluded. Returns a result with `n = 0` and NaN coefficients when there
/// is insufficient data.
pub fn linear_regression(xs: &[f64], ys: &[f64]) -> RegressionResult {
    let pairs: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys.iter())
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(&x, &y)| (x, y))
        .collect();

    let n = pairs.len();
    if n < 2 {
        return RegressionResult {
            slope: f64::NAN,
            intercept: f64::NAN,
            r_squared: f64::NAN,
            n,
        };
    }

    let n_f = n as f64;
    let mean_x = pairs.iter().map(|(x, _)| x).sum::<f64>() / n_f;
    let mean_y = pairs.iter().map(|(_, y)| y).sum::<f64>() / n_f;

    let mut ss_xy = 0_f64;
    let mut ss_xx = 0_f64;

    for (x, y) in &pairs {
        ss_xy += (x - mean_x) * (y - mean_y);
        ss_xx += (x - mean_x).powi(2);
    }

    if ss_xx == 0.0 {
        return RegressionResult {
            slope: f64::NAN,
            intercept: f64::NAN,
            r_squared: f64::NAN,
            n,
        };
    }

    let slope = ss_xy / ss_xx;
    let intercept = mean_y - slope * mean_x;

    // R² = 1 - SS_res / SS_tot
    let ss_res: f64 = pairs
        .iter()
        .map(|(x, y)| {
            let y_hat = slope * x + intercept;
            (y - y_hat).powi(2)
        })
        .sum();
    let ss_tot: f64 = pairs.iter().map(|(_, y)| (y - mean_y).powi(2)).sum();

    let r_squared = if ss_tot == 0.0 { 1.0 } else { 1.0 - ss_res / ss_tot };

    RegressionResult {
        slope,
        intercept,
        r_squared,
        n,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    // ------------------------------------------------------------------
    // extended_stats
    // ------------------------------------------------------------------

    #[test]
    fn test_median_odd() {
        let vals: Vec<&str> = vec!["3", "1", "2"];
        let s = extended_stats(&vals);
        assert_eq!(s.median, Some(2.0));
    }

    #[test]
    fn test_median_even() {
        let vals: Vec<&str> = vec!["1", "3", "5", "7"];
        let s = extended_stats(&vals);
        // Percentile interpolation: median = 4.0
        assert_eq!(s.median, Some(4.0));
    }

    #[test]
    fn test_stddev_known_values() {
        // Population stddev of {2, 4, 4, 4, 5, 5, 7, 9} = 2.0
        let vals: Vec<&str> = vec!["2", "4", "4", "4", "5", "5", "7", "9"];
        let s = extended_stats(&vals);
        assert!(approx_eq(s.stddev.unwrap(), 2.0, 1e-9));
    }

    #[test]
    fn test_quartiles() {
        let vals: Vec<&str> = vec!["1", "2", "3", "4", "5", "6", "7", "8"];
        let s = extended_stats(&vals);
        assert!(s.q1.is_some());
        assert!(s.q3.is_some());
        // Q1 = 25th percentile of sorted [1..8]
        let q1 = s.q1.unwrap();
        let q3 = s.q3.unwrap();
        assert!(q1 > 0.0 && q1 < q3);
    }

    #[test]
    fn test_iqr() {
        let vals: Vec<&str> = vec!["1", "2", "3", "4", "5", "6", "7", "8"];
        let s = extended_stats(&vals);
        let expected_iqr = s.q3.unwrap() - s.q1.unwrap();
        assert!(approx_eq(s.iqr.unwrap(), expected_iqr, 1e-9));
    }

    #[test]
    fn test_empty_values() {
        let vals: Vec<&str> = vec![];
        let s = extended_stats(&vals);
        assert!(s.median.is_none());
        assert!(s.stddev.is_none());
    }

    #[test]
    fn test_skewness_symmetric() {
        // A perfectly symmetric distribution should have skewness near 0
        let vals: Vec<&str> = vec!["1", "2", "3", "4", "5"];
        let s = extended_stats(&vals);
        assert!(approx_eq(s.skewness.unwrap(), 0.0, 1e-6));
    }

    // ------------------------------------------------------------------
    // pearson_correlation
    // ------------------------------------------------------------------

    #[test]
    fn test_pearson_correlation_positive() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![2.0, 4.0, 6.0, 8.0, 10.0];
        let r = pearson_correlation(&xs, &ys);
        assert!(approx_eq(r, 1.0, 1e-9));
    }

    #[test]
    fn test_pearson_correlation_negative() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![10.0, 8.0, 6.0, 4.0, 2.0];
        let r = pearson_correlation(&xs, &ys);
        assert!(approx_eq(r, -1.0, 1e-9));
    }

    #[test]
    fn test_pearson_no_variance() {
        let xs = vec![5.0, 5.0, 5.0];
        let ys = vec![1.0, 2.0, 3.0];
        let r = pearson_correlation(&xs, &ys);
        assert!(r.is_nan());
    }

    // ------------------------------------------------------------------
    // detect_anomalies
    // ------------------------------------------------------------------

    #[test]
    fn test_anomaly_detection_zscore() {
        // All values close together except one large outlier
        let mut rows: Vec<Vec<String>> = (0..20)
            .map(|i| vec![i.to_string()])
            .collect();
        // Add an extreme outlier
        rows.push(vec!["1000".to_string()]);

        let cols = vec![crate::engine::ColumnMeta {
            index: 0,
            name: "val".into(),
            kind: crate::engine::ColumnKind::Integer,
        }];
        let anomalies = detect_anomalies(&rows, &cols, &[0], 2.0);
        assert!(!anomalies.is_empty());
        assert!(anomalies.iter().any(|a| a.value == "1000"));
    }

    #[test]
    fn test_anomaly_detection_iqr() {
        // Dataset: [1,1,1,1,1,1,1,1,1,100] — 100 is an IQR outlier
        let mut rows: Vec<Vec<String>> = (0..9).map(|_| vec!["1".to_string()]).collect();
        rows.push(vec!["100".to_string()]);

        let cols = vec![crate::engine::ColumnMeta {
            index: 0,
            name: "v".into(),
            kind: crate::engine::ColumnKind::Integer,
        }];
        let anomalies = detect_anomalies(&rows, &cols, &[0], 10.0); // high z threshold, rely on IQR
        assert!(anomalies.iter().any(|a| a.iqr_flag && a.value == "100"));
    }

    // ------------------------------------------------------------------
    // linear_regression
    // ------------------------------------------------------------------

    #[test]
    fn test_linear_regression_perfect_line() {
        // y = 2x + 1
        let xs: Vec<f64> = (0..5).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
        let r = linear_regression(&xs, &ys);
        assert!(approx_eq(r.slope, 2.0, 1e-9));
        assert!(approx_eq(r.intercept, 1.0, 1e-9));
        assert!(approx_eq(r.r_squared, 1.0, 1e-9));
    }

    #[test]
    fn test_linear_regression_known_values() {
        // Dataset: x=[1,2,3,4,5], y=[2,4,5,4,5]
        // slope = 0.6, intercept = 2.2, R² = 0.6
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![2.0, 4.0, 5.0, 4.0, 5.0];
        let r = linear_regression(&xs, &ys);
        assert!(approx_eq(r.slope, 0.6, 1e-6));
        assert!(approx_eq(r.intercept, 2.2, 1e-6));
        assert!(approx_eq(r.r_squared, 0.6, 1e-6));
    }

    #[test]
    fn test_linear_regression_insufficient_data() {
        let xs = vec![1.0];
        let ys = vec![2.0];
        let r = linear_regression(&xs, &ys);
        assert!(r.slope.is_nan());
    }

    // ------------------------------------------------------------------
    // correlations (integration)
    // ------------------------------------------------------------------

    #[test]
    fn test_correlations_integration() {
        let rows: Vec<Vec<String>> = (1..=5)
            .map(|i| vec![i.to_string(), (i * 2).to_string()])
            .collect();
        let cols = vec![
            crate::engine::ColumnMeta { index: 0, name: "x".into(), kind: crate::engine::ColumnKind::Integer },
            crate::engine::ColumnMeta { index: 1, name: "y".into(), kind: crate::engine::ColumnKind::Integer },
        ];
        let corrs = correlations(&rows, &cols, &[0, 1]);
        assert_eq!(corrs.len(), 1);
        assert!(approx_eq(corrs[0].pearson, 1.0, 1e-9));
    }
}
