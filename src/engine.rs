//! Engine abstraction: every engine-specific value the runtime needs.

use crate::inputs::FrozenRun;
use std::path::{Path, PathBuf};

/// Which container stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// A sanitised progress message the host may print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress(pub String);

/// The engine's verdict on a finished run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub success: bool,
    pub effort: Option<String>,
    pub tokens: Option<u64>,
    /// Token usage and estimated cost summed over the run's messages.
    pub usage: Option<Usage>,
    /// The specific failure kind, when the run failed.
    pub failure: Option<String>,
}

/// Usage summed over every message of a run; a field is `None` when no
/// message reported it.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_estimate_usd: Option<f64>,
}

// Costs are finite sums of reported numbers, never NaN.
impl Eq for Usage {}

/// A read-only host path the engine needs inside the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthMount {
    pub host: PathBuf,
    pub container: String,
}

/// Credentials provision: a read-only mount and/or env var *names* passed
/// through as `--env NAME` (values never appear in argv).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Auth {
    pub mount: Option<AuthMount>,
    pub env: Vec<String>,
}

/// Where the frozen inputs and the outputs appear inside the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub task: String,
    pub agent: String,
    pub design: String,
    pub outputs: String,
    /// The container's working directory.
    pub workdir: String,
}

/// Per-run values the engine receives from the core, unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunContext<'a> {
    pub model: &'a str,
    pub provider: &'a str,
    /// The configured effort or thinking level, when set.
    pub effort: Option<&'a str>,
    /// The configured provider's key file on the host, when set.
    pub key_file: Option<&'a Path>,
}

/// Per-run stream classifier.
pub trait Classifier: Send {
    /// Classify one line; `Some` is printed as progress.
    fn line(&mut self, stream: Stream, line: &str) -> Option<Progress>;
    /// The verdict once the container has exited.
    fn finish(&mut self, code: Option<i32>, signal: Option<i32>) -> Outcome;
}

/// An engine that can run a frozen task in a container.
pub trait Engine {
    /// Short engine name.
    fn name(&self) -> &str;
    /// Container image reference.
    fn image(&self) -> String;
    /// The file a successful run must leave in the outputs folder, if any.
    fn deliverable(&self) -> Option<&str> {
        None
    }
    /// The command that builds the image, shown when it is missing.
    fn build_hint(&self) -> String {
        format!("docker build -t {} <image folder>", self.image())
    }
    /// Credentials provision for one run.
    fn auth(&self, run: &RunContext) -> Auth;
    /// In-container paths.
    fn layout(&self) -> Layout;
    /// The container command (after the image).
    fn argv(&self, run: &RunContext) -> Vec<String>;
    /// Host-side files staged for one run, mounted read-only.
    fn stage(&self, _frozen: &FrozenRun, _run: &RunContext) -> Result<Vec<AuthMount>, String> {
        Ok(Vec::new())
    }
    /// A fresh classifier for one run.
    fn classifier(&self, run: &RunContext) -> Box<dyn Classifier>;
}

/// The engines compiled into this build.
pub fn available() -> Vec<Box<dyn Engine>> {
    vec![Box::new(crate::engines::pi::PiEngine::new())]
}
