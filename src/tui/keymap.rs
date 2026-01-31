//! Keymap translation layer.
//!
//! This keeps key handling separate from app behavior.
//! - Vim keymap preserves existing behavior.
//! - Emacs keymap is "strict": vim-style letter keys are not active.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use directories::ProjectDirs;

use super::app::Mode;

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
    fn translate(&self, mode: Mode, key: KeyEvent) -> Option<Action> {
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

/// Translate a key event to an action based on the current keymap and mode.
///
/// Returns `None` if the key has no binding in the current context.
pub fn translate(keymap: &Keymap, mode: Mode, key: KeyEvent) -> Option<Action> {
    match keymap {
        Keymap::Vim => translate_vim(mode, key),
        Keymap::Emacs => translate_emacs(mode, key),
        Keymap::Custom(custom) => custom.translate(mode, key),
    }
}

#[derive(Debug, Deserialize)]
struct KeymapsFile {
    meta: Option<KeymapsMeta>,
    keymaps: Option<HashMap<String, KeymapFile>>,
}

#[derive(Debug, Deserialize)]
struct KeymapsMeta {
    default: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KeymapFile {
    description: Option<String>,
    normal: Option<HashMap<String, String>>,
    visual: Option<HashMap<String, String>>,
    edit: Option<HashMap<String, String>>,
    command: Option<HashMap<String, String>>,
}

pub fn load_keymap(requested: Option<&str>, keymap_file: Option<&PathBuf>) -> (Keymap, Vec<String>) {
    let mut warnings: Vec<String> = Vec::new();
    let config_path = keymap_file.cloned().or_else(user_keymaps_path);
    let mut file: Option<KeymapsFile> = None;

    if let Some(path) = config_path.as_ref() {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match toml::from_str::<KeymapsFile>(&content) {
                    Ok(parsed) => file = Some(parsed),
                    Err(err) => warnings.push(format!(
                        "Failed to parse {}: {}",
                        path.display(),
                        err
                    )),
                },
                Err(err) => warnings.push(format!(
                    "Failed to read {}: {}",
                    path.display(),
                    err
                )),
            }
        } else if keymap_file.is_some() {
            warnings.push(format!(
                "Keymap file not found: {}",
                path.display()
            ));
        }
    }

    let requested_name = requested.map(|name| name.trim()).filter(|s| !s.is_empty());
    let default_name = file
        .as_ref()
        .and_then(|f| f.meta.as_ref())
        .and_then(|m| m.default.as_ref())
        .map(|s| s.as_str());
    let target = requested_name.or(default_name).unwrap_or("vim");

    if let Some(file) = file.as_ref() {
        if let Some(keymaps) = file.keymaps.as_ref() {
            if let Some(entry) = keymaps.get(target) {
                match build_custom_keymap(target, entry) {
                    Ok(custom) => return (Keymap::Custom(custom), warnings),
                    Err(errs) => {
                        warnings.extend(errs);
                    }
                }
            } else if requested_name.is_some() {
                warnings.push(format!(
                    "Keymap '{}' not found in {}",
                    target,
                    config_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "keymaps.toml".to_string())
                ));
            }
        }
    }

    if target.eq_ignore_ascii_case("emacs") {
        (Keymap::Emacs, warnings)
    } else {
        if requested_name.is_some() && !target.eq_ignore_ascii_case("vim") {
            warnings.push(format!(
                "Falling back to built-in 'vim' keymap for '{}'",
                target
            ));
        }
        (Keymap::Vim, warnings)
    }
}

fn user_keymaps_path() -> Option<PathBuf> {
    let proj = ProjectDirs::from("", "", "gridline")?;
    let mut path = proj.config_dir().to_path_buf();
    path.push("keymaps.toml");
    Some(path)
}

fn build_custom_keymap(name: &str, entry: &KeymapFile) -> Result<CustomKeymap, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    let normal = parse_mode_bindings("normal", entry.normal.as_ref(), &mut errors);
    let visual = parse_mode_bindings("visual", entry.visual.as_ref(), &mut errors);
    let edit = parse_mode_bindings("edit", entry.edit.as_ref(), &mut errors);
    let command = parse_mode_bindings("command", entry.command.as_ref(), &mut errors);

    if errors.is_empty() {
        Ok(CustomKeymap {
            name: name.to_string(),
            description: entry.description.clone(),
            bindings: KeymapBindings {
                normal,
                visual,
                edit,
                command,
            },
        })
    } else {
        Err(errors)
    }
}

fn parse_mode_bindings(
    mode: &str,
    raw: Option<&HashMap<String, String>>,
    errors: &mut Vec<String>,
) -> Vec<Binding> {
    let mut bindings: Vec<Binding> = Vec::new();
    let Some(raw) = raw else {
        return bindings;
    };
    for (combo_str, action_str) in raw {
        match (parse_key_combo(combo_str), action_from_str(action_str)) {
            (Ok(combo), Some(action)) => bindings.push(Binding { combo, action }),
            (Ok(_), None) => errors.push(format!(
                "Invalid action '{}' in {} bindings",
                action_str, mode
            )),
            (Err(err), _) => errors.push(format!(
                "Invalid key '{}' in {} bindings: {}",
                combo_str, mode, err
            )),
        }
    }
    bindings
}

