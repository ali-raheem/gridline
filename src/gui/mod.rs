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
                let text = self.app.copy_selection_to_string();
                let count = text.lines().count();
                let ok = <SystemClipboard as ClipboardProvider>::set_text(
                    &mut self.clipboard,
                    text.clone(),
                );
                if ok {
                    self.app.status = format!("✓ Copied {} cells", count.max(1));
                } else {
                    self.app.status = "✗ Copy failed: clipboard unavailable".to_string();
                }
            }
            Action::CutSelection => {
                let text = self.app.copy_selection_to_string();
                let count = text.lines().count();
                let ok = <SystemClipboard as ClipboardProvider>::set_text(
                    &mut self.clipboard,
                    text.clone(),
                );
                self.app.clear_selection();
                if ok {
                    self.app.status = format!("✓ Cut {} cells", count.max(1));
                } else {
                    self.app.status = "✗ Cut failed: clipboard unavailable".to_string();
                }
            }
            Action::Paste(text) => {
                // If text is provided (from egui paste event), use it directly
                if !text.is_empty() {
                    let count = text.lines().count();
                    apply_action(&mut self.app, &mut self.state, Action::Paste(text));
                    self.app.status = format!("✓ Pasted {} cells", count.max(1));
                } else {
                    // Otherwise try to read from system clipboard
                    if let Some(clipboard_text) =
                        <SystemClipboard as ClipboardProvider>::get_text(&mut self.clipboard)
                    {
                        let count = clipboard_text.lines().count();
                        let paste_action = Action::Paste(clipboard_text);
                        apply_action(&mut self.app, &mut self.state, paste_action);
                        self.app.status = format!("✓ Pasted {} cells", count.max(1));
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
                let (_, r1, _, _) = self.app.selection_bounds();
                self.app.delete_row(r1);
                self.app.status = format!("✓ Deleted row {}", r1 + 1);
            }
            Action::DeleteColumn => {
                let (c1, _, _, _) = self.app.selection_bounds();
                self.app.delete_column(c1);
                self.app.status = format!("✓ Deleted column {}", CellRef::col_to_letters(c1));
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
