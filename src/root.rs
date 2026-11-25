//! Resource root resolution (agents, design packs, runs).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn non_empty(env: &dyn Fn(&str) -> Option<OsString>, key: &str) -> Option<PathBuf> {
    env(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Resolve the resource root from `SOLO_DESIGN_ROOT` or the documented default
/// (`${XDG_DATA_HOME:-$HOME/.local/share}/solo-design`), reading variables
/// through `env`. `with_design` requires the design-pack tree.
pub fn resource_root(
    env: &dyn Fn(&str) -> Option<OsString>,
    with_design: bool,
) -> Result<PathBuf, String> {
    let root = if let Some(root) = non_empty(env, "SOLO_DESIGN_ROOT") {
        root
    } else if let Some(data) = non_empty(env, "XDG_DATA_HOME") {
        data.join("solo-design")
    } else if let Some(home) = non_empty(env, "HOME") {
        home.join(".local/share/solo-design")
    } else {
        return Err(
            "cannot locate the resource root: set SOLO_DESIGN_ROOT (or HOME / XDG_DATA_HOME)"
                .to_owned(),
        );
    };
    let root = root.canonicalize().map_err(|e| {
        format!(
            "resource root {} is not usable ({e}); set SOLO_DESIGN_ROOT",
            root.display()
        )
    })?;
    require_dir(&root, "agents")?;
    if with_design {
        require_dir(&root, "lib")?;
    }
    Ok(root)
}

fn require_dir(root: &Path, name: &str) -> Result<(), String> {
    if root.join(name).is_dir() {
        Ok(())
    } else {
        Err(format!(
            "resource root {} has no {name}/ directory",
            root.display()
        ))
    }
}
