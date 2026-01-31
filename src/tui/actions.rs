use crossterm::event::{self, KeyCode, KeyModifiers};

use super::app::{App, Mode};
use super::keymap::Action;

/// Result of applying an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyResult {
    Continue,
    Quit,
}

/// Apply an action to the application state.
///
/// Returns `ApplyResult::Quit` if the application should exit.
pub fn apply_action(app: &mut App, action: Action, _key: event::KeyEvent) -> ApplyResult {
    match action {
        Action::Cancel => match app.mode {
            Mode::Edit => {
                app.mode = Mode::Normal;
                app.edit_buffer.clear();
                app.edit_cursor = 0;
            }
            Mode::Command => {
                app.mode = Mode::Normal;
                app.command_buffer.clear();
                app.command_cursor = 0;
            }
            Mode::Visual => {
                app.exit_visual_mode();
            }
            Mode::Normal => {}
        },

        Action::EnterEdit => app.enter_edit_mode(),
        Action::CommitEdit => app.commit_edit(),
        Action::EnterCommand => {
            app.mode = Mode::Command;
            app.command_buffer.clear();
            app.command_cursor = 0;
        }
        Action::ExecuteCommand => {
            if app.execute_command() {
                return ApplyResult::Quit;
            }
        }
        Action::EnterVisual => {
            if app.mode != Mode::Visual {
                app.enter_visual_mode();
            }
        }
        Action::ExitVisual => app.exit_visual_mode(),
        Action::Yank => app.yank(),
        Action::Paste => app.paste(),
        Action::Undo => app.undo(),
        Action::Redo => app.redo(),
        Action::ClearCell => app.clear_current_cell(),
        Action::OpenPlot => app.open_plot_modal_at_cursor(),

        Action::Move(dx, dy) => app.move_cursor(dx, dy),
        Action::Page(dir) => {
            let delta = app.visible_rows as i32 * dir;
            app.move_cursor(0, delta);
        }
        Action::HomeCol => {
            app.cursor_col = 0;
            app.update_viewport();
        }
        Action::EndCol => {
            app.cursor_col = app.max_cols.saturating_sub(1);
            app.update_viewport();
        }
        Action::GotoLast => app.goto_last(),
        Action::OpenGotoPrompt => {
            app.mode = Mode::Command;
            app.command_buffer = "goto ".to_string();
            app.command_cursor = app.command_buffer.len();
        }

        Action::IncColWidth => app.increase_column_width(),
        Action::DecColWidth => app.decrease_column_width(),
        Action::Save => app.save_file(),
    }
    ApplyResult::Continue
}

pub fn handle_edit_text(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Left => {
            // Move cursor left (handle UTF-8 char boundaries)
            if app.edit_cursor > 0 {
                let mut new_pos = app.edit_cursor - 1;
                while new_pos > 0 && !app.edit_buffer.is_char_boundary(new_pos) {
                    new_pos -= 1;
                }
                app.edit_cursor = new_pos;
            }
        }
        KeyCode::Right => {
            // Move cursor right (handle UTF-8 char boundaries)
            if app.edit_cursor < app.edit_buffer.len() {
                let mut new_pos = app.edit_cursor + 1;
                while new_pos < app.edit_buffer.len() && !app.edit_buffer.is_char_boundary(new_pos)
                {
                    new_pos += 1;
                }
                app.edit_cursor = new_pos;
            }
        }
        KeyCode::Home => {
            app.edit_cursor = 0;
        }
        KeyCode::End => {
            app.edit_cursor = app.edit_buffer.len();
        }
        KeyCode::Backspace => {
            // Delete char before cursor
            if app.edit_cursor > 0 {
                let mut del_start = app.edit_cursor - 1;
                while del_start > 0 && !app.edit_buffer.is_char_boundary(del_start) {
                    del_start -= 1;
                }
                app.edit_buffer.drain(del_start..app.edit_cursor);
                app.edit_cursor = del_start;
            }
        }
        // Some terminals send Backspace as Ctrl+H in raw mode.
        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.edit_cursor > 0 {
                let mut del_start = app.edit_cursor - 1;
                while del_start > 0 && !app.edit_buffer.is_char_boundary(del_start) {
                    del_start -= 1;
                }
                app.edit_buffer.drain(del_start..app.edit_cursor);
                app.edit_cursor = del_start;
            }
        }
        KeyCode::Delete => {
            // Delete char at cursor
            if app.edit_cursor < app.edit_buffer.len() {
                let mut del_end = app.edit_cursor + 1;
                while del_end < app.edit_buffer.len() && !app.edit_buffer.is_char_boundary(del_end)
                {
                    del_end += 1;
                }
                app.edit_buffer.drain(app.edit_cursor..del_end);
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                // Insert char at cursor position
                app.edit_buffer.insert(app.edit_cursor, c);
                app.edit_cursor += c.len_utf8();
            }
        }
        _ => {}
    }
}

pub fn handle_command_text(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Left => {
            if app.command_cursor > 0 {
                let mut new_pos = app.command_cursor - 1;
                while new_pos > 0 && !app.command_buffer.is_char_boundary(new_pos) {
                    new_pos -= 1;
                }
                app.command_cursor = new_pos;
            }
        }
        KeyCode::Right => {
            if app.command_cursor < app.command_buffer.len() {
                let mut new_pos = app.command_cursor + 1;
                while new_pos < app.command_buffer.len()
                    && !app.command_buffer.is_char_boundary(new_pos)
                {
                    new_pos += 1;
                }
                app.command_cursor = new_pos;
            }
        }
        KeyCode::Home => {
            app.command_cursor = 0;
        }
        KeyCode::End => {
            app.command_cursor = app.command_buffer.len();
        }
        KeyCode::Backspace => {
            if app.command_cursor > 0 {
                let mut del_start = app.command_cursor - 1;
                while del_start > 0 && !app.command_buffer.is_char_boundary(del_start) {
                    del_start -= 1;
                }
                app.command_buffer.drain(del_start..app.command_cursor);
                app.command_cursor = del_start;
            }
        }
        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.command_cursor > 0 {
                let mut del_start = app.command_cursor - 1;
                while del_start > 0 && !app.command_buffer.is_char_boundary(del_start) {
                    del_start -= 1;
                }
                app.command_buffer.drain(del_start..app.command_cursor);
                app.command_cursor = del_start;
            }
        }
        KeyCode::Delete => {
            if app.command_cursor < app.command_buffer.len() {
                let mut del_end = app.command_cursor + 1;
                while del_end < app.command_buffer.len()
                    && !app.command_buffer.is_char_boundary(del_end)
                {
                    del_end += 1;
                }
                app.command_buffer.drain(app.command_cursor..del_end);
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                app.command_buffer.insert(app.command_cursor, c);
                app.command_cursor += c.len_utf8();
            }
        }
        _ => {}
    }
}
