//! Rhai engine creation and formula evaluation.
//!
//! Creates the Rhai scripting engine with all spreadsheet built-in functions
//! registered (SUM, AVERAGE, cell accessors, etc.). Also handles evaluation
//! of formulas with optional user-defined custom functions from external files.

use rhai::{Engine, EvalAltResult};

use super::{AST, Dynamic, Grid, ValueCache};
use crate::builtins::ScriptModifications;

/// Create a Rhai engine with built-ins registered.
pub fn create_engine(grid: Grid) -> Engine {
    let value_cache = ValueCache::default();
    create_engine_with_cache(grid, value_cache)
}

/// Create a Rhai engine with built-ins registered and shared value cache.
pub fn create_engine_with_cache(grid: Grid, value_cache: ValueCache) -> Engine {
    let mut engine = Engine::new();
    crate::builtins::register_builtins(&mut engine, grid, value_cache);
    engine
}

/// Create a Rhai engine with built-ins registered.
/// Optionally compiles custom functions from the provided script.
/// Returns the engine, compiled AST (if any), and any error message.
pub fn create_engine_with_functions(
    grid: Grid,
    custom_script: Option<&str>,
) -> (Engine, Option<AST>, Option<String>) {
    let value_cache = ValueCache::default();
    create_engine_with_functions_and_cache(grid, value_cache, custom_script)
}

/// Create a Rhai engine with built-ins, custom functions, and shared value cache.
/// Returns the engine, compiled AST (if any), and any error message.
pub fn create_engine_with_functions_and_cache(
    grid: Grid,
    value_cache: ValueCache,
    custom_script: Option<&str>,
) -> (Engine, Option<AST>, Option<String>) {
    let engine = create_engine_with_cache(grid, value_cache);

    let (ast, error) = if let Some(script) = custom_script {
        match engine.compile(script) {
            Ok(ast) => (Some(ast), None),
            Err(e) => (None, Some(format!("Error in custom functions: {}", e))),
        }
    } else {
        (None, None)
    };

    (engine, ast, error)
}

/// Evaluate a formula, optionally with custom functions AST.
///
/// When custom functions are present, we need both the AST and the original script
/// to properly evaluate closures. Closures need access to registered functions,
/// which works better when evaluating as a script rather than merged ASTs.
pub fn eval_with_functions(
    engine: &Engine,
    formula: &str,
    custom_ast: Option<&AST>,
) -> Result<Dynamic, Box<EvalAltResult>> {
    if custom_ast.is_some() {
        // For now, use simple AST merging
        // TODO: This has issues with closures not accessing registered functions
        let formula_ast = engine.compile(formula).map_err(|e| {
            let parse_type = *e.0;
            let pos = e.1;
            Box::new(EvalAltResult::ErrorParsing(parse_type, pos))
        })?;
        let merged = custom_ast.unwrap().clone().merge(&formula_ast);
        engine.eval_ast(&merged)
    } else {
        engine.eval(formula)
    }
}

/// Evaluate a formula with custom functions provided as script text.
/// This version concatenates the scripts and evaluates them together,
/// which properly handles closures accessing registered functions.
pub fn eval_with_functions_script(
    engine: &Engine,
    formula: &str,
    custom_script: Option<&str>,
) -> Result<Dynamic, Box<EvalAltResult>> {
    if let Some(script) = custom_script {
        // Concatenate custom functions with formula and evaluate as one script
        // This ensures closures can access both custom and registered functions
        let combined = format!("{}\n{}", script, formula);
        engine.eval(&combined)
    } else {
        engine.eval(formula)
    }
}

/// Create a Rhai engine for script execution with write builtins.
/// This engine includes all read builtins plus write operations (set_cell, clear_cell, etc.).
/// Used for :call and :rhai commands, NOT for cell formula evaluation.
pub fn create_script_engine(
    grid: Grid,
    value_cache: ValueCache,
    modifications: ScriptModifications,
) -> Engine {
    let mut engine = Engine::new();
    crate::builtins::register_builtins(&mut engine, grid.clone(), value_cache);
    crate::builtins::register_script_builtins(&mut engine, grid, modifications);
    engine
}

/// Create a script engine with custom functions.
/// Returns the engine, compiled AST (if any), and any error message.
pub fn create_script_engine_with_functions(
    grid: Grid,
    value_cache: ValueCache,
    modifications: ScriptModifications,
    custom_script: Option<&str>,
) -> (Engine, Option<AST>, Option<String>) {
    let engine = create_script_engine(grid, value_cache, modifications);

    let (ast, error) = if let Some(script) = custom_script {
        match engine.compile(script) {
            Ok(ast) => (Some(ast), None),
            Err(e) => (None, Some(format!("Error in custom functions: {}", e))),
        }
    } else {
        (None, None)
    };

    (engine, ast, error)
}

// Backward compatibility aliases (deprecated)
#[doc(hidden)]
#[allow(dead_code)]
pub fn create_engine_with_spill(
    grid: Grid,
    value_cache: ValueCache,
    _deprecated: ValueCache,
) -> Engine {
    create_engine_with_cache(grid, value_cache)
}

#[doc(hidden)]
#[allow(dead_code)]
pub fn create_engine_with_functions_and_spill(
    grid: Grid,
    value_cache: ValueCache,
    custom_script: Option<&str>,
) -> (Engine, Option<AST>, Option<String>) {
    create_engine_with_functions_and_cache(grid, value_cache, custom_script)
}
