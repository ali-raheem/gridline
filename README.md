<p align="center">
  <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/Rust-2024-000000?logo=rust" /></a>
  <a href="https://rhai.rs/book/index.html"><img alt="Rhai" src="https://img.shields.io/badge/Rhai-scripting-2b2b2b" /></a>
  <a href="https://github.com/ratatui/ratatui"><img alt="ratatui" src="https://img.shields.io/badge/TUI-ratatui-1f2937" /></a>
</p>

# Gridline ✨

Gridline is a proof-of-concept terminal spreadsheet with Rhai support. Cells can contain numbers, text, or formulas powered by the [Rhai scripting language](https://rhai.rs/book/index.html). Your sheet lives in a plain text file, and your reusable logic can live in a separate `.rhai` functions file.

What you get (today):
- TUI grid with a formula bar and command mode
- A1-style references in formulas (`=A1 + B2`) and range functions (`=SUM(A1:B5)`)
- Dependency tracking and recalculation
- Load/reload user functions from a `.rhai` file (`-f` at startup, `:source` at runtime)
- Two keymaps: `vim` (default) and `emacs`
- Plain text storage format (one cell per line)
- Simple plotting in a modal (bar/line/scatter)

Project status: this is a POC. Expect rough edges (especially around plotting and any non-trivial spreadsheet ergonomics).

Why it's fun:
- 🧾 Plain-text sheets you can diff and version
- 🧠 Formulas are real Rhai scripts (with spreadsheet sugar)
- 📈 Quick plots right in the terminal

![gridline in action](screen.jpg)

## Quick Start 🚀

Build and run:

```bash
cargo run
```

Open an example file:

```bash
cargo run -- examples/test.grid
```

Load custom Rhai functions at startup:

```bash
cargo run -- -f examples/functions.rhai examples/test.grid
```

Reload / load functions at runtime:

```text
:source examples/functions.rhai
```

## Cell Input Rules 🧾

Gridline interprets cell input like this:
- empty / whitespace => empty cell
- leading `=` => formula (Rhai script; stored without the `=`)
- quoted `"text"` => text (quotes stripped)
- otherwise, parseable as `f64` => number
- else => text

Examples:
```text
A1: 10
A2: "hello"
A3: =A1 * 2
A4: =SUM(A1:A3)
```

## Formulas (Rhai + Spreadsheet Sugar) 🧠

Inside formulas:
- `A1` becomes `cell(0, 0)` (0-indexed internally)
- `@A1` becomes `value(0, 0)` (typed access: numbers/text/bools)
- `SUM(A1:B5)` becomes `sum_range(0, 0, 4, 1)`

Arrays "spill" down the column.
If you need to do an in-place operation that returns `()` (like Rhai's `Array.sort()`), use `OUTPUT`:

```text
A1: 30
A2: 10
A3: 20
B1: =OUTPUT(VEC(A1:A3), |v| { v.sort(); v })
```

Typed refs are useful when a referenced cell contains text (or a formula that returns text):

```text
B1: =if C1 > 100 { "expensive" } else { "cheap" }
A1: =len(@B1)
```

Built-in range functions (ALL CAPS):
- `SUM`, `AVG`, `COUNT`, `MIN`, `MAX`
- `BARCHART`, `LINECHART`, `SCATTER`
- `VEC` (convert a range to an array)

### Custom Functions Example 🧩

Create a `.rhai` file:

```rhai
fn fib(n) {
  if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
```

Load it (either `-f` or `:source`), then use it in a cell:

```text
A1: "Fibonacci"
B1: 10
C1: =fib(B1)
```

## Plotting 📈

Plotting works by making a formula cell return a tagged plot spec. The grid shows a placeholder (e.g. `<BAR>`), and you can open the plot modal.

Example (`examples/plot.grid`):

```text
C1: =SCATTER(A1:B5, "title", "xaxis", "yaxis")
D1: =BARCHART(B1:B5)
```

Open the plot modal:
- Vim keymap: `P`
- Emacs keymap: `M-p`

## Commands ⌨️

Command mode:
- Vim: `:`
- Emacs: `M-x`

Useful commands:
- `:w` or `:w <path>` save
- `:q` quit (warns if modified)
- `:q!` force quit
- `:wq` save and quit
- `:e <path>` open file
- `:goto A100` (alias `:g A100`) jump to a cell
- `:colwidth 15` set current column width
- `:colwidth A 15` set a specific column width
- `:source <file.rhai>` (alias `:so`) load functions; `:so` reloads the last functions file

## Keymaps 🗺️

Select keybindings:

```bash
gridline --keymap vim
gridline --keymap emacs
```

Status bar has an always-on cheat sheet, but the core controls are:

Vim:
- `hjkl` move, `i`/`Enter` edit, `Esc` cancel
- `v` visual select, `y` yank, `p` paste
- `:w` save, `:q` quit

Emacs:
- `C-n/p/f/b` move, `Enter` edit, `C-g` cancel
- `C-SPC` set mark (visual), `M-w` copy, `C-y` paste
- `C-s` save

## File Format 📝

Gridline files are plain text. Non-empty, non-comment lines look like:

```text
CELLREF: VALUE
```

Comments start with `#`. Values follow the same input rules as interactive editing.

## Development 🔧

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

## License 📜

Licensed under either of:
- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)
