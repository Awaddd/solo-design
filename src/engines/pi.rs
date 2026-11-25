//! The Pi engine.

use crate::engine::{
    Auth, AuthMount, Classifier, Engine, Layout, Outcome, Progress, RunContext, Stream, Usage,
};
use crate::inputs::FrozenRun;
use crate::providers;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The pinned Pi release the image is built from.
const PI_VERSION: &str = "0.83.0";
const SEPARATOR: &str = "\n\n---\n\n";
const STAGE_DIR: &str = "/solo/stage";
const STAGED_PROMPT: &str = "/solo/stage/system-prompt.md";
const KEY_PATH: &str = "/run/solo/key";
const TASK_FILE: &str = "/solo/task/task.md";
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// What staging decided about the task's files, for the command line.
#[derive(Debug, Default)]
struct Attachments {
    list: Option<String>,
    images: Vec<String>,
}

/// Runs the Pi coding agent from the pinned image.
#[derive(Debug, Clone, Default)]
pub struct PiEngine {
    attachments: Arc<Mutex<Attachments>>,
}

impl PiEngine {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The text after a leading `---` front-matter block, if there is one. The
/// closing fence is a line that is exactly `---`; empty front matter is fine.
fn strip_front_matter(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---\n") else {
        return text;
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        if line.trim_end_matches(['\n', '\r']) == "---" {
            return &rest[offset..];
        }
    }
    text
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {} ({e})", path.display()))
}

/// Sorted entries of `dir`, empty when it does not exist.
fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    entries.sort();
    entries
}

