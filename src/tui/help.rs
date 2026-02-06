//! Help text content for the help modal

use super::keymap::{Action, Binding, Keymap};

/// Get keybinding help text for the current keymap
pub fn get_help_text(keymap: &Keymap) -> Vec<String> {
    match keymap {
        Keymap::Vim => vim_help_text(),
        Keymap::Emacs => emacs_help_text(),
        Keymap::Custom(custom) => custom_help_text(custom),
    }
}

fn vim_help_text() -> Vec<String> {
    vec![
        "Navigation",
        "  h/j/k/l        Move left/down/up/right",
        "  [n]h/j/k/l     Move n cells (e.g., 5j)",
        "  Arrow keys     Move cursor",
        "  Tab/Shift+Tab  Move right/left",
        "  PageUp/Down    Scroll by page",
        "  Home/End       First/last column",
        "  Ctrl+Home/End  Go to first/last cell",
        "  G              Go to last row with data",
        "  gg             Go to first cell (A1)",
        "",
        "Editing",
        "  i / a / Enter  Edit cell (cursor at end)",
        "  I              Edit cell (cursor at start)",
        "  cc / S         Clear cell and edit",
        "  x / Delete     Clear cell",
        "  Esc            Cancel edit / exit mode",
        "",
        "Selection & Clipboard",
        "  v              Enter visual mode (range select)",
        "  y              Yank (copy) cell or selection",
        "  yy             Yank entire row",
        "  p              Paste at cursor",
        "  dd             Delete entire row",
        "",
        "Undo/Redo",
        "  u              Undo",
        "  Ctrl+r         Redo",
        "",
        "Display",
        "  +              Increase column width",
        "  -              Decrease column width",
        "  P              Open plot modal (chart cells)",
        "  ?              Show this help",
        "",
        "Command Mode",
        "  :              Enter command mode",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn emacs_help_text() -> Vec<String> {
    vec![
        "Navigation",
        "  C-n/C-p        Move down/up",
        "  C-f/C-b        Move right/left",
        "  Arrow keys     Move cursor",
        "  Tab/Shift+Tab  Move right/left",
        "  C-v/M-v        Page down/up",
        "  C-a/C-e        First/last column",
        "  Ctrl+Home/End  Go to first/last cell",
        "  M->            Go to last row with data",
        "  M-g            Open goto prompt",
        "",
        "Editing",
        "  Enter          Edit cell",
        "  C-d/Delete     Clear cell",
        "  C-g / Esc      Cancel",
        "",
        "Selection & Clipboard",
        "  C-SPC          Set mark (visual mode)",
        "  M-w            Copy (yank)",
        "  C-y            Paste",
        "",
        "Other",
        "  M-x            Enter command mode",
        "  C-s            Save",
        "  C-q            Quit",
        "  M-p            Open plot modal",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Get command help text
pub fn get_commands_help() -> Vec<String> {
    vec![
        "Commands",
        "",
        "File Operations",
        "  :w [file]      Save (optionally to new path)",
        "  :q             Quit (warns if unsaved)",
        "  :q!            Force quit (discard changes)",
        "  :wq            Save and quit",
        "  :e <file>      Open file (.grd format)",
        "  :new           New empty document",
        "",
        "Navigation",
        "  :goto <cell>   Go to cell (e.g. :goto A100)",
        "  :g <cell>      Alias for :goto",
        "",
        "Row/Column Operations",
        "  :ir            Insert row above cursor",
        "  :dr            Delete current row",
        "  :ic            Insert column left of cursor",
        "  :dc            Delete current column",
        "  :insertrow     Alias for :ir",
        "  :deleterow     Alias for :dr",
        "  :insertcol     Alias for :ic",
        "  :deletecol     Alias for :dc",
        "",
        "Display",
        "  :colwidth <n>  Set current column width",
        "  :cw [col] <n>  Set column width (e.g. :cw A 15)",
        "",
        "Import/Export",
        "  :import <csv>  Import CSV at cursor position",
        "  :export <csv>  Export grid (or selection) to CSV",
        "",
        "Functions & Scripts",
        "  :source <file> Load Rhai functions file",
        "  :so            Reload loaded function files",
        "  :call <expr>   Execute Rhai function",
        "  :rhai <expr>   Execute Rhai expression",
        "",
        "Help",
        "  :help / :h     Show this help modal",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Get built-in functions help
pub fn get_functions_help() -> Vec<String> {
    vec![
        "Built-in Functions",
        "",
        "Aggregation (use range syntax: A1:B10)",
        "  SUM(range)     Sum of numeric values",
        "  AVG(range)     Average of numeric values",
        "  COUNT(range)   Count of non-empty cells",
        "  MIN(range)     Minimum value",
        "  MAX(range)     Maximum value",
        "",
        "Conditional",
        "  IF(cond, a, b) Returns a if true, b if false",
        "  SUMIF(range, |x| condition)",
        "  COUNTIF(range, |x| condition)",
        "",
        "Arrays & Spilling",
        "  VEC(range)     Convert range to array",
        "  SPILL(array)   Spill array down from cell",
        "  SPILL(0..10)   Spill range as array",
        "",
        "Math",
        "  POW(base, exp) Exponentiation",
        "  SQRT(x)        Square root",
        "  ABS(x)         Absolute value",
        "  ROUND(n, dec)  Round to N decimal places",
        "  RAND()         Random float [0, 1)",
        "  RANDINT(a, b)  Random integer [a, b]",
        "",
        "Date/Time",
        "  TODAY()        Current date (YYYY-MM-DD)",
        "  NOW()          Current date and time",
        "",
        "Formatting",
        "  FIXED(n, dec)  Fixed decimal places",
        "  MONEY(n, sym)  Currency format",
        "",
        "Charts (displayed in cell, P to view)",
        "  BARCHART(range)",
        "  LINECHART(range)",
        "  SCATTER(range)",
        "",
        "Cell References",
        "  ROW()          Current row (1-indexed)",
        "  COL()          Current column (1-indexed)",
        "  @A1            Get typed value (not numeric)",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Get static project metadata shown in help modal.
pub fn get_about_help() -> Vec<String> {
    vec![
        format!("  Name: {}", env!("CARGO_PKG_NAME")),
        format!("  Version: {}", env!("CARGO_PKG_VERSION")),
        format!("  Author: {}", env!("CARGO_PKG_AUTHORS")),
        format!("  License: {}", env!("CARGO_PKG_LICENSE")),
        format!("  Repository: {}", env!("CARGO_PKG_REPOSITORY")),
        "".to_string(),
        "Navigation".to_string(),
        "  j/k, Up/Down   Scroll line by line".to_string(),
        "  PgUp/PgDn      Scroll by page".to_string(),
        "  g/G, Home/End  Jump to top/bottom".to_string(),
        "  Esc, q         Close help".to_string(),
    ]
}

fn custom_help_text(custom: &super::keymap::CustomKeymap) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(desc) = custom.description.as_ref() {
        lines.push(desc.clone());
        lines.push(String::new());
    }
    lines.push("Normal Mode".to_string());
    append_bindings(&mut lines, &custom.bindings.normal);
    lines.push(String::new());
    lines.push("Visual Mode".to_string());
    append_bindings(&mut lines, &custom.bindings.visual);
    lines.push(String::new());
    lines.push("Edit Mode".to_string());
    append_bindings(&mut lines, &custom.bindings.edit);
    lines.push(String::new());
    lines.push("Command Mode".to_string());
    append_bindings(&mut lines, &custom.bindings.command);
    lines
}

fn append_bindings(lines: &mut Vec<String>, bindings: &[Binding]) {
    if bindings.is_empty() {
        lines.push("  (no bindings)".to_string());
        return;
    }
    for binding in bindings {
        let label = action_label(&binding.action);
        lines.push(format!("  {:<14} {}", binding.combo.display(), label));
    }
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Cancel => "Cancel / Escape",
        Action::EnterEdit => "Edit cell",
        Action::InsertAtStart => "Edit cell (cursor at start)",
        Action::CommitEdit => "Commit edit",
        Action::EnterCommand => "Command mode",
        Action::ExecuteCommand => "Execute command",
        Action::EnterVisual => "Visual mode",
        Action::ExitVisual => "Exit visual",
        Action::Yank => "Yank (copy)",
        Action::Paste => "Paste",
        Action::Undo => "Undo",
        Action::Redo => "Redo",
        Action::ClearCell => "Clear cell",
        Action::ChangeCell => "Clear and edit cell",
        Action::OpenPlot => "Open plot modal",
        Action::Move(-1, 0) => "Move left",
        Action::Move(1, 0) => "Move right",
        Action::Move(0, -1) => "Move up",
        Action::Move(0, 1) => "Move down",
        Action::Move(_, _) => "Move",
        Action::Page(-1) => "Page up",
        Action::Page(1) => "Page down",
        Action::Page(_) => "Page",
        Action::HomeCol => "First column",
        Action::EndCol => "Last column",
        Action::GotoLast => "Last row with data",
        Action::GotoFirst => "First cell (A1)",
        Action::OpenGotoPrompt => "Goto cell prompt",
        Action::IncColWidth => "Widen column",
        Action::DecColWidth => "Narrow column",
        Action::Save => "Save file",
    }
}
