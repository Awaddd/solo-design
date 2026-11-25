//! Shared fixture helpers for integration tests.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A temp resource root (`agents/`, `lib/`) plus a caller task folder outside it.
pub struct Fixture {
    pub _dir: TempDir,
    pub root: PathBuf,
    pub task: PathBuf,
}

impl Fixture {
    /// Root with `agents/` and `lib/`, and a task folder with a brief.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("create isolated fixture");
        let root = dir.path().join("resources");
        let task = dir.path().join("caller/task");
        fs::create_dir_all(root.join("agents")).unwrap();
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(&task).unwrap();
        fs::write(task.join("task.md"), "Design a landing page.\n").unwrap();
        let root = root.canonicalize().unwrap();
        let task = task.canonicalize().unwrap();
        Self {
            _dir: dir,
            root,
            task,
        }
    }

    /// Write `text` at `path` under the resource root.
    pub fn put(&self, path: impl AsRef<Path>, text: impl AsRef<[u8]>) -> PathBuf {
        write(&self.root.join(path), text)
    }

    /// Write `text` at `path` under the task folder.
    pub fn task_file(&self, path: impl AsRef<Path>, text: impl AsRef<[u8]>) -> PathBuf {
        write(&self.task.join(path), text)
    }

    /// An agent whose front-matter name is `name`, with one skill and one rule.
    pub fn agent(&self, name: &str) -> PathBuf {
        self.agent_named(name, name)
    }

    /// `agents/<dir>/agent.md` whose front-matter says `name: <front_name>`.
    pub fn agent_named(&self, dir: &str, front_name: &str) -> PathBuf {
        self.put(
            format!("agents/{dir}/agent.md"),
            format!("---\nname: {front_name}\ndescription: Fixture agent.\n---\nRole body.\n"),
        );
        self.put(format!("agents/{dir}/skills/layout.md"), "Skill text.\n");
        self.put(format!("agents/{dir}/rules/style.md"), "Rule text.\n");
        self.root.join("agents").join(dir)
    }

    /// `lib/<slug>/` with DESIGN.md, globals.css and a font file.
    pub fn design(&self, slug: &str) -> PathBuf {
        self.put(format!("lib/{slug}/DESIGN.md"), format!("# {slug}\n"));
        self.put(format!("lib/{slug}/globals.css"), ":root { --x: 1; }\n");
        self.put(format!("lib/{slug}/fonts/a.woff2"), [0u8, 1, 2, 3]);
        self.root.join("lib").join(slug)
    }

    /// Entries under `<root>/runs` (empty when it does not exist).
    pub fn runs(&self) -> Vec<PathBuf> {
        match fs::read_dir(self.root.join("runs")) {
            Ok(entries) => entries.map(|e| e.unwrap().path()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

fn write(target: &Path, text: impl AsRef<[u8]>) -> PathBuf {
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
    target.to_path_buf()
}

/// Every regular file under `root`, keyed by `/`-separated relative path.
pub fn file_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, current: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                visit(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let key = rel.to_str().unwrap().replace('\\', "/");
                out.insert(key, fs::read(&path).unwrap_or_default());
            }
        }
    }
    let mut out = BTreeMap::new();
    if root.is_dir() {
        visit(root, root, &mut out);
    }
    out
}

