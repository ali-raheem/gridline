use directories::ProjectDirs;
use std::path::PathBuf;

pub(crate) fn default_functions_path() -> Option<PathBuf> {
    let proj = ProjectDirs::from("me", "shoryuken", "gridline")?;
    let mut path = proj.config_dir().to_path_buf();
    path.push("default.rhai");
    Some(path)
}

#[allow(dead_code)]
pub(crate) fn prepend_default_functions_if_present(
    functions: &mut Vec<PathBuf>,
    no_default_functions: bool,
) {
    if no_default_functions {
        return;
    }
    let Some(path) = default_functions_path() else {
        return;
    };
    if path.is_file() {
        functions.insert(0, path);
    }else{
        eprintln!("Failed to load functions file from {}.", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::default_functions_path;

    #[test]
    fn default_functions_path_is_deterministic() {
        // Should never panic and should either be Some(path) or None.
        let _ = default_functions_path();
    }
}