fn parse_key_combo(input: &str) -> Result<KeyCombo, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty key".to_string());
    }

    let parts: Vec<&str> = trimmed.split('-').collect();
    let (mods, key_part) = if parts.len() == 1 {
        (KeyModifiers::empty(), parts[0])
    } else {
        let (mod_parts, key_part) = parts.split_at(parts.len() - 1);
        let mut modifiers = KeyModifiers::empty();
        for part in mod_parts {
            let norm = part.trim().to_ascii_lowercase();
            match norm.as_str() {
                "c" | "ctrl" | "control" => modifiers.insert(KeyModifiers::CONTROL),
                "m" | "alt" | "meta" => modifiers.insert(KeyModifiers::ALT),
                "s" | "shift" => modifiers.insert(KeyModifiers::SHIFT),
                _ => {
                    return Err(format!("unknown modifier '{}'", part));
                }
            }
        }
        (modifiers, key_part[0])
    };

    let key = parse_key_code(key_part)?;
    Ok(KeyCombo { code: key, modifiers: mods })
}

fn parse_key_code(input: &str) -> Result<KeyCode, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty key".to_string());
    }
    if trimmed.len() == 1 {
        return Ok(KeyCode::Char(trimmed.chars().next().unwrap()));
    }
    let norm = trimmed.to_ascii_lowercase();
    match norm.as_str() {
        "enter" => Ok(KeyCode::Enter),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "backspace" => Ok(KeyCode::Backspace),
        "delete" => Ok(KeyCode::Delete),
        "tab" => Ok(KeyCode::Tab),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" => Ok(KeyCode::PageUp),
        "pagedown" => Ok(KeyCode::PageDown),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "space" => Ok(KeyCode::Char(' ')),
        _ => Err(format!("unknown key '{}'", input)),
    }
}

fn action_from_str(input: &str) -> Option<Action> {
    match input.trim().to_ascii_lowercase().as_str() {
        "cancel" => Some(Action::Cancel),
        "enter_edit" => Some(Action::EnterEdit),
        "commit_edit" => Some(Action::CommitEdit),
        "enter_command" => Some(Action::EnterCommand),
        "execute_command" => Some(Action::ExecuteCommand),
        "enter_visual" => Some(Action::EnterVisual),
        "exit_visual" => Some(Action::ExitVisual),
        "yank" => Some(Action::Yank),
        "paste" => Some(Action::Paste),
        "undo" => Some(Action::Undo),
        "redo" => Some(Action::Redo),
        "clear_cell" => Some(Action::ClearCell),
        "open_plot" => Some(Action::OpenPlot),
        "move_left" => Some(Action::Move(-1, 0)),
        "move_right" => Some(Action::Move(1, 0)),
        "move_up" => Some(Action::Move(0, -1)),
        "move_down" => Some(Action::Move(0, 1)),
        "page_up" => Some(Action::Page(-1)),
        "page_down" => Some(Action::Page(1)),
        "home_col" => Some(Action::HomeCol),
        "end_col" => Some(Action::EndCol),
        "goto_last" => Some(Action::GotoLast),
        "open_goto" => Some(Action::OpenGotoPrompt),
        "inc_col_width" => Some(Action::IncColWidth),
        "dec_col_width" => Some(Action::DecColWidth),
        "save" => Some(Action::Save),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_combo_ctrl() {
        let combo = parse_key_combo("C-s").expect("combo");
        assert_eq!(combo.code, KeyCode::Char('s'));
        assert!(combo.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn parse_key_combo_alt() {
        let combo = parse_key_combo("M-p").expect("combo");
        assert_eq!(combo.code, KeyCode::Char('p'));
        assert!(combo.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn parse_key_combo_enter() {
        let combo = parse_key_combo("Enter").expect("combo");
        assert_eq!(combo.code, KeyCode::Enter);
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn action_from_str_move_left() {
        let action = action_from_str("move_left").expect("action");
        assert_eq!(action, Action::Move(-1, 0));
    }

    #[test]
    fn parse_key_combo_invalid_key() {
        let err = parse_key_combo("C-NotAKey").unwrap_err();
        assert!(err.contains("unknown key"));
    }
}

fn translate_vim(mode: Mode, key: KeyEvent) -> Option<Action> {
    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('u') => Some(Action::Undo),
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Redo)
            }

            KeyCode::Up | KeyCode::Char('k') => Some(Action::Move(0, -1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::Move(0, 1)),
            KeyCode::Left | KeyCode::Char('h') => Some(Action::Move(-1, 0)),
            KeyCode::Right | KeyCode::Char('l') => Some(Action::Move(1, 0)),

            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),
            KeyCode::Home => Some(Action::HomeCol),
            KeyCode::End => Some(Action::EndCol),

            KeyCode::Enter | KeyCode::Char('i') => Some(Action::EnterEdit),
            KeyCode::Char('x') | KeyCode::Delete => Some(Action::ClearCell),
            KeyCode::Char(':') => Some(Action::EnterCommand),
            KeyCode::Char('v') => Some(Action::EnterVisual),
            KeyCode::Char('y') => Some(Action::Yank),
            KeyCode::Char('p') => Some(Action::Paste),
            KeyCode::Char('P') => Some(Action::OpenPlot),
            KeyCode::Char('+') | KeyCode::Char('>') => Some(Action::IncColWidth),
            KeyCode::Char('-') | KeyCode::Char('<') => Some(Action::DecColWidth),
            KeyCode::Char('G') => Some(Action::GotoLast),
            KeyCode::Char('g') => Some(Action::OpenGotoPrompt),
            _ => None,
        },

        Mode::Visual => match key.code {
            KeyCode::Esc => Some(Action::ExitVisual),

            KeyCode::Up | KeyCode::Char('k') => Some(Action::Move(0, -1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::Move(0, 1)),
            KeyCode::Left | KeyCode::Char('h') => Some(Action::Move(-1, 0)),
            KeyCode::Right | KeyCode::Char('l') => Some(Action::Move(1, 0)),

            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),
            KeyCode::Char('y') => Some(Action::Yank),
            _ => None,
        },

        Mode::Edit => match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::CommitEdit),
            _ => None,
        },

        Mode::Command => match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::ExecuteCommand),
            _ => None,
        },
    }
}

