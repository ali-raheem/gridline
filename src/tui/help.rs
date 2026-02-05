//! Help text content for the help modal

use super::keymap::{Action, Binding, Keymap};

/// Get keybinding help text for the current keymap
pub fn get_help_text(keymap: &Keymap) -> Vec<String> {
    match keymap {
        Keymap::Vim => vec![
            "Navigation:",
            "  h/j/k/l      Move left/down/up/right",
            "  Arrow keys   Move cursor",
            "  PageUp/Down  Scroll by page",
            "  Home/End     First/last column",
            "  G            Go to last row with data",
            "  g            Open goto prompt",
            "",
            "Editing:",
            "  i / Enter    Edit cell",
            "  x / Delete   Clear cell",
            "  Esc          Cancel edit",
            "",
            "Selection:",
            "  v            Enter visual mode",
            "  y            Yank (copy)",
            "  p            Paste",
            "",
            "Undo/Redo:",
            "  u            Undo",
            "  Ctrl+r       Redo",
            "",
            "Other:",
            "  :            Enter command mode",
            "  P            Open plot modal",
            "  +/-          Adjust column width",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        Keymap::Emacs => vec![
            "Navigation:",
            "  C-n/C-p      Move down/up",
            "  C-f/C-b      Move right/left",
            "  Arrow keys   Move cursor",
            "  C-v/M-v      Page down/up",
            "  C-a/C-e      First/last column",
            "  M->          Go to last row",
            "  M-g          Open goto prompt",
            "",
            "Editing:",
            "  Enter        Edit cell",
            "  C-d/Delete   Clear cell",
            "  C-g / Esc    Cancel",
            "",
            "Selection:",
            "  C-SPC        Set mark (visual)",
            "  M-w          Copy (yank)",
            "  C-y          Paste",
            "",
            "Other:",
            "  M-x          Enter command mode",
            "  C-s          Save",
            "  C-q          Quit",
            "  M-p          Open plot modal",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        Keymap::Custom(custom) => custom_help_text(custom),
    }
}

/// Get command help text
pub fn get_commands_help() -> Vec<String> {
    vec![
        "Commands",
        "",
        "File:",
        "  :w [file]       Save",
        "  :q              Quit",
        "  :q!             Force quit",
        "  :wq             Save and quit",
        "  :e <file>       Open file (.grd)",
        "  :open <file>    Open file (.grd)",
        "  :load <file>    Open file (.grd)",
        "",
        "Import/Export:",
        "  :import <csv>   Import CSV",
        "  :export <csv>   Export CSV",
        "",
        "Row/Column:",
        "  :ir             Insert row above",
        "  :dr             Delete current row",
        "  :ic             Insert column left",
        "  :dc             Delete current column",
        "",
        "Navigation:",
        "  :goto <cell>    Go to cell (e.g. :goto A100)",
        "",
        "Display:",
        "  :colwidth <n>   Set column width",
        "  :cw <col> <n>   Set specific column",
        "",
        "Functions:",
        "  :source <file>  Load Rhai functions",
        "  :so             Reload functions",
        "",
        "Press Esc or q to close",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn custom_help_text(custom: &super::keymap::CustomKeymap) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(desc) = custom.description.as_ref() {
        lines.push(desc.clone());
        lines.push(String::new());
    }
    lines.push("Normal:".to_string());
    append_bindings(&mut lines, &custom.bindings.normal);
    lines.push(String::new());
    lines.push("Visual:".to_string());
    append_bindings(&mut lines, &custom.bindings.visual);
    lines.push(String::new());
    lines.push("Edit:".to_string());
    append_bindings(&mut lines, &custom.bindings.edit);
    lines.push(String::new());
    lines.push("Command:".to_string());
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
        lines.push(format!("  {:<12} {}", binding.combo.display(), label));
    }
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Cancel => "Cancel",
        Action::EnterEdit => "Edit cell",
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
        Action::GotoLast => "Go to last row",
        Action::OpenGotoPrompt => "Goto prompt",
        Action::IncColWidth => "Increase column width",
        Action::DecColWidth => "Decrease column width",
        Action::Save => "Save",
    }
}
