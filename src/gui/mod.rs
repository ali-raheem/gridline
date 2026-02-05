//! Gridline GUI - egui-based desktop application.
//!
//! This module provides a modular GUI implementation:
//! - app.rs: Core application state and business logic
//! - state.rs: UI viewport and layout state
//! - actions.rs: Action types and dispatch
//! - input.rs: Keyboard input handling
//! - clipboard.rs: Clipboard abstraction
//! - ui.rs: egui rendering
//! - main.rs: Entry point and window setup

pub mod actions;
pub mod app;
pub mod clipboard;
pub mod input;
pub mod state;
pub mod ui;

use eframe::egui;
use gridline_core::{CellRef, Document};

use self::actions::{Action, apply_action};
use self::app::GuiApp;
use self::clipboard::{ClipboardProvider, SystemClipboard};
use self::input::handle_keyboard_input;
use self::state::GuiState;
use self::ui::{CellRenderer, apply_theme, draw_central_grid, draw_status_bar, draw_top_panel};

fn selection_cell_count(app: &GuiApp) -> usize {
    let (c1, r1, c2, r2) = app.selection_bounds();
    (c2 - c1 + 1) * (r2 - r1 + 1)
}

fn format_cell_count(count: usize) -> String {
    if count == 1 {
        "1 cell".to_string()
    } else {
        format!("{count} cells")
    }
}

fn handle_copy_selection<C: ClipboardProvider>(app: &mut GuiApp, clipboard: &mut C) {
    let text = app.copy_selection_to_string_and_store();
    let count = selection_cell_count(app);
    if clipboard.set_text(text) {
        app.status = format!("✓ Copied {}", format_cell_count(count));
    } else {
        app.status = "✗ Copy failed: clipboard unavailable".to_string();
    }
}

fn handle_cut_selection<C: ClipboardProvider>(app: &mut GuiApp, clipboard: &mut C) {
    let text = app.copy_selection_to_string_and_store();
    let count = selection_cell_count(app);
    if clipboard.set_text(text) {
        app.clear_selection();
        app.status = format!("✓ Cut {}", format_cell_count(count));
    } else {
        app.status = "✗ Cut failed: clipboard unavailable".to_string();
    }
}

/// Main GUI application wrapper implementing eframe::App trait.
pub struct GridlineGuiApp {
    app: GuiApp,
    state: GuiState,
    renderer: CellRenderer,
    clipboard: SystemClipboard,
    formula_id: egui::Id,
}

impl GridlineGuiApp {
    pub fn new(doc: Document) -> Self {
        Self {
            app: GuiApp::new(doc),
            state: GuiState::new(),
            renderer: CellRenderer::new(),
            clipboard: SystemClipboard,
            formula_id: egui::Id::new("gridline_formula_edit"),
        }
    }
}

