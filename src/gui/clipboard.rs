//! Clipboard abstraction layer.
//!
//! Provides a trait-based interface for clipboard operations,
//! allowing easy testing and future platform-specific implementations.

/// Trait for clipboard operations.
pub trait ClipboardProvider {
    /// Get text from clipboard.
    fn get_text(&mut self) -> Option<String>;

    /// Set text to clipboard.
    fn set_text(&mut self, text: String) -> bool;
}

/// System clipboard implementation using arboard.
pub struct SystemClipboard;

impl ClipboardProvider for SystemClipboard {
    fn get_text(&mut self) -> Option<String> {
        let mut cb = arboard::Clipboard::new().ok()?;
        cb.get_text().ok()
    }

    fn set_text(&mut self, text: String) -> bool {
        let mut cb = match arboard::Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return false,
        };
        cb.set_text(text).is_ok()
    }
}

/// In-memory clipboard for grid data.
#[derive(Clone, Debug)]
pub struct GridClipboard {
    pub cells: Vec<(usize, usize, String)>, // (col, row, input_string)
    pub width: usize,
    pub height: usize,
}

impl GridClipboard {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            width: 0,
            height: 0,
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.width = 0;
        self.height = 0;
    }

    pub fn add_cell(&mut self, col: usize, row: usize, text: String) {
        self.cells.push((col, row, text));
        self.width = self.width.max(col + 1);
        self.height = self.height.max(row + 1);
    }
}

impl Default for GridClipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_clipboard() {
        let mut clipboard = GridClipboard::new();
        clipboard.add_cell(0, 0, "A".to_string());
        clipboard.add_cell(1, 0, "B".to_string());
        clipboard.add_cell(0, 1, "C".to_string());

        assert_eq!(clipboard.width, 2);
        assert_eq!(clipboard.height, 2);
        assert_eq!(clipboard.cells.len(), 3);
    }
}
