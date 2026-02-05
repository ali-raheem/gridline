//! Cell reference parsing and formatting.
//!
//! Provides bidirectional conversion between spreadsheet-style cell references
//! (e.g., "A1", "B2", "AA100") and zero-indexed column/row coordinates.
//!
//! # Examples
//!
//! ```ignore
//! let cell = CellRef::from_str("B3").unwrap();
//! assert_eq!(cell.col, 1);  // 0-indexed
//! assert_eq!(cell.row, 2);
//! assert_eq!(cell.to_string(), "B3");
//! ```

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A reference to a cell by column and row indices (0-indexed).
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct CellRef {
    pub row: usize,
    pub col: usize,
}

impl CellRef {
    pub fn new(col: usize, row: usize) -> CellRef {
        CellRef { row, col }
    }

    /// Parse a cell reference from spreadsheet notation (e.g., "A1", "B2", "AA10").
    /// Returns None if the input is invalid.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(name: &str) -> Option<CellRef> {
        Self::parse_a1(name)
    }

    fn parse_a1(name: &str) -> Option<CellRef> {
        let re = Regex::new(r"^(?<letters>[A-Za-z]+)(?<numbers>[0-9]+)$").unwrap();
        let caps = re.captures(name)?;
        let letters = &caps["letters"];
        let numbers = &caps["numbers"];

        let mut col_acc = 0usize;
        for c in letters.to_ascii_uppercase().bytes() {
            let digit = (c - b'A') as usize + 1;
            col_acc = col_acc.checked_mul(26)?.checked_add(digit)?;
        }
        let col = col_acc.checked_sub(1)?;

        let row = numbers.parse::<usize>().ok()?.checked_sub(1)?;

        Some(CellRef::new(col, row))
    }

    /// Convert column index to spreadsheet-style letters (0 -> A, 25 -> Z, 26 -> AA).
    pub fn col_to_letters(col: usize) -> String {
        let mut result = String::new();
        let mut n = col as u128 + 1;
        while n > 0 {
            n -= 1;
            result.insert(0, (b'A' + (n % 26) as u8) as char);
            n /= 26;
        }
        result
    }
}

impl std::str::FromStr for CellRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_a1(s).ok_or_else(|| format!("Invalid cell reference: {}", s))
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", CellRef::col_to_letters(self.col), self.row + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::CellRef;

    #[test]
    fn test_parse_a1_overflow_returns_none() {
        let huge = format!("{}1", "Z".repeat(40));
        assert!(CellRef::from_str(&huge).is_none());
    }

    #[test]
    fn test_col_to_letters_handles_max_usize() {
        let letters = CellRef::col_to_letters(usize::MAX);
        assert!(!letters.is_empty());
        assert!(letters.chars().all(|c| c.is_ascii_uppercase()));
    }
}
