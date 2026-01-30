use rhai::Engine;
use std::sync::Arc;

use super::{AST, Dynamic, Grid};

/// Create a Rhai engine with built-ins registered.
pub fn create_engine(grid: Arc<Grid>) -> Engine {
    let mut engine = Engine::new();
    crate::builtins::register_builtins(&mut engine, grid);
    engine
}

/// Create a Rhai engine with built-ins registered.
/// Optionally compiles custom functions from the provided script.
/// Returns the engine, compiled AST (if any), and any error message.
pub fn create_engine_with_functions(
    grid: Arc<Grid>,
    custom_script: Option<&str>,
) -> (Engine, Option<AST>, Option<String>) {
    let engine = create_engine(grid);

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