impl GridlineGuiApp {
    fn handle_action(&mut self, action: Action) {
        match action {
            Action::CopySelection => {
                handle_copy_selection(&mut self.app, &mut self.clipboard);
            }
            Action::CutSelection => {
                handle_cut_selection(&mut self.app, &mut self.clipboard);
            }
            Action::Paste(text) => {
                // If text is provided (from egui paste event), use it directly
                if !text.is_empty() {
                    let _ = self.app.paste_from_clipboard(text);
                } else {
                    // Otherwise try to read from system clipboard
                    if let Some(clipboard_text) = self.clipboard.get_text() {
                        let _ = self.app.paste_from_clipboard(clipboard_text);
                    } else {
                        self.app.status = "✗ Paste failed: clipboard empty".to_string();
                    }
                }
            }
            Action::Save => match self.app.save() {
                Ok(_) => {
                    self.app.status = "✓ File saved".to_string();
                }
                Err(e) => {
                    self.app.status = format!("✗ Save failed: {}", e);
                }
            },
            Action::Undo => match self.app.undo() {
                Ok(()) => {
                    self.app.status = "✓ Undo".to_string();
                }
                Err(e) => {
                    self.app.status = format!("✗ Undo failed: {}", e);
                }
            },
            Action::Redo => match self.app.redo() {
                Ok(()) => {
                    self.app.status = "✓ Redo".to_string();
                }
                Err(e) => {
                    self.app.status = format!("✗ Redo failed: {}", e);
                }
            },
            Action::ClearSelection => {
                self.app.clear_selection();
                self.app.status = "✓ Cleared".to_string();
            }
            Action::DeleteRow => {
                let (_, r1, _, r2) = self.app.selection_bounds();
                if r1 == r2 {
                    self.app.delete_row(r1);
                    self.app.status = format!("✓ Deleted row {}", r1 + 1);
                } else {
                    self.app.status = "Select a single row to delete".to_string();
                }
            }
            Action::DeleteColumn => {
                let (c1, _, c2, _) = self.app.selection_bounds();
                if c1 == c2 {
                    self.app.delete_column(c1);
                    self.app.status = format!("✓ Deleted column {}", CellRef::col_to_letters(c1));
                } else {
                    self.app.status = "Select a single column to delete".to_string();
                }
            }
            Action::InsertRow => {
                let (_, r1, _, _) = self.app.selection_bounds();
                self.app.insert_row(r1);
                self.app.status = format!("✓ Inserted row at {}", r1 + 1);
            }
            Action::InsertColumn => {
                let (c1, _, _, _) = self.app.selection_bounds();
                self.app.insert_column(c1);
                self.app.status = format!("✓ Inserted column at {}", CellRef::col_to_letters(c1));
            }
            _ => {
                apply_action(&mut self.app, &mut self.state, action);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridline_core::{CellRef, Document};

    struct TestClipboard {
        text: Option<String>,
        set_ok: bool,
    }

    impl ClipboardProvider for TestClipboard {
        fn get_text(&mut self) -> Option<String> {
            self.text.clone()
        }

        fn set_text(&mut self, text: String) -> bool {
            if self.set_ok {
                self.text = Some(text);
                true
            } else {
                false
            }
        }
    }

    #[test]
    fn test_cut_does_not_clear_selection_when_clipboard_fails() {
        let mut doc = Document::new();
        doc.set_cell_from_input(CellRef::new(0, 0), "42").unwrap();
        let mut app = GuiApp::new(doc);
        let mut clipboard = TestClipboard {
            text: None,
            set_ok: false,
        };

        handle_cut_selection(&mut app, &mut clipboard);

        assert_eq!(app.cell_input_string(&CellRef::new(0, 0)), "42");
        assert_eq!(app.status, "✗ Cut failed: clipboard unavailable");
    }

    #[test]
    fn test_cut_clears_selection_when_clipboard_succeeds() {
        let mut doc = Document::new();
        doc.set_cell_from_input(CellRef::new(0, 0), "42").unwrap();
        let mut app = GuiApp::new(doc);
        let mut clipboard = TestClipboard {
            text: None,
            set_ok: true,
        };

        handle_cut_selection(&mut app, &mut clipboard);

        assert_eq!(app.cell_input_string(&CellRef::new(0, 0)), "");
        assert_eq!(app.status, "✓ Cut 1 cell");
        assert_eq!(clipboard.text.as_deref(), Some("42"));
    }

    #[test]
    fn test_delete_row_requires_single_row_selection() {
        let mut doc = Document::new();
        doc.set_cell_from_input(CellRef::new(0, 0), "10").unwrap();
        doc.set_cell_from_input(CellRef::new(0, 1), "20").unwrap();
        let mut gui = GridlineGuiApp::new(doc);
        gui.app.selection_anchor = CellRef::new(0, 0);
        gui.app.selection_end = CellRef::new(0, 1);

        gui.handle_action(Action::DeleteRow);

        assert_eq!(gui.app.cell_input_string(&CellRef::new(0, 0)), "10");
        assert_eq!(gui.app.cell_input_string(&CellRef::new(0, 1)), "20");
        assert_eq!(gui.app.status, "Select a single row to delete");
    }

    #[test]
    fn test_delete_column_requires_single_column_selection() {
        let mut doc = Document::new();
        doc.set_cell_from_input(CellRef::new(0, 0), "10").unwrap();
        doc.set_cell_from_input(CellRef::new(1, 0), "20").unwrap();
        let mut gui = GridlineGuiApp::new(doc);
        gui.app.selection_anchor = CellRef::new(0, 0);
        gui.app.selection_end = CellRef::new(1, 0);

        gui.handle_action(Action::DeleteColumn);

        assert_eq!(gui.app.cell_input_string(&CellRef::new(0, 0)), "10");
        assert_eq!(gui.app.cell_input_string(&CellRef::new(1, 0)), "20");
        assert_eq!(gui.app.status, "Select a single column to delete");
    }
}

impl eframe::App for GridlineGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme
        apply_theme(ctx);

        // Handle Ctrl+W to close
        let ctrl_w = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W));
        if ctrl_w {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Top panel: formula bar
        egui::TopBottomPanel::top("formula_bar").show(ctx, |ui| {
            draw_top_panel(ctx, ui, &mut self.app, &mut self.state);
        });

        // Bottom panel: status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            draw_status_bar(ui, &self.app, &self.state);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Handle other keyboard shortcuts in the grid area
            // Only when not actively editing
            if !self.state.editing {
                if let Some(action) = handle_keyboard_input(ctx) {
                    self.handle_action(action);
                    ctx.request_repaint(); // Ensure GUI updates after action
                }
            }

            // Draw the spreadsheet grid
            draw_central_grid(ui, &mut self.app, &mut self.state, &self.renderer);
        });

        // Handle focus management for formula bar
        if self.state.request_focus_formula {
            ctx.memory_mut(|m| m.request_focus(self.formula_id));
            self.state.request_focus_formula = false;
        }
    }
}
