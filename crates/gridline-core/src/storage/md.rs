//! Markdown export functionality

use crate::document::Document;
use gridline_engine::engine::CellRef;
use gridline_engine::plot::{PLOT_PREFIX, PlotData, PlotKind, PlotSpec, parse_plot_spec};
use std::io::Write;
use std::path::Path;

/// Write the grid to a markdown file
pub fn write_markdown(path: &Path, doc: &mut Document) -> std::io::Result<()> {
    // Find grid bounds (from populated cells + spilled values)
    let (min_row, min_col, max_row, max_col) = find_grid_bounds(doc);

    if min_row > max_row {
        // Empty grid
        let mut file = std::fs::File::create(path)?;
        writeln!(file, "# Sheet")?;
        writeln!(file)?;
        writeln!(file, "*Empty spreadsheet*")?;
        return Ok(());
    }

    let mut file = std::fs::File::create(path)?;
    let mut plots: Vec<PlotSpec> = Vec::new();

    // Write header
    writeln!(file, "# Sheet")?;
    writeln!(file)?;

    // Write markdown table header with column letters
    write!(file, "|   |")?;
    for col in min_col..=max_col {
        write!(file, " {} |", CellRef::col_to_letters(col))?;
    }
    writeln!(file)?;

    // Write separator row
    write!(file, "|---|")?;
    for _ in min_col..=max_col {
        write!(file, "---|")?;
    }
    writeln!(file)?;

    // Write data rows
    for row in min_row..=max_row {
        write!(file, "| {} |", row + 1)?; // 1-based row numbers

        for col in min_col..=max_col {
            let cell_ref = CellRef::new(row, col);
            let display = doc.get_cell_display(&cell_ref);

            // Check if this is a plot cell
            if display.starts_with(PLOT_PREFIX) {
                if let Some(spec) = parse_plot_spec(&display) {
                    plots.push(spec);
                    write!(file, " [Chart] |")?;
                } else {
                    write!(file, " {} |", escape_markdown(&display))?;
                }
            } else {
                write!(file, " {} |", escape_markdown(&display))?;
            }
        }
        writeln!(file)?;
    }

    // Write plot sections
    for spec in plots {
        writeln!(file)?;
        let title = spec.title.as_deref().unwrap_or("Chart");
        writeln!(file, "## {}", title)?;
        writeln!(file)?;
        writeln!(file, "```")?;
        render_plot_ascii(&mut file, &spec, doc)?;
        writeln!(file, "```")?;
    }

    Ok(())
}

/// Find the bounds of the grid (min/max row/col)
fn find_grid_bounds(doc: &Document) -> (usize, usize, usize, usize) {
    let mut min_row = usize::MAX;
    let mut min_col = usize::MAX;
    let mut max_row = 0usize;
    let mut max_col = 0usize;

    // Check grid cells
    for entry in doc.grid.iter() {
        let cell_ref = entry.key();
        min_row = min_row.min(cell_ref.row);
        min_col = min_col.min(cell_ref.col);
        max_row = max_row.max(cell_ref.row);
        max_col = max_col.max(cell_ref.col);
    }

    // Check value_cache for additional cells (e.g., array spills)
    for entry in doc.value_cache.iter() {
        let cell_ref = entry.key();
        min_row = min_row.min(cell_ref.row);
        min_col = min_col.min(cell_ref.col);
        max_row = max_row.max(cell_ref.row);
        max_col = max_col.max(cell_ref.col);
    }

    (min_row, min_col, max_row, max_col)
}

/// Escape special markdown characters in cell content
fn escape_markdown(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ").replace('\r', "")
}

