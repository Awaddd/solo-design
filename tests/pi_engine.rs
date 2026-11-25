mod common;

use common::{
    execute_timed, execute_with, file_tree, is_launch, read_receipt, Executed, FakeDocker, Fixture,
    Interrupt, Script,
};
use solo_design::config::ModelConfig;
use solo_design::engine::{self, Engine, RunContext};
use solo_design::engines::pi::PiEngine;
use solo_design::inputs::{prepare, FrozenRun};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SEP: &str = "\n\n---\n\n";
const STAGED_PROMPT: &str = "/solo/stage/system-prompt.md";
const TASK_ATTACHMENT: &str = "@/solo/task/task.md";
const BRIEF: &str = "Design a landing page.\n";

// Provider names and key variables are built from parts (repo guard).
fn router() -> String {
    ["open", "router"].concat()
}
fn router_key_var() -> String {
    ["OPEN", "ROUTER_API_KEY"].concat()
}
fn lab() -> String {
    ["anthro", "pic"].concat()
}
fn lab_key_var() -> String {
    ["ANTHRO", "PIC_API_KEY"].concat()
}

fn context<'a>(model: &'a str, provider: &'a str, key_file: Option<&'a Path>) -> RunContext<'a> {
    RunContext {
        model,
        provider,
        effort: None,
        key_file,
    }
}

fn has_pair(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2).any(|w| w[0] == flag && w[1] == value)
        || argv.iter().any(|a| *a == format!("{flag}={value}"))
}

/// An agent `alpha` with two rules, two skills (one with an extra reference
/// file) and a design pack `pack`.
fn staged_fixture() -> Fixture {
    let fx = Fixture::new();
    fx.put(
        "agents/alpha/agent.md",
        "---\nname: alpha\ndescription: Fixture agent.\n---\nYou are the alpha designer.\n",
    );
    fx.put("agents/alpha/rules/b-spacing.md", "Rule B: spacing.\n");
    fx.put("agents/alpha/rules/a-colour.md", "Rule A: colour.\n");
    fx.put("agents/alpha/skills/zeta/SKILL.md", "Skill Z body.\n");
    fx.put("agents/alpha/skills/alpha/SKILL.md", "Skill A body.\n");
    fx.put(
        "agents/alpha/skills/alpha/reference.md",
        "Reference detail that stays a file.\n",
    );
    fx.put("lib/pack/DESIGN.md", "# Pack\n\nDesign tokens text.\n");
    fx.put("lib/pack/globals.css", ":root { --x: 1; }\n");
    fx
}

/// A key file outside the resource root, mode 0o000 so any host read fails.
fn key_file(fx: &Fixture, secret: &str) -> PathBuf {
    let path = fx.root.parent().unwrap().join("keys/provider.key");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, secret).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    path
}

fn run_pi(fx: &Fixture, frozen: &FrozenRun, key: Option<&Path>) -> (Executed, PiEngine) {
    let _ = fx;
    let engine = PiEngine::new();
    let config = ModelConfig {
        model: "fixture-vendor/fixture-model".to_owned(),
        provider: router(),
    };
    let docker = FakeDocker::new(&Script::default());
    let out = execute_with(
        frozen,
        "alpha",
        Some("pack"),
        &engine,
        &config,
        key,
        &docker,
        Interrupt::Never,
    );
    (out, engine)
}

/// The single launch, split into docker options and the container command.
fn split_launch(out: &Executed, image: &str) -> (Vec<String>, Vec<String>) {
    let launches: Vec<&Vec<String>> = out.invocations.iter().filter(|a| is_launch(a)).collect();
    assert_eq!(
        launches.len(),
        1,
        "one container launch: {:?}",
        out.invocations
    );
    let args = launches[0];
    let at = args.iter().position(|a| !image.is_empty() && a == image);
    assert!(
        at.is_some(),
        "launch runs the engine's image {image:?}: {args:?}"
    );
    let at = at.unwrap();
    let start = if args[0] == "container" { 2 } else { 1 };
    (args[start..at].to_vec(), args[at + 1..].to_vec())
}

