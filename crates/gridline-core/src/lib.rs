//! gridline-core - UI-agnostic document model + storage.

pub mod document;
pub mod error;
pub mod storage;

pub use document::{Document, ScriptContext, UndoAction, UndoEntry};
pub use error::{GridlineError, Result};

pub use gridline_engine::engine::CellRef;
