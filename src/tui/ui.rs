//! UI rendering

use super::app::{App, Mode};
use super::help::{get_about_help, get_commands_help, get_functions_help, get_help_text};
use gridline_engine::engine::CellRef;
use gridline_engine::plot::{PLOT_PREFIX, PlotData, PlotKind, PlotSpec, parse_plot_spec};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};
use textplots::{AxisBuilder, Chart, LabelBuilder, LabelFormat, LineStyle, Plot, Shape};

pub(crate) const FORMULA_BAR_HEIGHT: u16 = 3;
pub(crate) const GRID_MIN_HEIGHT: u16 = 10;
pub(crate) const STATUS_BAR_HEIGHT: u16 = 1;
pub(crate) const ROW_HEADER_WIDTH: u16 = 4;
pub(crate) const GRID_COLUMN_SPACING: u16 = 1;

pub(crate) fn split_main_chunks(area: Rect) -> [Rect; 3] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(FORMULA_BAR_HEIGHT),
            Constraint::Min(GRID_MIN_HEIGHT),
            Constraint::Length(STATUS_BAR_HEIGHT),
        ])
        .split(area);
    [chunks[0], chunks[1], chunks[2]]
}

pub(crate) fn grid_cell_at(
    app: &App,
    grid_area: Rect,
    mouse_col: u16,
    mouse_row: u16,
) -> Option<(usize, usize)> {
    if grid_area.width < 3 || grid_area.height < 4 {
        return None;
    }

    let right = grid_area.x.saturating_add(grid_area.width);
    let bottom = grid_area.y.saturating_add(grid_area.height);
    if mouse_col < grid_area.x
        || mouse_col >= right
        || mouse_row < grid_area.y
        || mouse_row >= bottom
    {
        return None;
    }

    let inner_x = grid_area.x.saturating_add(1);
    let inner_y = grid_area.y.saturating_add(1);
    let inner_width = grid_area.width.saturating_sub(2);
    let inner_height = grid_area.height.saturating_sub(2);
    let inner_right = inner_x.saturating_add(inner_width);
    let inner_bottom = inner_y.saturating_add(inner_height);

    if mouse_col < inner_x
        || mouse_col >= inner_right
        || mouse_row < inner_y
        || mouse_row >= inner_bottom
    {
        return None;
    }

    // Header row contains column letters, not data cells.
    if inner_height <= 1 || mouse_row == inner_y {
        return None;
    }

    let rel_row = mouse_row.saturating_sub(inner_y.saturating_add(1)) as usize;
    if rel_row >= app.visible_rows {
        return None;
    }
    let row = app.viewport_row.saturating_add(rel_row);
    if row >= app.max_rows {
        return None;
    }

    let row_header_end = inner_x.saturating_add(ROW_HEADER_WIDTH);
    if mouse_col < row_header_end {
        return None;
    }

    let mut x = row_header_end;
    let first_spacing_end = x.saturating_add(GRID_COLUMN_SPACING);
    if mouse_col >= x && mouse_col < first_spacing_end {
        return None;
    }
    x = first_spacing_end;

    for offset in 0..app.visible_cols {
        let col = app.viewport_col + offset;
        if col >= app.max_cols {
            break;
        }

        let width = app.get_column_width(col) as u16;
        let cell_end = x.saturating_add(width);
        if mouse_col >= x && mouse_col < cell_end && mouse_col < inner_right {
            return Some((col, row));
        }

        x = cell_end;
        let spacing_end = x.saturating_add(GRID_COLUMN_SPACING);
        if mouse_col >= x && mouse_col < spacing_end {
            return None;
        }
        x = spacing_end;

        if x >= inner_right {
            break;
        }
    }

    None
}

/// Draw the application UI
pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = split_main_chunks(f.area());

    // Update visible dimensions based on actual size
    let grid_area = chunks[1];
    let available_width = grid_area.width.saturating_sub(ROW_HEADER_WIDTH + 2) as usize;
    let available_height = grid_area.height.saturating_sub(3) as usize; // header + borders

    app.visible_cols = (available_width / (app.col_width + 1)).max(1);
    app.visible_rows = available_height.max(1);
    app.update_viewport();

    draw_formula_bar(f, app, chunks[0]);
    draw_grid(f, app, chunks[1]);
    draw_status_bar(f, app, chunks[2]);

    if let Some(spec) = app.plot_modal.clone() {
        draw_plot_modal(f, app, &spec);
    }

    if app.help_modal {
        draw_help_modal(f, app);
    }
}

