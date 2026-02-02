//! Document state and logic (UI-agnostic).

mod eval;
mod io;
mod ops;
mod script;
mod state;

pub use script::ScriptContext;
pub use state::{Document, UndoAction, UndoEntry};
