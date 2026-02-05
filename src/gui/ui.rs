//! UI rendering and layout using egui.

use crate::gui::app::GuiApp;
use crate::gui::state::GuiState;
use eframe::egui;
use gridline_core::CellRef;

/// Cell rendering dimensions.
pub struct CellRenderer {
    pub cell_width: f32,
    pub cell_height: f32,
    pub row_header_width: f32,
}

impl CellRenderer {
    pub fn new() -> Self {
        Self {
            cell_width: 110.0,
            cell_height: 22.0,
            row_header_width: 44.0,
        }
    }
}

/// Draw status bar with keyboard shortcuts and info (Excel/Word style for GUI).
pub fn draw_status_bar(ui: &mut egui::Ui, app: &GuiApp, state: &GuiState) {
    // Show any status message in the status bar instead of inline
    let status = if !app.status.is_empty() {
        app.status.clone()
    } else {
        // Default shortcuts help (Excel/Word style)
        let editing = state.editing;
        if editing {
            "↵ Commit  |  Esc Cancel".to_string()
        } else {
            "↵ Edit  |  Ctrl+S Save  |  Ctrl+Z Undo  |  Ctrl+Y Redo  |  Ctrl+C Copy  |  Ctrl+X Cut  |  Ctrl+V Paste  |  Del Clear".to_string()
        }
    };

    ui.label(
        egui::RichText::new(status)
            .monospace()
            .size(11.0)
            .color(egui::Color32::from_rgb(150, 150, 150)),
    );
}

/// Apply dark theme and style configuration (TUI-inspired).
pub fn apply_theme(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());

    ctx.style_mut(|style| {
        // Spacing: minimal for compact spreadsheet feel
        style.spacing.item_spacing = egui::vec2(0.0, 0.0);
        style.spacing.button_padding = egui::vec2(4.0, 2.0);
        style.spacing.interact_size.y = 22.0;

        // Text styles
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(14.0));

        // Text colors for high contrast
        let fg = egui::Color32::from_gray(235);
        style.visuals.widgets.noninteractive.fg_stroke.color = fg;
        style.visuals.widgets.inactive.fg_stroke.color = fg;
        style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.open.fg_stroke.color = fg;

        // Better contrast for selected items
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(80, 130, 180);
        style.visuals.selection.stroke.color = egui::Color32::from_rgb(100, 150, 200);
        style.visuals.selection.stroke.width = 1.0;
    });
}

/// Draw the top panel with formula bar (TUI-inspired minimalist design).
pub fn draw_top_panel(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    state: &mut GuiState,
) {
    let formula_id = egui::Id::new("gridline_formula_edit");

    // Single line: cell reference + formula bar
    ui.horizontal(|ui| {
        // Cell reference label (compact)
        let cell_ref = app.selection_label();
        ui.label(egui::RichText::new(cell_ref).monospace().size(13.0));
        ui.separator();

        // Formula/value input - only show TextEdit when editing to avoid consuming keyboard shortcuts
        if state.editing {
            if state.request_focus_formula {
                ctx.memory_mut(|m| m.request_focus(formula_id));
                state.request_focus_formula = false;
            }

            let resp = ui.add(
                egui::TextEdit::singleline(&mut app.edit_buffer)
                    .id(formula_id)
                    .hint_text("numbers, \"text\", or =formula")
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            if resp.changed() {
                app.edit_dirty = true;
            }

            // Handle Enter/Escape in formula bar (only process if editing)
            let pressed_enter =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let pressed_escape =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));

            // Commit edit on Enter
            if pressed_enter {
                let input = app.edit_buffer.clone();
                if app.set_cell_from_input(&input).is_ok() {
                    state.editing = false;
                    state.request_focus_formula = false;
                    app.move_selection(0, 1, false);
                    state.ensure_selected_visible(&app.selected);
                } else {
                    state.request_focus_formula = true;
                }
            }

            // Cancel edit on Escape
            if pressed_escape {
                app.sync_edit_buffer();
                state.editing = false;
                state.request_focus_formula = false;
            }

            // Commit on focus loss (if was editing)
            if resp.lost_focus() {
                let committed = if app.edit_dirty {
                    let input = app.edit_buffer.clone();
                    app.set_cell_from_input(&input).is_ok()
                } else {
                    true
                };
                if committed {
                    state.editing = false;
                    state.request_focus_formula = false;
                } else {
                    state.request_focus_formula = true;
                }
            }
        } else {
            // When not editing, show read-only label instead
            ui.label(egui::RichText::new(&app.edit_buffer).monospace());
        }

        // File status indicator (minimal)
        if app.doc.modified {
            ui.label(egui::RichText::new("●").color(egui::Color32::from_rgb(255, 165, 0)));
        }
    });
}

