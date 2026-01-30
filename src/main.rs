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
    eprintln!("  -f, --functions <FILE>    Load custom Rhai functions from file");
    eprintln!("  --keymap <vim|emacs>      Select keybindings (default: vim)");
    eprintln!("  -h, --help                Print help");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut file_path: Option<PathBuf> = None;
    let mut functions_file: Option<PathBuf> = None;
    let mut keymap: tui::Keymap = tui::Keymap::Vim;

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
                functions_file = Some(PathBuf::from(&args[i]));
            }
            "--keymap" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --keymap requires a value (vim|emacs)");
                    std::process::exit(1);
                }
                let Some(k) = tui::Keymap::parse(&args[i]) else {
                    eprintln!("Error: Invalid keymap: {} (expected vim|emacs)", args[i]);
                    std::process::exit(1);
                };
                keymap = k;
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

    let mut app = match tui::App::with_file(file_path, functions_file, keymap) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = tui::run(&mut app) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
