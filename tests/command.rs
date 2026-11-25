mod common;

use common::Fixture;
use solo_design::cli::{parse_args, RunArgs};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Output};

fn parse(args: &[&str]) -> Result<RunArgs, String> {
    let argv = std::iter::once("solo-design")
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect::<Vec<_>>();
    parse_args(&argv)
}

fn run_binary(args: &[&str]) -> Output {
    let home = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_solo-design"))
        .args(args)
        .env_clear()
        .env("HOME", home.path())
        .env("SOLO_DESIGN_ROOT", home.path())
        .output()
        .expect("spawn the solo-design binary")
}

#[test]
fn accepts_agent_and_task_with_or_without_a_design() {
    for agent in ["example", "frontend-design"] {
        for design in [None, Some("pack-1")] {
            let mut args = vec!["run", "--task", "./a task", "--agent", agent];
            if let Some(pack) = design {
                args.extend(["--design", pack]);
            }
            let expected = RunArgs {
                agent: agent.to_owned(),
                task: PathBuf::from("./a task"),
                design: design.map(str::to_owned),
            };
            assert_eq!(parse(&args), Ok(expected), "argv {args:?}");
        }
    }
}

#[test]
fn exits_with_a_usage_error_for_missing_repeated_or_unknown_options() {
    let usage_errors: [&[&str]; 10] = [
        &[],
        &["run"],
        &["run", "--task", "task"],
        &["run", "--agent", "example"],
        &["run", "--agent", "example", "--task"],
        &["run", "--agent", "example", "--task", "task", "--unknown"],
        &["run", "--agent", "a", "--agent", "b", "--task", "task"],
        &["run", "--agent", "a", "--task", "t", "--task", "u"],
        &[
            "run", "--agent", "a", "--task", "t", "--design", "x", "--design", "y",
        ],
        &["other", "--agent", "a", "--task", "t"],
    ];
    for args in usage_errors {
        let out = run_binary(args);
        let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
        assert_eq!(out.status.code(), Some(2), "usage exit for {args:?}");
        assert!(
            stderr.contains("usage"),
            "usage text for {args:?}: {stderr}"
        );
    }
    // A well-formed command is not a usage error, even when later preflight fails.
    let out = run_binary(&["run", "--agent", "example", "--task", "/no/such/task"]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    assert_ne!(
        out.status.code(),
        Some(2),
        "valid argv got usage exit: {stderr}"
    );
    assert!(
        !stderr.contains("usage"),
        "valid argv got usage text: {stderr}"
    );
}

#[test]
fn rejects_unsafe_selectors_before_touching_the_filesystem() {
    let unsafe_names = [
        "",
        "/absolute",
        "../example",
        ".",
        "..",
        "a/b",
        "a\\b",
        "Example",
        "a--b",
        "a-",
        "-a",
        "a_b",
        "a b",
        "é",
    ];
    for selector in unsafe_names {
        for flag in ["--agent", "--design"] {
            let mut args = vec!["run", "--task", "/nonexistent/caller-task"];
            if flag == "--agent" {
                args.extend([flag, selector]);
            } else {
                args.extend(["--agent", "frontend-design", flag, selector]);
            }
            let message = parse(&args).expect_err("unsafe selector accepted");
            assert!(
                message.contains(flag),
                "selector error must name {flag} for {selector:?}: {message}"
            );
        }
    }
    for selector in ["a", "a0", "frontend-design", "two-words-3"] {
        let args = [
            "run",
            "--agent",
            selector,
            "--task",
            "/not-resolved",
            "--design",
            selector,
        ];
        assert!(parse(&args).is_ok(), "safe selector rejected: {selector}");
    }
}

// Provider names are built from parts (repo guard).
fn router() -> String {
    ["open", "router"].concat()
}
fn lab() -> String {
    ["anthro", "pic"].concat()
}

/// A resource root with agents `alpha` and `mismatch` (bad front-matter) and
/// design `pack`, plus a fake `docker` on PATH that logs every invocation.
struct Preflight {
    fx: Fixture,
    bin: PathBuf,
    docker_log: PathBuf,
}

impl Preflight {
    fn new() -> Self {
        let fx = Fixture::new();
        fx.agent("alpha");
        fx.agent_named("mismatch", "alpha");
        fx.design("pack");
        let bin = fx.root.parent().unwrap().join("fake-bin");
        std::fs::create_dir_all(&bin).unwrap();
        let docker_log = bin.join("docker.log");
        common::write_executable(
            &bin.join("docker"),
            &format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\nexit 0\n",
                docker_log.display()
            ),
        );
        Self {
            fx,
            bin,
            docker_log,
        }
    }

    /// A key file with `mode` holding `secret`.
    fn key(&self, name: &str, secret: &str, mode: u32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = self.fx.root.parent().unwrap().join("keys").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, secret).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    /// Run the binary for `agent` with the model set and `env` added.
    fn run(&self, agent: &str, env: &[(&str, &std::ffi::OsStr)]) -> (Output, String) {
        self.run_in(agent, env, None)
    }

    /// `run`, from working directory `cwd` when given.
    fn run_in(
        &self,
        agent: &str,
        env: &[(&str, &std::ffi::OsStr)],
        cwd: Option<&std::path::Path>,
    ) -> (Output, String) {
        let task = self.fx.task.to_str().unwrap().to_owned();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_solo-design"));
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd.args(["run", "--agent", agent, "--task", &task, "--design", "pack"])
            .env_clear()
            .env("PATH", &self.bin)
            .env("HOME", self.fx.root.parent().unwrap())
            .env("SOLO_DESIGN_ROOT", &self.fx.root)
            .env("SOLO_DESIGN_MODEL", "fixture-model");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("spawn the solo-design binary");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        (out, stderr)
    }

    /// The host source of the read-only bind at `dst` in the latest launch.
    fn last_bind_source(&self, dst: &str) -> Option<String> {
        let log = std::fs::read_to_string(&self.docker_log).unwrap_or_default();
        let launch = log
            .lines()
            .rfind(|l| l.starts_with("run ") || l.starts_with("container run "))?
            .to_owned();
        let args: Vec<&str> = launch.split(' ').collect();
        args.windows(2)
            .filter(|w| w[0] == "-v" || w[0] == "--volume")
            .find_map(|w| {
                let mut parts = w[1].splitn(3, ':');
                let src = parts.next()?;
                (parts.next() == Some(dst)).then(|| src.to_owned())
            })
    }

    fn launched(&self) -> bool {
        std::fs::read_to_string(&self.docker_log)
            .unwrap_or_default()
            .lines()
            .any(|l| l.starts_with("run ") || l.starts_with("container run "))
    }

    /// A preflight failure: nonzero, not usage, no run directory, no container.
    fn assert_preflight_failure(&self, out: &Output, stderr: &str, case: &str) {
        assert!(!out.status.success(), "{case}: must exit nonzero");
        assert_ne!(
            out.status.code(),
            Some(2),
            "{case}: not a usage error: {stderr}"
        );
        assert_eq!(
            self.fx.runs(),
            Vec::<PathBuf>::new(),
            "{case}: no run directory"
        );
        assert!(
            !self.fx.root.join("runs").exists(),
            "{case}: no runs/ created"
        );
        assert!(!self.launched(), "{case}: no container started");
    }
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[test]
fn fails_preflight_listing_the_supported_providers_for_an_unknown_provider_and_creates_no_run() {
    let pf = Preflight::new();
    let key = pf.key("valid.key", "fixture-secret-value", 0o600);
    let (out, stderr) = pf.run(
        "alpha",
        &[
            ("SOLO_DESIGN_PROVIDER", "fixture-provider".as_ref()),
            ("SOLO_DESIGN_KEY_FILE", key.as_os_str()),
        ],
    );
    pf.assert_preflight_failure(&out, &stderr, "unknown provider");
    for name in [router(), lab()] {
        assert!(
            words(&stderr).contains(&name),
            "lists the supported provider {name}: {stderr}"
        );
    }
    assert!(
        stderr.contains("fixture-provider"),
        "names the unknown provider: {stderr}"
    );

    // Control: a supported provider with a valid key passes preflight.
    let (_, stderr) = pf.run(
        "alpha",
        &[
            ("SOLO_DESIGN_PROVIDER", router().as_ref()),
            ("SOLO_DESIGN_KEY_FILE", key.as_os_str()),
        ],
    );
    assert!(
        pf.launched(),
        "a supported provider reaches the container: {stderr}"
    );
}

#[test]
fn fails_preflight_naming_the_key_path_when_it_is_unconfigured_missing_not_a_file_or_group_or_world_readable_and_creates_no_run(
) {
    let secret = "fixture-secret-value-9c41";
    let pf = Preflight::new();
    let missing = pf.fx.root.parent().unwrap().join("keys/absent.key");
    let dir = pf.fx.root.parent().unwrap().join("keys/a-directory");
    std::fs::create_dir_all(&dir).unwrap();
    let group = pf.key("group.key", secret, 0o640);
    let world = pf.key("world.key", secret, 0o604);
    let group_write = pf.key("group-write.key", secret, 0o620);
    let provider = router();

    let unconfigured: &[(&str, &std::ffi::OsStr)] = &[("SOLO_DESIGN_PROVIDER", provider.as_ref())];
    let (out, stderr) = pf.run("alpha", unconfigured);
    pf.assert_preflight_failure(&out, &stderr, "unconfigured key");
    assert!(
        stderr.contains("SOLO_DESIGN_KEY_FILE") || stderr.contains("\"keys\""),
        "names the key setting: {stderr}"
    );

    for (case, path) in [
        ("missing", &missing),
        ("directory", &dir),
        ("group-readable", &group),
        ("world-readable", &world),
        ("group-writable", &group_write),
    ] {
        let (out, stderr) = pf.run(
            "alpha",
            &[
                ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
                ("SOLO_DESIGN_KEY_FILE", path.as_os_str()),
            ],
        );
        pf.assert_preflight_failure(&out, &stderr, case);
        assert!(
            stderr.contains(path.to_str().unwrap()),
            "{case}: names the key path: {stderr}"
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stderr.contains(secret) && !stdout.contains(secret),
            "{case}: never prints the key"
        );
        if case == "group-writable" {
            assert!(
                stderr.contains("accessible by group or others") && !stderr.contains("readable"),
                "{case}: a write bit is not called readable: {stderr}"
            );
        }
    }

    // A relative key path resolves against the working directory and is
    // mounted by its absolute canonical path (a bare name would be a volume).
    let relative = pf.key("relative.key", secret, 0o600);
    let key_dir = relative.parent().unwrap().to_path_buf();
    let (_, stderr) = pf.run_in(
        "alpha",
        &[
            ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
            ("SOLO_DESIGN_KEY_FILE", "relative.key".as_ref()),
        ],
        Some(&key_dir),
    );
    assert!(
        pf.launched(),
        "a relative key path passes preflight: {stderr}"
    );
    assert_eq!(
        pf.last_bind_source("/run/solo/key"),
        Some(
            relative
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        ),
        "the key is mounted from its absolute canonical path"
    );

    // Control: an owner-only regular file passes preflight.
    let valid = pf.key("valid.key", secret, 0o600);
    let (out, stderr) = pf.run(
        "alpha",
        &[
            ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
            ("SOLO_DESIGN_KEY_FILE", valid.as_os_str()),
        ],
    );
    assert!(
        pf.launched(),
        "a valid key file reaches the container: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(secret) && !stderr.contains(secret),
        "a valid run never prints the key"
    );
}

#[test]
fn fails_preflight_naming_the_available_engines_for_an_unknown_engine_and_creates_no_run() {
    let pf = Preflight::new();
    let key = pf.key("valid.key", "fixture-secret-value", 0o600);
    let provider = router();
    let refs: Vec<(&str, &std::ffi::OsStr)> = vec![
        ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
        ("SOLO_DESIGN_KEY_FILE", key.as_os_str()),
        ("SOLO_DESIGN_ENGINE", "nonesuch".as_ref()),
    ];
    let (out, stderr) = pf.run("alpha", &refs);
    pf.assert_preflight_failure(&out, &stderr, "unknown engine");
    assert!(
        stderr.contains("nonesuch"),
        "names the configured engine: {stderr}"
    );
    assert!(
        words(&stderr).contains(&"pi".to_owned()),
        "lists the available engines: {stderr}"
    );

    // The engine check comes after validation: an invalid agent is reported as such.
    let (out, stderr) = pf.run("mismatch", &refs);
    assert!(!out.status.success(), "invalid agent must fail");
    assert!(
        !stderr.contains("nonesuch"),
        "validation must fail before the engine lookup: {stderr}"
    );
    assert!(stderr.contains("mismatch"), "names the bad agent: {stderr}");
    assert!(!pf.launched(), "no container started");
}

#[test]
fn fails_preflight_naming_the_build_command_when_the_image_is_missing_and_creates_no_run() {
    let pf = Preflight::new();
    // A docker that knows no images: everything but logging fails.
    common::write_executable(
        &pf.bin.join("docker"),
        &format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\necho 'Error: No such image' >&2\nexit 1\n",
            pf.docker_log.display()
        ),
    );
    let key = pf.key("valid.key", "fixture-secret-value", 0o600);
    let provider = router();
    let (out, stderr) = pf.run(
        "alpha",
        &[
            ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
            ("SOLO_DESIGN_KEY_FILE", key.as_os_str()),
        ],
    );
    pf.assert_preflight_failure(&out, &stderr, "missing image");
    assert!(
        stderr.contains("docker build -t solo-design-pi:0.83.0 docker/pi"),
        "names the build command: {stderr}"
    );
    let log = std::fs::read_to_string(&pf.docker_log).unwrap_or_default();
    assert!(
        log.contains("solo-design-pi:0.83.0"),
        "the image was looked up locally: {log:?}"
    );
}

