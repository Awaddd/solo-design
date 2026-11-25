//! One hardened, supervised container per run, parameterised by an engine.

use crate::config::ModelConfig;
use crate::engine::{AuthMount, Engine, Outcome, RunContext, Stream};
use crate::inputs::FrozenRun;
use crate::receipt::{self, Receipt, Status};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// What a finished container run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    /// Engine success and a zero container exit.
    pub success: bool,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub outcome: Outcome,
}

const POLL: Duration = Duration::from_millis(50);
/// A non-root identity used when the host user is root.
const FALLBACK_ID: u32 = 65534;

/// The started container: dropped while armed (error, panic, interruption) it
/// kills the docker client and removes exactly the container it owns.
struct Owned<'a> {
    docker: &'a Path,
    name: String,
    child: Child,
    armed: bool,
}

impl Owned<'_> {
    fn cleanup(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = Command::new(self.docker)
            .args(["rm", "-f", &self.name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for Owned<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.cleanup();
        }
    }
}

fn container_user() -> String {
    // SAFETY: getuid/getgid have no preconditions and cannot fail.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    let uid = if uid == 0 { FALLBACK_ID } else { uid };
    let gid = if gid == 0 { FALLBACK_ID } else { gid };
    format!("{uid}:{gid}")
}

fn launch_args(
    frozen: &FrozenRun,
    engine: &dyn Engine,
    name: &str,
    context: &RunContext,
    staged: &[AuthMount],
) -> Vec<String> {
    let layout = engine.layout();
    let auth = engine.auth(context);
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--name",
        name,
        "--user",
        &container_user(),
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--tmpfs",
        "/tmp",
        "--pull",
        "never",
        "-w",
        &layout.workdir,
    ]
    .map(str::to_owned)
    .to_vec();
    let mut bind = |src: &Path, dst: &str, ro: bool| {
        args.push("-v".to_owned());
        let mode = if ro { ":ro" } else { "" };
        args.push(format!("{}:{dst}{mode}", src.display()));
    };
    bind(&frozen.inputs.join("task"), &layout.task, true);
    bind(&frozen.inputs.join("agent"), &layout.agent, true);
    let design = frozen.inputs.join("design");
    if design.is_dir() {
        bind(&design, &layout.design, true);
    }
    bind(&frozen.outputs, &layout.outputs, false);
    if let Some(mount) = &auth.mount {
        bind(&mount.host, &mount.container, true);
    }
    for mount in staged {
        bind(&mount.host, &mount.container, true);
    }
    for var in &auth.env {
        args.push("--env".to_owned());
        args.push(var.clone());
    }
    args.push(engine.image());
    args.extend(engine.argv(context));
    args
}

fn read_lines(stream: Stream, pipe: impl Read, tx: mpsc::SyncSender<(Stream, String)>) {
    let mut reader = BufReader::new(pipe);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim_end_matches(['\n', '\r']).to_owned();
                if tx.send((stream, line)).is_err() {
                    break;
                }
            }
        }
    }
}

/// Launch and supervise the container for `frozen` with `docker` as the Docker
/// CLI program. Lines for the operator go to `report`; only fixed messages and
/// the engine's progress text are reported, never raw container output.
pub fn run(
    frozen: &FrozenRun,
    engine: &dyn Engine,
    config: &ModelConfig,
    docker: &Path,
    interrupt: &AtomicBool,
    report: &mut dyn FnMut(&str),
) -> Result<RunResult, String> {
    let context = RunContext {
        model: &config.model,
        provider: &config.provider,
        effort: None,
        key_file: None,
    };
    run_with(frozen, engine, &context, docker, interrupt, report)
}

/// `run` with the engine's full per-run context (effort, key file).
pub fn run_with(
    frozen: &FrozenRun,
    engine: &dyn Engine,
    context: &RunContext,
    docker: &Path,
    interrupt: &AtomicBool,
    report: &mut dyn FnMut(&str),
) -> Result<RunResult, String> {
    supervise(frozen, engine, context, None, docker, interrupt, report).map(|(result, _)| result)
}

/// How a supervised container ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Finished,
    TimedOut,
    Interrupted,
}