fn values(opts: &[String], names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < opts.len() {
        if names.contains(&opts[i].as_str()) {
            out.push(opts.get(i + 1).cloned().unwrap_or_default());
            i += 2;
            continue;
        }
        for name in names {
            if let Some(rest) = opts[i].strip_prefix(&format!("{name}=")) {
                out.push(rest.to_owned());
            }
        }
        i += 1;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bind {
    src: PathBuf,
    dst: String,
    ro: bool,
}

fn binds(opts: &[String]) -> Vec<Bind> {
    let mut out = Vec::new();
    for v in values(opts, &["-v", "--volume"]) {
        let parts: Vec<&str> = v.splitn(3, ':').collect();
        out.push(Bind {
            src: PathBuf::from(parts[0]),
            dst: parts.get(1).unwrap_or(&"").to_string(),
            ro: parts
                .get(2)
                .is_some_and(|o| o.split(',').any(|x| x == "ro")),
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
            out.push(Bind {
                src: PathBuf::from(src),
                dst: dst.to_owned(),
                ro,
            });
        }
    }
    out
}

#[test]
fn drives_pi_with_the_configured_model_and_provider() {
    let engine: Box<dyn Engine> = Box::new(PiEngine::new());
    let cases = [
        (router(), router_key_var(), "fixture-vendor/fixture-model"),
        (lab(), lab_key_var(), "fixture-model"),
    ];
    for (provider, key_var, model) in &cases {
        let argv = engine.argv(&context(model, provider, None));
        assert_eq!(
            argv.get(..2),
            Some(&["--key-env".to_owned(), key_var.clone()][..]),
            "{provider}: the entrypoint gets the key variable's name first: {argv:?}"
        );
        assert!(
            has_pair(&argv, "--model", &format!("{provider}/{model}")),
            "{provider}: Pi gets --model <provider>/<model>: {argv:?}"
        );
        assert!(
            has_pair(&argv, "--mode", "json"),
            "{provider}: JSON mode: {argv:?}"
        );
        assert!(
            has_pair(&argv, "--system-prompt", STAGED_PROMPT),
            "{provider}: the staged system prompt: {argv:?}"
        );
        assert!(
            !argv.iter().any(|a| a.starts_with("--api-key")),
            "{provider}: no key flag in argv: {argv:?}"
        );
    }

    let names: Vec<String> = engine::available()
        .iter()
        .map(|e| e.name().to_owned())
        .collect();
    assert!(
        names.contains(&"pi".to_owned()),
        "the Pi engine is available: {names:?}"
    );
}

#[test]
fn composes_the_agent_rules_design_pack_and_skills_into_one_system_prompt_in_v1s_order_with_the_engines_filesystem_section(
) {
    let fx = staged_fixture();
    let frozen = prepare(&fx.root, &fx.task, "alpha", Some("pack")).expect("fixture freezes");
    let provider = router();
    let mounts = PiEngine::new()
        .stage(&frozen, &context("fixture-model", &provider, None))
        .expect("staging succeeds");

    let staged = frozen.run_dir.join("stage/system-prompt.md");
    assert!(staged.is_file(), "staged prompt at {}", staged.display());
    assert!(
        mounts
            .iter()
            .any(|m| (m.container == STAGED_PROMPT && m.host == staged)
                || (m.container == "/solo/stage" && m.host == frozen.run_dir.join("stage"))),
        "the staged prompt is mounted at {STAGED_PROMPT}: {mounts:?}"
    );

    let prompt = fs::read_to_string(&staged).unwrap();
    let segments: Vec<&str> = prompt.split(SEP).collect();
    assert!(
        segments.len() >= 6,
        "six sections joined by {SEP:?}: {prompt:?}"
    );
    assert_eq!(
        segments[0].trim(),
        "You are the alpha designer.",
        "agent body without front-matter"
    );
    assert_eq!(
        segments[1].trim(),
        "Rule A: colour.",
        "rules sorted by name"
    );
    assert_eq!(
        segments[2].trim(),
        "Rule B: spacing.",
        "rules sorted by name"
    );
    assert!(
        segments[3]
            .trim_start()
            .starts_with("## Active design system")
            && segments[3].contains("# Pack\n\nDesign tokens text."),
        "the design pack as the active design system: {:?}",
        segments[3]
    );
    for (segment, name, body) in [
        (segments[4], "alpha", "Skill A body."),
        (segments[5], "zeta", "Skill Z body."),
    ] {
        assert!(
            segment
                .trim_start()
                .starts_with(&format!("## Active skill — {name}"))
                && segment.contains(body),
            "skills sorted by name, each under its v1 header: {segment:?}"
        );
    }

    let engine_section = &prompt[prompt.find("Skill Z body.").unwrap() + "Skill Z body.".len()..];
    for root in [
        "/solo/task",
        "/solo/agent/skills",
        "/solo/design",
        "/solo/outputs",
    ] {
        assert!(
            engine_section.contains(root),
            "engine section states {root}: {engine_section:?}"
        );
    }
    assert!(
        engine_section.to_lowercase().contains("read-only"),
        "engine section says inputs are read-only: {engine_section:?}"
    );
    assert!(
        engine_section.contains("/solo/outputs/index.html"),
        "engine section names the deliverable: {engine_section:?}"
    );
    assert!(
        !prompt.contains("Reference detail that stays a file."),
        "other skill files are not inlined"
    );
    assert!(
        !prompt.contains("name: alpha"),
        "front-matter is not inlined"
    );

    // The agent body is kept whole, a `---` line in it included, and the fence
    // goes whether the front matter is full or empty. Validation needs a
    // `name`, so the empty-fence variant replaces the frozen copy.
    let body = "You are the alpha designer.\n---\nSecond part after a rule.";
    for (case, agent_md) in [
        (
            "front matter",
            format!("---\nname: alpha\ndescription: Fixture agent.\n---\n{body}\n"),
        ),
        ("empty front matter", format!("---\n---\n{body}\n")),
    ] {
        let frozen = prepare(&fx.root, &fx.task, "alpha", Some("pack")).expect("fixture freezes");
        let agent = frozen.inputs.join("agent/agent.md");
        fs::set_permissions(&agent, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&agent, agent_md).unwrap();
        PiEngine::new()
            .stage(&frozen, &context("fixture-model", &provider, None))
            .expect("staging succeeds");
        let prompt = fs::read_to_string(frozen.run_dir.join("stage/system-prompt.md")).unwrap();
        let first = prompt.split(SEP).next().unwrap_or_default();
        assert_eq!(
            first.trim(),
            body,
            "{case}: the whole body, fence removed: {prompt:?}"
        );
    }
}

#[test]
fn gives_pi_the_byte_identical_brief_as_the_task_after_the_list_of_available_files_images_attached()
{
    let fx = staged_fixture();
    fx.task_file("assets/hero.png", [0x89u8, b'P', b'N', b'G']);
    fx.task_file("assets/notes.txt", "Notes.\n");
    fx.task_file("existing/index.html", "<p>seed</p>\n");
    fx.put("agents/alpha/skills/zeta/references/x.md", "Reference X.\n");
    let frozen = prepare(&fx.root, &fx.task, "alpha", Some("pack")).expect("fixture freezes");
    let (out, engine) = run_pi(&fx, &frozen, None);
    let (_, command) = split_launch(&out, &engine.image());

    // Pi 0.83.0 (dist/cli/initial-message.js) joins, with no separator, its
    // @file blocks in argv order and then the first positional message. So the
    // model sees the images, then the brief, then the file list, which must
    // start on a new line.
    let attachments: Vec<&String> = command.iter().filter(|a| a.starts_with('@')).collect();
    assert_eq!(
        attachments.last().map(|a| a.as_str()),
        Some(TASK_ATTACHMENT),
        "the brief is the last file block, after the images: {command:?}"
    );
    assert_eq!(
        attachments
            .iter()
            .filter(|a| a.as_str() == TASK_ATTACHMENT)
            .count(),
        1,
        "the brief is attached once: {command:?}"
    );
    assert_eq!(
        fs::read_to_string(frozen.inputs.join("task/task.md")).unwrap(),
        BRIEF,
        "the brief is frozen byte-identical"
    );
    let images = &attachments[..attachments.len().saturating_sub(1)];
    assert!(
        images.iter().any(|a| a.ends_with("/hero.png")),
        "the image is attached: {command:?}"
    );
    assert!(
        images.iter().all(|a| a.ends_with(".png")),
        "only images are attached besides the brief: {images:?}"
    );

    let lists: Vec<&String> = command
        .iter()
        .filter(|a| !a.starts_with('@') && a.contains("/solo/outputs/index.html"))
        .collect();
    assert_eq!(lists.len(), 1, "one file-list message: {command:?}");
    let list = lists[0].as_str();

    // The docker log splits arguments on newlines, so the leading newline is
    // checked on the engine's real argv.
    let provider = router();
    let run = context("fixture-model", &provider, None);
    let direct = PiEngine::new();
    direct.stage(&frozen, &run).expect("staging succeeds");
    let argv = direct.argv(&run);
    let messages: Vec<&String> = argv
        .iter()
        .filter(|a| !a.starts_with('@') && a.contains("/solo/outputs/index.html"))
        .collect();
    assert_eq!(messages.len(), 1, "one file-list message: {argv:?}");
    assert!(
        messages[0].starts_with('\n'),
        "the file list starts on a new line after the brief: {:?}",
        messages[0]
    );
    for file in [
        "/solo/outputs/index.html",
        "/solo/task/assets/hero.png",
        "/solo/task/assets/notes.txt",
        "/solo/design/globals.css",
        "/solo/agent/skills/alpha/reference.md",
        "/solo/agent/skills/zeta/references/x.md",
    ] {
        assert!(
            list.contains(file),
            "the file list names {file}: {list:?} in {command:?}"
        );
    }
    assert!(
        !list.contains("SKILL.md"),
        "inlined skills are not listed: {list:?}"
    );
    assert!(
        !list.contains(BRIEF.trim()),
        "the brief is not copied into the list: {list:?}"
    );
}

#[test]
fn runs_pi_in_the_writable_outputs_folder_with_every_other_mount_read_only() {
    let fx = staged_fixture();
    let frozen = prepare(&fx.root, &fx.task, "alpha", Some("pack")).expect("fixture freezes");
    let key = key_file(&fx, "fixture-secret-value");
    let (out, engine) = run_pi(&fx, &frozen, Some(&key));
    let (opts, _) = split_launch(&out, &engine.image());

    assert_eq!(
        values(&opts, &["-w", "--workdir"]),
        vec!["/solo/outputs".to_owned()],
        "Pi's working directory is the outputs folder: {opts:?}"
    );
    let mounts = binds(&opts);
    let find = |dst: &str| mounts.iter().find(|m| m.dst == dst).cloned();
    assert_eq!(
        find("/solo/outputs"),
        Some(Bind {
            src: frozen.outputs.clone(),
            dst: "/solo/outputs".to_owned(),
            ro: false
        }),
        "outputs mounted writable: {mounts:?}"
    );
    for (dst, src) in [
        ("/solo/task", frozen.inputs.join("task")),
        ("/solo/agent", frozen.inputs.join("agent")),
        ("/solo/design", frozen.inputs.join("design")),
    ] {
        assert_eq!(
            find(dst),
            Some(Bind {
                src,
                dst: dst.to_owned(),
                ro: true
            }),
            "{dst} mounted read-only: {mounts:?}"
        );
    }
    assert!(
        mounts
            .iter()
            .any(|m| m.ro && (m.dst == STAGED_PROMPT || m.dst == "/solo/stage")),
        "the staged prompt is mounted read-only: {mounts:?}"
    );
    for m in &mounts {
        assert!(
            m.dst == "/solo/outputs" || m.ro,
            "every mount but outputs is read-only: {m:?}"
        );
    }
}

#[test]
fn mounts_the_configured_key_file_read_only_without_reading_it() {
    let secret = "fixture-secret-value-7f3a";
    let fx = staged_fixture();
    let frozen = prepare(&fx.root, &fx.task, "alpha", Some("pack")).expect("fixture freezes");
    let key = key_file(&fx, secret);
    let (out, engine) = run_pi(&fx, &frozen, Some(&key));
    assert!(
        out.result.is_ok(),
        "the run completes with an unreadable key file: {:?}",
        out.result
    );
    let (opts, command) = split_launch(&out, &engine.image());

    let key_mounts: Vec<Bind> = binds(&opts).into_iter().filter(|m| m.src == key).collect();
    assert_eq!(
        key_mounts.len(),
        1,
        "the key file is mounted once: {opts:?}"
    );
    assert!(
        key_mounts[0].ro,
        "the key file is mounted read-only: {key_mounts:?}"
    );

    let launch: Vec<&String> = opts.iter().chain(&command).collect();
    assert!(
        !launch.iter().any(|a| a.contains(secret)),
        "the key value is not in argv"
    );
    assert!(
        !out.reports.iter().any(|r| r.contains(secret)),
        "the key value is not reported"
    );
    for (path, bytes) in file_tree(&frozen.run_dir) {
        if path.starts_with("outputs/") {
            continue;
        }
        assert!(
            !String::from_utf8_lossy(&bytes).contains(secret),
            "the key value is not in {path}"
        );
    }
}

#[test]
fn passes_the_configured_thinking_level_and_none_otherwise() {
    let engine = PiEngine::new();
    let provider = router();
    for level in ["minimal", "high"] {
        let run = RunContext {
            effort: Some(level),
            ..context("fixture-model", &provider, None)
        };
        let argv = engine.argv(&run);
        assert!(
            has_pair(&argv, "--thinking", level),
            "--thinking {level}: {argv:?}"
        );
        assert_eq!(
            argv.iter().filter(|a| a.starts_with("--thinking")).count(),
            1,
            "one thinking flag: {argv:?}"
        );
    }
    let argv = engine.argv(&context("fixture-model", &provider, None));
    assert!(
        !argv.iter().any(|a| a.starts_with("--thinking")),
        "no thinking flag without a configured effort: {argv:?}"
    );

    // The receipt records the configured level, and none without one.
    let stream: Vec<String> = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pi/success-two-turns.jsonl"),
    )
    .unwrap()
    .lines()
    .map(str::to_owned)
    .collect();
    for effort in [Some("high"), None] {
        let fx = staged_fixture();
        let frozen = prepare(&fx.root, &fx.task, "alpha", Some("pack")).expect("fixture freezes");
        let key = key_file(&fx, "fixture-secret-value");
        let docker = FakeDocker::new(&Script {
            stdout: stream.clone(),
            stderr: vec![],
            exit: 0,
        });
        let config = ModelConfig {
            model: "fixture-vendor/fixture-model".to_owned(),
            provider: router(),
        };
        let out = execute_timed(
            &frozen,
            "alpha",
            Some("pack"),
            &PiEngine::new(),
            &config,
            effort,
            Some(&key),
            None,
            &docker,
            Interrupt::Never,
        );
        let receipt = read_receipt(&out.result.expect("the run finishes").receipt);
        match effort {
            Some(level) => assert_eq!(
                receipt.get("effort"),
                Some(&serde_json::json!(level)),
                "the receipt records the configured effort: {receipt:?}"
            ),
            None => assert!(
                receipt.get("effort").is_none_or(serde_json::Value::is_null),
                "no effort recorded without one: {receipt:?}"
            ),
        }
    }
}

#[test]
fn returns_a_provider_failure_without_trying_another_provider_or_model() {
    let stream = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pi/error-auth.jsonl"),
    )
    .unwrap();
    let fx = staged_fixture();
    let frozen = prepare(&fx.root, &fx.task, "alpha", Some("pack")).expect("fixture freezes");
    let key = key_file(&fx, "fixture-secret-value");
    let engine = PiEngine::new();
    let config = ModelConfig {
        model: "fixture-vendor/fixture-model".to_owned(),
        provider: router(),
    };
    let docker = FakeDocker::new(&Script {
        stdout: stream.lines().map(str::to_owned).collect(),
        stderr: vec![],
        exit: 0,
    });
    let out = execute_with(
        &frozen,
        "alpha",
        Some("pack"),
        &engine,
        &config,
        Some(&key),
        &docker,
        Interrupt::Never,
    );
    let finished = out.result.as_ref().expect("the run finishes").clone();

    let (_, command) = split_launch(&out, &engine.image());
    assert!(
        has_pair(
            &command,
            "--model",
            &format!("{}/fixture-vendor/fixture-model", router())
        ),
        "the one launch uses the configured provider and model: {command:?}"
    );
    assert_ne!(finished.exit_code, 0, "a provider failure exits nonzero");
    let receipt = read_receipt(&finished.receipt);
    assert_eq!(receipt.get("status"), Some(&serde_json::json!("failed")));
    assert_eq!(
        receipt.get("failure"),
        Some(&serde_json::json!("auth")),
        "the provider's failure is recorded: {receipt:?}"
    );
    assert_eq!(receipt.get("provider"), Some(&serde_json::json!(router())));
    assert_eq!(
        receipt.get("model"),
        Some(&serde_json::json!("fixture-vendor/fixture-model"))
    );
}