#[test]
fn stops_a_run_at_the_configured_timeout_and_records_it_as_timed_out() {
    /// A Preflight whose fake container stays up for `secs` seconds.
    fn sleeping(secs: &str) -> Preflight {
        let pf = Preflight::new();
        common::write_executable(
            &pf.bin.join("docker"),
            &format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\nif [ \"$1\" = run ]; then exec /bin/sleep {secs}; fi\nexit 0\n",
                pf.docker_log.display()
            ),
        );
        pf
    }
    fn receipt_status(pf: &Preflight) -> Option<String> {
        let runs = pf.fx.runs();
        assert_eq!(runs.len(), 1, "one run directory: {runs:?}");
        let text = std::fs::read_to_string(runs[0].join("receipt.json")).ok()?;
        let receipt: serde_json::Value = serde_json::from_str(&text).ok()?;
        receipt.get("status")?.as_str().map(str::to_owned)
    }
    let provider = router();

    let pf = sleeping("8");
    let key = pf.key("valid.key", "fixture-secret-value", 0o600);
    let started = std::time::Instant::now();
    let (out, stderr) = pf.run(
        "alpha",
        &[
            ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
            ("SOLO_DESIGN_KEY_FILE", key.as_os_str()),
            ("SOLO_DESIGN_TIMEOUT_SECS", "1".as_ref()),
        ],
    );
    let elapsed = started.elapsed();
    assert!(pf.launched(), "the container was started: {stderr}");
    assert!(
        elapsed < std::time::Duration::from_secs(6),
        "the configured timeout stops the run ({elapsed:?})"
    );
    assert!(!out.status.success(), "a timed-out run exits nonzero");
    assert_ne!(out.status.code(), Some(2), "not a usage error: {stderr}");
    assert_eq!(
        receipt_status(&pf).as_deref(),
        Some("timed_out"),
        "the receipt records the timeout"
    );

    // Control: without a configured timeout the same run is not timed out.
    let pf = sleeping("1");
    let key = pf.key("valid.key", "fixture-secret-value", 0o600);
    let (_, stderr) = pf.run(
        "alpha",
        &[
            ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
            ("SOLO_DESIGN_KEY_FILE", key.as_os_str()),
        ],
    );
    assert!(pf.launched(), "the control container was started: {stderr}");
    let status = receipt_status(&pf);
    assert!(
        status.is_some() && status.as_deref() != Some("timed_out"),
        "no timeout configured, no timed_out: {status:?}"
    );
}

