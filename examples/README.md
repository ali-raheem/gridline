# Example Sheets

## New Example Suite

- `examples/builtins.grid`
  - Core built-ins only (no custom Rhai file required).
  - Covers range aggregations, lookup, scalar helpers, typed refs, arrays, and chart spec output.

- `examples/default_functions.grid`
  - Showcase for helper functions in `examples/default.rhai`.
  - Covers stats, finance, text helpers, array transforms, and formatting helpers.

- `examples/script_ops.grid`
  - Script-operation walkthrough for `:rhai` / `:call`.
  - Pair with `examples/script_ops.rhai`.

- `examples/script_ops.rhai`
  - Script helper functions that use script-only write builtins:
    `SET_CELL`, `CLEAR_CELL`, `SET_RANGE`, `CLEAR_RANGE`.

## Run Commands

```bash
# Built-ins only
cargo run -- examples/builtins.grid

# default.rhai helper showcase
cargo run -- -f examples/default.rhai examples/default_functions.grid

# Script operation playground
cargo run -- examples/script_ops.grid
# then inside Gridline:
#   :source examples/script_ops.rhai
#   :call seed_sales_table()
```

## Existing Examples

- `examples/plot.grid` (plots, arrays, typed refs)
- `examples/vec_test.grid` (VEC/SPILL chaining)
- `examples/markdown_test.grid` (legacy default-function-heavy sheet)
