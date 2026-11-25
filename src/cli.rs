//! The `run` command surface.

use std::ffi::OsString;
use std::path::PathBuf;

/// Parsed `solo-design run` options.
#[derive(Debug, PartialEq, Eq)]
pub struct RunArgs {
    pub agent: String,
    pub task: PathBuf,
    pub design: Option<String>,
}

/// Why argv was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    /// Missing, repeated or unknown arguments: the caller should print usage.
    Usage(String),
    /// A well-formed command with an unsafe agent or design selector.
    Selector(String),
}

impl CliError {
    pub fn message(&self) -> &str {
        match self {
            CliError::Usage(m) | CliError::Selector(m) => m,
        }
    }
}

fn usage(reason: &str) -> CliError {
    CliError::Usage(format!("{reason}\n{}", crate::USAGE))
}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`
fn is_safe_selector(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

fn check_selector(flag: &str, value: &str) -> Result<(), CliError> {
    if is_safe_selector(value) {
        Ok(())
    } else {
        Err(CliError::Selector(format!(
            "{flag}: {value:?} is not a safe name (lowercase letters, digits and single hyphens)"
        )))
    }
}

/// Parse a full argv (program name first), distinguishing usage errors.
pub fn parse(args: &[OsString]) -> Result<RunArgs, CliError> {
    let mut rest = args.iter().skip(1);
    match rest.next().and_then(|a| a.to_str()) {
        Some("run") => {}
        _ => return Err(usage("expected the `run` command")),
    }
    let (mut agent, mut task, mut design): (Option<String>, Option<PathBuf>, Option<String>) =
        (None, None, None);
    while let Some(flag) = rest.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| usage("arguments must be valid UTF-8 option names"))?;
        if !matches!(flag, "--agent" | "--task" | "--design") {
            return Err(usage(&format!("unknown argument {flag}")));
        }
        let value = rest
            .next()
            .filter(|v| !v.to_string_lossy().starts_with("--"))
            .ok_or_else(|| usage(&format!("{flag} needs a value")))?;
        let repeated = match flag {
            "--agent" => agent.replace(text(flag, value)?).is_some(),
            "--design" => design.replace(text(flag, value)?).is_some(),
            _ => task.replace(PathBuf::from(value)).is_some(),
        };
        if repeated {
            return Err(usage(&format!("{flag} given more than once")));
        }
    }
    let agent = agent.ok_or_else(|| usage("--agent is required"))?;
    let task = task.ok_or_else(|| usage("--task is required"))?;
    check_selector("--agent", &agent)?;
    if let Some(design) = &design {
        check_selector("--design", design)?;
    }
    Ok(RunArgs {
        agent,
        task,
        design,
    })
}

fn text(flag: &str, value: &OsString) -> Result<String, CliError> {
    value
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| CliError::Selector(format!("{flag}: value is not valid UTF-8")))
}

/// Parse a full argv (program name first). The error is a message.
pub fn parse_args(args: &[OsString]) -> Result<RunArgs, String> {
    parse(args).map_err(|e| e.message().to_owned())
}
