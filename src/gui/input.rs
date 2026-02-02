//! Keyboard input handling and event translation to actions.

use crate::gui::actions::Action;
use eframe::egui;
use egui::{Key, Modifiers};

/// Check if a keyboard shortcut was consumed.
/// `COMMAND` maps to Ctrl on Windows/Linux and Cmd on macOS.
pub fn consume_shortcut(ctx: &egui::Context, key: Key) -> bool {
    ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, key)))
}

/// Check if formula bar is currently focused.
pub fn is_formula_focused(ctx: &egui::Context, formula_id: egui::Id) -> bool {
    ctx.memory(|m| m.focused()) == Some(formula_id)
}

/// Extract any paste event from input.
pub fn extract_paste_event(ctx: &egui::Context) -> Option<String> {
    ctx.input(|i| {
        i.events.iter().find_map(|ev| {
            if let egui::Event::Paste(s) = ev {
                Some(s.clone())
            } else {
                None
            }
        })
    })
}

/// Translate keyboard events to actions (when not editing).
pub fn handle_keyboard_input(ctx: &egui::Context) -> Option<Action> {
    let input = ctx.input(|i| i.clone());

    // Cursor movement
    let mut dx: isize = 0;
    let mut dy: isize = 0;

    if input.key_pressed(Key::ArrowLeft) {
        dx -= 1;
    }
    if input.key_pressed(Key::ArrowRight) {
        dx += 1;
    }
    if input.key_pressed(Key::ArrowUp) {
        dy -= 1;
    }
    if input.key_pressed(Key::ArrowDown) {
        dy += 1;
    }

    if dx != 0 || dy != 0 {
        return Some(Action::MoveCursor {
            dx,
            dy,
            extend: input.modifiers.shift,
        });
    }

    // Enter to edit
    if input.key_pressed(Key::Enter) {
        return Some(Action::BeginEdit);
    }

    // Delete/Backspace to clear
    let pressed_delete = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete));
    if pressed_delete {
        return Some(Action::ClearSelection);
    }

    let pressed_backspace = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace));
    if pressed_backspace {
        return Some(Action::ClearSelection);
    }

    // Check for shortcuts using key_pressed (more reliable than consume_shortcut)
    let input = ctx.input(|i| i.clone());
    let cmd_pressed = input.modifiers.command;
    let shift_pressed = input.modifiers.shift;

    // Undo: Ctrl+Z (standard Excel/Word shortcut)
    if cmd_pressed && input.key_pressed(Key::Z) && !shift_pressed {
        return Some(Action::Undo);
    }

    // Redo: Ctrl+Y (standard Excel/Word shortcut)
    if cmd_pressed && input.key_pressed(Key::Y) {
        return Some(Action::Redo);
    }

    // Also support Ctrl+Shift+Z for redo (alternative)
    if cmd_pressed && shift_pressed && input.key_pressed(Key::Z) {
        return Some(Action::Redo);
    }

    // Check for egui Copy/Paste/Cut events (egui converts Ctrl+C/V/X to these events)
    for event in &input.events {
        match event {
            egui::Event::Copy => {
                return Some(Action::CopySelection);
            }
            egui::Event::Cut => {
                return Some(Action::CutSelection);
            }
            egui::Event::Paste(text) => {
                return Some(Action::Paste(text.clone()));
            }
            _ => {}
        }
    }

    // Delete row: Alt+-
    let alt_minus = ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::Minus));
    if alt_minus {
        return Some(Action::DeleteRow);
    }

    // Insert row/col: Alt++
    // Coordinate order is col/row elsewhere.
    let alt_plus = ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::Plus));
    if alt_plus {
        return Some(Action::InsertRow);
    }

    // Save: Ctrl+S
    if cmd_pressed && input.key_pressed(Key::S) {
        return Some(Action::Save);
    }

    None
}
