//! Spreadsheet engine API.

mod cell;
mod cell_ref;
mod cycle;
mod deps;
mod eval;
mod format;
mod preprocess;

pub use cell::{Cell, CellType, Grid};
pub use cell_ref::CellRef;
pub use cycle::detect_cycle;
pub use deps::{extract_dependencies, parse_range};
pub use eval::{create_engine, create_engine_with_functions, eval_with_functions};
pub use format::{format_dynamic, format_number};
pub use preprocess::preprocess_script;

pub use rhai::{AST, Dynamic};