/// Render a plot as ASCII art
fn render_plot_ascii<W: Write>(
    w: &mut W,
    spec: &PlotSpec,
    doc: &mut Document,
) -> std::io::Result<()> {
    // Create cell value accessor
    let cell_value = |row: usize, col: usize| -> Option<f64> {
        let cell_ref = CellRef::new(row, col);
        let display = doc.get_cell_display(&cell_ref);
        display.parse::<f64>().ok()
    };

    match PlotData::from_spec(spec, cell_value) {
        Ok(data) => {
            match spec.kind {
                PlotKind::Bar => render_bar_chart(w, &data)?,
                PlotKind::Line => render_line_chart(w, &data)?,
                PlotKind::Scatter => render_scatter_chart(w, &data)?,
            }

            // Print warnings if any
            for warning in &data.warnings {
                writeln!(w, "Note: {}", warning)?;
            }
        }
        Err(e) => {
            writeln!(w, "Error rendering chart: {}", e)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_markdown;
    use crate::document::Document;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn markdown_export_matches_expected_simple() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let grid_path = repo_root.join("tests/fixtures/simple.grid");
        let expected_path = repo_root.join("tests/fixtures/simple.expected.md");
        let output_path = std::env::temp_dir().join(format!(
            "gridline_simple_export_{}_{}_{:?}.md",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(output_path.clone());

        let mut doc = Document::with_file(Some(grid_path.to_path_buf()), Vec::new()).unwrap();
        write_markdown(&output_path, &mut doc).unwrap();

        let actual = fs::read_to_string(&output_path).unwrap();
        let expected = fs::read_to_string(expected_path).unwrap();

        let normalize = |text: String| text.replace("\r\n", "\n");
        assert_eq!(normalize(actual), normalize(expected));
    }
}

/// Render a simple ASCII bar chart
fn render_bar_chart<W: Write>(w: &mut W, data: &PlotData) -> std::io::Result<()> {
    let max_y = data.y_range.1;
    let height = 10;
    let width = data.points.len().min(40);

    // Scale values to fit
    let scaled: Vec<usize> = data
        .points
        .iter()
        .take(width)
        .map(|(_, y)| {
            if max_y > 0.0 {
                ((y / max_y) * height as f32).round() as usize
            } else {
                0
            }
        })
        .collect();

    // Print from top to bottom
    for row in (1..=height).rev() {
        // Y-axis label
        let y_val = (row as f32 / height as f32) * max_y;
        write!(w, "{:>6.0} |", y_val)?;

        for &bar_height in &scaled {
            if bar_height >= row {
                write!(w, " # ")?;
            } else {
                write!(w, "   ")?;
            }
        }
        writeln!(w)?;
    }

    // X-axis
    write!(w, "       +")?;
    for _ in 0..width {
        write!(w, "---")?;
    }
    writeln!(w)?;

    // X-axis labels (indices)
    write!(w, "        ")?;
    for i in 0..width {
        write!(w, "{:^3}", i + 1)?;
    }
    writeln!(w)?;

    Ok(())
}

/// Render a simple ASCII line chart
fn render_line_chart<W: Write>(w: &mut W, data: &PlotData) -> std::io::Result<()> {
    let height = 10;
    let width = data.points.len().min(60);
    let (y_min, y_max) = data.y_range;
    let y_range = y_max - y_min;

    // Scale y values to grid positions
    let scaled: Vec<usize> = data
        .points
        .iter()
        .take(width)
        .map(|(_, y)| {
            if y_range > 0.0 {
                (((y - y_min) / y_range) * (height - 1) as f32).round() as usize
            } else {
                height / 2
            }
        })
        .collect();

    // Create grid
    let mut grid = vec![vec![' '; width]; height];
    for (x, &y) in scaled.iter().enumerate() {
        if y < height {
            grid[y][x] = '*';
        }
    }

    // Print from top to bottom
    for row in (0..height).rev() {
        let y_val = y_min + (row as f32 / (height - 1) as f32) * y_range;
        write!(w, "{:>6.1} |", y_val)?;
        for col in 0..width {
            write!(w, "{}", grid[row][col])?;
        }
        writeln!(w)?;
    }

    // X-axis
    write!(w, "       +")?;
    for _ in 0..width {
        write!(w, "-")?;
    }
    writeln!(w)?;

    Ok(())
}

/// Render a simple ASCII scatter chart
fn render_scatter_chart<W: Write>(w: &mut W, data: &PlotData) -> std::io::Result<()> {
    let height = 10;
    let width = 40;
    let (x_min, x_max) = data.x_range;
    let (y_min, y_max) = data.y_range;
    let x_range = x_max - x_min;
    let y_range = y_max - y_min;

    // Create grid
    let mut grid = vec![vec![' '; width]; height];

    // Plot points
    for (x, y) in &data.points {
        let col = if x_range > 0.0 {
            (((x - x_min) / x_range) * (width - 1) as f32).round() as usize
        } else {
            width / 2
        };
        let row = if y_range > 0.0 {
            (((y - y_min) / y_range) * (height - 1) as f32).round() as usize
        } else {
            height / 2
        };
        if col < width && row < height {
            grid[row][col] = '*';
        }
    }

    // Print from top to bottom
    for row in (0..height).rev() {
        let y_val = y_min + (row as f32 / (height - 1) as f32) * y_range;
        write!(w, "{:>6.1} |", y_val)?;
        for col in 0..width {
            write!(w, "{}", grid[row][col])?;
        }
        writeln!(w)?;
    }

    // X-axis
    write!(w, "       +")?;
    for _ in 0..width {
        write!(w, "-")?;
    }
    writeln!(w)?;

    // X-axis labels
    writeln!(
        w,
        "        {:<20}{:>20}",
        format!("{:.1}", x_min),
        format!("{:.1}", x_max)
    )?;

    Ok(())
}
