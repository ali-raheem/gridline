//! Plot spec encoding for chart cells.
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
