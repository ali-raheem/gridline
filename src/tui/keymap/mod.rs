//! Keymap translation layer.
//!
//! This keeps key handling separate from app behavior.
//! - Vim keymap preserves existing behavior.
//! - Emacs keymap is "strict": vim-style letter keys are not active.

mod defaults;
mod parse;
mod types;

pub use parse::load_keymap;
pub use types::{Action, Binding, CustomKeymap, KeyCombo, Keymap, KeymapBindings};

use crate::tui::app::Mode;
use crossterm::event::KeyEvent;

/// Translate a key event to an action based on the current keymap and mode.
///
/// Returns `None` if the key has no binding in the current context.
pub fn translate(keymap: &Keymap, mode: Mode, key: KeyEvent) -> Option<Action> {
    match keymap {
        Keymap::Vim => defaults::translate_vim(mode, key),
        Keymap::Emacs => defaults::translate_emacs(mode, key),
        Keymap::Custom(custom) => custom.translate(mode, key),
    }
}