/// Files under `dir` as `/`-separated paths relative to it, sorted.
fn files_under(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        for path in sorted_entries(dir) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let rel = if prefix.is_empty() {
                name.into_owned()
            } else {
                format!("{prefix}/{name}")
            };
            if path.is_dir() {
                walk(&path, &rel, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out
}

fn is_image(rel: &str) -> bool {
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn engine_section() -> &'static str {
    "## Environment\n\n\
     The brief is attached as the task, from /solo/task/task.md. The task folder is \
     /solo/task, your agent's files are in /solo/agent (reference files for your skills \
     are under /solo/agent/skills) and the design pack is in /solo/design. All of these \
     are read-only.\n\n\
     Your working directory is /solo/outputs, the only writable folder. Write every file \
     you produce there. The deliverable is /solo/outputs/index.html.\n"
}

fn compose_prompt(inputs: &Path) -> Result<String, String> {
    let agent = inputs.join("agent");
    let mut parts = Vec::new();
    parts.push(
        strip_front_matter(&read(&agent.join("agent.md"))?)
            .trim()
            .to_owned(),
    );
    for rule in sorted_entries(&agent.join("rules")) {
        if rule.extension().is_some_and(|e| e == "md") && rule.is_file() {
            parts.push(read(&rule)?.trim().to_owned());
        }
    }
    let design = inputs.join("design/DESIGN.md");
    if design.is_file() {
        parts.push(format!(
            "## Active design system\n\n{}",
            read(&design)?.trim()
        ));
    }
    for skill in sorted_entries(&agent.join("skills")) {
        let file = skill.join("SKILL.md");
        if file.is_file() {
            let name = skill.file_name().unwrap_or_default().to_string_lossy();
            parts.push(format!(
                "## Active skill — {name}\n\nFollow this skill's workflow exactly.\n\n{}",
                strip_front_matter(&read(&file)?).trim()
            ));
        }
    }
    Ok(format!("{}\n\n{}", parts.join(SEPARATOR), engine_section()))
}

impl Engine for PiEngine {
    fn name(&self) -> &str {
        "pi"
    }
    fn image(&self) -> String {
        format!("solo-design-pi:{PI_VERSION}")
    }
    fn deliverable(&self) -> Option<&str> {
        Some("index.html")
    }
    fn build_hint(&self) -> String {
        format!("docker build -t {} docker/pi", self.image())
    }
    fn auth(&self, run: &RunContext) -> Auth {
        Auth {
            mount: run.key_file.map(|path| AuthMount {
                host: path.to_path_buf(),
                container: KEY_PATH.to_owned(),
            }),
            env: Vec::new(),
        }
    }
    fn layout(&self) -> Layout {
        Layout {
            task: "/solo/task".to_owned(),
            agent: "/solo/agent".to_owned(),
            design: "/solo/design".to_owned(),
            outputs: "/solo/outputs".to_owned(),
            workdir: "/solo/outputs".to_owned(),
        }
    }
    fn stage(&self, frozen: &FrozenRun, _run: &RunContext) -> Result<Vec<AuthMount>, String> {
        let stage = frozen.run_dir.join("stage");
        fs::create_dir_all(&stage)
            .map_err(|e| format!("cannot create {} ({e})", stage.display()))?;
        let prompt = compose_prompt(&frozen.inputs)?;
        let file = stage.join("system-prompt.md");
        fs::write(&file, prompt).map_err(|e| format!("cannot write {} ({e})", file.display()))?;

        let task = frozen.inputs.join("task");
        let mut listed = Vec::new();
        for rel in files_under(&frozen.outputs) {
            if rel != "task.md" {
                listed.push(format!("/solo/outputs/{rel}"));
            }
        }
        let mut images = Vec::new();
        for rel in files_under(&task) {
            if rel == "task.md" {
                continue;
            }
            if is_image(&rel) {
                images.push(format!("@/solo/task/{rel}"));
            }
            listed.push(format!("/solo/task/{rel}"));
        }
        if frozen.inputs.join("design/globals.css").is_file() {
            listed.push("/solo/design/globals.css".to_owned());
        }
        for rel in files_under(&frozen.inputs.join("agent/skills")) {
            if rel.rsplit('/').next() != Some("SKILL.md") {
                listed.push(format!("/solo/agent/skills/{rel}"));
            }
        }
        let list =
            (!listed.is_empty()).then(|| format!("\nAvailable files: {}", listed.join(", ")));
        *self.attachments.lock().unwrap_or_else(|e| e.into_inner()) = Attachments { list, images };
        Ok(vec![AuthMount {
            host: stage,
            container: STAGE_DIR.to_owned(),
        }])
    }
    fn argv(&self, run: &RunContext) -> Vec<String> {
        let (pi_provider, key_env) = providers::lookup(run.provider)
            .map(|p| (p.pi_provider, p.key_env))
            .unwrap_or((run.provider, ""));
        let mut argv: Vec<String> = [
            "--key-env",
            key_env,
            "--mode",
            "json",
            "--print",
            "--no-session",
            "--no-extensions",
            "--no-skills",
            "--no-prompt-templates",
            "--no-themes",
            "--no-context-files",
            "--tools",
            "read,write",
            "--model",
            &format!("{pi_provider}/{}", run.model),
            "--system-prompt",
            STAGED_PROMPT,
        ]
        .map(str::to_owned)
        .to_vec();
        if let Some(effort) = run.effort {
            argv.push("--thinking".to_owned());
            argv.push(effort.to_owned());
        }
        let attachments = self.attachments.lock().unwrap_or_else(|e| e.into_inner());
        argv.extend(attachments.list.iter().cloned());
        argv.extend(attachments.images.iter().cloned());
        argv.push(format!("@{TASK_FILE}"));
        argv
    }
    fn classifier(&self, _run: &RunContext) -> Box<dyn Classifier> {
        Box::new(PiClassifier::default())
    }
}

/// Reads Pi's JSON event stream: the run succeeded when the last assistant
/// message ended normally; usage is summed over every assistant `message_end`.
#[derive(Default)]
struct PiClassifier {
    last_stop: Option<String>,
    last_error: Option<String>,
    malformed: bool,
    model_not_found: bool,
    turns: u32,
    usage: Usage,
    total_tokens: Option<u64>,
}

fn add(slot: &mut Option<u64>, value: Option<u64>) {
    if let Some(v) = value {
        *slot = Some(slot.unwrap_or(0).saturating_add(v));
    }
}

/// The failure kind for a provider's `errorMessage` (`<status>: <body>`).
fn error_kind(message: &str) -> &'static str {
    let status: String = message.chars().take_while(char::is_ascii_digit).collect();
    match status.as_str() {
        "401" | "403" => "auth",
        "404" => "unknown_model",
        "400" if message.contains("not a valid model") => "unknown_model",
        "400" if message.contains("credit balance") => "rate_or_credit_limit",
        "402" | "429" => "rate_or_credit_limit",
        _ => "provider_error",
    }
}

/// A tool's target path for display: a path inside the container's `/solo`
/// tree or a relative one (Pi's cwd is the outputs folder), with no `..`
/// component and no control characters, shortened. Anything else is hidden.
fn display_path(path: &str) -> String {
    let shown = path.strip_prefix("/solo/outputs/").unwrap_or(path);
    let inside = path.starts_with("/solo/") || !path.starts_with('/');
    if !inside || shown.chars().any(char::is_control) || shown.split('/').any(|part| part == "..") {
        return "<path>".to_owned();
    }
    shown.chars().take(80).collect()
}

impl PiClassifier {
    fn message_end(&mut self, message: &serde_json::Value) -> Option<Progress> {
        if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            return None;
        }
        self.last_stop = message
            .get("stopReason")
            .and_then(|r| r.as_str())
            .map(str::to_owned);
        self.last_error = (self.last_stop.as_deref() == Some("error")).then(|| {
            message
                .get("errorMessage")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_owned()
        });
        if let Some(usage) = message.get("usage") {
            let count = |key: &str| usage.get(key).and_then(|v| v.as_u64());
            add(&mut self.usage.input_tokens, count("input"));
            add(&mut self.usage.output_tokens, count("output"));
            add(&mut self.usage.cache_read_tokens, count("cacheRead"));
            add(&mut self.usage.cache_write_tokens, count("cacheWrite"));
            add(&mut self.total_tokens, count("totalTokens"));
            if let Some(cost) = usage
                .get("cost")
                .and_then(|c| c.get("total"))
                .and_then(|t| t.as_f64())
                .filter(|c| c.is_finite())
            {
                *self.usage.cost_estimate_usd.get_or_insert(0.0) += cost;
            }
        }
        self.last_error.as_ref().map(|message| {
            let kind = error_kind(message).replace('_', " ");
            Progress(format!("error: provider failure ({kind})"))
        })
    }
}

