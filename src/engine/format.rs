use super::Dynamic;

/// Format a Dynamic value for display.
pub fn format_dynamic(value: &Dynamic) -> String {
    if value.is_unit() {
        String::new()
    } else if let Ok(n) = value.as_float() {
        format_number(n)
    } else if let Ok(n) = value.as_int() {
        n.to_string()
    } else if let Ok(b) = value.as_bool() {
        if b { "TRUE" } else { "FALSE" }.to_string()
    } else if let Ok(s) = value.clone().into_string() {
        s
    } else {
        format!("{:?}", value)
    }
}

/// Format a number for display.
pub fn format_number(n: f64) -> String {
    if n.is_nan() {
        "#NAN!".to_string()
    } else if n.is_infinite() {
        "#INF!".to_string()
    } else if n.fract() == 0.0 && n.abs() < 1e10 {
        format!("{:.0}", n)
    } else {
        format!("{:.2}", n)
    }
}
