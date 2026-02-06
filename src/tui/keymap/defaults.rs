use crate::tui::app::Mode;
use crate::tui::keymap::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn translate_vim(mode: Mode, key: KeyEvent) -> Option<Action> {
    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('u') => Some(Action::Undo),
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Redo)
            }

            KeyCode::Up | KeyCode::Char('k') => Some(Action::Move(0, -1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::Move(0, 1)),
            KeyCode::Left | KeyCode::Char('h') => Some(Action::Move(-1, 0)),
            KeyCode::Right | KeyCode::Char('l') => Some(Action::Move(1, 0)),
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => Some(Action::Move(-1, 0)),
            KeyCode::Tab => Some(Action::Move(1, 0)),
            KeyCode::BackTab => Some(Action::Move(-1, 0)),

            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),
            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::GotoFirst),
            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::GotoLast),
            KeyCode::Home => Some(Action::HomeCol),
            KeyCode::End => Some(Action::EndCol),

            KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('a') | KeyCode::Char('A') => {
                Some(Action::EnterEdit)
            }
            KeyCode::Char('I') => Some(Action::InsertAtStart),
            KeyCode::Char('x') | KeyCode::Delete => Some(Action::ClearCell),
            KeyCode::Char('S') => Some(Action::ChangeCell),
            KeyCode::Char(':') => Some(Action::EnterCommand),
            KeyCode::Char('v') => Some(Action::EnterVisual),
            KeyCode::Char('y') => Some(Action::Yank),
            KeyCode::Char('p') => Some(Action::Paste),
            KeyCode::Char('P') => Some(Action::OpenPlot),
            KeyCode::Char('+') | KeyCode::Char('>') => Some(Action::IncColWidth),
            KeyCode::Char('-') | KeyCode::Char('<') => Some(Action::DecColWidth),
            KeyCode::Char('G') => Some(Action::GotoLast),
            // 'g' is handled specially in input.rs for gg sequence
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

pub(crate) fn translate_emacs(mode: Mode, key: KeyEvent) -> Option<Action> {
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

            // Tab navigation
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => Some(Action::Move(-1, 0)),
            KeyCode::Tab => Some(Action::Move(1, 0)),
            KeyCode::BackTab => Some(Action::Move(-1, 0)),

            // Home/End column
            KeyCode::Char('a') if ctrl => Some(Action::HomeCol),
            KeyCode::Char('e') if ctrl => Some(Action::EndCol),
            KeyCode::Home if ctrl => Some(Action::GotoFirst),
            KeyCode::End if ctrl => Some(Action::GotoLast),
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