/// Every entry (files, dirs, links) under `root` with its kind and bytes, for
/// "unchanged" comparisons.
pub fn snapshot(root: &Path) -> BTreeMap<String, (String, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, out: &mut BTreeMap<String, (String, Vec<u8>)>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let key = path.strip_prefix(root).unwrap().display().to_string();
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&path).unwrap().display().to_string();
                out.insert(key, ("link".into(), target.into_bytes()));
            } else if meta.is_dir() {
                out.insert(key, ("dir".into(), Vec::new()));
                visit(root, &path, out);
            } else if meta.is_file() {
                out.insert(key, ("file".into(), fs::read(&path).unwrap()));
            } else {
                out.insert(key, ("special".into(), Vec::new()));
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

// ---------------------------------------------------------------------------
// Runtime fixtures: a test-only engine and a fake `docker` program.

use solo_design::engine::{
    Auth, AuthMount, Classifier, Engine, Layout, Outcome, Progress, RunContext, Stream, Usage,
};
use std::sync::{Arc, Mutex};

/// A test-only engine. Lines `<marker> <x>` become progress `step <x>`; a line
/// `<marker>-RESULT ok` makes the run a success; `<marker>-USAGE <in> <out>`
/// reports usage.
#[derive(Clone)]
pub struct TestEngine {
    pub name: String,
    pub image: String,
    pub auth: Auth,
    pub layout: Layout,
    pub marker: String,
    /// Every (model, provider) the engine was handed, in order.
    pub seen: Arc<Mutex<Vec<(String, String)>>>,
    /// The classifier panics on a line equal to this (host failure fixture).
    pub panic_on: Option<String>,
}

impl TestEngine {
    pub fn new(name: &str, auth_host: &Path) -> Self {
        Self {
            name: name.to_owned(),
            image: format!("registry.invalid/{name}:fixture"),
            auth: Auth {
                mount: Some(AuthMount {
                    host: auth_host.to_path_buf(),
                    container: format!("/{name}/auth"),
                }),
                env: vec![format!(
                    "{}_TOKEN",
                    name.to_ascii_uppercase().replace('-', "_")
                )],
            },
            layout: Layout {
                task: format!("/{name}/task"),
                agent: format!("/{name}/agent"),
                design: format!("/{name}/design"),
                outputs: format!("/{name}/outputs"),
                workdir: format!("/{name}/outputs"),
            },
            marker: format!("{}-PROGRESS", name.to_ascii_uppercase()),
            seen: Arc::new(Mutex::new(Vec::new())),
            panic_on: None,
        }
    }

    /// The container command this engine produces for `model`/`provider`.
    pub fn expected_argv(&self, model: &str, provider: &str) -> Vec<String> {
        vec![
            format!("{}-cli", self.name),
            "--model".to_owned(),
            model.to_owned(),
            "--provider".to_owned(),
            provider.to_owned(),
            "--task".to_owned(),
            self.layout.task.clone(),
        ]
    }
}

impl Engine for TestEngine {
    fn name(&self) -> &str {
        &self.name
    }
    fn image(&self) -> String {
        self.image.clone()
    }
    fn auth(&self, _run: &RunContext) -> Auth {
        self.auth.clone()
    }
    fn layout(&self) -> Layout {
        self.layout.clone()
    }
    fn argv(&self, run: &RunContext) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .push((run.model.to_owned(), run.provider.to_owned()));
        self.expected_argv(run.model, run.provider)
    }
    fn classifier(&self, _run: &RunContext) -> Box<dyn Classifier> {
        Box::new(TestClassifier {
            marker: self.marker.clone(),
            ok: false,
            effort: None,
            tokens: None,
            usage: None,
            panic_on: self.panic_on.clone(),
        })
    }
}

struct TestClassifier {
    marker: String,
    ok: bool,
    effort: Option<String>,
    tokens: Option<u64>,
    usage: Option<Usage>,
    panic_on: Option<String>,
}

impl Classifier for TestClassifier {
    fn line(&mut self, _stream: Stream, line: &str) -> Option<Progress> {
        if self.panic_on.as_deref() == Some(line) {
            panic!("fixture classifier failure");
        }
        if let Some(effort) = line.strip_prefix(&format!("{}-EFFORT ", self.marker)) {
            self.effort = Some(effort.to_owned());
            return None;
        }
        if let Some(usage) = line.strip_prefix(&format!("{}-USAGE ", self.marker)) {
            let mut n = usage.split(' ').map(|v| v.parse().ok());
            self.usage = Some(Usage {
                input_tokens: n.next().flatten(),
                output_tokens: n.next().flatten(),
                ..Usage::default()
            });
            return None;
        }
        if let Some(tokens) = line.strip_prefix(&format!("{}-TOKENS ", self.marker)) {
            self.tokens = tokens.parse().ok();
            return None;
        }
        if line == format!("{}-RESULT ok", self.marker) {
            self.ok = true;
            return Some(Progress("result ok".to_owned()));
        }
        line.strip_prefix(&format!("{} ", self.marker))
            .map(|rest| Progress(format!("step {rest}")))
    }
    fn finish(&mut self, _code: Option<i32>, _signal: Option<i32>) -> Outcome {
        Outcome {
            success: self.ok,
            effort: self.effort.clone(),
            tokens: self.tokens,
            usage: self.usage.clone(),
            ..Outcome::default()
        }
    }
}

/// What the fake `docker` does when asked to `run` a container.
#[derive(Clone, Default)]
pub struct Script {
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub exit: i32,
}

/// A fake `docker` program in its own temp dir. Every invocation appends its
/// argv (one arg per line, then `<<end>>`) to a log; `run` prints the script's
/// lines and exits with its code; anything else exits 0.
pub struct FakeDocker {
    pub _dir: TempDir,
    pub program: PathBuf,
    pub log: PathBuf,
}

