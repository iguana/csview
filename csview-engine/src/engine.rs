//! Core CSV engine types shared across the workspace.
//!
//! This module contains the foundational types (`ColumnKind`, `ColumnMeta`,
//! `ColumnStats`, etc.) used throughout `csview-engine` and exposed to
//! downstream crates.

use serde::{Deserialize, Serialize};

/// Inferred semantic type of a CSV column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    Integer,
    Float,
    Boolean,
    Date,
    String,
    Empty,
}

/// Metadata describing one column in a CSV file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub index: usize,
    pub name: String,
    pub kind: ColumnKind,
}

/// Basic descriptive statistics for one column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub column: usize,
    pub count: usize,
    pub empty: usize,
    pub unique: usize,
    pub numeric_count: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub sum: Option<f64>,
    pub shortest: Option<String>,
    pub longest: Option<String>,
    pub top_values: Vec<(String, usize)>,
}
