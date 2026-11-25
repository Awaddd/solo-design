mod common;

use common::{
    execute_job, execute_timed, is_launch, read_invocations, read_receipt, FakeDocker, Fixture,
    Interrupt, Script, TestEngine,
};
use serde_json::{json, Value};
use solo_design::config::ModelConfig;
use solo_design::engine::Outcome;
use solo_design::inputs::{prepare, FrozenRun};
use solo_design::receipt::Status;
use solo_design::runtime::{execute, run, Job, RunResult};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use common::TIME_LIMIT;

fn config() -> ModelConfig {
    ModelConfig {
        model: "fixture-model".to_owned(),
        provider: "fixture-provider".to_owned(),
    }
}

/// A fixture with agent `alpha` and design `pack`, plus an auth folder.
fn fixture() -> (Fixture, PathBuf) {
    let fx = Fixture::new();
    fx.agent("alpha");
    fx.design("pack");
    let auth = fx.root.parent().unwrap().join("auth");
    fs::create_dir_all(&auth).unwrap();
    fs::write(auth.join("credentials"), "fixture-secret").unwrap();
    (fx, auth)
}

fn freeze(fx: &Fixture, design: Option<&str>) -> FrozenRun {
    prepare(&fx.root, &fx.task, "alpha", design).expect("fixture selection freezes")
}

struct Launched {
    result: Result<RunResult, String>,
    /// Each reported line, with whether a container launch had already been
    /// invoked when it was reported.
    reports: Vec<(String, bool)>,
    invocations: Vec<Vec<String>>,
}

impl Launched {
    fn launch_args(&self) -> Vec<String> {
        let launches: Vec<_> = self.invocations.iter().filter(|a| is_launch(a)).collect();
        assert_eq!(
            launches.len(),
            1,
            "exactly one container launch: {:?}",
            self.invocations
        );
        launches[0].clone()
    }
    fn reported(&self, needle: &str) -> bool {
        self.reports.iter().any(|(line, _)| line.contains(needle))
    }
}

/// Run the runtime on another thread under a time limit.
fn launch(
    frozen: &FrozenRun,
    engine: &TestEngine,
    config: ModelConfig,
    docker: &FakeDocker,
) -> Launched {
    let (frozen, engine) = (frozen.clone(), engine.clone());
    let (program, log) = (docker.program.clone(), docker.log.clone());
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = reports.clone();
    let (tx, rx) = mpsc::channel();
    let log_for_thread = log.clone();
    std::thread::spawn(move || {
        let interrupt = AtomicBool::new(false);
        let mut report = |line: &str| {
            let launched = read_invocations(&log_for_thread)
                .iter()
                .any(|a| is_launch(a));
            sink.lock().unwrap().push((line.to_owned(), launched));
        };
        let result = run(&frozen, &engine, &config, &program, &interrupt, &mut report);
        let _ = tx.send(result);
    });
    let result = rx
        .recv_timeout(TIME_LIMIT)
        .expect("runtime finished within the time limit");
    let reports = reports.lock().unwrap().clone();
    Launched {
        result,
        reports,
        invocations: read_invocations(&log),
    }
}

/// The docker options of a launch: between `run` and the image.
fn options<'a>(args: &'a [String], image: &str) -> &'a [String] {
    let start = if args[0] == "container" { 2 } else { 1 };
    let end = args
        .iter()
        .position(|a| a == image)
        .unwrap_or_else(|| panic!("image {image} not in launch argv {args:?}"));
    &args[start..end]
}

fn values(opts: &[String], names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < opts.len() {
        let arg = &opts[i];
        if names.contains(&arg.as_str()) {
            out.push(opts.get(i + 1).cloned().unwrap_or_default());
            i += 2;
            continue;
        }
        for name in names {
            if let Some(rest) = arg.strip_prefix(&format!("{name}=")) {
                out.push(rest.to_owned());
            }
        }
        i += 1;
    }
    out
}

