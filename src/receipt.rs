//! The host-owned run receipt: `<run_dir>/receipt.json`, allow-listed fields only.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// How a run ended, as recorded in the receipt (`"succeeded"`, `"failed"`,
/// `"interrupted"`, `"timed_out"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Succeeded,
    Failed,
    Interrupted,
    TimedOut,
}

/// The closed allow-list of receipt keys. `effort`, `tokens` and `usage` are present
/// when the engine reports them.
pub const FIELDS: &[&str] = &[
    "status",
    "exit_code",
    "signal",
    "output_path",
    "agent",
    "design",
    "engine",
    "model",
    "provider",
    "effort",
    "tokens",
    "usage",
    "failure",
    "started_at",
    "ended_at",
    "input_hashes",
    "output_hashes",
];

/// The receipt as written: a closed struct, so nothing outside [`FIELDS`] can
/// reach the file. Times are unix seconds.
#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub status: Status,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub output_path: String,
    pub agent: String,
    pub design: Option<String>,
    pub engine: String,
    pub model: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::engine::Usage>,
    /// The failure kind the engine classified; `null` unless the run failed.
    pub failure: Option<String>,
    pub started_at: u64,
    pub ended_at: u64,
    pub input_hashes: BTreeMap<String, String>,
    pub output_hashes: BTreeMap<String, String>,
}

/// Write the receipt atomically with mode 0600: a new temp file, fsync, rename.
pub fn write(path: &Path, receipt: &Receipt) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(receipt)
        .map_err(|e| format!("cannot serialise the receipt ({e})"))?;
    text.push('\n');
    let temp = path.with_extension("json.tmp");
    let _ = fs::remove_file(&temp);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| format!("cannot create {} ({e})", temp.display()))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .and_then(|()| file.write_all(text.as_bytes()))
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("cannot write {} ({e})", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("cannot write {} ({e})", path.display())
    })
}

/// SHA-256 of every regular file under `root` (links and special files are
/// skipped, never followed), keyed by `/`-separated path relative to `root`.
pub fn hash_tree(root: &Path) -> BTreeMap<String, String> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = fs::symlink_metadata(&path).map(|m| m.file_type()) else {
                continue;
            };
            if kind.is_dir() {
                visit(root, &path, out);
            } else if kind.is_file() {
                if let Some(hex) = hash_file(&path) {
                    let rel = path.strip_prefix(root).unwrap_or(&path);
                    out.insert(rel.to_string_lossy().replace('\\', "/"), hex);
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn hash_file(path: &Path) -> Option<String> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}