fn draw_formula_bar(f: &mut Frame, app: &App, area: Rect) {
    let cell_ref = app.current_cell_ref();
    let cell_name = format!("{}", cell_ref);

    let content = match app.mode {
        Mode::Edit => {
            // Insert cursor marker at cursor position
            let (before, after) = app.edit_buffer.split_at(app.edit_cursor);
            format!("{}: {}│{}", cell_name, before, after)
        }
        Mode::Command => {
            let (before, after) = app.command_buffer.split_at(app.command_cursor);
            format!(":{}│{}", before, after)
        }
        Mode::Visual => {
            if let Some(range) = app.get_selection_range_string() {
                format!("{} ({})", cell_name, range)
            } else {
                cell_name
            }
        }
        Mode::Normal => {
            if let Some(cell) = app.core.grid.get(&cell_ref) {
                format!("{}: {}", cell_name, cell.to_input_string())
            } else {
                format!("{}: (empty)", cell_name)
            }
        }
    };

    let title = match app.mode {
        Mode::Edit => " Edit ",
        Mode::Command => " Command ",
        Mode::Visual => " Visual ",
        Mode::Normal => " Cell ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(match app.mode {
            Mode::Edit => Color::Yellow,
            Mode::Command => Color::Cyan,
            Mode::Visual => Color::Magenta,
            Mode::Normal => Color::White,
        }));

    let paragraph = Paragraph::new(content).block(block);
    f.render_widget(paragraph, area);
}

