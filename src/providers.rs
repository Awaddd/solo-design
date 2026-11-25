//! The provider adapter: the only place naming providers, their key
//! variables and endpoints.

use std::path::{Path, PathBuf};

/// A supported provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// The configured provider name.
    pub name: &'static str,
    /// Pi's provider id (the `--model <pi_provider>/<model>` prefix).
    pub pi_provider: &'static str,
    /// The env var Pi reads the key from inside the container.
    pub key_env: &'static str,
}

const PROVIDERS: &[Provider] = &[
    Provider {
        name: "openrouter",
        pi_provider: "openrouter",
        key_env: "OPENROUTER_API_KEY",
    },
    Provider {
        name: "anthropic",
        pi_provider: "anthropic",
        key_env: "ANTHROPIC_API_KEY",
    },
];

/// The adapter for `name`; an unknown name is an error listing the supported ones.
pub fn lookup(name: &str) -> Result<&'static Provider, String> {
    PROVIDERS.iter().find(|p| p.name == name).ok_or_else(|| {
        format!(
            "unknown provider {name:?}; supported providers: {}",
            supported().join(", ")
        )
    })
}

/// Every supported provider name.
pub fn supported() -> Vec<&'static str> {
    PROVIDERS.iter().map(|p| p.name).collect()
}

/// Preflight for the configured key file: it must be set, exist, be a regular
/// file and not be accessible to group or others. Only metadata is read, never
/// the contents. Returns the canonical absolute path, which is what gets
/// mounted (a relative source would be read by Docker as a volume name).
pub fn check_key_file(path: Option<&Path>) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let path = path.ok_or_else(|| {
        "the provider's key file is not configured; set SOLO_DESIGN_KEY_FILE or add the \
         provider to \"keys\" in the config file"
            .to_owned()
    })?;
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("key file {} is not accessible ({e})", path.display()))?;
    if !meta.is_file() {
        return Err(format!("key file {} is not a regular file", path.display()));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "key file {} is accessible by group or others; make it owner-only (chmod 600)",
            path.display()
        ));
    }
    path.canonicalize()
        .map_err(|e| format!("key file {} is not accessible ({e})", path.display()))
}
