//! gridline-gui - desktop GUI (early implementation).

use eframe::egui;
use egui::{Event, Key, KeyboardShortcut, Modifiers};
use gridline_core::{CellRef, Document};
use std::path::PathBuf;

#[path = "../default_functions.rs"]
mod default_functions;

struct GridlineGui {
    doc: Document,

    viewport_row: usize,
    viewport_col: usize,
    viewport_rows: usize,
    viewport_cols: usize,

    selected: CellRef,
    selection_anchor: CellRef,
    selection_end: CellRef,
    edit_buffer: String,
    edit_dirty: bool,

    editing: bool,
    request_focus_formula: bool,

    status: String,
}

impl Default for GridlineGui {
    fn default() -> Self {
        let selected = CellRef::new(0, 0);
        Self {
            doc: Document::new(),
            viewport_row: 0,
            viewport_col: 0,
            viewport_rows: 30,
            viewport_cols: 12,
            selected: selected.clone(),
            selection_anchor: selected.clone(),
            selection_end: selected.clone(),
            edit_buffer: String::new(),
            edit_dirty: false,

            editing: false,
            request_focus_formula: false,

            status: String::new(),
        }
    }
}

impl GridlineGui {
    fn formula_id() -> egui::Id {
        egui::Id::new("gridline_formula_edit")
    }

    fn cell_input_string(&self, cell: &CellRef) -> String {
        self.doc
            .grid
            .get(cell)
            .map(|c| c.to_input_string())
            .unwrap_or_default()
    }

    fn selection_bounds(&self) -> (usize, usize, usize, usize) {
        let r1 = self.selection_anchor.row.min(self.selection_end.row);
        let c1 = self.selection_anchor.col.min(self.selection_end.col);
        let r2 = self.selection_anchor.row.max(self.selection_end.row);
        let c2 = self.selection_anchor.col.max(self.selection_end.col);
        (r1, c1, r2, c2)
    }

    fn selection_label(&self) -> String {
        let (r1, c1, r2, c2) = self.selection_bounds();
        if r1 == r2 && c1 == c2 {
            format!("{}", CellRef::new(r1, c1))
        } else {
            format!("{}:{}", CellRef::new(r1, c1), CellRef::new(r2, c2))
        }
    }

    fn in_selection(&self, cell: &CellRef) -> bool {
        let (r1, c1, r2, c2) = self.selection_bounds();
        cell.row >= r1 && cell.row <= r2 && cell.col >= c1 && cell.col <= c2
    }

    fn clear_selection(&mut self) {
        let (r1, c1, r2, c2) = self.selection_bounds();
        for r in r1..=r2 {
            for c in c1..=c2 {
                self.doc.clear_cell(&CellRef::new(r, c));
            }
        }
        self.sync_edit_buffer();
        self.status = format!("Cleared {}", self.selection_label());
    }

    fn copy_selection_to_clipboard(&mut self, ctx: &egui::Context) {
        let (r1, c1, r2, c2) = self.selection_bounds();
        let mut out = String::new();
        for r in r1..=r2 {
            if r != r1 {
                out.push('\n');
            }
            for c in c1..=c2 {
                if c != c1 {
                    out.push('\t');
                }
                out.push_str(&self.cell_input_string(&CellRef::new(r, c)));
            }
        }
        // Update system clipboard immediately (so a follow-up paste reads the new value).
        let ok = Self::clipboard_set_text(&out);

        // Also set egui's output clipboard (useful for web builds later).
        ctx.copy_text(out);

        if ok {
            self.status = format!("Copied {}", self.selection_label());
        } else {
            self.status = format!("Copied {} (clipboard unavailable)", self.selection_label());
        }
    }

    fn cut_selection_to_clipboard(&mut self, ctx: &egui::Context) {
        self.copy_selection_to_clipboard(ctx);
        self.clear_selection();
        self.status = format!("Cut {}", self.selection_label());
    }

