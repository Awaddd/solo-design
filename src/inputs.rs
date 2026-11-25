//! Input validation (task folder, later agent/design and freezing).

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Check the task folder and return its canonical path.
pub fn check_task(path: &Path) -> Result<PathBuf, String> {
    let shown = path.display();
    let meta = fs::symlink_metadata(path)
        .map_err(|e| format!("task folder {shown} is not accessible ({e})"))?;
    if meta.file_type().is_symlink() {
        return Err(format!("task folder {shown} must not be a symbolic link"));
    }
    if !meta.is_dir() {
        return Err(format!("task path {shown} is not a directory"));
    }
    let brief = path.join("task.md");
    let brief_meta = fs::symlink_metadata(&brief)
        .map_err(|_| format!("task folder {shown} has no task.md at its root"))?;
    if !brief_meta.file_type().is_file() {
        return Err(format!("task.md in {shown} must be a regular file"));
    }
    if brief_meta.len() == 0 {
        return Err(format!("task.md in {shown} is empty"));
    }
    path.canonicalize()
        .map_err(|e| format!("task folder {shown} cannot be resolved ({e})"))
}

/// Read-only result of validating a selection under a resource root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The resource root (holds `agents/`, `lib/`, `runs/`).
    pub root: PathBuf,
    /// The canonical task folder.
    pub task: PathBuf,
    /// The selected `agents/<name>/` directory.
    pub agent: PathBuf,
    /// The selected `lib/<slug>/` directory, when `--design` was given.
    pub design: Option<PathBuf>,
}

/// A frozen run under `<root>/runs/<run-id>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenRun {
    /// `<root>/runs/<run-id>`.
    pub run_dir: PathBuf,
    /// `<run_dir>/inputs` holding `task/`, `agent/` and (if selected) `design/`.
    pub inputs: PathBuf,
    /// `<run_dir>/outputs`, seeded from the task.
    pub outputs: PathBuf,
    /// Lowercase hex SHA-256 per frozen file, keyed by its `/`-separated path
    /// relative to `inputs` (e.g. `task/task.md`).
    pub hashes: std::collections::BTreeMap<String, String>,
}

/// Validate task, agent and design without writing anything.
pub fn validate(
    root: &Path,
    task: &Path,
    agent: &str,
    design: Option<&str>,
) -> Result<Selected, String> {
    let task = check_task(task)?;
    if root.starts_with(&task) {
        return Err(format!(
            "task folder {} contains the resource root",
            task.display()
        ));
    }
    if task.starts_with(root.join("runs")) {
        return Err(format!(
            "task folder {} is inside the runs directory",
            task.display()
        ));
    }
    check_tree(&task).map_err(|e| format!("task folder {}: {e}", task.display()))?;

    let agent_dir = root.join("agents").join(agent);
    check_dir(&agent_dir).map_err(|e| format!("agent {agent}: {e}"))?;
    check_agent_name(&agent_dir, agent)?;
    check_tree(&agent_dir).map_err(|e| format!("agent {agent}: {e}"))?;

    let design = match design {
        None => None,
        Some(slug) => {
            let dir = root.join("lib").join(slug);
            check_dir(&dir).map_err(|e| format!("design {slug}: {e}"))?;
            for name in ["DESIGN.md", "globals.css"] {
                let meta = fs::symlink_metadata(dir.join(name))
                    .map_err(|_| format!("design {slug}: {name} is missing"))?;
                if !meta.file_type().is_file() {
                    return Err(format!("design {slug}: {name} must be a regular file"));
                }
            }
            check_tree(&dir).map_err(|e| format!("design {slug}: {e}"))?;
            Some(dir)
        }
    };
    Ok(Selected {
        root: root.to_owned(),
        task,
        agent: agent_dir,
        design,
    })
}

/// The path must be a real directory (not a link, not missing).
fn check_dir(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|_| "not found".to_owned())?;
    if meta.file_type().is_symlink() {
        return Err("must not be a symbolic link".to_owned());
    }
    if !meta.is_dir() {
        return Err("is not a directory".to_owned());
    }
    Ok(())
}

/// `agent.md` must be a regular file whose front-matter `name` equals the selector.
fn check_agent_name(dir: &Path, agent: &str) -> Result<(), String> {
    let file = dir.join("agent.md");
    let mut text = String::new();
    open_regular(&file)
        .and_then(|mut f| f.read_to_string(&mut text).map_err(|e| e.to_string()))
        .map_err(|e| format!("agent {agent}: agent.md is unusable ({e})"))?;
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Err(format!("agent {agent}: agent.md has no front-matter"));
    }
    let mut front = String::new();
    let mut closed = false;
    for line in lines {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        front.push_str(line);
        front.push('\n');
    }
    if !closed {
        return Err(format!(
            "agent {agent}: agent.md front-matter is not closed"
        ));
    }
    let value: serde_yaml::Value = serde_yaml::from_str(&front)
        .map_err(|e| format!("agent {agent}: front-matter is not valid YAML ({e})"))?;
    match value.get("name").and_then(serde_yaml::Value::as_str) {
        Some(name) if name == agent => Ok(()),
        Some(name) => Err(format!(
            "agent {agent}: front-matter name {name:?} does not match the selector (mismatch)"
        )),
        None => Err(format!("agent {agent}: front-matter has no name")),
    }
}

