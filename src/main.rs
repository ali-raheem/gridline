//! Gridline - A spreadsheet application with TUI

mod error;
mod core;
mod storage;
mod tui;

use std::env;
use std::path::PathBuf;
use gridline_engine::engine::CellRef;

/// Run command mode: evaluate a formula and print the result
fn run_command_mode(
    formula: String,
    functions_files: Vec<PathBuf>,
    output_file: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    use gridline_engine::engine::{Cell, CellRef};

    // Create minimal core instance
    let mut core = core::Core::new();

    // Load custom functions
    for func_path in &functions_files {
        if let Err(e) = core.load_functions(func_path) {
            eprintln!("Warning: failed to load functions from {:?}: {}", func_path, e);
        }
    }

    // Create cell with formula (prepend '=' if not present)
    let formula_with_eq = if formula.starts_with('=') {
        formula
    } else {
        format!("={}", formula)
    };

    let cell_ref = CellRef::new(0, 0);
    let cell = Cell::from_input(&formula_with_eq);
    core.grid.insert(cell_ref.clone(), cell);
    core.rebuild_dependents();

    // Evaluate and get result
    let result = core.get_cell_display(&cell_ref);

    // Check for errors (for exit code)
    let is_error = result.starts_with("#ERR")
        || result.starts_with("#CYCLE")
        || result.starts_with("#SPILL")
        || result.starts_with("#INF")
        || result.starts_with("#NAN");

    // Output handling
    if let Some(output_path) = output_file {
        // Write to markdown (handles arrays as spilled grid)
        write_command_result_markdown(&output_path, core)?;
        eprintln!("Result written to {}", output_path.display());
    } else {
        // Print to stdout
        print_command_result(&result, &cell_ref, &core);
    }

    // Exit with appropriate code
    if is_error {
        std::process::exit(1);
    }

    Ok(())
}

/// Print command result to stdout, handling array/spill results
fn print_command_result(result: &str, cell_ref: &CellRef, core: &core::Core) {
    use gridline_engine::engine::format_dynamic;

    // Check if this is a spill source (array result)
    let has_spill = core.spill_sources.values().any(|src| src == cell_ref);

    if has_spill {
        // Print array elements one per line
        // Start with the source cell
        println!("{}", result);

        // Print each spilled cell
        let mut row = cell_ref.row + 1;
        loop {
            let spill_ref = CellRef::new(row, cell_ref.col);
            if let Some(src) = core.spill_sources.get(&spill_ref) {
                if src == cell_ref {
                    if let Some(val) = core.value_cache.get(&spill_ref) {
                        println!("{}", format_dynamic(&val));
                    }
                    row += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    } else {
        // Simple scalar result
        println!("{}", result);
    }
}

/// Write command result to markdown file
fn write_command_result_markdown(
    path: &PathBuf,
    core: core::Core,
) -> Result<(), Box<dyn std::error::Error>> {
    // Use existing markdown writer through App wrapper
    let (keymap, _) = tui::load_keymap(None, None);
    let mut app = tui::App::new_with_core(core, keymap);
    storage::write_markdown(path, &mut app)?;
    Ok(())
}

fn print_usage() {
    eprintln!("Usage: gridline [OPTIONS] [FILE]");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  [FILE]                    Spreadsheet file to open (.grd)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -c, --command <FORMULA>   Evaluate formula and print result");
    eprintln!("  -f, --functions <FILE>    Load custom Rhai functions (can be repeated)");
    eprintln!("  -o, --output <FILE>       Export to markdown file (non-interactive)");
    eprintln!("  --keymap <name>           Select keybindings (default: vim)");
    eprintln!("  --keymap-file <path>      Load keybindings from TOML file");
    eprintln!("  -h, --help                Print help");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut file_path: Option<PathBuf> = None;
    let mut functions_files: Vec<PathBuf> = Vec::new();
    let mut output_file: Option<PathBuf> = None;
    let mut keymap_name: Option<String> = None;
    let mut keymap_file: Option<PathBuf> = None;
    let mut command_formula: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-c" | "--command" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --command requires a formula string");
                    std::process::exit(1);
                }
                command_formula = Some(args[i].to_string());
            }
            "-f" | "--functions" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --functions requires a file path");
                    std::process::exit(1);
                }
                functions_files.push(PathBuf::from(&args[i]));
            }
            "-o" | "--output" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --output requires a file path");
                    std::process::exit(1);
                }
                output_file = Some(PathBuf::from(&args[i]));
            }
            "--keymap" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --keymap requires a value");
                    std::process::exit(1);
                }
                keymap_name = Some(args[i].to_string());
            }
            "--keymap-file" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --keymap-file requires a file path");
                    std::process::exit(1);
                }
                keymap_file = Some(PathBuf::from(&args[i]));
            }
            arg if arg.starts_with('-') => {
                eprintln!("Error: Unknown option: {}", arg);
                print_usage();
                std::process::exit(1);
            }
            _ => {
                if file_path.is_none() {
                    file_path = Some(PathBuf::from(&args[i]));
                } else {
                    eprintln!("Error: Unexpected argument: {}", args[i]);
                    print_usage();
                    std::process::exit(1);
                }
            }
        }
        i += 1;
    }

    // Command mode: evaluate formula and exit
    if let Some(formula) = command_formula {
        if let Err(e) = run_command_mode(formula, functions_files, output_file) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    let (keymap, warnings) = tui::load_keymap(keymap_name.as_deref(), keymap_file.as_ref());
    for warning in warnings {
        eprintln!("Warning: {}", warning);
    }

    let mut app = match tui::App::with_file(file_path, functions_files, keymap) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if let Some(output_path) = output_file {
        if let Err(e) = storage::write_markdown(&output_path, &mut app) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        println!("Exported to {}", output_path.display());
    } else {
        if let Err(e) = tui::run(&mut app) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