    fn parse_clipboard_grid(s: &str) -> Vec<Vec<String>> {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        let mut lines: Vec<&str> = s.split('\n').collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            return Vec::new();
        }
        lines
            .iter()
            .map(|line| {
                if line.contains('\t') {
                    line.split('\t').map(|c| c.to_string()).collect()
                } else {
                    vec![line.to_string()]
                }
            })
            .collect()
    }

    fn paste_into_selection(&mut self, s: String) {
        let grid = Self::parse_clipboard_grid(&s);
        if grid.is_empty() {
            self.status = "Paste failed: empty clipboard".to_string();
            return;
        }

        let (r1, c1, r2, c2) = self.selection_bounds();
        let sel_rows = r2 - r1 + 1;
        let sel_cols = c2 - c1 + 1;

        let single_value = grid.len() == 1 && grid[0].len() == 1;
        if single_value && (sel_rows > 1 || sel_cols > 1) {
            let v = grid[0][0].clone();
            for r in r1..=r2 {
                for c in c1..=c2 {
                    let _ = self.doc.set_cell_from_input(CellRef::new(r, c), &v);
                }
            }
            self.sync_edit_buffer();
            self.status = format!("Pasted into {}", self.selection_label());
            return;
        }

        for (dr, row) in grid.iter().enumerate() {
            for (dc, v) in row.iter().enumerate() {
                let r = r1 + dr;
                let c = c1 + dc;
                let _ = self.doc.set_cell_from_input(CellRef::new(r, c), v);
            }
        }
        self.sync_edit_buffer();
        self.status = format!("Pasted into {}", self.selection_label());
    }

    fn clipboard_get_text() -> Option<String> {
        let mut cb = arboard::Clipboard::new().ok()?;
        cb.get_text().ok()
    }

    fn clipboard_set_text(text: &str) -> bool {
        let mut cb = match arboard::Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return false,
        };
        cb.set_text(text.to_string()).is_ok()
    }

    fn consume_shortcut(ctx: &egui::Context, key: Key) -> bool {
        // `COMMAND` maps to Ctrl on Windows/Linux and Cmd on macOS.
        ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key)))
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if Self::consume_shortcut(ctx, Key::S) {
            match self.doc.save_file() {
                Ok(p) => self.status = format!("Saved {}", p.display()),
                Err(e) => self.status = format!("Save failed: {}", e),
            }
        }

        // If the formula bar is focused (editing), let egui handle clipboard shortcuts.
        let formula_focused = ctx.memory(|m| m.focused()) == Some(Self::formula_id());
        if self.editing || formula_focused {
            return;
        }

        // Delete clears selected cell(s).
        let pressed_delete =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
        let pressed_backspace =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace));
        if pressed_delete || pressed_backspace {
            self.clear_selection();
            return;
        }

        if Self::consume_shortcut(ctx, Key::C) {
            self.copy_selection_to_clipboard(ctx);
        }

        if Self::consume_shortcut(ctx, Key::X) {
            self.cut_selection_to_clipboard(ctx);
        }

        // Paste: support Ctrl+V and Ctrl+P.
        let mut handled_paste = false;
        if Self::consume_shortcut(ctx, Key::V) || Self::consume_shortcut(ctx, Key::P) {
            handled_paste = true;
            if let Some(s) = Self::clipboard_get_text() {
                self.paste_into_selection(s);
            } else {
                self.status = "Paste failed: clipboard unavailable".to_string();
            }
        }

        // If the platform delivered an explicit paste event and nothing is focused, honor it.
        // This covers environments where clipboard reads are restricted.
        let paste_event: Option<String> = ctx.input(|i| {
            i.events.iter().find_map(|ev| {
                if let Event::Paste(s) = ev {
                    Some(s.clone())
                } else {
                    None
                }
            })
        });
        if !handled_paste {
            if let Some(s) = paste_event {
                let focused = ctx.memory(|m| m.focused());
                if focused.is_none() {
                    self.paste_into_selection(s);
                }
            }
        }
    }

    fn new(doc: Document) -> Self {
        let mut s = Self {
            doc,
            ..Self::default()
        };
        s.sync_edit_buffer();
        s
    }

    fn sync_edit_buffer(&mut self) {
        if let Some(cell) = self.doc.grid.get(&self.selected) {
            self.edit_buffer = cell.to_input_string();
        } else {
            self.edit_buffer.clear();
        }
        self.edit_dirty = false;
    }

    fn ensure_selected_visible(&mut self) {
        let row = self.selected.row;
        let col = self.selected.col;

        if row < self.viewport_row {
            self.viewport_row = row;
        } else if row >= self.viewport_row + self.viewport_rows {
            self.viewport_row = row.saturating_sub(self.viewport_rows.saturating_sub(1));
        }

        if col < self.viewport_col {
            self.viewport_col = col;
        } else if col >= self.viewport_col + self.viewport_cols {
            self.viewport_col = col.saturating_sub(self.viewport_cols.saturating_sub(1));
        }
    }

    fn move_selection(&mut self, dx: isize, dy: isize, extend_selection: bool) {
        let r = self.selected.row as isize + dy;
        let c = self.selected.col as isize + dx;
        self.set_selected(
            CellRef::new(r.max(0) as usize, c.max(0) as usize),
            extend_selection,
        );
    }

    fn set_selected(&mut self, cell: CellRef, extend_selection: bool) {
        self.selected = cell;
        self.ensure_selected_visible();
        if extend_selection {
            self.selection_end = self.selected.clone();
        } else {
            self.selection_anchor = self.selected.clone();
            self.selection_end = self.selected.clone();
        }
        self.sync_edit_buffer();
    }

    fn begin_edit(&mut self) {
        self.editing = true;
        self.request_focus_formula = true;
    }

    fn end_edit(&mut self) {
        self.editing = false;
        self.request_focus_formula = false;
    }

    fn commit_edit(&mut self) {
        let input = self.edit_buffer.clone();
        match self.doc.set_cell_from_input(self.selected.clone(), &input) {
            Ok(()) => {
                self.status = format!("Updated {}", self.selected);
                self.edit_dirty = false;
            }
            Err(e) => {
                self.status = format!("Error: {}", e);
            }
        }
    }
}

