//! Integration tests for command mode (-c/--command flag)

use std::process::Command;

fn run_command(args: &[&str]) -> (String, String, i32) {
    let output = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("--")
        // Tests must be deterministic and not depend on a user's ~/.config/gridline/default.rhai.
        .arg("--no-default-functions")
        .args(args)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit_code)
}

#[test]
fn test_basic_arithmetic() {
    let (stdout, _, code) = run_command(&["-c", "5 + 3"]);
    assert_eq!(stdout.trim(), "8");
    assert_eq!(code, 0);
}

#[test]
fn test_array_output() {
    let (stdout, _, code) = run_command(&["-c", "(0..=5).SPILL()"]);
    assert_eq!(stdout.trim(), "0\n1\n2\n3\n4\n5");
    assert_eq!(code, 0);
}

#[test]
fn test_array_map() {
    let (stdout, _, code) = run_command(&["-c", "(0..=4).SPILL().map(|x| x * x)"]);
    assert_eq!(stdout.trim(), "0\n1\n4\n9\n16");
    assert_eq!(code, 0);
}

#[test]
fn test_array_reduce() {
    let (stdout, _, code) = run_command(&["-c", "(0..=10).SPILL().reduce(|x, y| x + y, 0)"]);
    assert_eq!(stdout.trim(), "55");
    assert_eq!(code, 0);
}

#[test]
fn test_complex_formula_sum_of_squares() {
    let (stdout, _, code) = run_command(&[
        "-c",
        "(0..=10).SPILL().map(|x| x*x).reduce(|x, y| x + y, 0)",
    ]);
    assert_eq!(stdout.trim(), "385");
    assert_eq!(code, 0);
}

#[test]
fn test_auto_prepend_equals() {
    let (stdout1, _, _) = run_command(&["-c", "10 + 5"]);
    let (stdout2, _, _) = run_command(&["-c", "=10 + 5"]);
    assert_eq!(stdout1, stdout2);
}

#[test]
fn test_error_exit_code() {
    let (stdout, _, code) = run_command(&["-c", "undefined_function()"]);
    assert!(stdout.starts_with("#ERR"));
    assert_eq!(code, 1);
}

#[test]
fn test_division_by_zero() {
    let (stdout, _, code) = run_command(&["-c", "1/0"]);
    assert!(stdout.starts_with("#ERR"));
    assert_eq!(code, 1);
}

#[test]
fn test_pow_function() {
    let (stdout, _, code) = run_command(&["-c", "POW(2, 10)"]);
    assert_eq!(stdout.trim(), "1024");
    assert_eq!(code, 0);
}

#[test]
fn test_boolean_true() {
    let (stdout, _, code) = run_command(&["-c", "true"]);
    assert_eq!(stdout.trim(), "TRUE");
    assert_eq!(code, 0);
}

#[test]
fn test_boolean_false() {
    let (stdout, _, code) = run_command(&["-c", "false"]);
    assert_eq!(stdout.trim(), "FALSE");
    assert_eq!(code, 0);
}

#[test]
fn test_custom_functions() {
    use std::fs;
    use std::io::Write;

    let func_file = "/tmp/gridline_test_func.rhai";
    let mut file = fs::File::create(func_file).unwrap();
    writeln!(file, "fn double(x) {{ x * 2 }}").unwrap();
    drop(file);

    let (stdout, _, code) = run_command(&["-c", "double(21)", "-f", func_file]);
    assert_eq!(stdout.trim(), "42");
    assert_eq!(code, 0);

    fs::remove_file(func_file).ok();
}

#[test]
fn test_markdown_output_scalar() {
    use std::fs;

    let output_file = "/tmp/gridline_test_scalar.md";

    let (_, stderr, code) = run_command(&["-c", "POW(2, 10)", "-o", output_file]);
    assert_eq!(code, 0);
    assert!(stderr.contains("Result written to"));

    let content = fs::read_to_string(output_file).unwrap();
    assert!(content.contains("| 1 | 1024 |"));

    fs::remove_file(output_file).ok();
}

#[test]
fn test_markdown_output_array() {
    use std::fs;

    let output_file = "/tmp/gridline_test_array.md";

    let (_, stderr, code) = run_command(&["-c", "(0..=3).SPILL()", "-o", output_file]);
    assert_eq!(code, 0);
    assert!(stderr.contains("Result written to"));

    let content = fs::read_to_string(output_file).unwrap();
    assert!(content.contains("| 1 | 0 |"));
    assert!(content.contains("| 2 | 1 |"));
    assert!(content.contains("| 3 | 2 |"));
    assert!(content.contains("| 4 | 3 |"));

    fs::remove_file(output_file).ok();
}

#[test]
fn test_empty_result() {
    let (stdout, _, code) = run_command(&["-c", "\"\""]);
    assert_eq!(stdout.trim(), "");
    assert_eq!(code, 0);
}
