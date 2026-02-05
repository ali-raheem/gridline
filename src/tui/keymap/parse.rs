use super::{Action, Binding, CustomKeymap, KeyCombo, Keymap, KeymapBindings};
use crossterm::event::{KeyCode, KeyModifiers};
use directories::ProjectDirs;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

const MAX_KEYMAP_FILE_BYTES: u64 = 1_048_576; // 1 MiB
const MAX_BINDINGS_PER_MODE: usize = 512;
const MAX_TOTAL_BINDINGS: usize = 1_024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeymapsFile {
    meta: Option<KeymapsMeta>,
    keymaps: Option<HashMap<String, KeymapFile>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeymapsMeta {
    default: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
            match std::fs::metadata(path) {
                Ok(meta) if meta.len() > MAX_KEYMAP_FILE_BYTES => {
                    warnings.push(format!(
                        "Refusing to read {}: file too large ({} bytes, max {})",
                        path.display(),
                        meta.len(),
                        MAX_KEYMAP_FILE_BYTES
                    ));
                }
                Ok(_) => match std::fs::read_to_string(path) {
                    Ok(content) => match toml::from_str::<KeymapsFile>(&content) {
                        Ok(parsed) => file = Some(parsed),
                        Err(err) => {
                            warnings.push(format!("Failed to parse {}: {}", path.display(), err))
                        }
                    },
                    Err(err) => {
                        warnings.push(format!("Failed to read {}: {}", path.display(), err))
                    }
                },
                Err(err) => warnings.push(format!(
                    "Failed to read metadata for {}: {}",
                    path.display(),
                    err
                )),
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
        } else if requested_name.is_some() && !is_builtin_keymap(target) {
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

    if requested_name.is_none() && default_name.is_some() && !is_builtin_keymap(target) {
        let default_exists = file
            .as_ref()
            .and_then(|f| f.keymaps.as_ref())
            .is_some_and(|keymaps| keymaps.contains_key(target));
        if !default_exists {
            warnings.push(format!(
                "Default keymap '{}' not found in {}; falling back to built-in 'vim'",
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

fn is_builtin_keymap(name: &str) -> bool {
    name.eq_ignore_ascii_case("vim") || name.eq_ignore_ascii_case("emacs")
}

fn build_custom_keymap(name: &str, entry: &KeymapFile) -> Result<CustomKeymap, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    let normal = parse_mode_bindings("normal", entry.normal.as_ref(), &mut errors);
    let visual = parse_mode_bindings("visual", entry.visual.as_ref(), &mut errors);
    let edit = parse_mode_bindings("edit", entry.edit.as_ref(), &mut errors);
    let command = parse_mode_bindings("command", entry.command.as_ref(), &mut errors);
    let total_bindings = normal.len() + visual.len() + edit.len() + command.len();
    if total_bindings > MAX_TOTAL_BINDINGS {
        errors.push(format!(
            "Too many total bindings: {} (max {})",
            total_bindings, MAX_TOTAL_BINDINGS
        ));
    }

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
    if raw.len() > MAX_BINDINGS_PER_MODE {
        errors.push(format!(
            "Too many {} bindings: {} (max {})",
            mode,
            raw.len(),
            MAX_BINDINGS_PER_MODE
        ));
        return bindings;
    }
    for (combo_str, action_str) in raw {
        match (parse_key_combo(combo_str), action_from_str(action_str)) {
            (Ok(combo), Some(action)) => {
                if bindings.iter().any(|binding| binding.combo == combo) {
                    errors.push(format!(
                        "Duplicate key '{}' in {} bindings",
                        combo.display(),
                        mode
                    ));
                    continue;
                }
                bindings.push(Binding { combo, action });
            }
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
        let mod_str = mod_str.trim_end_matches('-');
        if mod_str.is_empty() {
            return Err("missing modifier before '-'".to_string());
        }
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
    let mut seen_any = false;
    for part in input.split('-') {
        let raw = part.trim();
        if raw.is_empty() {
            return Err("empty modifier segment".to_string());
        }
        seen_any = true;
        let norm = raw.to_ascii_lowercase();
        let flag = match norm.as_str() {
            "c" | "ctrl" | "control" => KeyModifiers::CONTROL,
            "m" | "alt" | "meta" => KeyModifiers::ALT,
            "s" | "shift" => KeyModifiers::SHIFT,
            _ => {
                return Err(format!("unknown modifier '{}'", part));
            }
        };
        if modifiers.contains(flag) {
            return Err(format!("duplicate modifier '{}'", raw));
        }
        modifiers.insert(flag);
    }
    if !seen_any {
        return Err("empty modifier".to_string());
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
    fn parse_key_combo_rejects_missing_modifier_for_dash_key() {
        let err = parse_key_combo("--").unwrap_err();
        assert!(err.contains("missing modifier"));
    }

    #[test]
    fn parse_key_combo_rejects_duplicate_modifier() {
        let err = parse_key_combo("C-C-s").unwrap_err();
        assert!(err.contains("duplicate modifier"));
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

    #[test]
    fn load_keymap_rejects_oversized_file() {
        let temp_path = std::env::temp_dir().join("gridline_keymaps_large.toml");
        let oversized = "a".repeat(MAX_KEYMAP_FILE_BYTES as usize + 1);
        std::fs::write(&temp_path, oversized).expect("write oversized keymap");

        let (keymap, warnings) = load_keymap(None, Some(&temp_path));
        assert_eq!(keymap, Keymap::Vim);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("file too large") && w.contains("Refusing to read"))
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn load_keymap_rejects_excessive_bindings() {
        let temp_path = std::env::temp_dir().join("gridline_keymaps_many.toml");
        let mut content = String::from("[meta]\ndefault = \"big\"\n\n[keymaps.big]\n");
        content.push_str("description = \"too many\"\n\n[keymaps.big.normal]\n");
        for i in 0..=MAX_BINDINGS_PER_MODE {
            content.push_str(&format!("\"C-{}\" = \"save\"\n", i));
        }
        std::fs::write(&temp_path, content).expect("write keymap with too many bindings");

        let (keymap, warnings) = load_keymap(Some("big"), Some(&temp_path));
        assert_eq!(keymap, Keymap::Vim);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Too many normal bindings"))
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn load_keymap_rejects_duplicate_key_combos() {
        let temp_path = std::env::temp_dir().join("gridline_keymaps_dups.toml");
        let content = r#"
[meta]
default = "dup"

[keymaps.dup]
description = "duplicate combo"

[keymaps.dup.normal]
"C-s" = "save"
"ctrl-s" = "undo"
"#;
        std::fs::write(&temp_path, content).expect("write duplicate combo keymap");

        let (keymap, warnings) = load_keymap(Some("dup"), Some(&temp_path));
        assert_eq!(keymap, Keymap::Vim);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Duplicate key") && w.contains("normal bindings"))
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn load_keymap_warns_when_default_keymap_is_missing() {
        let temp_path = std::env::temp_dir().join("gridline_keymaps_missing_default.toml");
        let content = r#"
[meta]
default = "nonexistent"

[keymaps.vim]
description = "custom vim"
"#;
        std::fs::write(&temp_path, content).expect("write missing-default keymap");

        let (keymap, warnings) = load_keymap(None, Some(&temp_path));
        assert_eq!(keymap, Keymap::Vim);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Default keymap 'nonexistent' not found"))
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn load_keymap_requested_builtin_does_not_warn_missing_custom_entry() {
        let temp_path = std::env::temp_dir().join("gridline_keymaps_builtin_request.toml");
        let content = r#"
[meta]
default = "vim"

[keymaps.custom]
description = "custom map"
"#;
        std::fs::write(&temp_path, content).expect("write builtin request keymap");

        let (keymap, warnings) = load_keymap(Some("emacs"), Some(&temp_path));
        assert_eq!(keymap, Keymap::Emacs);
        assert!(!warnings.iter().any(|w| w.contains("not found in")));

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn load_keymap_rejects_unknown_fields() {
        let temp_path = std::env::temp_dir().join("gridline_keymaps_unknown_field.toml");
        let content = r#"
[meta]
default = "vim"
extra = "not-allowed"
"#;
        std::fs::write(&temp_path, content).expect("write unknown-field keymap");

        let (keymap, warnings) = load_keymap(None, Some(&temp_path));
        assert_eq!(keymap, Keymap::Vim);
        assert!(warnings.iter().any(|w| w.contains("Failed to parse")));

        let _ = std::fs::remove_file(&temp_path);
    }
}
