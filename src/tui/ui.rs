//! UI rendering

use super::app::{App, Mode};
use super::help::{get_commands_help, get_help_text};
use super::keymap::Keymap;
use gridline_engine::engine::CellRef;
use gridline_engine::plot::{PLOT_PREFIX, PlotData, PlotKind, PlotSpec, parse_plot_spec};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
};
use textplots::{AxisBuilder, Chart, LabelBuilder, LabelFormat, LineStyle, Plot, Shape};

/// Draw the application UI
pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Formula bar
            Constraint::Min(10),   // Grid
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    // Update visible dimensions based on actual size
    let grid_area = chunks[1];
    let row_header_width = 4; // "999 " width
    let available_width = grid_area.width.saturating_sub(row_header_width + 2) as usize;
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
            if let Some(cell) = app.grid.get(&cell_ref) {
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

            let cell_ref = CellRef::new(row, col);
            let display = app.get_cell_display(&cell_ref);
            let display = if display.starts_with(PLOT_PREFIX) {
                plot_placeholder(&display)
            } else {
                display
            };

            let is_cursor = row == app.cursor_row && col == app.cursor_col;
            let is_selected = if let Some(((r1, c1), (r2, c2))) = app.get_selection() {
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
    let mut widths = vec![Constraint::Length(4)]; // Row header
    for col in app.viewport_col..app.viewport_col + app.visible_cols {
        if col >= app.max_cols {
            break;
        }
        widths.push(Constraint::Length(app.get_column_width(col) as u16));
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Gridline "))
        .column_spacing(1);

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
                parts.push(render_textplots(&data, plot_width_points, plot_height_points));
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
fn cell_value_for_plot(app: &mut App, row: usize, col: usize) -> Option<f64> {
    let s = app.get_cell_display(&CellRef::new(row, col));
    s.parse::<f64>().ok()
}

/// Prepare plot data from a spec using the app's cell accessor.
fn prepare_plot_data(app: &mut App, spec: &PlotSpec) -> Result<PlotData, String> {
    PlotData::from_spec(spec, |r, c| cell_value_for_plot(app, r, c))
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
    let file_info = if let Some(ref path) = app.file_path {
        let modified_indicator = if app.modified { " [+]" } else { "" };
        format!("{}{}", path.display(), modified_indicator)
    } else if app.modified {
        "[New File] [+]".to_string()
    } else {
        "[New File]".to_string()
    };

    let help = match app.keymap {
        Keymap::Vim => {
            "hjkl:move  i:edit  v:visual  y:yank  p:paste  P:plot  +/-:colwidth  G:last  :w:save  :q:quit"
        }
        Keymap::Emacs => {
            "C-n/p/f/b:move  Enter:edit  M-x:cmd  C-s:save  M-w:copy  C-y:paste  C-SPC:mark  C-g:cancel  M-p:plot"
        }
    };

    let status = if !app.status_message.is_empty() {
        app.status_message.clone()
    } else {
        format!("{}  |  {}", file_info, help)
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
    let area = centered_rect(70, 80, f.area());

    let modal_style = Style::default().fg(Color::White).bg(Color::Black);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (Esc to close) ")
        .border_style(Style::default().fg(Color::Green))
        .style(modal_style);

    // Combine keybindings and commands help
    let mut lines: Vec<Line> = Vec::new();

    for text in get_help_text(app.keymap) {
        let style = if text.starts_with("===") {
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

    for text in get_commands_help() {
        let style = if text.starts_with("===") {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if text.starts_with("  ") {
            Style::default().fg(Color::White)
        } else if text.starts_with("Press") {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    let paragraph = Paragraph::new(lines).block(block).style(modal_style);

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}