fn has_flag(opts: &[String], name: &str) -> bool {
    opts.iter()
        .any(|a| a == name || a == &format!("{name}=true"))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Mount {
    src: PathBuf,
    dst: String,
    ro: bool,
}

fn binds(opts: &[String]) -> Vec<Mount> {
    let mut out = Vec::new();
    for v in values(opts, &["-v", "--volume"]) {
        let parts: Vec<&str> = v.splitn(3, ':').collect();
        let ro = parts
            .get(2)
            .is_some_and(|o| o.split(',').any(|x| x == "ro"));
        out.push(Mount {
            src: PathBuf::from(parts[0]),
            dst: parts.get(1).unwrap_or(&"").to_string(),
            ro,
        });
    }
    for v in values(opts, &["--mount"]) {
        let (mut kind, mut src, mut dst, mut ro) = ("", "", "", false);
        for kv in v.split(',') {
            let (k, val) = kv.split_once('=').unwrap_or((kv, ""));
            match k {
                "type" => kind = val,
                "source" | "src" => src = val,
                "target" | "dst" | "destination" => dst = val,
                "readonly" | "ro" => ro = val.is_empty() || val == "true" || val == "1",
                _ => {}
            }
        }
        if kind != "tmpfs" {
            out.push(Mount {
                src: PathBuf::from(src),
                dst: dst.to_owned(),
                ro,
            });
        }
    }
    out
}

fn tmpfs_targets(opts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = values(opts, &["--tmpfs"])
        .iter()
        .map(|v| v.split(':').next().unwrap().to_owned())
        .collect();
    for v in values(opts, &["--mount"]) {
        if v.split(',').any(|kv| kv == "type=tmpfs") {
            for kv in v.split(',') {
                if let Some((k, val)) = kv.split_once('=') {
                    if matches!(k, "target" | "dst" | "destination") {
                        out.push(val.to_owned());
                    }
                }
            }
        }
    }
    out
}

fn container_name(opts: &[String]) -> String {
    let names = values(opts, &["--name"]);
    assert_eq!(names.len(), 1, "one --name: {opts:?}");
    names[0].clone()
}

#[test]
fn takes_image_auth_layout_argv_and_classification_only_from_the_engine_which_receives_the_configured_model_and_provider_unchanged(
) {
    let (fx, auth) = fixture();
    let configured = ModelConfig {
        model: "fixture-model:v1.2 @edge".to_owned(),
        provider: "fixture provider/x".to_owned(),
    };
    let alpha = TestEngine::new("alpha-engine", &auth);
    let beta = TestEngine::new("beta-engine", &auth);
    let script = Script {
        stdout: vec![
            format!("{} from-alpha", alpha.marker),
            format!("{} from-beta", beta.marker),
        ],
        stderr: vec![],
        exit: 0,
    };
    for (engine, other) in [(&alpha, &beta), (&beta, &alpha)] {
        let frozen = freeze(&fx, None);
        let docker = FakeDocker::new(&script);
        let out = launch(&frozen, engine, configured.clone(), &docker);
        assert!(
            out.result.is_ok(),
            "{}: run failed: {:?}",
            engine.name,
            out.result
        );

        let args = out.launch_args();
        let mut tail = vec![engine.image.clone()];
        tail.extend(engine.expected_argv(&configured.model, &configured.provider));
        assert!(
            args.ends_with(&tail),
            "{}: launch must end with the engine's image and argv {tail:?}: {args:?}",
            engine.name
        );
        assert!(!args.contains(&other.image), "other engine's image leaked");
        assert_eq!(
            engine
                .seen
                .lock()
                .unwrap()
                .clone()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([(configured.model.clone(), configured.provider.clone())]),
            "{}: the engine receives the configured model and provider unchanged",
            engine.name
        );

        let opts = options(&args, &engine.image);
        assert_eq!(
            values(opts, &["-e", "--env"]),
            engine.auth.env,
            "{}: auth env passed by name only",
            engine.name
        );
        let dsts: BTreeSet<String> = binds(opts).into_iter().map(|m| m.dst).collect();
        let layout = &engine.layout;
        let auth_mount = engine.auth.mount.as_ref().unwrap();
        assert_eq!(
            dsts,
            BTreeSet::from([
                layout.task.clone(),
                layout.agent.clone(),
                layout.outputs.clone(),
                auth_mount.container.clone(),
            ]),
            "{}: container paths come from the engine layout and auth",
            engine.name
        );

        let own = if engine.name == "alpha-engine" {
            "from-alpha"
        } else {
            "from-beta"
        };
        let foreign = if own == "from-alpha" {
            "from-beta"
        } else {
            "from-alpha"
        };
        assert!(
            out.reported(&format!("step {own}")),
            "{}: own classification",
            engine.name
        );
        assert!(
            !out.reported(&format!("step {foreign}")),
            "{}: only its classifier",
            engine.name
        );
    }
}

#[test]
fn mounts_frozen_inputs_read_only_and_only_outputs_writable() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let layout = engine.layout.clone();
    let auth_mount = engine.auth.mount.clone().unwrap();
    for design in [Some("pack"), None] {
        let frozen = freeze(&fx, design);
        let docker = FakeDocker::new(&Script::default());
        let out = launch(&frozen, &engine, config(), &docker);
        assert!(out.result.is_ok(), "run failed: {:?}", out.result);
        let args = out.launch_args();
        let mut expected = BTreeSet::from([
            Mount {
                src: frozen.inputs.join("task"),
                dst: layout.task.clone(),
                ro: true,
            },
            Mount {
                src: frozen.inputs.join("agent"),
                dst: layout.agent.clone(),
                ro: true,
            },
            Mount {
                src: frozen.outputs.clone(),
                dst: layout.outputs.clone(),
                ro: false,
            },
            Mount {
                src: auth_mount.host.clone(),
                dst: auth_mount.container.clone(),
                ro: true,
            },
        ]);
        if design.is_some() {
            expected.insert(Mount {
                src: frozen.inputs.join("design"),
                dst: layout.design.clone(),
                ro: true,
            });
        }
        let actual: BTreeSet<Mount> = binds(options(&args, &engine.image)).into_iter().collect();
        assert_eq!(actual, expected, "design {design:?}: exact mount set");
    }
}