/// Draw the central grid panel with spreadsheet.
pub fn draw_central_grid(
    ui: &mut egui::Ui,
    app: &mut GuiApp,
    state: &mut GuiState,
    renderer: &CellRenderer,
) {
    let row_header_w = renderer.row_header_width;
    let cell_w = renderer.cell_width;
    let cell_h = renderer.cell_height;

    // Calculate available space and update viewport dimensions
    let available_size = ui.available_size();
    let available_height = available_size.y - cell_h; // Account for header row
    let available_width = available_size.x - row_header_w; // Account for row labels

    let cols_fit = (available_width / (cell_w + 0.5)).floor() as usize;
    let rows_fit = (available_height / (cell_h + 0.5)).floor() as usize;

    state.viewport_cols = cols_fit.max(1);
    state.viewport_rows = rows_fit.max(1);

    // Ensure selected cell is visible with updated viewport size
    state.ensure_selected_visible(&app.selected);

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.5, 0.5);

            egui::Grid::new("sheet_grid")
                .spacing(egui::vec2(0.5, 0.5))
                .striped(true)
                .show(ui, |ui| {
                    // Column headers
                    ui.add_sized(
                        [row_header_w, cell_h],
                        egui::Label::new(egui::RichText::new("").monospace()),
                    );
                    for c in 0..state.viewport_cols {
                        let col = state.viewport_col + c;
                        let label = CellRef::col_to_letters(col);
                        ui.add_sized(
                            [cell_w, cell_h],
                            egui::Label::new(
                                egui::RichText::new(label)
                                    .strong()
                                    .monospace()
                                    .color(egui::Color32::from_rgb(180, 180, 180)),
                            ),
                        );
                    }
                    ui.end_row();

                    // Grid cells
                    for r in 0..state.viewport_rows {
                        let row = state.viewport_row + r;
                        ui.add_sized(
                            [row_header_w, cell_h],
                            egui::Label::new(
                                egui::RichText::new(format!("{}", row + 1))
                                    .monospace()
                                    .color(egui::Color32::from_rgb(180, 180, 180)),
                            ),
                        );

                        for c in 0..state.viewport_cols {
                            let col = state.viewport_col + c;
                            let cell_ref = CellRef::new(col, row);
                            let display = app.cell_display(&cell_ref);

                            let is_selected = app.selected == cell_ref;
                            let is_in_range = app.in_selection(&cell_ref);

                            // Format text with better visual feedback
                            let text = if is_selected {
                                egui::RichText::new(display)
                                    .monospace()
                                    .strong()
                                    .color(egui::Color32::WHITE)
                            } else if is_in_range {
                                egui::RichText::new(display)
                                    .monospace()
                                    .color(egui::Color32::from_rgb(230, 230, 230))
                            } else {
                                egui::RichText::new(display).monospace()
                            };

                            let resp = ui.add_sized(
                                [cell_w, cell_h],
                                egui::SelectableLabel::new(is_selected || is_in_range, text),
                            );
                            if resp.clicked() {
                                let extend = ui.input(|i| i.modifiers.shift);
                                app.set_selected(cell_ref, extend);
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}