impl FakeDocker {
    pub fn new(script: &Script) -> Self {
        Self::sleeping(script, 0)
    }

    /// Like `new`, but a `run` keeps running for `secs` after printing its
    /// lines (`exec sleep`, so killing the client kills the sleeper).
    pub fn sleeping(script: &Script, secs: u32) -> Self {
        Self::with_setup(script, secs, "")
    }

    /// Like `sleeping`, running the shell lines `setup` first in a `run` (the
    /// fake container's own work, e.g. writing into a mounted folder).
    pub fn with_setup(script: &Script, secs: u32, setup: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("docker");
        let log = dir.path().join("docker.log");
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let mut body = format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> {log}; done\necho '<<end>>' >> {log}\n\
             if [ \"$1\" = run ] || {{ [ \"$1\" = container ] && [ \"$2\" = run ]; }}; then\n",
            log = quote(log.to_str().unwrap())
        );
        for line in setup.lines() {
            body += &format!("  {line}\n");
        }
        for line in &script.stdout {
            body += &format!("  printf '%s\\n' {}\n", quote(line));
        }
        for line in &script.stderr {
            body += &format!("  printf '%s\\n' {} >&2\n", quote(line));
        }
        if secs > 0 {
            body += &format!("  exec sleep {secs}\n");
        }
        body += &format!("  exit {}\nfi\nexit 0\n", script.exit);
        write_executable(&program, &body);
        Self {
            _dir: dir,
            program,
            log,
        }
    }

    /// Every invocation's argv so far (without the program name).
    pub fn invocations(&self) -> Vec<Vec<String>> {
        read_invocations(&self.log)
    }
}

pub fn read_invocations(log: &Path) -> Vec<Vec<String>> {
    let text = fs::read_to_string(log).unwrap_or_default();
    let mut out = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        if line == "<<end>>" {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(line.to_owned());
        }
    }
    out
}

/// Whether an invocation launches a container.
pub fn is_launch(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("run")
        || (args.first().map(String::as_str) == Some("container")
            && args.get(1).map(String::as_str) == Some("run"))
}

/// Host paths bound into the container by a launch argv (`-v`/`--volume`
/// `src:dst[:opts]` and `--mount type=bind,source=…`).
pub fn bind_sources(args: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let (flag, value) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_owned())),
            _ => (arg, None),
        };
        if matches!(flag, "-v" | "--volume" | "--mount") {
            let value = match value {
                Some(v) => v,
                None => {
                    i += 1;
                    args.get(i).cloned().unwrap_or_default()
                }
            };
            if flag == "--mount" {
                if !value.split(',').any(|kv| kv == "type=tmpfs") {
                    for kv in value.split(',') {
                        if let Some(("source" | "src", src)) = kv.split_once('=') {
                            out.push(PathBuf::from(src));
                        }
                    }
                }
            } else {
                out.push(PathBuf::from(value.split(':').next().unwrap()));
            }
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Running a whole job (receipt, interruption, host failure) under a time limit.

use solo_design::config::ModelConfig;
use solo_design::inputs::FrozenRun;
use solo_design::runtime::{execute, Finished, Job};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// When the test raises the injected interruption flag.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    Never,
    /// Before `execute` is called (the run directory already exists).
    BeforeStart,
    /// Once the fake docker has logged the container launch.
    AfterLaunch,
}

pub struct Executed {
    /// `Err` holds the error, or `"panicked: …"` if `execute` panicked.
    pub result: Result<Finished, String>,
    pub reports: Vec<String>,
    pub invocations: Vec<Vec<String>>,
}

pub const TIME_LIMIT: Duration = Duration::from_secs(30);

pub fn execute_job(
    frozen: &FrozenRun,
    agent: &str,
    design: Option<&str>,
    engine: &TestEngine,
    config: &ModelConfig,
    docker: &FakeDocker,
    interrupt: Interrupt,
) -> Executed {
    execute_with(
        frozen, agent, design, engine, config, None, docker, interrupt,
    )
}

/// `execute_job` for any engine, with an optional configured key file.
#[allow(clippy::too_many_arguments)]
pub fn execute_with<E: Engine + Clone + Send + 'static>(
    frozen: &FrozenRun,
    agent: &str,
    design: Option<&str>,
    engine: &E,
    config: &ModelConfig,
    key_file: Option<&Path>,
    docker: &FakeDocker,
    interrupt: Interrupt,
) -> Executed {
    execute_timed(
        frozen, agent, design, engine, config, None, key_file, None, docker, interrupt,
    )
}