#[test]
fn starts_a_hardened_non_root_container_without_the_socket_or_home() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, Some("pack"));
    let docker = FakeDocker::new(&Script::default());
    let out = launch(&frozen, &engine, config(), &docker);
    assert!(out.result.is_ok(), "run failed: {:?}", out.result);
    let args = out.launch_args();
    let opts = options(&args, &engine.image);

    let users = values(opts, &["-u", "--user"]);
    assert_eq!(users.len(), 1, "an explicit user: {opts:?}");
    let user = users[0].split(':').next().unwrap();
    assert!(
        !user.is_empty() && user != "0" && user != "root",
        "non-root user: {}",
        users[0]
    );
    assert!(has_flag(opts, "--read-only"), "read-only root fs: {opts:?}");
    assert!(
        values(opts, &["--cap-drop"])
            .iter()
            .any(|c| c.eq_ignore_ascii_case("all")),
        "all capabilities dropped: {opts:?}"
    );
    assert!(
        values(opts, &["--security-opt"])
            .iter()
            .any(|s| s == "no-new-privileges"
                || s == "no-new-privileges:true"
                || s == "no-new-privileges=true"),
        "no-new-privileges: {opts:?}"
    );
    assert!(
        tmpfs_targets(opts).contains(&"/tmp".to_owned()),
        "tmpfs /tmp: {opts:?}"
    );
    assert!(!has_flag(opts, "--privileged"), "never privileged");
    assert!(
        !args.iter().any(|a| a.contains("docker.sock")),
        "no Docker socket: {args:?}"
    );
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for mount in binds(opts) {
            assert!(
                !home.starts_with(&mount.src),
                "host home exposed by {mount:?}"
            );
        }
    }
}

