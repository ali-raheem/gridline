//! Keymap translation layer.
//!
//! This keeps key handling separate from app behavior.
//! - Vim keymap preserves existing behavior.
//! - Emacs keymap is "strict": vim-style letter keys are not active.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::Mode;

/// Available keybinding schemes.
///
/// Gridline supports two keybinding schemes:
/// - [`Vim`](Keymap::Vim): hjkl navigation, `:` commands, modal editing
/// - [`Emacs`](Keymap::Emacs): C-n/p/f/b navigation, M-x commands, C-SPC mark
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keymap {
    /// Vim-style keybindings (hjkl, :commands, modal editing).
    Vim,
    /// Emacs-style keybindings (C-n/p/f/b, M-x commands).
    Emacs,
}

impl Keymap {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vim" => Some(Keymap::Vim),
            "emacs" => Some(Keymap::Emacs),
            _ => None,
        }
    }
}

/// Actions that can be triggered by key presses.
///
/// Actions decouple key handling from application logic. The keymap translates
/// key events into actions, which are then applied to the application state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Cancel current operation and return to Normal mode.
    Cancel,
    /// Enter Edit mode for the current cell.
    EnterEdit,
    /// Commit the current edit and return to Normal mode.
    CommitEdit,
    /// Enter Command mode (`:` prompt).
    EnterCommand,
    /// Execute the command in the command buffer.
    ExecuteCommand,
    /// Enter Visual selection mode.
    EnterVisual,
    /// Exit Visual mode without action.
    ExitVisual,
    /// Yank (copy) current cell or selection.
    Yank,
    /// Paste clipboard at cursor position.
    Paste,
    /// Undo the last action.
    Undo,
    /// Redo the last undone action.
    Redo,
    /// Clear the current cell.
    ClearCell,
    /// Open plot modal for current cell.
    OpenPlot,

    /// Move cursor by (dx, dy).
    Move(i32, i32),
    /// Page up (-1) or down (+1).
    Page(i32),
    /// Jump to first column.
    HomeCol,
    /// Jump to last column.
    EndCol,
    /// Jump to last row with data.
    GotoLast,
    /// Open the goto cell prompt.
    OpenGotoPrompt,

    /// Increase current column width.
    IncColWidth,
    /// Decrease current column width.
    DecColWidth,
    /// Save the file.
    Save,
}

/// Translate a key event to an action based on the current keymap and mode.
///
/// Returns `None` if the key has no binding in the current context.
pub fn translate(keymap: Keymap, mode: Mode, key: KeyEvent) -> Option<Action> {
    match keymap {
        Keymap::Vim => translate_vim(mode, key),
        Keymap::Emacs => translate_emacs(mode, key),
    }
}

fn translate_vim(mode: Mode, key: KeyEvent) -> Option<Action> {
    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Undo)
            }
            KeyCode::Char('u') => Some(Action::Undo),
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Redo)
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Redo)
            }

            KeyCode::Up | KeyCode::Char('k') => Some(Action::Move(0, -1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::Move(0, 1)),
            KeyCode::Left | KeyCode::Char('h') => Some(Action::Move(-1, 0)),
            KeyCode::Right | KeyCode::Char('l') => Some(Action::Move(1, 0)),

            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),
            KeyCode::Home => Some(Action::HomeCol),
            KeyCode::End => Some(Action::EndCol),

            KeyCode::Enter | KeyCode::Char('i') => Some(Action::EnterEdit),
            KeyCode::Char('x') | KeyCode::Delete => Some(Action::ClearCell),
            KeyCode::Char(':') => Some(Action::EnterCommand),
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Save)
            }
            KeyCode::Char('v') => Some(Action::EnterVisual),
            KeyCode::Char('y') => Some(Action::Yank),
            KeyCode::Char('p') => Some(Action::Paste),
            KeyCode::Char('P') => Some(Action::OpenPlot),
            KeyCode::Char('+') | KeyCode::Char('>') => Some(Action::IncColWidth),
            KeyCode::Char('-') | KeyCode::Char('<') => Some(Action::DecColWidth),
            KeyCode::Char('G') => Some(Action::GotoLast),
            KeyCode::Char('g') => Some(Action::OpenGotoPrompt),
            _ => None,
        },

        Mode::Visual => match key.code {
            KeyCode::Esc => Some(Action::ExitVisual),

            KeyCode::Up | KeyCode::Char('k') => Some(Action::Move(0, -1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::Move(0, 1)),
            KeyCode::Left | KeyCode::Char('h') => Some(Action::Move(-1, 0)),
            KeyCode::Right | KeyCode::Char('l') => Some(Action::Move(1, 0)),

            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),
            KeyCode::Char('y') => Some(Action::Yank),
            _ => None,
        },

        Mode::Edit => match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::CommitEdit),
            _ => None,
        },

        Mode::Command => match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::ExecuteCommand),
            _ => None,
        },
    }
}