fn draw_grid(f: &mut Frame, app: &mut App, area: Rect) {
    // Build header row
    let mut header_cells = vec![Cell::from(" ")]; // Corner
    for col in app.viewport_col..app.viewport_col + app.visible_cols {
        if col >= app.max_cols {
            break;
        }
        let col_name = CellRef::col_to_letters(col);
        let style = if col == app.cursor_col {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        header_cells.push(Cell::from(col_name).style(style));
    }
    let header = Row::new(header_cells).height(1);

    // Build data rows
    let mut rows = Vec::new();
    for row in app.viewport_row..app.viewport_row + app.visible_rows {
        if row >= app.max_rows {
            break;
        }

        let mut cells = Vec::new();

        // Row header
        let row_style = if row == app.cursor_row {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        cells.push(Cell::from(format!("{}", row + 1)).style(row_style));

        // Data cells
        for col in app.viewport_col..app.viewport_col + app.visible_cols {
            if col >= app.max_cols {
                break;
            }

            let cell_ref = CellRef::new(col, row);
            let display = app.core.get_cell_display(&cell_ref);
            let display = if display.starts_with(PLOT_PREFIX) {
                plot_placeholder(&display)
            } else {
                display
            };

            let is_cursor = row == app.cursor_row && col == app.cursor_col;
            let is_selected = if let Some(((c1, r1), (c2, r2))) = app.get_selection() {
                row >= r1 && row <= r2 && col >= c1 && col <= c2
            } else {
                false
            };

            let style = if is_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::White).bg(Color::Blue)
            } else if display.starts_with('#') {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };

            cells.push(Cell::from(display).style(style));
        }

        rows.push(Row::new(cells));
    }

    // Build column widths dynamically based on per-column settings
    let mut widths = vec![Constraint::Length(ROW_HEADER_WIDTH)]; // Row header
    for col in app.viewport_col..app.viewport_col + app.visible_cols {
        if col >= app.max_cols {
            break;
        }
        widths.push(Constraint::Length(app.get_column_width(col) as u16));
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Gridline "))
        .column_spacing(GRID_COLUMN_SPACING);

    f.render_widget(table, area);
}

fn plot_placeholder(s: &str) -> String {
    let Some(spec) = parse_plot_spec(s) else {
        return "<PLOT>".to_string();
    };
    let tag = match spec.kind {
        PlotKind::Bar => "BAR",
        PlotKind::Line => "LINE",
        PlotKind::Scatter => "SCAT",
    };
    format!("<{}>", tag)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_plot_modal(f: &mut Frame, app: &mut App, spec: &PlotSpec) {
    let area = centered_rect(80, 70, f.area());
    let inner_width = area.width.saturating_sub(2);
    let inner_height = area.height.saturating_sub(2);

    let base_title = match spec.kind {
        PlotKind::Bar => "Plot: BAR",
        PlotKind::Line => "Plot: LINE",
        PlotKind::Scatter => "Plot: SCATTER",
    };
    let title = if let Some(t) = spec.title.as_deref()
        && !t.is_empty()
    {
        format!(" {} - {} ", base_title, t)
    } else {
        format!(" {} ", base_title)
    };

    let modal_style = Style::default().fg(Color::White).bg(Color::Black);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan))
        .style(modal_style);

    // textplots uses a Braille canvas where one terminal character is 2x4 points.
    // To fill the modal, scale character dimensions to point dimensions.
    let plot_width_points = (inner_width as u32).saturating_mul(2);

    let labels_line = if spec.x_label.is_some() || spec.y_label.is_some() {
        let x = spec.x_label.as_deref().unwrap_or("");
        let y = spec.y_label.as_deref().unwrap_or("");
        Some(format!("X: {}    Y: {}", x, y))
    } else {
        None
    };

    // Reserve space for labels and warnings
    let has_labels = labels_line.is_some();
    let plot_height_chars = inner_height.saturating_sub(if has_labels { 2 } else { 1 });
    let plot_height_points = (plot_height_chars as u32).saturating_mul(4);

    let content = if plot_width_points < 32 || plot_height_points < 3 {
        "Terminal too small for plot".to_string()
    } else {
        // Prepare plot data using PlotData
        match prepare_plot_data(app, spec) {
            Ok(data) => {
                let mut parts = Vec::new();
                if let Some(labels) = &labels_line {
                    parts.push(labels.clone());
                }
                parts.push(render_textplots(
                    &data,
                    plot_width_points,
                    plot_height_points,
                ));
                if !data.warnings.is_empty() {
                    parts.push(format!("Warning: {}", data.warnings.join("; ")));
                }
                parts.join("\n")
            }
            Err(e) => e,
        }
    };

    let paragraph = Paragraph::new(content).block(block).style(modal_style);

    // Clear area behind modal so plot whitespace is visible.
    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

/// Parse a cell as a numeric value for plotting.
/// Returns `Some(value)` if the cell contains a valid number, `None` otherwise.
fn cell_value_for_plot(app: &mut App, col: usize, row: usize) -> Option<f64> {
    let s = app.core.get_cell_display(&CellRef::new(col, row));
    s.parse::<f64>().ok()
}

/// Prepare plot data from a spec using the app's cell accessor.
fn prepare_plot_data(app: &mut App, spec: &PlotSpec) -> Result<PlotData, String> {
    PlotData::from_spec(spec, |c, r| cell_value_for_plot(app, c, r))
}

/// Render plot data to a string using textplots.
///
/// This function isolates the textplots dependency, making it easy to swap
/// for a different renderer (e.g., a GUI library).
fn render_textplots(data: &PlotData, width: u32, height: u32) -> String {
    let (xmin, xmax) = data.x_range;
    let (ymin, ymax) = data.y_range;
    let span_x = xmax - xmin;
    let span_y = ymax - ymin;

    // Shift points so minimums map to 0 (textplots draws axes at x=0, y=0)
    let shifted_points: Vec<(f32, f32)> = data
        .points
        .iter()
        .map(|(x, y)| (x - xmin, y - ymin))
        .collect();

    let mut chart = Chart::new_with_y_range(width, height, 0.0, span_x, 0.0, span_y);

    let shape = match data.spec.kind {
        PlotKind::Bar => Shape::Bars(&shifted_points),
        PlotKind::Line => Shape::Lines(&shifted_points),
        PlotKind::Scatter => Shape::Points(&shifted_points),
    };

    let chart = chart
        .x_label_format(LabelFormat::Custom(Box::new(move |v| {
            format!("{:.1}", v + xmin)
        })))
        .y_label_format(LabelFormat::Custom(Box::new(move |v| {
            format!("{:.1}", v + ymin)
        })))
        .x_axis_style(LineStyle::Solid)
        .y_axis_style(LineStyle::Solid)
        .lineplot(&shape);
    chart.borders();
    chart.axis();
    chart.figures();
    chart.frame()
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let file_info = if let Some(ref path) = app.core.file_path {
        let modified_indicator = if app.core.modified { " [+]" } else { "" };
        format!("{}{}", path.display(), modified_indicator)
    } else if app.core.modified {
        "[New File] [+]".to_string()
    } else {
        "[New File]".to_string()
    };

    let help = app.keymap.status_hint();

    let status = if !app.status_message.is_empty() {
        app.status_message.clone()
    } else {
        format!("{}  |  [{}]  |  {}", file_info, app.keymap.name(), help)
    };

    let style = if app.status_message.starts_with("Error") {
        Style::default().fg(Color::Red)
    } else if !app.status_message.is_empty() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let spans = vec![Span::styled(status, style)];
    let paragraph = Paragraph::new(Line::from(spans));
    f.render_widget(paragraph, area);
}

fn draw_help_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(88, 88, f.area());

    let modal_style = Style::default().fg(Color::White).bg(Color::Black);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" About Gridline ")
        .border_style(Style::default().fg(Color::Green))
        .style(modal_style);

    // Combine about, keybindings, and commands help
    let mut lines: Vec<Line> = Vec::new();

    for text in get_about_help() {
        let style = if text == "About Gridline" {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if text.starts_with("  ") {
            Style::default().fg(Color::White)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    // Add separator
    lines.push(Line::from(""));

    for text in get_help_text(&app.keymap) {
        let style = if text.starts_with("  ") {
            Style::default().fg(Color::White)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    // Add separator
    lines.push(Line::from(""));

    for text in get_commands_help() {
        let style = if text == "Commands" {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if text.starts_with("  ") {
            Style::default().fg(Color::White)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    // Add separator
    lines.push(Line::from(""));

    for text in get_functions_help() {
        let style = if text == "Built-in Functions" {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if text.starts_with("  ") {
            Style::default().fg(Color::White)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    let viewport_height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(viewport_height);
    let effective_scroll = app.help_scroll.min(max_scroll);
    let scroll_y = u16::try_from(effective_scroll).unwrap_or(u16::MAX);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(modal_style)
        .scroll((scroll_y, 0))
        .wrap(Wrap { trim: false });

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_body_cell_point(grid_area: Rect) -> (u16, u16) {
        (
            grid_area.x + 1 + ROW_HEADER_WIDTH + GRID_COLUMN_SPACING,
            grid_area.y + 2,
        )
    }

    #[test]
    fn grid_cell_at_maps_first_visible_cell_to_viewport_origin() {
        let mut app = App::new();
        app.viewport_col = 5;
        app.viewport_row = 7;
        app.visible_cols = 4;
        app.visible_rows = 4;

        let grid_area = Rect::new(0, 0, 80, 20);
        let (x, y) = first_body_cell_point(grid_area);

        assert_eq!(grid_cell_at(&app, grid_area, x, y), Some((5, 7)));
    }

    #[test]
    fn grid_cell_at_maps_second_column_with_custom_width_and_spacing() {
        let mut app = App::new();
        app.viewport_col = 3;
        app.viewport_row = 2;
        app.visible_cols = 3;
        app.visible_rows = 3;
        app.column_widths.insert(3, 10);
        app.column_widths.insert(4, 8);

        let grid_area = Rect::new(0, 0, 80, 20);
        let (first_x, y) = first_body_cell_point(grid_area);
        let second_col_start = first_x + app.get_column_width(3) as u16 + GRID_COLUMN_SPACING;

        assert_eq!(
            grid_cell_at(&app, grid_area, second_col_start + 1, y),
            Some((4, 2))
        );
    }

    #[test]
    fn grid_cell_at_ignores_row_headers() {
        let mut app = App::new();
        app.visible_cols = 4;
        app.visible_rows = 4;

        let grid_area = Rect::new(0, 0, 80, 20);
        assert_eq!(
            grid_cell_at(&app, grid_area, grid_area.x + 2, grid_area.y + 2),
            None
        );
    }

    #[test]
    fn grid_cell_at_ignores_column_headers() {
        let mut app = App::new();
        app.visible_cols = 4;
        app.visible_rows = 4;

        let grid_area = Rect::new(0, 0, 80, 20);
        let (x, _y) = first_body_cell_point(grid_area);

        assert_eq!(grid_cell_at(&app, grid_area, x, grid_area.y + 1), None);
    }

    #[test]
    fn grid_cell_at_ignores_outside_grid_and_spacing() {
        let mut app = App::new();
        app.visible_cols = 4;
        app.visible_rows = 4;

        let grid_area = Rect::new(0, 0, 80, 20);
        let (first_x, y) = first_body_cell_point(grid_area);
        let spacing_x = first_x + app.get_column_width(app.viewport_col) as u16;

        assert_eq!(
            grid_cell_at(&app, grid_area, grid_area.x, grid_area.y),
            None
        );
        assert_eq!(grid_cell_at(&app, grid_area, spacing_x, y), None);
    }

    #[test]
    fn grid_cell_at_respects_max_bounds() {
        let mut app = App::new();
        app.max_cols = 1;
        app.max_rows = 1;
        app.visible_cols = 3;
        app.visible_rows = 3;

        let grid_area = Rect::new(0, 0, 80, 20);
        let (first_x, first_y) = first_body_cell_point(grid_area);
        let second_col_x = first_x + app.get_column_width(0) as u16 + GRID_COLUMN_SPACING + 1;
        let second_row_y = first_y + 1;

        assert_eq!(
            grid_cell_at(&app, grid_area, first_x, first_y),
            Some((0, 0))
        );
        assert_eq!(grid_cell_at(&app, grid_area, second_col_x, first_y), None);
        assert_eq!(grid_cell_at(&app, grid_area, first_x, second_row_y), None);
    }
}
