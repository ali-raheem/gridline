use crate::tui::app::Mode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Available keybinding schemes.
///
/// Gridline supports two keybinding schemes:
/// - [`Vim`](Keymap::Vim): hjkl navigation, `:` commands, modal editing
/// - [`Emacs`](Keymap::Emacs): C-n/p/f/b navigation, M-x commands, C-SPC mark
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keymap {
    /// Vim-style keybindings (hjkl, :commands, modal editing).
    Vim,
    /// Emacs-style keybindings (C-n/p/f/b, M-x commands).
    Emacs,
    /// Custom keymap loaded from user config.
    Custom(CustomKeymap),
}

impl Keymap {
    pub fn name(&self) -> &str {
        match self {
            Keymap::Vim => "vim",
            Keymap::Emacs => "emacs",
            Keymap::Custom(custom) => &custom.name,
        }
    }

    pub fn status_hint(&self) -> String {
        match self {
            Keymap::Vim => {
                "hjkl:move  i:edit  v:visual  y:yank  p:paste  P:plot  +/-:colwidth  G:last  :w:save  :q:quit".to_string()
            }
            Keymap::Emacs => {
                "C-n/p/f/b:move  Enter:edit  M-x:cmd  C-s:save  M-w:copy  C-y:paste  C-SPC:mark  C-g:cancel  M-p:plot".to_string()
            }
            Keymap::Custom(custom) => {
                format!("custom keymap: {}  :help for bindings", custom.name)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomKeymap {
    pub name: String,
    pub description: Option<String>,
    pub bindings: KeymapBindings,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeymapBindings {
    pub normal: Vec<Binding>,
    pub visual: Vec<Binding>,
    pub edit: Vec<Binding>,
    pub command: Vec<Binding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub combo: KeyCombo,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCombo {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyCombo {
    fn matches(&self, key: KeyEvent) -> bool {
        if self.code != key.code {
            return false;
        }
        if self.modifiers.is_empty() {
            return true;
        }
        key.modifiers.contains(self.modifiers)
    }

    pub fn display(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("C");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("M");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("S");
        }
        let key = match self.code {
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            _ => "Unknown".to_string(),
        };
        if parts.is_empty() {
            key
        } else {
            format!("{}-{}", parts.join("-"), key)
        }
    }
}

impl CustomKeymap {
    pub(crate) fn translate(&self, mode: Mode, key: KeyEvent) -> Option<Action> {
        let bindings = self.bindings.for_mode(mode);
        bindings
            .iter()
            .filter(|binding| !binding.combo.modifiers.is_empty())
            .find(|binding| binding.combo.matches(key))
            .or_else(|| {
                bindings
                    .iter()
                    .filter(|binding| binding.combo.modifiers.is_empty())
                    .find(|binding| binding.combo.matches(key))
            })
            .map(|binding| binding.action.clone())
    }
}

impl KeymapBindings {
    fn for_mode(&self, mode: Mode) -> &Vec<Binding> {
        match mode {
            Mode::Normal => &self.normal,
            Mode::Visual => &self.visual,
            Mode::Edit => &self.edit,
            Mode::Command => &self.command,
        }
    }
}

/// Actions that can be triggered by key presses.
///
/// Actions decouple key handling from application logic. The keymap translates
/// key events into actions, which are then applied to the application state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Cancel current operation and return to Normal mode.
    Cancel,
    /// Enter Edit mode for the current cell.
    EnterEdit,
    /// Commit the current edit and return to Normal mode.
    CommitEdit,
    /// Enter Command mode (`:` prompt).
    EnterCommand,
    /// Execute the command in the command buffer.
    ExecuteCommand,
    /// Enter Visual selection mode.
    EnterVisual,
    /// Exit Visual mode without action.
    ExitVisual,
    /// Yank (copy) current cell or selection.
    Yank,
    /// Paste clipboard at cursor position.
    Paste,
    /// Undo the last action.
    Undo,
    /// Redo the last undone action.
    Redo,
    /// Clear the current cell.
    ClearCell,
    /// Open plot modal for current cell.
    OpenPlot,

    /// Move cursor by (dx, dy).
    Move(i32, i32),
    /// Page up (-1) or down (+1).
    Page(i32),
    /// Jump to first column.
    HomeCol,
    /// Jump to last column.
    EndCol,
    /// Jump to last row with data.
    GotoLast,
    /// Open the goto cell prompt.
    OpenGotoPrompt,

    /// Increase current column width.
    IncColWidth,
    /// Decrease current column width.
    DecColWidth,
    /// Save the file.
    Save,
}