#[test]
fn names_the_container_uniquely_and_removes_only_that_container() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let docker = FakeDocker::new(&Script::default());
    let mut names = Vec::new();
    for _ in 0..2 {
        let frozen = freeze(&fx, None);
        let out = launch(&frozen, &engine, config(), &docker);
        assert!(out.result.is_ok(), "run failed: {:?}", out.result);
        let launches: Vec<_> = out
            .invocations
            .iter()
            .filter(|a| is_launch(a))
            .cloned()
            .collect();
        let args = launches.last().expect("a launch").clone();
        let name = container_name(options(&args, &engine.image));
        assert!(
            name.starts_with("solo-design-") && name.len() > "solo-design-".len(),
            "owned prefix: {name}"
        );
        names.push((name, args));
    }
    assert_ne!(names[0].0, names[1].0, "names are unique per run");

    let invocations = docker.invocations();
    for (name, launch) in &names {
        let removed_after = launch.iter().any(|a| a == "--rm")
            || invocations.iter().any(|a| {
                !is_launch(a)
                    && a.iter().any(|x| x == "rm" || x == "kill" || x == "stop")
                    && a.contains(name)
            });
        assert!(
            removed_after,
            "{name} is removed when the run ends: {invocations:?}"
        );
    }
    let owned: Vec<&String> = names.iter().map(|(n, _)| n).collect();
    for args in invocations.iter().filter(|a| !is_launch(a)) {
        let mentioned: Vec<&String> = args.iter().filter(|a| owned.contains(a)).collect();
        assert_eq!(
            mentioned.len(),
            1,
            "each non-launch call targets one owned name: {args:?}"
        );
        for broad in ["prune", "--filter", "-a", "--all", "ps"] {
            assert!(
                !args.iter().any(|a| a == broad),
                "no broad selection ({broad}): {args:?}"
            );
        }
        assert!(
            !args
                .iter()
                .any(|a| a.starts_with("solo-design-") && !owned.contains(&a)),
            "no other container touched: {args:?}"
        );
    }
}

#[test]
fn prints_the_absolute_output_path_before_the_container_starts() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, None);
    assert!(frozen.outputs.is_absolute());
    let docker = FakeDocker::new(&Script::default());
    let out = launch(&frozen, &engine, config(), &docker);
    let path = frozen.outputs.to_str().unwrap();
    let line = out.reports.iter().find(|(line, _)| line.contains(path));
    assert!(
        line.is_some(),
        "the output path {path} is reported: {:?}",
        out.reports
    );
    assert!(!line.unwrap().1, "reported before the container launch");
    out.launch_args();
}

#[test]
fn prints_the_engines_progress_for_classified_lines() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, None);
    let docker = FakeDocker::new(&Script {
        stdout: vec![
            format!("{} one", engine.marker),
            "raw noise on stdout".to_owned(),
            format!("{}-RESULT ok", engine.marker),
        ],
        stderr: vec![
            format!("{} two", engine.marker),
            "raw noise on stderr".to_owned(),
        ],
        exit: 0,
    });
    let out = launch(&frozen, &engine, config(), &docker);
    assert!(out.result.is_ok(), "run failed: {:?}", out.result);
    for progress in ["step one", "step two", "result ok"] {
        assert!(
            out.reported(progress),
            "progress {progress:?} printed: {:?}",
            out.reports
        );
    }
    assert!(
        !out.reported("raw noise"),
        "unclassified lines are not printed: {:?}",
        out.reports
    );
    assert!(
        !out.reported(&engine.marker),
        "raw marker lines are not echoed: {:?}",
        out.reports
    );
}

#[test]
fn succeeds_only_on_engine_success_with_a_zero_container_exit() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    for (engine_ok, exit, success) in [
        (true, 0, true),
        (true, 3, false),
        (false, 0, false),
        (false, 5, false),
    ] {
        let frozen = freeze(&fx, None);
        let stdout = if engine_ok {
            vec![format!("{}-RESULT ok", engine.marker)]
        } else {
            vec![]
        };
        let docker = FakeDocker::new(&Script {
            stdout,
            stderr: vec![],
            exit,
        });
        let out = launch(&frozen, &engine, config(), &docker);
        assert_eq!(
            out.result,
            Ok(RunResult {
                success,
                exit_code: Some(exit),
                signal: None,
                outcome: Outcome {
                    success: engine_ok,
                    effort: None,
                    tokens: None,
                    ..Outcome::default()
                },
            }),
            "engine ok {engine_ok}, container exit {exit}"
        );
    }
}

fn launched_name(invocations: &[Vec<String>]) -> Option<(usize, String)> {
    let at = invocations.iter().position(|a| is_launch(a))?;
    let args = &invocations[at];
    let name = values(args, &["--name"]).into_iter().next()?;
    Some((at, name))
}

