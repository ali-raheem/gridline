# gridline-core

gridline-core provides Gridline's UI-agnostic document model, evaluation, and storage helpers. It is the layer you embed in a CLI, TUI, or GUI.

## Usage

```rust
use gridline_core::{Document, CellRef};

let mut doc = Document::new();
doc.set_cell_from_input(CellRef::new(0, 0), "1").unwrap();
doc.set_cell_from_input(CellRef::new(1, 0), "=A1 + 1").unwrap();

let display = doc.get_cell_display(&CellRef::new(1, 0));
assert_eq!(display, "2");
```

## Core APIs

- `Document`: grid storage, evaluation, undo/redo, import/export
- `ScriptContext`: evaluation context for custom functions
- `storage`: CSV/markdown/grid file helpers

See `crates/gridline-engine/README.md` for the engine layer and the top-level `README.md` for the full app.
