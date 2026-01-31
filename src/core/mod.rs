//! Core spreadsheet state and logic, UI-agnostic.

mod eval;
mod io;
mod ops;
mod state;

pub use state::{Core, UndoAction};