fn translate_emacs(mode: Mode, key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match mode {
        Mode::Normal => match key.code {
            // Cancel
            KeyCode::Char('g') if ctrl => Some(Action::Cancel),

            // Movement
            KeyCode::Up => Some(Action::Move(0, -1)),
            KeyCode::Down => Some(Action::Move(0, 1)),
            KeyCode::Left => Some(Action::Move(-1, 0)),
            KeyCode::Right => Some(Action::Move(1, 0)),
            KeyCode::Char('p') if ctrl => Some(Action::Move(0, -1)),
            KeyCode::Char('n') if ctrl => Some(Action::Move(0, 1)),
            KeyCode::Char('b') if ctrl => Some(Action::Move(-1, 0)),
            KeyCode::Char('f') if ctrl => Some(Action::Move(1, 0)),
            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),
            KeyCode::Char('v') if ctrl => Some(Action::Page(1)),
            KeyCode::Char('v') if alt => Some(Action::Page(-1)),

            // Home/End column
            KeyCode::Char('a') if ctrl => Some(Action::HomeCol),
            KeyCode::Char('e') if ctrl => Some(Action::EndCol),
            KeyCode::Home => Some(Action::HomeCol),
            KeyCode::End => Some(Action::EndCol),

            // Edit
            KeyCode::Enter => Some(Action::EnterEdit),

            // Command prompt
            KeyCode::Char('x') if alt => Some(Action::EnterCommand),
            KeyCode::Char(':') => None, // strict

            // Save
            KeyCode::Char('s') if ctrl => Some(Action::Save),

            // Copy / Paste
            KeyCode::Char('w') if alt => Some(Action::Yank),
            KeyCode::Char('y') if ctrl => Some(Action::Paste),

            // Visual selection (set mark)
            KeyCode::Char(' ') if ctrl => Some(Action::EnterVisual),

            // Clear cell
            KeyCode::Char('d') if ctrl => Some(Action::ClearCell),
            KeyCode::Delete => Some(Action::ClearCell),

            // Plot modal
            KeyCode::Char('p') if alt => Some(Action::OpenPlot),

            // Go to last
            KeyCode::Char('>') if alt => Some(Action::GotoLast),

            // Goto prompt
            KeyCode::Char('g') if alt => Some(Action::OpenGotoPrompt),

            _ => None,
        },

        Mode::Visual => match key.code {
            KeyCode::Char('g') if ctrl => Some(Action::ExitVisual),
            KeyCode::Esc => Some(Action::ExitVisual),

            // Movement extends selection
            KeyCode::Up => Some(Action::Move(0, -1)),
            KeyCode::Down => Some(Action::Move(0, 1)),
            KeyCode::Left => Some(Action::Move(-1, 0)),
            KeyCode::Right => Some(Action::Move(1, 0)),
            KeyCode::Char('p') if ctrl => Some(Action::Move(0, -1)),
            KeyCode::Char('n') if ctrl => Some(Action::Move(0, 1)),
            KeyCode::Char('b') if ctrl => Some(Action::Move(-1, 0)),
            KeyCode::Char('f') if ctrl => Some(Action::Move(1, 0)),
            KeyCode::Char('v') if ctrl => Some(Action::Page(1)),
            KeyCode::Char('v') if alt => Some(Action::Page(-1)),
            KeyCode::PageUp => Some(Action::Page(-1)),
            KeyCode::PageDown => Some(Action::Page(1)),

            // Copy selection
            KeyCode::Char('w') if alt => Some(Action::Yank),

            _ => None,
        },

        Mode::Edit => match key.code {
            KeyCode::Char('g') if ctrl => Some(Action::Cancel),
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::CommitEdit),
            _ => None,
        },

        Mode::Command => match key.code {
            KeyCode::Char('g') if ctrl => Some(Action::Cancel),
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::ExecuteCommand),
            _ => None,
        },
    }
}
