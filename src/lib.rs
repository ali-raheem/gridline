//! gridline_engine - Spreadsheet engine + Rhai integration.

pub(crate) mod builtins;
pub mod engine;
pub mod plot;

#[cfg(test)]
mod tests {
    use crate::engine::*;
    use dashmap::DashMap;
    use std::sync::Arc;

    #[test]
    fn test_from_str_single_letter_columns() {
        let a1 = CellRef::from_str("A1").unwrap();
        assert_eq!(a1.row, 0);
        assert_eq!(a1.col, 0);

        let b1 = CellRef::from_str("B1").unwrap();
        assert_eq!(b1.row, 0);
        assert_eq!(b1.col, 1);

        let z1 = CellRef::from_str("Z1").unwrap();
        assert_eq!(z1.row, 0);
        assert_eq!(z1.col, 25);
    }

    #[test]
    fn test_from_str_multi_letter_columns() {
        let aa1 = CellRef::from_str("AA1").unwrap();
        assert_eq!(aa1.col, 26);

        let ab1 = CellRef::from_str("AB1").unwrap();
        assert_eq!(ab1.col, 27);

        let az1 = CellRef::from_str("AZ1").unwrap();
        assert_eq!(az1.col, 51);

        let ba1 = CellRef::from_str("BA1").unwrap();
        assert_eq!(ba1.col, 52);
    }

    #[test]
    fn test_from_str_row_numbers() {
        let a1 = CellRef::from_str("A1").unwrap();
        assert_eq!(a1.row, 0);

        let a10 = CellRef::from_str("A10").unwrap();
        assert_eq!(a10.row, 9);

        let a100 = CellRef::from_str("A100").unwrap();
        assert_eq!(a100.row, 99);
    }

    #[test]
    fn test_from_str_case_insensitive() {
        let lower = CellRef::from_str("a1").unwrap();
        assert_eq!(lower.row, 0);
        assert_eq!(lower.col, 0);

        let mixed = CellRef::from_str("aA1").unwrap();
        assert_eq!(mixed.col, 26);
    }

    #[test]
    fn test_from_str_invalid_inputs() {
        assert!(CellRef::from_str("").is_none());
        assert!(CellRef::from_str("123").is_none());
        assert!(CellRef::from_str("ABC").is_none());
        assert!(CellRef::from_str("A0").is_none());
        assert!(CellRef::from_str("1A").is_none());
        assert!(CellRef::from_str("A 1").is_none());
    }

    #[test]
    fn test_preprocess_script_simple() {
        assert_eq!(preprocess_script("A1"), "cell(0, 0)");
        assert_eq!(preprocess_script("B1"), "cell(0, 1)");
        assert_eq!(preprocess_script("A2"), "cell(1, 0)");
    }

    #[test]
    fn test_preprocess_script_typed_refs() {
        assert_eq!(preprocess_script("@A1"), "value(0, 0)");
        assert_eq!(preprocess_script("len(@B1)"), "len(value(0, 1))");
        assert_eq!(preprocess_script("@A1 + B1"), "value(0, 0) + cell(0, 1)");
    }

    #[test]
    fn test_preprocess_script_expression() {
        assert_eq!(preprocess_script("A1 + B1"), "cell(0, 0) + cell(0, 1)");
        assert_eq!(
            preprocess_script("A1 * B2 + C3"),
            "cell(0, 0) * cell(1, 1) + cell(2, 2)"
        );
    }

    #[test]
    fn test_preprocess_script_preserves_other_content() {
        assert_eq!(preprocess_script("A1 + 10"), "cell(0, 0) + 10");
        assert_eq!(preprocess_script("print(A1)"), "print(cell(0, 0))");
    }

    #[test]
    fn test_extract_dependencies_empty() {
        assert!(extract_dependencies("").is_empty());
        assert!(extract_dependencies("10 + 20").is_empty());
    }