/// Launch and supervise one container, stopping it at `timeout` or on an
/// interrupt (the run's outcome then keeps what its stream already reported).
fn supervise(
    frozen: &FrozenRun,
    engine: &dyn Engine,
    context: &RunContext,
    timeout: Option<Duration>,
    docker: &Path,
    interrupt: &AtomicBool,
    report: &mut dyn FnMut(&str),
) -> Result<(RunResult, End), String> {
    let staged = engine.stage(frozen, context)?;
    let run_id = frozen
        .run_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("run directory has no name")?;
    let name = format!("solo-design-{run_id}");
    let args = launch_args(frozen, engine, &name, context, &staged);
    let mut classifier = engine.classifier(context);

    report(&format!("outputs: {}", frozen.outputs.display()));
    if interrupt.load(Ordering::SeqCst) {
        return Err("interrupted before the container started".to_owned());
    }

    // The deliverable must be new or changed by the run, not just the seeded copy.
    let seed = engine.deliverable().and_then(|name| {
        let path = frozen.outputs.join(name);
        let regular = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file());
        regular.then(|| std::fs::read(&path).ok()).flatten()
    });
    let mut child = Command::new(docker)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {} ({e})", docker.display()))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut owned = Owned {
        docker,
        name,
        child,
        armed: true,
    };

    let (tx, rx) = mpsc::sync_channel::<(Stream, String)>(256);
    let mut readers = Vec::new();
    if let Some(pipe) = stdout {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            read_lines(Stream::Stdout, pipe, tx)
        }));
    }
    if let Some(pipe) = stderr {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            read_lines(Stream::Stderr, pipe, tx)
        }));
    }
    drop(tx);

    let started = std::time::Instant::now();
    let mut timed_out = false;
    let mut interrupted = false;
    let mut status = None;
    let mut streams_open = true;
    while status.is_none() || streams_open {
        if timeout.is_some_and(|t| started.elapsed() >= t) {
            owned.cleanup();
            owned.armed = false;
            timed_out = true;
            break;
        }
        if interrupt.load(Ordering::SeqCst) {
            owned.cleanup();
            owned.armed = false;
            interrupted = true;
            break;
        }
        if streams_open {
            match rx.recv_timeout(POLL) {
                Ok((stream, line)) => {
                    if let Some(progress) = classifier.line(stream, &line) {
                        report(&progress.0);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => streams_open = false,
            }
        } else {
            std::thread::sleep(POLL);
        }
        if status.is_none() {
            status = owned
                .child
                .try_wait()
                .map_err(|e| format!("cannot supervise the container ({e})"))?;
        }
    }
    // `--rm` removed the container; nothing is left to clean up.
    owned.armed = false;
    if timed_out || interrupted {
        // Killing the client closes the pipes. Lines already queued still
        // reach the classifier (so reported usage is kept); then dropping the
        // receiver makes a reader blocked on a full channel fail its send.
        while let Ok((stream, line)) = rx.try_recv() {
            let _ = classifier.line(stream, &line);
        }
        drop(rx);
        for reader in readers {
            let _ = reader.join();
        }
        let mut outcome = classifier.finish(None, None);
        outcome.success = false;
        outcome.failure = None;
        let result = RunResult {
            success: false,
            exit_code: None,
            signal: None,
            outcome,
        };
        let end = if timed_out {
            End::TimedOut
        } else {
            End::Interrupted
        };
        return Ok((result, end));
    }
    for reader in readers {
        let _ = reader.join();
    }

    let status = status.expect("loop ends only with an exit status");
    let (exit_code, signal) = (status.code(), status.signal());
    let mut outcome = classifier.finish(exit_code, signal);
    if outcome.effort.is_none() {
        outcome.effort = context.effort.map(str::to_owned);
    }
    if let Some(name) = engine.deliverable() {
        let regular = std::fs::symlink_metadata(frozen.outputs.join(name))
            .is_ok_and(|m| m.file_type().is_file());
        let changed = regular
            && seed.as_ref().is_none_or(|seed| {
                std::fs::read(frozen.outputs.join(name)).is_ok_and(|now| now != *seed)
            });
        if outcome.success && !changed {
            outcome.success = false;
            outcome.failure = Some("no_deliverable".to_owned());
        }
    }
    let result = RunResult {
        success: outcome.success && exit_code == Some(0),
        exit_code,
        signal,
        outcome,
    };
    Ok((result, End::Finished))
}

/// Everything a run needs besides the host plumbing.
pub struct Job<'a> {
    pub frozen: &'a FrozenRun,
    pub agent: &'a str,
    pub design: Option<&'a str>,
    pub engine: &'a dyn Engine,
    pub config: &'a ModelConfig,
    /// The configured effort or thinking level, when set.
    pub effort: Option<&'a str>,
    /// The configured provider's key file, when set.
    pub key_file: Option<&'a Path>,
    /// The run timeout, when configured.
    pub timeout: Option<Duration>,
}