fn removed_after(invocations: &[Vec<String>], at: usize, name: &str) -> bool {
    invocations[at + 1..].iter().any(|a| {
        a.iter().any(|x| x == "rm" || x == "stop" || x == "kill") && a.iter().any(|x| x == name)
    })
}

#[test]
fn on_interruption_stops_and_removes_the_container_keeps_outputs_and_writes_the_receipt() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, None);
    fs::write(frozen.outputs.join("partial.html"), "<p>work so far</p>").unwrap();
    let docker = FakeDocker::sleeping(
        &Script {
            stdout: vec![format!("{} working", engine.marker)],
            stderr: vec![],
            exit: 0,
        },
        20,
    );
    let started = std::time::Instant::now();
    let out = execute_job(
        &frozen,
        "alpha",
        None,
        &engine,
        &config(),
        &docker,
        Interrupt::AfterLaunch,
    );
    let elapsed = started.elapsed();

    let (at, name) = launched_name(&out.invocations).expect("the container was launched");
    assert!(
        removed_after(&out.invocations, at, &name),
        "owned container stopped/removed: {:?}",
        out.invocations
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "interruption honoured promptly ({elapsed:?})"
    );
    let finished = out.result.expect("an interrupted run still finishes");
    assert_eq!(finished.status, Status::Interrupted);
    assert_ne!(finished.exit_code, 0, "interrupted runs exit nonzero");
    assert_eq!(
        fs::read_to_string(frozen.outputs.join("partial.html"))
            .ok()
            .as_deref(),
        Some("<p>work so far</p>"),
        "outputs kept"
    );
    assert_eq!(
        read_receipt(&finished.receipt).get("status"),
        Some(&json!("interrupted"))
    );
}

#[test]
fn removes_the_container_when_supervision_fails_after_launch() {
    let (fx, auth) = fixture();
    let mut engine = TestEngine::new("alpha-engine", &auth);
    engine.panic_on = Some("BOOM".to_owned());
    let frozen = freeze(&fx, None);
    let docker = FakeDocker::sleeping(
        &Script {
            stdout: vec!["BOOM".to_owned()],
            stderr: vec![],
            exit: 0,
        },
        20,
    );
    let started = std::time::Instant::now();
    let out = execute_job(
        &frozen,
        "alpha",
        None,
        &engine,
        &config(),
        &docker,
        Interrupt::Never,
    );
    let elapsed = started.elapsed();

    let (at, name) = launched_name(&out.invocations).expect("the container was launched");
    assert!(
        removed_after(&out.invocations, at, &name),
        "owned container removed after a supervision failure: {:?}",
        out.invocations
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "no wait for the container to end ({elapsed:?})"
    );
    if let Ok(finished) = &out.result {
        assert_ne!(
            finished.status,
            Status::Succeeded,
            "a host failure is not a success"
        );
    }
}

#[test]
fn an_interruption_before_launch_starts_no_container_and_writes_an_interrupted_receipt() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, None);
    let docker = FakeDocker::new(&Script::default());
    let out = execute_job(
        &frozen,
        "alpha",
        None,
        &engine,
        &config(),
        &docker,
        Interrupt::BeforeStart,
    );

    assert!(
        !out.invocations.iter().any(|a| is_launch(a)),
        "no container launched: {:?}",
        out.invocations
    );
    let finished = out.result.expect("an interrupted run still finishes");
    assert_eq!(finished.status, Status::Interrupted);
    assert_ne!(finished.exit_code, 0);
    assert_eq!(finished.run, None);
    let receipt = read_receipt(&finished.receipt);
    assert_eq!(receipt.get("status"), Some(&json!("interrupted")));
    assert_eq!(receipt.get("exit_code"), Some(&Value::Null));
    assert_eq!(receipt.get("signal"), Some(&Value::Null));
}

