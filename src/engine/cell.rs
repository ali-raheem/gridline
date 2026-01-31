//! Cell data structures for the spreadsheet grid.
//!
//! This module provides the core data types for representing cells:
//! - [`CellType`] - The type of content in a cell (empty, text, number, or formula)
//! - [`Cell`] - A cell with content, dependencies, and cached evaluation state
//! - [`Grid`] - Thread-safe sparse storage for cells (backed by `DashMap`)
//! - [`SpillMap`] - Thread-safe storage for array formula spill values

use dashmap::DashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use super::cell_ref::CellRef;
use super::deps::extract_dependencies;

/// The type of content stored in a cell.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CellType {
    Empty,
    Text(String),
    Number(f64),
    Script(String),
}

/// A cell in the spreadsheet grid.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub contents: CellType,
    pub depends_on: Vec<CellRef>,
    pub dirty: bool,
    /// Cached display string for script cells (not serialized).
    #[serde(skip)]
    pub cached_value: Option<String>,
}

impl Cell {
    pub fn new_empty() -> Cell {
        Cell {
            contents: CellType::Empty,
            depends_on: vec![],
            dirty: false,
            cached_value: None,
        }
    }

    pub fn new_text(text: &str) -> Cell {
        Cell {
            contents: CellType::Text(text.to_string()),
            depends_on: vec![],
            dirty: false,
            cached_value: None,
        }
    }

    pub fn new_number(n: f64) -> Cell {
        Cell {
            contents: CellType::Number(n),
            depends_on: vec![],
            dirty: false,
            cached_value: None,
        }
    }

    /// Create a new cell containing a script/formula.
    /// Dependencies are automatically extracted from the script.
    pub fn new_script(script: &str) -> Cell {
        Cell {
            depends_on: extract_dependencies(script),
            contents: CellType::Script(script.to_string()),
            dirty: true,
            cached_value: None,
        }
    }

    /// Parse user input and create appropriate cell type.
    /// - Empty string or whitespace -> Empty
    /// - Starts with '=' -> Script (without the '=')
    /// - Quoted string -> Text (without quotes)
    /// - Valid number -> Number
    /// - Otherwise -> Text
    pub fn from_input(input: &str) -> Cell {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Cell::new_empty();
        }

        if let Some(formula) = trimmed.strip_prefix('=') {
            return Cell::new_script(formula);
        }

        if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
            let text = &trimmed[1..trimmed.len() - 1];
            return Cell::new_text(text);
        }

        if let Ok(n) = trimmed.parse::<f64>() {
            return Cell::new_number(n);
        }

        Cell::new_text(trimmed)
    }

    /// Get a display string for the cell content (for editing).
    pub fn to_input_string(&self) -> String {
        match &self.contents {
            CellType::Empty => String::new(),
            CellType::Text(s) => s.clone(),
            CellType::Number(n) => n.to_string(),
            CellType::Script(s) => format!("={}", s),
        }
    }
}

/// Thread-safe sparse grid storage.
pub type Grid = DashMap<CellRef, Cell>;

/// Thread-safe cache for computed cell values (both scalars and arrays).
/// Maps cell positions to their evaluated Dynamic values.
/// This allows:
/// - Cell references to use pre-computed values instead of re-evaluating
/// - Array formulas to store spill values for chaining
pub type ValueCache = Arc<DashMap<CellRef, rhai::Dynamic>>;

// Legacy type aliases for backward compatibility during refactoring
#[doc(hidden)]
pub type SpillMap = ValueCache;
#[doc(hidden)]
pub type ComputedMap = ValueCache;
