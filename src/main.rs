//! Gridline - A spreadsheet application with TUI

mod error;
mod storage;
mod tui;

use std::env;
use std::path::PathBuf;

fn print_usage() {
    eprintln!("Usage: gridline [OPTIONS] [FILE]");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  [FILE]                    Spreadsheet file to open (.grd)");
    eprintln!();
    eprintln!("Options:");
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

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
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