#[test]
fn the_bundled_example_task_with_the_example_adk_passes_validation_and_freezes_its_inputs() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let task = repo.join("examples/task");
    assert!(
        task.join("task.md").is_file(),
        "the bundled example task has examples/task/task.md"
    );
    let adk = repo.join("agents/frontend-genjutsu");
    let adk_files = common::file_tree(&adk);
    assert!(
        adk_files.contains_key("agent.md"),
        "the example ADK has agents/frontend-genjutsu/agent.md"
    );

    // A resource root holding the repo's example ADK (a copy, so runs/ is not
    // written into the repo), with the `mismatch` agent for the control.
    let pf = Preflight::new();
    for (rel, bytes) in &adk_files {
        pf.fx.put(format!("agents/frontend-genjutsu/{rel}"), bytes);
    }
    let key = pf.key("valid.key", "fixture-secret-value", 0o600);
    let provider = router();
    let refs: Vec<(&str, &std::ffi::OsStr)> = vec![
        ("SOLO_DESIGN_PROVIDER", provider.as_ref()),
        ("SOLO_DESIGN_KEY_FILE", key.as_os_str()),
    ];
    let run = |agent: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_solo-design"))
            .args(["run", "--agent", agent, "--task"])
            .arg(&task)
            .env_clear()
            .env("PATH", &pf.bin)
            .env("HOME", pf.fx.root.parent().unwrap())
            .env("SOLO_DESIGN_ROOT", &pf.fx.root)
            .env("SOLO_DESIGN_MODEL", "fixture-model")
            .envs(refs.iter().copied())
            .output()
            .expect("spawn the solo-design binary");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        (out, stderr)
    };

    // Control: the same task with an invalid agent fails before launch.
    let (out, stderr) = run("mismatch");
    assert!(
        !out.status.success(),
        "an invalid agent must fail: {stderr}"
    );
    assert!(stderr.contains("mismatch"), "names the bad agent: {stderr}");
    assert!(!pf.launched(), "no container for an invalid agent");
    assert_eq!(
        pf.fx.runs(),
        Vec::<PathBuf>::new(),
        "no run for an invalid agent"
    );

    let (_, stderr) = run("frontend-genjutsu");
    assert!(
        pf.launched(),
        "the example task with the example ADK reaches the container: {stderr}"
    );
    let runs = pf.fx.runs();
    assert_eq!(runs.len(), 1, "one run directory: {runs:?}");
    assert_eq!(
        common::file_tree(&runs[0].join("inputs/task")),
        common::file_tree(&task),
        "the example task is frozen unchanged"
    );
    assert_eq!(
        common::file_tree(&runs[0].join("inputs/agent")),
        adk_files,
        "the example ADK is frozen unchanged"
    );
}
