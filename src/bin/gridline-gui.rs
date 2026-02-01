//! gridline-gui - desktop GUI (scaffold).
//!
//! This is intentionally minimal on branch 0.1.9.
//! The real GUI implementation lands on branch 0.2.0.

use eframe::egui;
use gridline_core::Document;

struct GridlineGui {
    #[allow(dead_code)]
    doc: Document,
}

impl Default for GridlineGui {
    fn default() -> Self {
        Self {
            doc: Document::new(),
        }
    }
}

impl eframe::App for GridlineGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Gridline GUI");
            ui.label("Scaffold build: GUI implementation starts on branch 0.2.0");
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Gridline",
        options,
        Box::new(|_cc| Ok(Box::new(GridlineGui::default()))),
    )
}