impl Classifier for PiClassifier {
    fn line(&mut self, stream: Stream, line: &str) -> Option<Progress> {
        if stream == Stream::Stderr {
            if line.starts_with("Error: Model \"") && line.contains("not found") {
                self.model_not_found = true;
            }
            return None;
        }
        if line.trim().is_empty() {
            return None;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            self.malformed = true;
            return None;
        };
        match event.get("type").and_then(|t| t.as_str()) {
            Some("turn_start") => {
                self.turns += 1;
                Some(Progress(format!("turn {}", self.turns)))
            }
            Some("message_end") => event
                .get("message")
                .and_then(|message| self.message_end(message)),
            Some("tool_execution_start") => {
                let name: String = event
                    .get("toolName")
                    .and_then(|n| n.as_str())
                    .unwrap_or("tool")
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .take(24)
                    .collect();
                let path = event
                    .get("args")
                    .and_then(|a| a.get("path"))
                    .and_then(|p| p.as_str())
                    .map(display_path);
                Some(Progress(match path {
                    Some(path) => format!("{name} {path}"),
                    None => name,
                }))
            }
            _ => None,
        }
    }
    fn finish(&mut self, code: Option<i32>, signal: Option<i32>) -> Outcome {
        let success = code == Some(0)
            && signal.is_none()
            && !self.malformed
            && self.last_stop.as_deref() == Some("stop");
        let failure = if success {
            None
        } else if let Some(message) = &self.last_error {
            Some(error_kind(message))
        } else if self.model_not_found {
            Some("unknown_model")
        } else {
            Some("incomplete")
        };
        let usage = std::mem::take(&mut self.usage);
        let reported = usage != Usage::default();
        Outcome {
            success,
            tokens: self.total_tokens,
            usage: reported.then_some(usage),
            failure: failure.map(str::to_owned),
            ..Outcome::default()
        }
    }
}
