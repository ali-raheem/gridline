use crossterm::event::{self, KeyCode, KeyModifiers};

use super::app::{App, Mode};
use super::keymap::Action;

fn modifiers_only_include(modifiers: KeyModifiers, allowed: KeyModifiers) -> bool {
    (modifiers & !allowed).is_empty()
}

fn allows_text_char_input(modifiers: KeyModifiers) -> bool {
    if modifiers.is_empty() {
        return true;
    }
    if modifiers_only_include(modifiers, KeyModifiers::SHIFT) {
        return true;
    }

    // On Windows and some Linux layouts, AltGr is reported as Ctrl+Alt.
    // Treat AltGr as printable text input for symbols like '|', '@', and '{'.
    let alt_gr = KeyModifiers::CONTROL | KeyModifiers::ALT;
    modifiers.contains(alt_gr) && modifiers_only_include(modifiers, alt_gr | KeyModifiers::SHIFT)
}

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
            if allows_text_char_input(key.modifiers) {
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
        Action::CommitEditDown => {
            app.commit_edit();
            app.move_cursor(0, 1);
        }
        Action::CommitEditRight => {
            app.commit_edit();
            app.move_cursor(1, 0);
        }
        Action::CommitEditLeft => {
            app.commit_edit();
            app.move_cursor(-1, 0);
        }
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
        Action::OpenRowBelowEdit => {
            app.move_cursor(0, 1);
            app.insert_row();
            app.enter_edit_mode();
        }
        Action::OpenRowAboveEdit => {
            app.insert_row();
            app.enter_edit_mode();
        }

        Action::Move(dx, dy) => app.move_cursor(dx, dy),
        Action::Page(dir) => {
            let delta = app.visible_rows as i32 * dir;
            app.move_cursor(0, delta);
        }
        Action::HomeDataCol => app.goto_row_data_first_col(),
        Action::EndDataCol => app.goto_row_data_last_col(),
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
        Action::SearchPrompt => {
            app.mode = Mode::Command;
            app.command_buffer = "/".to_string();
            app.command_cursor = app.command_buffer.len();
        }
        Action::SearchNext => app.search_next(),
        Action::SearchPrev => app.search_prev(),
    }
    ApplyResult::Continue
}

pub fn handle_edit_text(app: &mut App, key: event::KeyEvent) {
    handle_text_input(&mut app.edit_buffer, &mut app.edit_cursor, key);
}

pub fn handle_command_text(app: &mut App, key: event::KeyEvent) {
    handle_text_input(&mut app.command_buffer, &mut app.command_cursor, key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;
    use gridline_engine::engine::CellRef;

    #[test]
    fn handle_text_input_accepts_altgr_symbols() {
        let mut buffer = String::new();
        let mut cursor = 0;
        let key = KeyEvent::new(
            KeyCode::Char('|'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );

        handle_text_input(&mut buffer, &mut cursor, key);

        assert_eq!(buffer, "|");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn handle_text_input_accepts_shift_altgr_symbols() {
        let mut buffer = String::new();
        let mut cursor = 0;
        let key = KeyEvent::new(
            KeyCode::Char('€'),
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT,
        );

        handle_text_input(&mut buffer, &mut cursor, key);

        assert_eq!(buffer, "€");
        assert_eq!(cursor, '€'.len_utf8());
    }

    #[test]
    fn handle_text_input_rejects_plain_ctrl_chars() {
        let mut buffer = String::new();
        let mut cursor = 0;
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);

        handle_text_input(&mut buffer, &mut cursor, key);

        assert_eq!(buffer, "");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn apply_action_open_row_above_edit_inserts_and_enters_edit_mode() {
        let mut app = App::new();
        app.cursor_col = 1;
        app.cursor_row = 0;
        app.core
            .set_cell_from_input(CellRef::new(1, 0), "top")
            .expect("seed top cell");

        let result = apply_action(
            &mut app,
            Action::OpenRowAboveEdit,
            KeyEvent::new(KeyCode::Char('O'), KeyModifiers::empty()),
        );

        assert_eq!(result, ApplyResult::Continue);
        assert!(matches!(app.mode, Mode::Edit));
        assert_eq!(app.cursor_col, 1);
        assert_eq!(app.cursor_row, 0);
        assert_eq!(app.core.get_cell_display(&CellRef::new(1, 0)), "");
        assert_eq!(app.core.get_cell_display(&CellRef::new(1, 1)), "top");
    }

    #[test]
    fn apply_action_open_row_below_edit_inserts_and_enters_edit_mode() {
        let mut app = App::new();
        app.cursor_col = 1;
        app.cursor_row = 0;
        app.core
            .set_cell_from_input(CellRef::new(1, 0), "top")
            .expect("seed top cell");
        app.core
            .set_cell_from_input(CellRef::new(1, 1), "below")
            .expect("seed second row");

        let result = apply_action(
            &mut app,
            Action::OpenRowBelowEdit,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::empty()),
        );

        assert_eq!(result, ApplyResult::Continue);
        assert!(matches!(app.mode, Mode::Edit));
        assert_eq!(app.cursor_col, 1);
        assert_eq!(app.cursor_row, 1);
        assert_eq!(app.core.get_cell_display(&CellRef::new(1, 0)), "top");
        assert_eq!(app.core.get_cell_display(&CellRef::new(1, 1)), "");
        assert_eq!(app.core.get_cell_display(&CellRef::new(1, 2)), "below");
    }

    #[test]
    fn apply_action_home_and_end_data_col_use_data_bounds() {
        let mut app = App::new();
        app.cursor_row = 3;
        app.cursor_col = 5;
        app.core
            .set_cell_from_input(CellRef::new(7, 3), "right")
            .expect("set right");
        app.core
            .set_cell_from_input(CellRef::new(2, 3), "left")
            .expect("set left");

        let _ = apply_action(
            &mut app,
            Action::HomeDataCol,
            KeyEvent::new(KeyCode::Char('0'), KeyModifiers::empty()),
        );
        assert_eq!(app.cursor_col, 2);

        let _ = apply_action(
            &mut app,
            Action::EndDataCol,
            KeyEvent::new(KeyCode::Char('$'), KeyModifiers::SHIFT),
        );
        assert_eq!(app.cursor_col, 7);
    }

    #[test]
    fn apply_action_end_data_col_falls_back_to_a_when_row_is_empty() {
        let mut app = App::new();
        app.cursor_row = 8;
        app.cursor_col = 4;

        let _ = apply_action(
            &mut app,
            Action::EndDataCol,
            KeyEvent::new(KeyCode::Char('$'), KeyModifiers::SHIFT),
        );
        assert_eq!(app.cursor_col, 0);
    }
}