/// `execute_with` plus an optional run timeout.
#[allow(clippy::too_many_arguments)]
pub fn execute_timed<E: Engine + Clone + Send + 'static>(
    frozen: &FrozenRun,
    agent: &str,
    design: Option<&str>,
    engine: &E,
    config: &ModelConfig,
    effort: Option<&str>,
    key_file: Option<&Path>,
    timeout: Option<Duration>,
    docker: &FakeDocker,
    interrupt: Interrupt,
) -> Executed {
    let (frozen, engine, config) = (frozen.clone(), engine.clone(), config.clone());
    let (agent, design) = (agent.to_owned(), design.map(str::to_owned));
    let key_file = key_file.map(Path::to_path_buf);
    let effort = effort.map(str::to_owned);
    let program = docker.program.clone();
    let flag = Arc::new(AtomicBool::new(interrupt == Interrupt::BeforeStart));
    let reports = Arc::new(Mutex::new(Vec::new()));
    let (flag_t, sink) = (flag.clone(), reports.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let job = Job {
                frozen: &frozen,
                agent: &agent,
                design: design.as_deref(),
                engine: &engine,
                config: &config,
                effort: effort.as_deref(),
                key_file: key_file.as_deref(),
                timeout,
            };
            let mut report = |line: &str| sink.lock().unwrap().push(line.to_owned());
            execute(&job, &program, &flag_t, &mut report)
        }));
        let result = outcome.unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            Err(format!("panicked: {message}"))
        });
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + TIME_LIMIT;
    let result = loop {
        if interrupt == Interrupt::AfterLaunch
            && !flag.load(Ordering::SeqCst)
            && docker.invocations().iter().any(|a| is_launch(a))
        {
            flag.store(true, Ordering::SeqCst);
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => break result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                assert!(
                    Instant::now() < deadline,
                    "execute did not finish within the time limit"
                );
            }
            Err(e) => panic!("runtime thread vanished: {e}"),
        }
    };
    let reports = reports.lock().unwrap().clone();
    Executed {
        result,
        reports,
        invocations: docker.invocations(),
    }
}

/// Parse `receipt.json`.
pub fn read_receipt(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("receipt {} unreadable: {e}", path.display()));
    match serde_json::from_str(&text) {
        Ok(serde_json::Value::Object(map)) => map,
        other => panic!("receipt is not a JSON object: {other:?}"),
    }
}

/// Lowercase hex SHA-256 of every regular file under `root`, by relative path.
pub fn hashes(root: &Path) -> BTreeMap<String, String> {
    use sha2::{Digest, Sha256};
    file_tree(root)
        .into_iter()
        .map(|(path, bytes)| {
            let hex = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            (path, hex)
        })
        .collect()
}

/// Write an executable script without ever holding a write fd in this
/// process: a concurrent fork() in another test thread would inherit such an
/// fd until its exec, and exec'ing the script meanwhile fails with ETXTBSY
/// ("Text file busy"). A waited-on `sh` child writes and chmods it instead.
pub fn write_executable(path: &Path, body: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("/bin/sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn sh to write a fixture script");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .expect("send fixture script");
    let status = child.wait().expect("wait for fixture script writer");
    assert!(
        status.success(),
        "writing fixture script {} failed",
        path.display()
    );
}

// --- Public tree: manifest and deny-list -----------------------------------

pub const DENY_FILE: &str = "scripts/private-deny.sha256";

/// The deny-list hashes in `<root>/scripts/private-deny.sha256`: SHA-256 (hex)
/// of lowercased private names, whole tokens or two adjacent tokens joined by
/// `-`. One hash per line, `#` comments. Empty when the file is absent (the
/// public tree); when git lists it (the archive) it must hold a hash, so the
/// archive's scan cannot go vacuous.
pub fn deny_hashes(root: &Path) -> Vec<String> {
    let text = fs::read_to_string(root.join(DENY_FILE)).unwrap_or_default();
    let hashes = parse_hashes(&text);
    if git_listed(root).iter().any(|f| f == DENY_FILE) {
        assert!(!hashes.is_empty(), "{DENY_FILE} holds at least one hash");
    }
    hashes
}

/// The 64-hex lines of a deny-list file, lowercased, comments skipped.
pub fn parse_hashes(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            l.split('#')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .filter(|l| l.len() == 64 && l.chars().all(|c| c.is_ascii_hexdigit()))
        .collect()
}

