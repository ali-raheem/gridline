use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use std::io;

use super::actions::{ApplyResult, apply_action, handle_command_text, handle_edit_text};
use super::app::{App, Mode};
use super::keymap::{Action, Keymap, translate};
use super::ui;

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if let Event::Key(key) = event::read()? {
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
                            app.pending_count.unwrap_or(0).saturating_mul(10).saturating_add(digit)
                        );
                        continue;
                    }
                } else if let KeyCode::Char('0') = key.code {
                    // '0' only counts as part of number if we already have a count
                    if key.modifiers.is_empty() && app.pending_count.is_some() {
                        app.pending_count = Some(
                            app.pending_count.unwrap_or(0).saturating_mul(10)
                        );
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
                        app.delete_row();
                        continue;
                    } else {
                        app.pending_d = true;
                        app.pending_g = false;
                        app.pending_y = false;
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
                        continue;
                    }
                } else if app.pending_y {
                    // 'y' pressed but not followed by 'y' - do normal yank
                    app.pending_y = false;
                    app.yank();
                    continue;
                }
            }

            if let Some(action) = translate(&app.keymap, app.mode, key) {
                // Apply pending count to movement actions
                let count = app.pending_count.take().unwrap_or(1) as i32;
                let action = match action {
                    Action::Move(dx, dy) => Action::Move(dx * count, dy * count),
                    Action::Page(dir) => Action::Page(dir * count),
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
    }
}
