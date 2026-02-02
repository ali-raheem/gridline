//! Plot spec encoding and data preparation for chart cells.
//!
//! This module provides:
//! - [`PlotSpec`]: Specification for a plot (type, range, labels)
//! - [`PlotData`]: Prepared data for rendering (frontend-agnostic)
//! - Encoding/decoding of plot specs to/from cell display strings
//!
//! The engine returns a tagged string for plot formulas (e.g. `=BARCHART(A1:A10)`).
//! The TUI detects and renders these in a modal.

pub const PLOT_PREFIX: &str = "@PLOT:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlotKind {
    Bar,
    Line,
    Scatter,
}

impl PlotKind {
    pub fn as_tag(self) -> &'static str {
        match self {
            PlotKind::Bar => "BAR",
            PlotKind::Line => "LINE",
            PlotKind::Scatter => "SCATTER",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "BAR" => Some(PlotKind::Bar),
            "LINE" => Some(PlotKind::Line),
            "SCATTER" => Some(PlotKind::Scatter),
            _ => None,
        }
    }
}

/// Specification for a plot (parsed from a plot cell).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlotSpec {
    pub kind: PlotKind,
    pub r1: usize,
    pub c1: usize,
    pub r2: usize,
    pub c2: usize,

    pub title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
}

impl PlotSpec {
    /// Validate that the plot spec is valid for its type.
    ///
    /// Returns `Ok(())` if valid, or an error message describing the problem.
    pub fn validate(&self) -> Result<(), String> {
        let cols = self.c2.abs_diff(self.c1) + 1;

        match self.kind {
            PlotKind::Scatter => {
                if cols != 2 {
                    return Err(format!(
                        "SCATTER requires exactly 2 columns (X and Y), got {}",
                        cols
                    ));
                }
            }
            PlotKind::Bar | PlotKind::Line => {
                // Bar and Line can work with any range
            }
        }
        Ok(())
    }
}

/// Prepared data for rendering a plot (frontend-agnostic).
///
/// This intermediate representation separates data extraction from rendering,
/// allowing different frontends (TUI, GUI) to use the same data preparation logic.
#[derive(Clone, Debug)]
pub struct PlotData {
    /// The original plot specification.
    pub spec: PlotSpec,
    /// Data points as (x, y) pairs.
    pub points: Vec<(f32, f32)>,
    /// X-axis range (min, max).
    pub x_range: (f32, f32),
    /// Y-axis range (min, max).
    pub y_range: (f32, f32),
    /// Warnings about data quality (e.g., skipped non-numeric cells).
    pub warnings: Vec<String>,
}

impl PlotData {
    /// Extract plot data from a spec using a cell value accessor.
    ///
    /// The `cell_value` closure takes (row, col) and returns the numeric value
    /// at that position, or `None` if the cell is empty or non-numeric.
    pub fn from_spec<F>(spec: &PlotSpec, mut cell_value: F) -> Result<Self, String>
    where
        F: FnMut(usize, usize) -> Option<f64>,
    {
        // Validate first
        spec.validate()?;

        let r1 = spec.r1.min(spec.r2);
        let r2 = spec.r1.max(spec.r2);
        let c1 = spec.c1.min(spec.c2);
        let c2 = spec.c1.max(spec.c2);

        let mut points = Vec::new();
        let mut warnings = Vec::new();
        let mut skipped_count = 0;

        match spec.kind {
            PlotKind::Scatter => {
                for r in r1..=r2 {
                    let x = cell_value(r, c1);
                    let y = cell_value(r, c2);
                    match (x, y) {
                        (Some(x), Some(y)) => points.push((x as f32, y as f32)),
                        _ => skipped_count += 1,
                    }
                }
            }
            PlotKind::Bar | PlotKind::Line => {
                let mut ys = Vec::new();
                if r1 == r2 {
                    // Single row: iterate columns
                    for c in c1..=c2 {
                        match cell_value(r1, c) {
                            Some(v) => ys.push(v as f32),
                            None => {
                                ys.push(0.0);
                                skipped_count += 1;
                            }
                        }
                    }
                } else if c1 == c2 {
                    // Single column: iterate rows
                    for r in r1..=r2 {
                        match cell_value(r, c1) {
                            Some(v) => ys.push(v as f32),
                            None => {
                                ys.push(0.0);
                                skipped_count += 1;
                            }
                        }
                    }
                } else {
                    // Multi-row, multi-column: iterate row-major
                    for r in r1..=r2 {
                        for c in c1..=c2 {
                            match cell_value(r, c) {
                                Some(v) => ys.push(v as f32),
                                None => {
                                    ys.push(0.0);
                                    skipped_count += 1;
                                }
                            }
                        }
                    }
                }
                points = ys
                    .into_iter()
                    .enumerate()
                    .map(|(i, y)| (i as f32, y))
                    .collect();
            }
        }

        if skipped_count > 0 {
            warnings.push(format!(
                "{} non-numeric cell(s) treated as 0",
                skipped_count
            ));
        }

        if points.is_empty() {
            return Err("No data points to plot".to_string());
        }

        // Calculate ranges
        let (mut xmin, mut xmax) = (points[0].0, points[0].0);
        let (mut ymin, mut ymax) = (points[0].1, points[0].1);
        for (x, y) in &points {
            xmin = xmin.min(*x);
            xmax = xmax.max(*x);
            ymin = ymin.min(*y);
            ymax = ymax.max(*y);
        }

        // Ensure non-zero ranges
        if xmax == xmin {
            xmax = xmin + 1.0;
        }
        if ymax == ymin {
            ymax = ymin + 1.0;
        }

        Ok(PlotData {
            spec: spec.clone(),
            points,
            x_range: (xmin, xmax),
            y_range: (ymin, ymax),
            warnings,
        })
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'%' | b'|' | b':' | b'\n' | b'\r' => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
            _ => out.push(*b as char),
        }
    }
    out
}