/// The deny-list hash of `name` (lowercased first).
pub fn deny_hash(name: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(name.to_ascii_lowercase().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether `line` holds a token, or a `-`-joined pair of adjacent tokens,
/// whose hash is in `hashes`. Tokens are runs of ASCII letters and digits.
pub fn names_denied(line: &str, hashes: &[String]) -> bool {
    let toks: Vec<String> = line
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let hit = |s: &str| {
        let h = deny_hash(s);
        hashes.contains(&h)
    };
    toks.iter().any(|t| hit(t)) || toks.windows(2).any(|w| hit(&format!("{}-{}", w[0], w[1])))
}

/// Whether `line` holds an absolute home path: the home prefix, a user name
/// and a `/`.
pub fn names_home_path(line: &str) -> bool {
    let prefix = ["/ho", "me/"].concat();
    line.match_indices(&prefix).any(|(i, _)| {
        let rest = &line[i + prefix.len()..];
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || "._-".contains(c)))
            .unwrap_or(rest.len());
        name_len > 0 && rest[name_len..].starts_with('/')
    })
}

/// Files git lists under `root` (tracked, plus untracked not ignored), as
/// `/`-separated relative paths, sorted.
pub fn git_listed(root: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(root)
        .output()
        .expect("run git ls-files");
    assert!(
        out.status.success(),
        "git ls-files failed in {}",
        root.display()
    );
    let mut files: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter(|p| root.join(p).is_file())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    files
}

pub const MANIFEST: &str = "scripts/public-manifest.txt";

/// The manifest's (includes, excludes): one path per line, `#` comments,
/// `!path` excludes, blank lines ignored.
pub fn manifest_entries(text: &str) -> (Vec<String>, Vec<String>) {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for line in text.lines() {
        let entry = line.split('#').next().unwrap_or("").trim();
        if entry.is_empty() {
            continue;
        }
        match entry.strip_prefix('!') {
            Some(path) => exclude.push(path.trim().to_owned()),
            None => include.push(entry.to_owned()),
        }
    }
    (include, exclude)
}

/// Whether manifest entry `entry` (a file, or a directory prefix with or
/// without a trailing `/`) covers `file`.
pub fn manifest_covers(entry: &str, file: &str) -> bool {
    let dir = entry.trim_end_matches('/');
    file == dir || file.starts_with(&format!("{dir}/"))
}

/// The manifest at `root` expanded over `files`.
pub fn manifest_expand(root: &Path, files: &[String]) -> Vec<String> {
    let text = fs::read_to_string(root.join(MANIFEST)).unwrap_or_default();
    let (include, exclude) = manifest_entries(&text);
    files
        .iter()
        .filter(|f| include.iter().any(|e| manifest_covers(e, f)))
        .filter(|f| !exclude.iter().any(|e| manifest_covers(e, f)))
        .cloned()
        .collect()
}

/// The git-listed files at `root` that its public manifest covers.
pub fn manifest_files(root: &Path) -> Vec<String> {
    manifest_expand(root, &git_listed(root))
}

/// Every `/solo/agent/<rel>` named in each pack's agent.md and skill SKILL.md
/// files under `agents` that does not exist in that pack, as `pack/file:line <rel>`.
pub fn dangling_agent_refs(agents: &Path) -> Vec<String> {
    const PREFIX: &str = "/solo/agent/";
    let mut packs: Vec<String> = fs::read_dir(agents)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    packs.sort();
    let mut hits = Vec::new();
    for pack in packs {
        let dir = agents.join(&pack);
        let docs: Vec<String> = file_tree(&dir)
            .into_keys()
            .filter(|f| {
                f == "agent.md"
                    || (f.starts_with("skills/")
                        && f.ends_with("/SKILL.md")
                        && f.matches('/').count() == 2)
            })
            .collect();
        for doc in docs {
            let text =
                String::from_utf8_lossy(&fs::read(dir.join(&doc)).unwrap_or_default()).into_owned();
            for (n, line) in text.lines().enumerate() {
                for (i, _) in line.match_indices(PREFIX) {
                    let rest = &line[i + PREFIX.len()..];
                    let end = rest
                        .find(|c: char| c.is_whitespace() || "`'\"()<>[],;".contains(c))
                        .unwrap_or(rest.len());
                    let rel = rest[..end].trim_end_matches(['.', ':']);
                    if !rel.is_empty() && !dir.join(rel).exists() {
                        hits.push(format!("{pack}/{doc}:{} {rel}", n + 1));
                    }
                }
            }
        }
    }
    hits
}