#[test]
fn stops_the_container_at_the_run_timeout_keeps_outputs_and_records_a_timeout() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, None);
    fs::write(frozen.outputs.join("partial.html"), "<p>work so far</p>").unwrap();
    let docker = FakeDocker::sleeping(
        &Script {
            stdout: vec![format!("{} working", engine.marker)],
            stderr: vec![],
            exit: 0,
        },
        8,
    );
    let started = std::time::Instant::now();
    let out = execute_timed(
        &frozen,
        "alpha",
        None,
        &engine,
        &config(),
        None,
        None,
        Some(Duration::from_secs(1)),
        &docker,
        Interrupt::Never,
    );
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "the run stops at its timeout, not when the container exits ({elapsed:?})"
    );
    let (at, name) = launched_name(&out.invocations).expect("the container was launched");
    assert!(
        removed_after(&out.invocations, at, &name),
        "owned container stopped/removed: {:?}",
        out.invocations
    );
    let finished = out.result.expect("a timed-out run still finishes");
    assert_ne!(finished.exit_code, 0, "a timed-out run exits nonzero");
    assert_eq!(
        fs::read_to_string(frozen.outputs.join("partial.html"))
            .ok()
            .as_deref(),
        Some("<p>work so far</p>"),
        "outputs kept"
    );
    let receipt = read_receipt(&finished.receipt);
    assert_eq!(
        receipt.get("status"),
        Some(&json!("timed_out")),
        "{receipt:?}"
    );
    assert!(
        receipt["output_hashes"]
            .as_object()
            .is_some_and(|m| m.contains_key("partial.html")),
        "the kept outputs are hashed"
    );

    // Control: without a timeout the same container runs to its own end.
    let quick = FakeDocker::sleeping(
        &Script {
            stdout: vec![format!("{}-RESULT ok", engine.marker)],
            stderr: vec![],
            exit: 0,
        },
        2,
    );
    let frozen = freeze(&fx, None);
    let out = execute_timed(
        &frozen,
        "alpha",
        None,
        &engine,
        &config(),
        None,
        None,
        None,
        &quick,
        Interrupt::Never,
    );
    let finished = out.result.expect("the run finishes");
    assert_eq!(
        finished.status,
        Status::Succeeded,
        "no timeout, no timed_out"
    );

    // Output still queued when the timeout fires (more lines than the line
    // channel and pipe hold) must not wedge the run. No wall-clock race: the
    // report callback stalls the supervisor on the first progress line until
    // the timeout has passed, so the burst piles up behind it. The fake marks
    // a *successful* end of its burst (`cat && touch`); a cat killed by SIGPIPE
    // once the reader goes away leaves no marker, so the marker must never appear.
    let dir = tempfile::tempdir().unwrap();
    let burst = dir.path().join("burst.txt");
    fs::write(
        &burst,
        format!("{} first\n{}", engine.marker, "x\n".repeat(2_000_000)),
    )
    .unwrap();
    let burst_done = dir.path().join("burst-done");
    let program = dir.path().join("docker");
    common::write_executable(
        &program,
        &format!(
            "#!/bin/sh\nif [ \"$1\" = run ]; then /bin/cat '{}' && : > '{}'; exec /bin/sleep 8; fi\nexit 0\n",
            burst.display(),
            burst_done.display()
        ),
    );
    let frozen = freeze(&fx, None);
    let (tx, rx) = mpsc::channel();
    let (frozen_t, engine_t) = (frozen.clone(), engine.clone());
    let stalled = Arc::new(AtomicBool::new(false));
    let stalled_t = stalled.clone();
    std::thread::spawn(move || {
        let config = config();
        let job = Job {
            frozen: &frozen_t,
            agent: "alpha",
            design: None,
            engine: &engine_t,
            config: &config,
            effort: None,
            key_file: None,
            timeout: Some(Duration::from_secs(1)),
        };
        let interrupt = AtomicBool::new(false);
        let mut report = |line: &str| {
            if line.starts_with("step ") && !stalled_t.swap(true, Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1500));
            }
        };
        let _ = tx.send(execute(&job, &program, &interrupt, &mut report));
    });
    let finished = rx
        .recv_timeout(Duration::from_secs(8))
        .expect("a timed-out run with queued output returns within the bound")
        .expect("a timed-out run still finishes");
    assert!(
        stalled.load(Ordering::SeqCst),
        "the burst reached the supervisor"
    );
    assert!(
        !burst_done.exists(),
        "the burst was still queued when the container was stopped"
    );
    assert_eq!(
        read_receipt(&finished.receipt).get("status"),
        Some(&json!("timed_out")),
        "a burst before the timeout is still recorded as timed out"
    );
}