fn from_hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = from_hex_digit(bytes[i + 1])?;
            let lo = from_hex_digit(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn format_plot_spec(spec: &PlotSpec) -> String {
    let base = format!(
        "{}{}:{},{},{},{}",
        PLOT_PREFIX,
        spec.kind.as_tag(),
        spec.r1,
        spec.c1,
        spec.r2,
        spec.c2
    );

    let title = spec.title.as_deref().unwrap_or("");
    let x = spec.x_label.as_deref().unwrap_or("");
    let y = spec.y_label.as_deref().unwrap_or("");
    if title.is_empty() && x.is_empty() && y.is_empty() {
        return base;
    }

    format!(
        "{}|{}|{}|{}",
        base,
        percent_encode(title),
        percent_encode(x),
        percent_encode(y)
    )
}

pub fn parse_plot_spec(s: &str) -> Option<PlotSpec> {
    let s = s.trim();
    let rest = s.strip_prefix(PLOT_PREFIX)?;
    let (kind_tag, rest) = rest.split_once(':')?;
    let kind = PlotKind::from_tag(kind_tag)?;

    let (coords, meta) = rest
        .split_once('|')
        .map_or((rest, None), |(a, b)| (a, Some(b)));

    let mut it = coords.split(',');
    let r1 = it.next()?.parse::<usize>().ok()?;
    let c1 = it.next()?.parse::<usize>().ok()?;
    let r2 = it.next()?.parse::<usize>().ok()?;
    let c2 = it.next()?.parse::<usize>().ok()?;
    if it.next().is_some() {
        return None;
    }

    let mut title: Option<String> = None;
    let mut x_label: Option<String> = None;
    let mut y_label: Option<String> = None;
    if let Some(meta) = meta {
        let parts: Vec<&str> = meta.split('|').collect();
        if let Some(p) = parts.first()
            && !p.is_empty()
        {
            title = percent_decode(p);
        }
        if let Some(p) = parts.get(1)
            && !p.is_empty()
        {
            x_label = percent_decode(p);
        }
        if let Some(p) = parts.get(2)
            && !p.is_empty()
        {
            y_label = percent_decode(p);
        }
    }

    Some(PlotSpec {
        kind,
        r1,
        c1,
        r2,
        c2,
        title,
        x_label,
        y_label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plot_spec_round_trip() {
        let spec = PlotSpec {
            kind: PlotKind::Bar,
            r1: 0,
            c1: 1,
            r2: 9,
            c2: 1,
            title: Some("My Plot".to_string()),
            x_label: Some("X".to_string()),
            y_label: Some("Y".to_string()),
        };
        let s = format_plot_spec(&spec);
        assert_eq!(parse_plot_spec(&s), Some(spec));
    }
}