impl eframe::App for GridlineGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Force a known-good theme so text/contrast is predictable.
        ctx.set_visuals(egui::Visuals::dark());

        // Make the sheet grid feel like a spreadsheet: tight spacing.
        ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(2.0, 2.0);
            style.spacing.interact_size.y = 22.0;

            // Make sure default text is comfortably readable.
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));

            // Harden text color against platform/theme oddities.
            let fg = egui::Color32::from_gray(230);
            style.visuals.widgets.noninteractive.fg_stroke.color = fg;
            style.visuals.widgets.inactive.fg_stroke.color = fg;
            style.visuals.widgets.hovered.fg_stroke.color = fg;
            style.visuals.widgets.active.fg_stroke.color = fg;
            style.visuals.widgets.open.fg_stroke.color = fg;
        });

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Gridline");
                ui.separator();
                if let Some(p) = &self.doc.file_path {
                    ui.label(p.display().to_string());
                } else {
                    ui.label("(unsaved)");
                }
                if self.doc.modified {
                    ui.label("*");
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label(format!("Selection: {}", self.selection_label()));

                let formula_id = Self::formula_id();
                if self.request_focus_formula {
                    ctx.memory_mut(|m| m.request_focus(formula_id));
                    self.request_focus_formula = false;
                }

                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.edit_buffer)
                        .id(formula_id)
                        .hint_text("Enter value (numbers), \"text\", or =formula")
                        .desired_width(f32::INFINITY),
                );
                if resp.changed() {
                    self.edit_dirty = true;
                }

                // Track editing state.
                if resp.has_focus() {
                    self.editing = true;
                }

                // Use consume_key so Enter/Escape work reliably while TextEdit is focused.
                let pressed_enter = resp.has_focus()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                let pressed_escape = resp.has_focus()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));

                // Commit on Enter while editing.
                if self.editing && pressed_enter {
                    self.commit_edit();
                    self.end_edit();
                    self.move_selection(0, 1, false);
                }

                // Revert on Escape while editing.
                if self.editing && pressed_escape {
                    self.sync_edit_buffer();
                    self.end_edit();
                }

                // If the user clicks away from the formula bar, commit edits (spreadsheet-like).
                if self.editing && resp.lost_focus() {
                    if self.edit_dirty {
                        self.commit_edit();
                    }
                    self.end_edit();
                }

                if ui
                    .add_enabled(self.edit_dirty, egui::Button::new("Apply"))
                    .clicked()
                {
                    self.commit_edit();
                    self.end_edit();
                }

                if ui.button("Revert").clicked() {
                    self.sync_edit_buffer();
                    self.end_edit();
                }
            });

            if !self.status.is_empty() {
                ui.label(self.status.clone());
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Spreadsheet-like keyboard navigation when not editing.
            if !self.editing {
                let input = ui.input(|i| i.clone());
                let mut dx: isize = 0;
                let mut dy: isize = 0;

                if input.key_pressed(egui::Key::ArrowLeft) {
                    dx -= 1;
                }
                if input.key_pressed(egui::Key::ArrowRight) {
                    dx += 1;
                }
                if input.key_pressed(egui::Key::ArrowUp) {
                    dy -= 1;
                }
                if input.key_pressed(egui::Key::ArrowDown) {
                    dy += 1;
                }

                if dx != 0 || dy != 0 {
                    self.move_selection(dx, dy, input.modifiers.shift);
                    ctx.memory_mut(|m| m.surrender_focus(Self::formula_id()));
                }

                if input.key_pressed(egui::Key::Enter) {
                    self.begin_edit();
                }

                if input.key_pressed(egui::Key::Escape) {
                    self.status.clear();
                }
            }

            ui.horizontal(|ui| {
                ui.label("Viewport");
                ui.add(egui::DragValue::new(&mut self.viewport_row).prefix("row "));
                ui.add(egui::DragValue::new(&mut self.viewport_col).prefix("col "));
                ui.add(
                    egui::DragValue::new(&mut self.viewport_rows)
                        .prefix("rows ")
                        .range(1..=200),
                );
                ui.add(
                    egui::DragValue::new(&mut self.viewport_cols)
                        .prefix("cols ")
                        .range(1..=100),
                );
            });

            ui.separator();

            let row_header_w = 44.0;
            let cell_w = 110.0;
            let cell_h = 22.0;

            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(1.0, 1.0);

                    egui::Grid::new("sheet_grid")
                        .spacing(egui::vec2(1.0, 1.0))
                        .striped(true)
                        .show(ui, |ui| {
                            // Column headers
                            ui.add_sized([row_header_w, cell_h], egui::Label::new(""));
                            for c in 0..self.viewport_cols {
                                let col = self.viewport_col + c;
                                let label = CellRef::col_to_letters(col);
                                ui.add_sized(
                                    [cell_w, cell_h],
                                    egui::Label::new(
                                        egui::RichText::new(label).strong().monospace(),
                                    ),
                                );
                            }
                            ui.end_row();

                            for r in 0..self.viewport_rows {
                                let row = self.viewport_row + r;
                                ui.add_sized(
                                    [row_header_w, cell_h],
                                    egui::Label::new(
                                        egui::RichText::new(format!("{}", row + 1))
                                            .strong()
                                            .monospace(),
                                    ),
                                );

                                for c in 0..self.viewport_cols {
                                    let col = self.viewport_col + c;
                                    let cell_ref = CellRef::new(row, col);
                                    let display = self.doc.get_cell_display(&cell_ref);

                                    let is_selected = self.selected == cell_ref;
                                    let is_in_range = self.in_selection(&cell_ref);
                                    let text = egui::RichText::new(display).monospace();
                                    let resp = ui.add_sized(
                                        [cell_w, cell_h],
                                        egui::SelectableLabel::new(
                                            is_selected || is_in_range,
                                            text,
                                        ),
                                    );
                                    if resp.clicked() {
                                        let extend = ui.input(|i| i.modifiers.shift);
                                        self.set_selected(cell_ref, extend);
                                        self.end_edit();
                                        ctx.memory_mut(|m| m.surrender_focus(Self::formula_id()));
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        });

        // Handle keyboard shortcuts after UI has processed clicks for this frame.
        self.handle_shortcuts(ctx);
    }
}

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

    let options = eframe::NativeOptions::default();
    // Put the renderer in the window title so we can debug backend issues.
    eframe::run_native(
        "Gridline (eframe/wgpu)",
        options,
        Box::new(|_cc| Ok(Box::new(GridlineGui::new(doc)))),
    )
}
