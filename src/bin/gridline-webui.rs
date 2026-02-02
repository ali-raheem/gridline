//! gridline-webui - placeholder.
//!
//! This is intentionally a stub on branch 0.1.9.

fn main() {
    // Keep CLI surface consistent with other frontends.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("Usage: gridline-webui [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --no-default-functions    Do not auto-load default.rhai from config dir");
        println!("  -h, --help                Print help");
        return;
    }

    eprintln!("gridline-webui is not implemented yet");
    eprintln!("Planned directions: Tauri IPC or a server + web frontend");
    std::process::exit(2);
}
