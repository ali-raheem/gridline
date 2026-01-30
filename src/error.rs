//! Error types for the Gridline application

use thiserror::Error;

/// Errors that can occur in the Gridline application
#[derive(Error, Debug)]
pub enum GridlineError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
}

pub type Result<T> = std::result::Result<T, GridlineError>;
