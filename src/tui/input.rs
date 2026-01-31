use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use std::io;

use super::actions::{apply_action, handle_command_text, handle_edit_text, ApplyResult};
use super::app::{App, Mode};
use super::keymap::translate;
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
                    _ => {}
                }
                continue;
            }

            if let Some(action) = translate(&app.keymap, app.mode, key) {
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
