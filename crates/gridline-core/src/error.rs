//! Error types for Gridline core.

use thiserror::Error;

use rhai::EvalAltResult;

/// Errors that can occur in the Gridline application
#[derive(Error, Debug)]
pub enum GridlineError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },

    #[error("Circular dependency detected")]
    CircularDependency,

    #[error("No file path set")]
    NoFilePath,

    #[error("No functions file loaded")]
    NoFunctionsLoaded,

    #[error("CSV file is empty")]
    EmptyCsv,

    #[error("Nothing to undo")]
    NothingToUndo,

    #[error("Nothing to redo")]
    NothingToRedo,

    #[error("Rhai error: {0}")]
    Rhai(
        #[from]
        #[source]
        Box<EvalAltResult>,
    ),

    #[error("Rhai compile error: {0}")]
    RhaiCompile(String),
}

pub type Result<T> = std::result::Result<T, GridlineError>;