/// Every entry under `dir` must be a directory or a regular file.
fn check_tree(dir: &Path) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {} ({e})", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {} ({e})", dir.display()))?;
        let path = entry.path();
        let kind = fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot inspect {} ({e})", path.display()))?
            .file_type();
        if kind.is_dir() {
            check_tree(&path)?;
        } else if !kind.is_file() {
            return Err(format!(
                "{} is a symbolic link or special file; only regular files are allowed",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Open a regular file without following links or blocking on special files.
fn open_regular(path: &Path) -> Result<fs::File, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("cannot open {} ({e})", path.display()))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("cannot inspect {} ({e})", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(file)
}

/// Copies a tree with new files only, hashing what it reads.
struct Copier<'a> {
    hashes: &'a mut BTreeMap<String, String>,
    /// Record a hash per copied file (frozen inputs); off for output seeding.
    record: bool,
}

impl Copier<'_> {
    fn copy_dir(&mut self, from: &Path, to: &Path, rel: &str) -> Result<(), String> {
        fs::create_dir(to).map_err(|e| format!("cannot create {} ({e})", to.display()))?;
        let mut names: Vec<_> = fs::read_dir(from)
            .map_err(|e| format!("cannot read {} ({e})", from.display()))?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("cannot read {} ({e})", from.display()))?;
        names.sort();
        for name in names {
            let (src, dst) = (from.join(&name), to.join(&name));
            let child = format!("{rel}/{}", name.to_string_lossy());
            let kind = fs::symlink_metadata(&src)
                .map_err(|e| format!("cannot inspect {} ({e})", src.display()))?
                .file_type();
            if kind.is_dir() {
                self.copy_dir(&src, &dst, &child)?;
            } else {
                self.copy_file(&src, &dst, &child)?;
            }
        }
        fs::set_permissions(to, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("cannot set permissions on {} ({e})", to.display()))
    }

    fn copy_file(&mut self, from: &Path, to: &Path, rel: &str) -> Result<(), String> {
        let mut source = open_regular(from)?;
        let executable = source
            .metadata()
            .map(|m| m.permissions().mode() & 0o100 != 0)
            .unwrap_or(false);
        let mut dest = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|e| format!("cannot create {} ({e})", to.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = source
                .read(&mut buf)
                .map_err(|e| format!("cannot read {} ({e})", from.display()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            dest.write_all(&buf[..n])
                .map_err(|e| format!("cannot write {} ({e})", to.display()))?;
        }
        let mode = if executable { 0o755 } else { 0o644 };
        dest.set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|e| format!("cannot set permissions on {} ({e})", to.display()))?;
        if self.record {
            let hex: String = hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            self.hashes.insert(rel.to_owned(), hex);
        }
        Ok(())
    }
}

/// Freeze a validated selection into a new run directory and seed its outputs.
/// On failure the run directory created here is removed.
pub fn freeze(selected: &Selected) -> Result<FrozenRun, String> {
    let runs = selected.root.join("runs");
    fs::create_dir_all(&runs).map_err(|e| format!("cannot create {} ({e})", runs.display()))?;
    let run_dir = new_run_dir(&runs)?;
    match freeze_into(selected, &run_dir) {
        Ok(run) => Ok(run),
        Err(e) => {
            let _ = fs::remove_dir_all(&run_dir);
            Err(e)
        }
    }
}

/// Create a fresh `<runs>/<id>` directory; ids are time- and process-based.
fn new_run_dir(runs: &Path) -> Result<PathBuf, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for attempt in 0..100u32 {
        let dir = runs.join(format!("{now:x}-{}-{attempt}", std::process::id()));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "cannot create run directory {} ({e})",
                    dir.display()
                ))
            }
        }
    }
    Err("cannot find an unused run directory name".to_owned())
}

fn freeze_into(selected: &Selected, run_dir: &Path) -> Result<FrozenRun, String> {
    let inputs = run_dir.join("inputs");
    let outputs = run_dir.join("outputs");
    fs::create_dir(&inputs).map_err(|e| format!("cannot create {} ({e})", inputs.display()))?;
    let mut hashes = BTreeMap::new();
    let mut hashed = Copier {
        hashes: &mut hashes,
        record: true,
    };
    hashed.copy_dir(&selected.task, &inputs.join("task"), "task")?;
    hashed.copy_dir(&selected.agent, &inputs.join("agent"), "agent")?;
    if let Some(design) = &selected.design {
        hashed.copy_dir(design, &inputs.join("design"), "design")?;
    }
    fs::set_permissions(&inputs, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot set permissions on {} ({e})", inputs.display()))?;

    // Seed outputs from the task: existing/index.html and assets/.
    fs::create_dir(&outputs).map_err(|e| format!("cannot create {} ({e})", outputs.display()))?;
    let mut scratch = BTreeMap::new();
    let mut seed = Copier {
        hashes: &mut scratch,
        record: false,
    };
    let index = selected.task.join("existing/index.html");
    if fs::symlink_metadata(&index).is_ok_and(|m| m.file_type().is_file()) {
        seed.copy_file(&index, &outputs.join("index.html"), "index.html")?;
    }
    let assets = selected.task.join("assets");
    if fs::symlink_metadata(&assets).is_ok_and(|m| m.file_type().is_dir()) {
        seed.copy_dir(&assets, &outputs.join("assets"), "assets")?;
    }
    fs::set_permissions(&outputs, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot set permissions on {} ({e})", outputs.display()))?;
    Ok(FrozenRun {
        run_dir: run_dir.to_owned(),
        inputs,
        outputs,
        hashes,
    })
}

/// `validate` then `freeze`; creates nothing when validation fails.
pub fn prepare(
    root: &Path,
    task: &Path,
    agent: &str,
    design: Option<&str>,
) -> Result<FrozenRun, String> {
    freeze(&validate(root, task, agent, design)?)
}
