use crossterm::event::{self, KeyCode, KeyModifiers};

use super::app::{App, Mode};
use super::keymap::Action;

pub fn apply_action(app: &mut App, action: Action, _key: event::KeyEvent) {
    match action {
        Action::Cancel => match app.mode {
            Mode::Edit => {
                app.mode = Mode::Normal;
                app.edit_buffer.clear();
            }
            Mode::Command => {
                app.mode = Mode::Normal;
                app.command_buffer.clear();
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
        }
        Action::ExecuteCommand => app.execute_command(),
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
        }

        Action::IncColWidth => app.increase_column_width(),
        Action::DecColWidth => app.decrease_column_width(),
        Action::Save => app.save_file(),
    }
}

pub fn handle_edit_text(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Backspace => {
            app.edit_buffer.pop();
        }
        // Some terminals send Backspace as Ctrl+H in raw mode.
        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.edit_buffer.pop();
        }
        KeyCode::Char(c) => {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                app.edit_buffer.push(c);
            }
        }
        _ => {}
    }
}

pub fn handle_command_text(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Backspace => {
            app.command_buffer.pop();
        }
        // Some terminals send Backspace as Ctrl+H in raw mode.
        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.command_buffer.pop();
        }
        KeyCode::Char(c) => {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                app.command_buffer.push(c);
            }
        }
        _ => {}
    }
}
