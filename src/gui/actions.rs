//! Action types and dispatch logic.
//!
//! Actions represent all possible user operations that modify state.
//! The apply_action function dispatches actions to update the app state.

use crate::gui::app::GuiApp;
use crate::gui::state::GuiState;

/// All possible user actions in the GUI.
#[derive(Debug, Clone)]
pub enum Action {
    /// Move cursor by (dx, dy) - negative values move up/left.
    MoveCursor { dx: isize, dy: isize, extend: bool },

    /// Start editing the selected cell.
    BeginEdit,

    /// Commit the current edit to the cell.
    CommitEdit,

    /// Cancel editing and revert changes.
    CancelEdit,

    /// Copy selected cells to clipboard.
    CopySelection,

    /// Cut selected cells to clipboard and clear them.
    CutSelection,

    /// Paste from clipboard.
    Paste(String),

    /// Clear (delete) selected cells.
    ClearSelection,

    /// Delete the current row.
    DeleteRow,

    /// Delete the current column.
    DeleteColumn,

    /// Insert a row above the current row.
    InsertRow,

    /// Insert a column to the left of the current column.
    InsertColumn,

    /// Undo the last action.
    Undo,

    /// Redo the last undone action.
    Redo,

    /// Save the document.
    Save,

    // Future actions:
    // EnterCommandMode,
    // ExecuteCommand(String),
    // OpenModal(ModalType),
    // SetSelection(CellRef, CellRef),
}

/// Apply an action to update app and state.
pub fn apply_action(app: &mut GuiApp, state: &mut GuiState, action: Action) {
    match action {
        Action::MoveCursor { dx, dy, extend } => {
            app.move_selection(dx, dy, extend);
            state.ensure_selected_visible(&app.selected);
        }

        Action::BeginEdit => {
            state.editing = true;
            state.request_focus_formula = true;
        }

        Action::CommitEdit => {
            let input = app.edit_buffer.clone();
            let _ = app.set_cell_from_input(&input);
            state.editing = false;
            state.request_focus_formula = false;
        }

        Action::CancelEdit => {
            app.sync_edit_buffer();
            state.editing = false;
            state.request_focus_formula = false;
        }

        Action::CopySelection => {
            // Note: Actual clipboard set happens in input handler
            // This just prepares the string
            let _ = app.copy_selection_to_string();
            app.status = format!("Copied {}", app.selection_label());
        }

        Action::CutSelection => {
            let _ = app.copy_selection_to_string();
            app.clear_selection();
            app.status = format!("Cut {}", app.selection_label());
        }

        Action::Paste(clipboard_text) => {
            app.paste_from_clipboard(clipboard_text);
        }

        Action::ClearSelection => {
            app.clear_selection();
        }

        Action::DeleteRow => {
            let (r1, _, _, _) = app.selection_bounds();
            if r1 == app.selection_end.row {
                // Single row selected
                app.delete_row(r1);
            } else {
                app.status = "Select a single row to delete".to_string();
            }
        }

        Action::DeleteColumn => {
            let (_, c1, _, c2) = app.selection_bounds();
            if c1 == c2 {
                // Single column selected
                app.delete_column(c1);
            } else {
                app.status = "Select a single column to delete".to_string();
            }
        }

        Action::InsertRow => {
            let (r1, _, _, _) = app.selection_bounds();
            app.insert_row(r1);
        }

        Action::InsertColumn => {
            let (_, c1, _, _) = app.selection_bounds();
            app.insert_column(c1);
        }

        Action::Undo => {
            match app.undo() {
                Ok(()) => {
                    eprintln!("DEBUG: Undo succeeded");
                    state.ensure_selected_visible(&app.selected);
                }
                Err(e) => {
                    eprintln!("DEBUG: Undo failed: {}", e);
                }
            }
        }

        Action::Redo => {
            eprintln!("DEBUG: Attempting redo...");
            match app.redo() {
                Ok(()) => {
                    eprintln!("DEBUG: Redo succeeded, selected cell: {}", app.selected);
                    state.ensure_selected_visible(&app.selected);
                }
                Err(e) => {
                    eprintln!("DEBUG: Redo failed: {}", e);
                }
            }
        }

        Action::Save => {
            let _ = app.save();
        }
    }
}
