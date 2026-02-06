use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::prelude::*;
use std::io;

use super::actions::{ApplyResult, apply_action, handle_command_text, handle_edit_text};
use super::app::{App, Mode};
use super::keymap::{Action, Keymap, translate};
use super::ui;

fn clear_pending_vim_state(app: &mut App) {
    app.pending_g = false;
    app.pending_d = false;
    app.pending_y = false;
    app.pending_c = false;
    app.pending_z = false;
    app.pending_count = None;
}

fn execute_vim_dd(app: &mut App) {
    app.yank_row();
    app.delete_row();
}

fn handle_mouse_event(app: &mut App, terminal_area: Rect, mouse: MouseEvent) {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    if app.plot_modal.is_some() || app.help_modal || app.mode != Mode::Normal {
        return;
    }

    let [_formula_area, grid_area, _status_area] = ui::split_main_chunks(terminal_area);
    if let Some((col, row)) = ui::grid_cell_at(app, grid_area, mouse.column, mouse.row) {
        app.cursor_col = col;
        app.cursor_row = row;
        app.update_viewport();
        clear_pending_vim_state(app);
    }
}

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        match event::read()? {
            Event::Key(key) => {
                // Only process key press events (Windows reports Press + Release)
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Plot modal takes over input
                if app.plot_modal.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.close_plot_modal();
                        }
                        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.close_plot_modal();
                        }
                        _ => {}
                    }
                    continue;
                }

                // Help modal takes over input
                if app.help_modal {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.close_help_modal();
                        }
                        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.close_help_modal();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.scroll_help_by(1);
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.scroll_help_by(-1);
                        }
                        KeyCode::PageDown => {
                            app.scroll_help_by(12);
                        }
                        KeyCode::PageUp => {
                            app.scroll_help_by(-12);
                        }
                        KeyCode::Home | KeyCode::Char('g') => {
                            app.scroll_help_to_top();
                        }
                        KeyCode::End | KeyCode::Char('G') => {
                            app.scroll_help_to_end();
                        }
                        _ => {}
                    }
                    continue;
                }

                // Handle Vim number prefix (e.g., 5j) in Normal and Visual modes
                if matches!(app.keymap, Keymap::Vim)
                    && matches!(app.mode, Mode::Normal | Mode::Visual)
                {
                    if let KeyCode::Char(c @ '1'..='9') = key.code {
                        if key.modifiers.is_empty() {
                            let digit = c.to_digit(10).unwrap() as usize;
                            app.pending_count = Some(
                                app.pending_count
                                    .unwrap_or(0)
                                    .saturating_mul(10)
                                    .saturating_add(digit),
                            );
                            continue;
                        }
                    } else if let KeyCode::Char('0') = key.code {
                        // '0' only counts as part of number if we already have a count
                        if key.modifiers.is_empty() && app.pending_count.is_some() {
                            app.pending_count =
                                Some(app.pending_count.unwrap_or(0).saturating_mul(10));
                            continue;
                        }
                    }
                }

                // Handle Vim key sequences (gg, dd, yy) in Normal mode
                if matches!(app.keymap, Keymap::Vim) && app.mode == Mode::Normal {
                    // Handle 'gg' sequence (go to first cell)
                    if key.code == KeyCode::Char('g') && key.modifiers.is_empty() {
                        if app.pending_g {
                            app.pending_g = false;
                            if apply_action(app, Action::GotoFirst, key) == ApplyResult::Quit {
                                return Ok(());
                            }
                            continue;
                        } else {
                            app.pending_g = true;
                            app.pending_d = false;
                            app.pending_y = false;
                            app.pending_c = false;
                            app.pending_z = false;
                            continue;
                        }
                    } else if app.pending_g {
                        app.pending_g = false;
                        if apply_action(app, Action::OpenGotoPrompt, key) == ApplyResult::Quit {
                            return Ok(());
                        }
                        continue;
                    }

                    // Handle 'dd' sequence (delete row)
                    if key.code == KeyCode::Char('d') && key.modifiers.is_empty() {
                        if app.pending_d {
                            app.pending_d = false;
                            execute_vim_dd(app);
                            continue;
                        } else {
                            app.pending_d = true;
                            app.pending_g = false;
                            app.pending_y = false;
                            app.pending_c = false;
                            app.pending_z = false;
                            continue;
                        }
                    } else if app.pending_d {
                        // 'd' followed by something else - clear pending
                        app.pending_d = false;
                        // Let the key be processed normally
                    }

                    // Handle 'yy' sequence (yank row)
                    if key.code == KeyCode::Char('y') && key.modifiers.is_empty() {
                        if app.pending_y {
                            app.pending_y = false;
                            app.yank_row();
                            continue;
                        } else {
                            app.pending_y = true;
                            app.pending_g = false;
                            app.pending_d = false;
                            app.pending_c = false;
                            app.pending_z = false;
                            continue;
                        }
                    } else if app.pending_y {
                        // 'y' pressed but not followed by 'y' - do normal yank
                        app.pending_y = false;
                        app.yank();
                        continue;
                    }

                    // Handle 'cc' sequence (change cell) and 'S' shortcut
                    if key.code == KeyCode::Char('c') && key.modifiers.is_empty() {
                        if app.pending_c {
                            app.pending_c = false;
                            if apply_action(app, Action::ChangeCell, key) == ApplyResult::Quit {
                                return Ok(());
                            }
                            continue;
                        } else {
                            app.pending_c = true;
                            app.pending_g = false;
                            app.pending_d = false;
                            app.pending_y = false;
                            app.pending_z = false;
                            continue;
                        }
                    } else if app.pending_c {
                        // 'c' followed by something else - clear pending
                        app.pending_c = false;
                    }

                    // Handle 'zf' / 'zF' sequences (freeze current / freeze all)
                    if key.code == KeyCode::Char('z') && key.modifiers.is_empty() {
                        app.pending_z = true;
                        app.pending_g = false;
                        app.pending_d = false;
                        app.pending_y = false;
                        app.pending_c = false;
                        continue;
                    } else if app.pending_z {
                        app.pending_z = false;
                        match key.code {
                            KeyCode::Char('f') => {
                                if apply_action(app, Action::FreezeCell, key) == ApplyResult::Quit {
                                    return Ok(());
                                }
                                continue;
                            }
                            KeyCode::Char('F') => {
                                if apply_action(app, Action::FreezeAll, key) == ApplyResult::Quit {
                                    return Ok(());
                                }
                                continue;
                            }
                            _ => {
                                // Let non-zf keys fall through for normal processing.
                            }
                        }
                    }
                }

                if let Some(action) = translate(&app.keymap, app.mode, key) {
                    // Apply pending count to movement and paste actions
                    let count = app.pending_count.take().unwrap_or(1);
                    let action = match action {
                        Action::Move(dx, dy) => Action::Move(dx * count as i32, dy * count as i32),
                        Action::Page(dir) => Action::Page(dir * count as i32),
                        Action::Paste => {
                            // Handle paste with count directly
                            app.paste_with_count(count);
                            continue;
                        }
                        other => other,
                    };
                    if apply_action(app, action, key) == ApplyResult::Quit {
                        return Ok(());
                    }
                    continue;
                }

                // Text entry fallbacks (not bound in keymaps).
                match app.mode {
                    Mode::Edit => handle_edit_text(app, key),
                    Mode::Command => handle_command_text(app, key),
                    _ => {}
                }
            }
            Event::Mouse(mouse) => {
                let size = terminal.size()?;
                let terminal_area = Rect::new(0, 0, size.width, size.height);
                handle_mouse_event(app, terminal_area, mouse);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridline_engine::engine::CellRef;

    fn left_click(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    fn first_body_cell_point(grid_area: Rect) -> (u16, u16) {
        (
            grid_area.x + 1 + ui::ROW_HEADER_WIDTH + ui::GRID_COLUMN_SPACING,
            grid_area.y + 2,
        )
    }

    #[test]
    fn handle_mouse_event_moves_cursor_in_normal_mode_and_clears_pending_state() {
        let mut app = App::new();
        app.pending_g = true;
        app.pending_d = true;
        app.pending_y = true;
        app.pending_c = true;
        app.pending_z = true;
        app.pending_count = Some(3);

        let terminal_area = Rect::new(0, 0, 80, 24);
        let [_formula, grid_area, _status] = ui::split_main_chunks(terminal_area);
        let (x, y) = first_body_cell_point(grid_area);

        handle_mouse_event(&mut app, terminal_area, left_click(x, y));

        assert_eq!(app.cursor_col, 0);
        assert_eq!(app.cursor_row, 0);
        assert!(!app.pending_g);
        assert!(!app.pending_d);
        assert!(!app.pending_y);
        assert!(!app.pending_c);
        assert!(!app.pending_z);
        assert!(app.pending_count.is_none());
    }

    #[test]
    fn handle_mouse_event_ignores_clicks_outside_normal_mode() {
        let mut app = App::new();
        app.mode = Mode::Edit;
        app.cursor_col = 3;
        app.cursor_row = 4;

        let terminal_area = Rect::new(0, 0, 80, 24);
        let [_formula, grid_area, _status] = ui::split_main_chunks(terminal_area);
        let (x, y) = first_body_cell_point(grid_area);

        handle_mouse_event(&mut app, terminal_area, left_click(x, y));

        assert_eq!(app.cursor_col, 3);
        assert_eq!(app.cursor_row, 4);
    }

    #[test]
    fn handle_mouse_event_ignores_clicks_when_modal_open() {
        let mut app = App::new();
        app.help_modal = true;
        app.cursor_col = 2;
        app.cursor_row = 2;

        let terminal_area = Rect::new(0, 0, 80, 24);
        let [_formula, grid_area, _status] = ui::split_main_chunks(terminal_area);
        let (x, y) = first_body_cell_point(grid_area);

        handle_mouse_event(&mut app, terminal_area, left_click(x, y));

        assert_eq!(app.cursor_col, 2);
        assert_eq!(app.cursor_row, 2);
    }

    #[test]
    fn handle_mouse_event_ignores_non_cell_clicks() {
        let mut app = App::new();
        app.cursor_col = 1;
        app.cursor_row = 1;

        let terminal_area = Rect::new(0, 0, 80, 24);
        let [_formula, grid_area, _status] = ui::split_main_chunks(terminal_area);
        let row_header_click = left_click(grid_area.x + 2, grid_area.y + 2);

        handle_mouse_event(&mut app, terminal_area, row_header_click);

        assert_eq!(app.cursor_col, 1);
        assert_eq!(app.cursor_row, 1);
    }

    #[test]
    fn execute_vim_dd_yanks_row_before_delete() {
        let mut app = App::new();
        app.cursor_row = 0;
        app.cursor_col = 0;
        app.core
            .set_cell_from_input(CellRef::new(0, 0), "\"alpha\"")
            .unwrap();
        app.core
            .set_cell_from_input(CellRef::new(1, 0), "\"beta\"")
            .unwrap();
        app.core
            .set_cell_from_input(CellRef::new(0, 1), "\"next\"")
            .unwrap();

        execute_vim_dd(&mut app);

        let clipboard = app.clipboard.as_ref().expect("dd should yank row");
        assert_eq!(clipboard.height, 1);
        assert_eq!(clipboard.width, 2);
        assert_eq!(clipboard.cells.len(), 2);
        assert_eq!(app.core.get_cell_display(&CellRef::new(0, 0)), "next");
    }
}
