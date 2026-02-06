use crossterm::event::{self, KeyCode, KeyModifiers};

use super::app::{App, Mode};
use super::keymap::Action;

/// Handle text editing operations on a buffer with UTF-8 aware cursor movement.
fn handle_text_input(buffer: &mut String, cursor: &mut usize, key: event::KeyEvent) {
    match key.code {
        KeyCode::Left => {
            if *cursor > 0 {
                let mut new_pos = *cursor - 1;
                while new_pos > 0 && !buffer.is_char_boundary(new_pos) {
                    new_pos -= 1;
                }
                *cursor = new_pos;
            }
        }
        KeyCode::Right => {
            if *cursor < buffer.len() {
                let mut new_pos = *cursor + 1;
                while new_pos < buffer.len() && !buffer.is_char_boundary(new_pos) {
                    new_pos += 1;
                }
                *cursor = new_pos;
            }
        }
        KeyCode::Home => {
            *cursor = 0;
        }
        KeyCode::End => {
            *cursor = buffer.len();
        }
        KeyCode::Backspace | KeyCode::Char('h')
            if key.code == KeyCode::Backspace || key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            if *cursor > 0 {
                let mut del_start = *cursor - 1;
                while del_start > 0 && !buffer.is_char_boundary(del_start) {
                    del_start -= 1;
                }
                buffer.drain(del_start..*cursor);
                *cursor = del_start;
            }
        }
        KeyCode::Delete => {
            if *cursor < buffer.len() {
                let mut del_end = *cursor + 1;
                while del_end < buffer.len() && !buffer.is_char_boundary(del_end) {
                    del_end += 1;
                }
                buffer.drain(*cursor..del_end);
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                buffer.insert(*cursor, c);
                *cursor += c.len_utf8();
            }
        }
        _ => {}
    }
}

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
        Action::InsertAtStart => app.enter_edit_mode_at(true),
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
        Action::SelectRow => app.select_row(),
        Action::ExitVisual => app.exit_visual_mode(),
        Action::Yank => app.yank(),
        Action::Paste => app.paste(),
        Action::Undo => app.undo(),
        Action::Redo => app.redo(),
        Action::ClearCell => app.clear_current_cell(),
        Action::ChangeCell => {
            app.clear_current_cell();
            app.mode = Mode::Edit;
            app.edit_buffer.clear();
            app.edit_cursor = 0;
        }
        Action::OpenPlot => app.open_plot_modal_at_cursor(),
        Action::FreezeCell => app.freeze_current_cell(),
        Action::FreezeAll => app.freeze_all_cells(),

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
        Action::GotoFirst => app.goto_first(),
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
    handle_text_input(&mut app.edit_buffer, &mut app.edit_cursor, key);
}

pub fn handle_command_text(app: &mut App, key: event::KeyEvent) {
    handle_text_input(&mut app.command_buffer, &mut app.command_cursor, key);
}
