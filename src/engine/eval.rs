//! Rhai engine creation and formula evaluation.
//!
//! Creates the Rhai scripting engine with all spreadsheet built-in functions
//! registered (SUM, AVERAGE, cell accessors, etc.). Also handles evaluation
//! of formulas with optional user-defined custom functions from external files.

use rhai::Engine;
use std::sync::Arc;

use super::{AST, ComputedMap, Dynamic, Grid, SpillMap};

/// Create a Rhai engine with built-ins registered.
pub fn create_engine(grid: Arc<Grid>) -> Engine {
    let spill_map = Arc::new(SpillMap::new());
    let computed_map = Arc::new(ComputedMap::new());
    create_engine_with_spill(grid, spill_map, computed_map)
}

/// Create a Rhai engine with built-ins registered and shared maps.
pub fn create_engine_with_spill(
    grid: Arc<Grid>,
    spill_map: Arc<SpillMap>,
    computed_map: Arc<ComputedMap>,
) -> Engine {
    let mut engine = Engine::new();
    crate::builtins::register_builtins(&mut engine, grid, spill_map, computed_map);
    engine
}

/// Create a Rhai engine with built-ins registered.
/// Optionally compiles custom functions from the provided script.
/// Returns the engine, compiled AST (if any), and any error message.
pub fn create_engine_with_functions(
    grid: Arc<Grid>,
    custom_script: Option<&str>,
) -> (Engine, Option<AST>, Option<String>) {
    let spill_map = Arc::new(SpillMap::new());
    let computed_map = Arc::new(ComputedMap::new());
    create_engine_with_functions_and_spill(grid, spill_map, computed_map, custom_script)
}

/// Create a Rhai engine with built-ins, custom functions, and shared maps.
/// Returns the engine, compiled AST (if any), and any error message.
pub fn create_engine_with_functions_and_spill(
    grid: Arc<Grid>,
    spill_map: Arc<SpillMap>,
    computed_map: Arc<ComputedMap>,
    custom_script: Option<&str>,
) -> (Engine, Option<AST>, Option<String>) {
    let engine = create_engine_with_spill(grid, spill_map, computed_map);

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
pub fn eval_with_functions(
    engine: &Engine,
    formula: &str,
    custom_ast: Option<&AST>,
) -> Result<Dynamic, String> {
    if let Some(ast) = custom_ast {
        match engine.compile(formula) {
            Ok(formula_ast) => {
                let merged = ast.clone().merge(&formula_ast);
                engine.eval_ast(&merged).map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    } else {
        engine.eval(formula).map_err(|e| e.to_string())
    }
}
