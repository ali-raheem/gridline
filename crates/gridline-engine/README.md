# gridline-engine

gridline-engine is the core spreadsheet engine used by Gridline. It handles cell storage, formula preprocessing, Rhai evaluation, dependency tracking, and plot specs.

## Usage

```rust
use dashmap::DashMap;
use gridline_engine::engine::{create_engine, preprocess_script, Cell, CellRef, Grid};

let grid: Grid = std::sync::Arc::new(DashMap::new());
grid.insert(CellRef::new(0, 0), Cell::new_number(10.0));

let engine = create_engine(grid);
let processed = preprocess_script("A1 + 5");
let result: f64 = engine.eval(&processed).unwrap();
assert_eq!(result, 15.0);
```

## Core APIs

- `engine` module: `Cell`, `CellRef`, `Grid`, `create_engine`, formula preprocessing
- `builtins` module: register Rhai spreadsheet functions
- `plot` module: tagged plot specs for chart rendering

See the top-level `README.md` for the full Gridline experience.