/// A run that reached its end (any outcome) and wrote its receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub status: Status,
    /// The host process exit code: 0 only for a successful run.
    pub exit_code: i32,
    /// `<run_dir>/receipt.json`.
    pub receipt: PathBuf,
    /// The container result, when a container ran to its end.
    pub run: Option<RunResult>,
}

/// Host exit code for an interrupted run (128 + SIGINT).
const INTERRUPTED_EXIT: i32 = 130;

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Run `job` (launch, supervise, clean up) and write its receipt, including on
/// interruption before or after launch and on host failure after launch.
pub fn execute(
    job: &Job,
    docker: &Path,
    interrupt: &AtomicBool,
    report: &mut dyn FnMut(&str),
) -> Result<Finished, String> {
    let started_at = unix_now();
    let (status, run_result) = if interrupt.load(Ordering::SeqCst) {
        (Status::Interrupted, None)
    } else {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let context = RunContext {
                model: &job.config.model,
                provider: &job.config.provider,
                effort: job.effort,
                key_file: job.key_file,
            };
            supervise(
                job.frozen,
                job.engine,
                &context,
                job.timeout,
                docker,
                interrupt,
                report,
            )
        }));
        match attempt {
            Ok(Ok((result, end))) => {
                let status = if end == End::TimedOut {
                    report("run failed: the run timeout was reached");
                    Status::TimedOut
                } else if end == End::Interrupted {
                    Status::Interrupted
                } else if result.success {
                    Status::Succeeded
                } else {
                    Status::Failed
                };
                (status, Some(result))
            }
            Ok(Err(message)) => {
                if interrupt.load(Ordering::SeqCst) {
                    (Status::Interrupted, None)
                } else {
                    report(&format!("run failed: {message}"));
                    (Status::Failed, None)
                }
            }
            // The container guard already removed the container while unwinding.
            Err(_) => {
                report("run failed: the host failed while supervising the container");
                (Status::Failed, None)
            }
        }
    };

    let receipt_path = job.frozen.run_dir.join("receipt.json");
    let outcome = run_result.as_ref().map(|r| &r.outcome);
    let receipt = Receipt {
        status,
        exit_code: run_result.as_ref().and_then(|r| r.exit_code),
        signal: run_result.as_ref().and_then(|r| r.signal),
        output_path: job.frozen.outputs.to_string_lossy().into_owned(),
        agent: job.agent.to_owned(),
        design: job.design.map(str::to_owned),
        engine: job.engine.name().to_owned(),
        model: job.config.model.clone(),
        provider: job.config.provider.clone(),
        effort: outcome.and_then(|o| o.effort.clone()),
        tokens: outcome.and_then(|o| o.tokens),
        usage: outcome.and_then(|o| o.usage.clone()),
        failure: outcome.and_then(|o| o.failure.clone()),
        started_at,
        ended_at: unix_now().max(started_at),
        input_hashes: job.frozen.hashes.clone(),
        output_hashes: receipt::hash_tree(&job.frozen.outputs),
    };
    receipt::write(&receipt_path, &receipt)?;

    let exit_code = match status {
        Status::Succeeded => 0,
        Status::Interrupted => INTERRUPTED_EXIT,
        Status::Failed | Status::TimedOut => 1,
    };
    Ok(Finished {
        status,
        exit_code,
        receipt: receipt_path,
        run: run_result,
    })
}