fn translate_emacs(mode: Mode, key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match mode {
        Mode::Normal => match key.code {
            // Cancel
            KeyCode::Char('g') if ctrl => Some(Action::Cancel),

            // Movement
            KeyCode::Up => Some(Action::Move(0, -1)),
            KeyCode::Down => Some(Action::Move(0, 1)),
            KeyCode::Left => Some(Action::Move(-1, 0)),
            KeyCode::Right => Some(Action::Move(1, 0)),
            KeyCode::Char('p') if ctrl => Some(Action::Move(0, -1)),
            KeyCode::Char('n') if ctrl => Some(Action::Move(0, 1)),
            KeyCode::Char('b') if ctrl => Some(Action::Move(-1, 0)),
            KeyCode::Char('f') if ctrl => Some(Action::Move(1, 0)),
            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),
            KeyCode::Char('v') if ctrl => Some(Action::Page(1)),
            KeyCode::Char('v') if alt => Some(Action::Page(-1)),

            // Home/End column
            KeyCode::Char('a') if ctrl => Some(Action::HomeCol),
            KeyCode::Char('e') if ctrl => Some(Action::EndCol),
            KeyCode::Home => Some(Action::HomeCol),
            KeyCode::End => Some(Action::EndCol),

            // Edit
            KeyCode::Enter => Some(Action::EnterEdit),

            // Command prompt
            KeyCode::Char('x') if alt => Some(Action::EnterCommand),
            KeyCode::Char(':') => None, // strict

            // Save
            KeyCode::Char('s') if ctrl => Some(Action::Save),

            // Copy / Paste
            KeyCode::Char('w') if alt => Some(Action::Yank),
            KeyCode::Char('y') if ctrl => Some(Action::Paste),

            // Visual selection (set mark)
            KeyCode::Char(' ') if ctrl => Some(Action::EnterVisual),

            // Clear cell
            KeyCode::Char('d') if ctrl => Some(Action::ClearCell),
            KeyCode::Delete => Some(Action::ClearCell),

            // Plot modal
            KeyCode::Char('p') if alt => Some(Action::OpenPlot),

            // Go to last
            KeyCode::Char('>') if alt => Some(Action::GotoLast),

            // Goto prompt
            KeyCode::Char('g') if alt => Some(Action::OpenGotoPrompt),

            _ => None,
        },

        Mode::Visual => match key.code {
            KeyCode::Char('g') if ctrl => Some(Action::ExitVisual),
            KeyCode::Esc => Some(Action::ExitVisual),

            // Movement extends selection
            KeyCode::Up => Some(Action::Move(0, -1)),
            KeyCode::Down => Some(Action::Move(0, 1)),
            KeyCode::Left => Some(Action::Move(-1, 0)),
            KeyCode::Right => Some(Action::Move(1, 0)),
            KeyCode::Char('p') if ctrl => Some(Action::Move(0, -1)),
            KeyCode::Char('n') if ctrl => Some(Action::Move(0, 1)),
            KeyCode::Char('b') if ctrl => Some(Action::Move(-1, 0)),
            KeyCode::Char('f') if ctrl => Some(Action::Move(1, 0)),
            KeyCode::Char('v') if ctrl => Some(Action::Page(1)),
            KeyCode::Char('v') if alt => Some(Action::Page(-1)),
            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),

            // Copy selection
            KeyCode::Char('w') if alt => Some(Action::Yank),

            _ => None,
        },

        Mode::Edit => match key.code {
            KeyCode::Char('g') if ctrl => Some(Action::Cancel),
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::CommitEdit),
            _ => None,
        },

        Mode::Command => match key.code {
            KeyCode::Char('g') if ctrl => Some(Action::Cancel),
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::ExecuteCommand),
            _ => None,
        },
    }
}
