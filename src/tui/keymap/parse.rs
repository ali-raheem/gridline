use super::{Action, Binding, CustomKeymap, KeyCombo, Keymap, KeymapBindings};
use crossterm::event::{KeyCode, KeyModifiers};
use directories::ProjectDirs;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

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

pub fn load_keymap(
    requested: Option<&str>,
    keymap_file: Option<&PathBuf>,
) -> (Keymap, Vec<String>) {
    let mut warnings: Vec<String> = Vec::new();
    let config_path = keymap_file.cloned().or_else(user_keymaps_path);
    let mut file: Option<KeymapsFile> = None;

    if let Some(path) = config_path.as_ref() {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match toml::from_str::<KeymapsFile>(&content) {
                    Ok(parsed) => file = Some(parsed),
                    Err(err) => {
                        warnings.push(format!("Failed to parse {}: {}", path.display(), err))
                    }
                },
                Err(err) => warnings.push(format!("Failed to read {}: {}", path.display(), err)),
            }
        } else if keymap_file.is_some() {
            warnings.push(format!("Keymap file not found: {}", path.display()));
        }
    }

    let requested_name = requested.map(|name| name.trim()).filter(|s| !s.is_empty());
    let default_name = file
        .as_ref()
        .and_then(|f| f.meta.as_ref())
        .and_then(|m| m.default.as_ref())
        .map(|s| s.as_str());
    let target = requested_name.or(default_name).unwrap_or("vim");

    if let Some(file) = file.as_ref()
        && let Some(keymaps) = file.keymaps.as_ref()
    {
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
    if let Some(ch) = parse_single_char(trimmed) {
        return Ok(KeyCombo {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::empty(),
        });
    }

    let (mods, key_part) = if !trimmed.contains('-') {
        (KeyModifiers::empty(), trimmed)
    } else if let Some(mod_str) = trimmed.strip_suffix('-') {
        let modifiers = parse_modifiers(mod_str)?;
        (modifiers, "-")
    } else {
        let mut split = trimmed.rsplitn(2, '-');
        let key_part = split.next().ok_or_else(|| "empty key".to_string())?;
        let mod_str = split.next().unwrap_or_default();
        let modifiers = parse_modifiers(mod_str)?;
        (modifiers, key_part)
    };

    let key = parse_key_code(key_part)?;
    Ok(KeyCombo {
        code: key,
        modifiers: mods,
    })
}

fn parse_modifiers(input: &str) -> Result<KeyModifiers, String> {
    let mut modifiers = KeyModifiers::empty();
    for part in input.split('-') {
        if part.trim().is_empty() {
            continue;
        }
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
    Ok(modifiers)
}

fn parse_key_code(input: &str) -> Result<KeyCode, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty key".to_string());
    }
    if let Some(ch) = parse_single_char(trimmed) {
        return Ok(KeyCode::Char(ch));
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
        "space" | "spc" => Ok(KeyCode::Char(' ')),
        "dash" | "minus" => Ok(KeyCode::Char('-')),
        "plus" => Ok(KeyCode::Char('+')),
        "greater" => Ok(KeyCode::Char('>')),
        "less" => Ok(KeyCode::Char('<')),
        "comma" => Ok(KeyCode::Char(',')),
        "period" | "dot" => Ok(KeyCode::Char('.')),
        "slash" => Ok(KeyCode::Char('/')),
        "backslash" => Ok(KeyCode::Char('\\')),
        "semicolon" => Ok(KeyCode::Char(';')),
        "quote" | "apostrophe" => Ok(KeyCode::Char('\'')),
        "doublequote" => Ok(KeyCode::Char('"')),
        "backtick" | "grave" => Ok(KeyCode::Char('`')),
        "lbracket" | "leftbracket" => Ok(KeyCode::Char('[')),
        "rbracket" | "rightbracket" => Ok(KeyCode::Char(']')),
        "equal" => Ok(KeyCode::Char('=')),
        _ => Err(format!("unknown key '{}'", input)),
    }
}

fn parse_single_char(input: &str) -> Option<char> {
    let mut chars = input.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        Some(ch)
    } else {
        None
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
    fn parse_key_combo_dash() {
        let combo = parse_key_combo("-").expect("combo");
        assert_eq!(combo.code, KeyCode::Char('-'));
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn parse_key_combo_ctrl_dash() {
        let combo = parse_key_combo("C--").expect("combo");
        assert_eq!(combo.code, KeyCode::Char('-'));
        assert!(combo.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn parse_key_combo_unicode_char() {
        let combo = parse_key_combo("é").expect("combo");
        assert_eq!(combo.code, KeyCode::Char('é'));
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn parse_key_combo_ctrl_unicode_char() {
        let combo = parse_key_combo("C-ø").expect("combo");
        assert_eq!(combo.code, KeyCode::Char('ø'));
        assert!(combo.modifiers.contains(KeyModifiers::CONTROL));
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

    #[test]
    fn load_keymap_falls_back_with_warning() {
        let temp_path = std::env::temp_dir().join("gridline_keymaps_test.toml");
        let content = r#"
[meta]
default = "vim"

[keymaps.vim]
description = "Vim defaults"
"#;
        std::fs::write(&temp_path, content).expect("write temp keymap");

        let (keymap, warnings) = load_keymap(Some("nonexistent"), Some(&temp_path));
        assert_eq!(keymap, Keymap::Vim);
        assert!(!warnings.is_empty());

        let _ = std::fs::remove_file(&temp_path);
    }
}
