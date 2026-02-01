//! Gridline GUI - Desktop application entry point.

use eframe::egui;
use gridline_core::Document;
use std::path::PathBuf;

#[path = "../default_functions.rs"]
mod default_functions;

#[path = "../gui/mod.rs"]
mod gui;

fn main() -> eframe::Result<()> {
    let mut path: Option<PathBuf> = None;
    let mut functions: Vec<PathBuf> = Vec::new();
    let mut no_default_functions: bool = false;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-f" | "--functions" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --functions requires a file path");
                    std::process::exit(1);
                }
                functions.push(PathBuf::from(&args[i]));
            }
            "--no-default-functions" => {
                no_default_functions = true;
            }
            "-h" | "--help" => {
                println!("Usage: gridline-gui [OPTIONS] [FILE]");
                println!();
                println!("Options:");
                println!("  -f, --functions <FILE>    Load custom Rhai functions (repeatable)");
                println!(
                    "  --no-default-functions    Do not auto-load default.rhai from config dir"
                );
                println!("  -h, --help                Print help");
                return Ok(());
            }
            arg if arg.starts_with('-') => {
                eprintln!("Error: unknown option: {}", arg);
                std::process::exit(1);
            }
            _ => {
                if path.is_none() {
                    path = Some(PathBuf::from(&args[i]));
                } else {
                    eprintln!("Error: unexpected argument: {}", args[i]);
                    std::process::exit(1);
                }
            }
        }
        i += 1;
    }

    // Autoload default functions first, then user-specified functions.
    default_functions::prepend_default_functions_if_present(&mut functions, no_default_functions);

    let doc = Document::with_file(path, functions).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });

    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_fullscreen(true);

    eframe::run_native(
        "Gridline",
        options,
        Box::new(|_cc| Ok(Box::new(gui::GridlineGuiApp::new(doc)))),
    )
}