    #[test]
    fn test_extract_dependencies_single() {
        let deps = extract_dependencies("A1");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], CellRef::new(0, 0));
    }

    #[test]
    fn test_extract_dependencies_multiple() {
        let deps = extract_dependencies("A1 + B1 + C2");
        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0], CellRef::new(0, 0));
        assert_eq!(deps[1], CellRef::new(0, 1));
        assert_eq!(deps[2], CellRef::new(1, 2));
    }

    #[test]
    fn test_extract_dependencies_duplicates() {
        let deps = extract_dependencies("A1 + A1");
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn test_detect_cycle_no_cycle() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(0, 1), Cell::new_number(20.0));
        grid.insert(CellRef::new(0, 2), Cell::new_script("A1 + B1"));

        assert!(detect_cycle(&CellRef::new(0, 2), &grid).is_none());
    }

    #[test]
    fn test_detect_cycle_direct() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_script("B1"));
        grid.insert(CellRef::new(0, 1), Cell::new_script("A1"));

        assert!(detect_cycle(&CellRef::new(0, 0), &grid).is_some());
        assert!(detect_cycle(&CellRef::new(0, 1), &grid).is_some());
    }

    #[test]
    fn test_detect_cycle_indirect() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_script("B1"));
        grid.insert(CellRef::new(0, 1), Cell::new_script("C1"));
        grid.insert(CellRef::new(0, 2), Cell::new_script("A1"));

        let cycle = detect_cycle(&CellRef::new(0, 0), &grid);
        assert!(cycle.is_some());
        let path = cycle.unwrap();
        assert!(path.len() >= 3);
    }

    #[test]
    fn test_detect_cycle_self_reference() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_script("A1"));

        assert!(detect_cycle(&CellRef::new(0, 0), &grid).is_some());
    }

    #[test]
    fn test_parse_range() {
        let result = parse_range("A1:B5");
        assert_eq!(result, Some((0, 0, 4, 1)));

        let result = parse_range("B2:D10");
        assert_eq!(result, Some((1, 1, 9, 3)));

        let result = parse_range("A1");
        assert_eq!(result, None);

        let result = parse_range("invalid");
        assert_eq!(result, None);
    }

    #[test]
    fn test_preprocess_script_range_functions() {
        assert_eq!(preprocess_script("SUM(A1:B5)"), "sum_range(0, 0, 4, 1)");
        assert_eq!(preprocess_script("AVG(A1:A10)"), "avg_range(0, 0, 9, 0)");
        assert_eq!(preprocess_script("COUNT(B2:D5)"), "count_range(1, 1, 4, 3)");
        assert_eq!(preprocess_script("MIN(A1:C3)"), "min_range(0, 0, 2, 2)");
        assert_eq!(preprocess_script("MAX(A1:Z100)"), "max_range(0, 0, 99, 25)");
        assert_eq!(
            preprocess_script("BARCHART(A1:A10)"),
            "barchart_range(0, 0, 9, 0)"
        );
        assert_eq!(
            preprocess_script("LINECHART(A1:A10)"),
            "linechart_range(0, 0, 9, 0)"
        );
        assert_eq!(
            preprocess_script("SCATTER(A1:B10)"),
            "scatter_range(0, 0, 9, 1)"
        );
        assert_eq!(
            preprocess_script("SCATTER(A1:B10, \"My Plot\", \"X\", \"Y\")"),
            "scatter_range(0, 0, 9, 1, \"My Plot\", \"X\", \"Y\")"
        );
        assert_eq!(
            preprocess_script("SCATTER(A1:B10, \"A1\", \"B2\", \"C3\")"),
            "scatter_range(0, 0, 9, 1, \"A1\", \"B2\", \"C3\")"
        );
    }

    #[test]
    fn test_preprocess_script_mixed() {
        assert_eq!(
            preprocess_script("SUM(A1:A3) + B1"),
            "sum_range(0, 0, 2, 0) + cell(0, 1)"
        );
        assert_eq!(
            preprocess_script("SUM(A1:A3) * 2 + AVG(B1:B5)"),
            "sum_range(0, 0, 2, 0) * 2 + avg_range(0, 1, 4, 1)"
        );
    }

    #[test]
    fn test_range_functions_evaluation() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));
        grid.insert(CellRef::new(1, 0), Cell::new_number(20.0));
        grid.insert(CellRef::new(2, 0), Cell::new_number(30.0));

        let engine = create_engine(Arc::new(grid));

        let result: f64 = engine.eval("sum_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result, 60.0);

        let result: f64 = engine.eval("avg_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result, 20.0);

        let result: f64 = engine.eval("min_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result, 10.0);

        let result: f64 = engine.eval("max_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result, 30.0);

        let result: f64 = engine.eval("count_range(0, 0, 2, 0)").unwrap();
        assert_eq!(result, 3.0);
    }

    #[test]
    fn test_typed_ref_len_over_script_string() {
        let grid: Grid = DashMap::new();
        grid.insert(CellRef::new(0, 2), Cell::new_number(150.0)); // C1
        grid.insert(
            CellRef::new(0, 1),
            Cell::new_script("if C1 > 100 { \"expensive\" } else { \"cheap\" }"),
        ); // B1

        let engine = create_engine(Arc::new(grid));
        let processed = preprocess_script("len(@B1)");
        let result = eval_with_functions(&engine, &processed, None).unwrap();
        assert_eq!(result.as_int().unwrap(), 9);
    }

    #[test]
    fn test_extract_dependencies_with_ranges() {
        let deps = extract_dependencies("SUM(A1:A3)");
        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&CellRef::new(0, 0)));
        assert!(deps.contains(&CellRef::new(1, 0)));
        assert!(deps.contains(&CellRef::new(2, 0)));
    }

    #[test]
    fn test_custom_functions() {
        let grid: Grid = DashMap::new();
        let custom_script = r#"
            fn double(x) { x * 2.0 }
            fn square(x) { x * x }
        "#;

        let (engine, custom_ast, error) =
            create_engine_with_functions(Arc::new(grid), Some(custom_script));
        assert!(error.is_none());
        assert!(custom_ast.is_some());

        let result = eval_with_functions(&engine, "double(5.0)", custom_ast.as_ref()).unwrap();
        assert_eq!(result.as_float().unwrap(), 10.0);

        let result = eval_with_functions(&engine, "square(4.0)", custom_ast.as_ref()).unwrap();
        assert_eq!(result.as_float().unwrap(), 16.0);
    }

    #[test]
    fn test_custom_functions_with_syntax_error() {
        let grid: Grid = DashMap::new();
        let bad_script = "fn broken( { }";

        let (_engine, _ast, error) = create_engine_with_functions(Arc::new(grid), Some(bad_script));
        assert!(error.is_some());
        assert!(error.unwrap().contains("Error"));
    }
}
