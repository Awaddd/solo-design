mod common;

use common::{
    bind_sources, execute_job, file_tree, hashes, is_launch, read_receipt, FakeDocker, Fixture,
    Interrupt, Script, TestEngine,
};
use serde_json::{json, Value};
use solo_design::config::ModelConfig;
use solo_design::inputs::{prepare, FrozenRun};
use solo_design::receipt::{Status, FIELDS};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn config() -> ModelConfig {
    ModelConfig {
        model: "fixture-model".to_owned(),
        provider: "fixture-provider".to_owned(),
    }
}

fn fixture() -> (Fixture, PathBuf) {
    let fx = Fixture::new();
    fx.agent("alpha");
    fx.design("pack");
    fx.task_file("existing/index.html", "<p>seed</p>");
    let auth = fx.root.parent().unwrap().join("auth");
    fs::create_dir_all(&auth).unwrap();
    fs::write(auth.join("credentials"), "fixture-secret-value").unwrap();
    (fx, auth)
}

fn freeze(fx: &Fixture, design: Option<&str>) -> FrozenRun {
    prepare(&fx.root, &fx.task, "alpha", design).expect("fixture selection freezes")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn hashes_json(map: &std::collections::BTreeMap<String, String>) -> Value {
    Value::Object(
        map.iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
    )
}

fn reporting_script(engine: &TestEngine, ok: bool, exit: i32) -> Script {
    let mut stdout = vec![
        format!("{}-EFFORT fixture-effort", engine.marker),
        format!("{}-TOKENS 1234", engine.marker),
        format!("{}-USAGE 1000 234", engine.marker),
    ];
    if ok {
        stdout.push(format!("{}-RESULT ok", engine.marker));
    }
    Script {
        stdout,
        stderr: vec![],
        exit,
    }
}

#[test]
fn records_status_exit_paths_selection_configured_model_and_provider_times_and_hashes_in_the_receipt(
) {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);

    // A successful run with a design, the engine reporting effort, tokens and usage.
    let frozen = freeze(&fx, Some("pack"));
    let docker = FakeDocker::new(&reporting_script(&engine, true, 0));
    let before = now();
    let out = execute_job(
        &frozen,
        "alpha",
        Some("pack"),
        &engine,
        &config(),
        &docker,
        Interrupt::Never,
    );
    let after = now();
    let finished = out.result.expect("the run finishes");
    assert_eq!(finished.status, Status::Succeeded);
    assert_eq!(finished.exit_code, 0);
    assert_eq!(finished.receipt, frozen.run_dir.join("receipt.json"));
    let receipt = read_receipt(&finished.receipt);
    let expect = [
        ("status", json!("succeeded")),
        ("exit_code", json!(0)),
        ("signal", Value::Null),
        ("output_path", json!(frozen.outputs.to_str().unwrap())),
        ("agent", json!("alpha")),
        ("design", json!("pack")),
        ("engine", json!("alpha-engine")),
        ("model", json!("fixture-model")),
        ("provider", json!("fixture-provider")),
        ("effort", json!("fixture-effort")),
        ("tokens", json!(1234)),
        ("usage", json!({"input_tokens": 1000, "output_tokens": 234})),
        ("input_hashes", hashes_json(&frozen.hashes)),
        ("output_hashes", hashes_json(&hashes(&frozen.outputs))),
    ];
    for (key, value) in expect {
        assert_eq!(receipt.get(key), Some(&value), "receipt field {key}");
    }
    assert!(
        receipt["output_hashes"]
            .as_object()
            .is_some_and(|m| m.contains_key("index.html")),
        "output hashes cover the outputs"
    );
    let started = receipt["started_at"]
        .as_u64()
        .expect("started_at in unix seconds");
    let ended = receipt["ended_at"]
        .as_u64()
        .expect("ended_at in unix seconds");
    assert!(
        before <= started && started <= ended && ended <= after,
        "times {started}..{ended} within {before}..{after}"
    );

    // A failed run without a design and without effort, tokens or usage.
    let frozen = freeze(&fx, None);
    let docker = FakeDocker::new(&Script {
        stdout: vec![],
        stderr: vec![],
        exit: 3,
    });
    let out = execute_job(
        &frozen,
        "alpha",
        None,
        &engine,
        &config(),
        &docker,
        Interrupt::Never,
    );
    let finished = out.result.expect("the run finishes");
    assert_eq!(finished.status, Status::Failed);
    assert_ne!(finished.exit_code, 0, "a failed run exits nonzero");
    let receipt = read_receipt(&finished.receipt);
    assert_eq!(receipt.get("status"), Some(&json!("failed")));
    assert_eq!(receipt.get("exit_code"), Some(&json!(3)));
    assert_eq!(receipt.get("design"), Some(&Value::Null));
    for optional in ["effort", "tokens", "usage"] {
        assert!(
            receipt.get(optional).is_none_or(Value::is_null),
            "{optional} only when the engine reports it"
        );
    }
}

#[test]
fn keeps_the_receipt_outside_every_container_mount() {
    let (fx, auth) = fixture();
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, Some("pack"));
    let docker = FakeDocker::new(&reporting_script(&engine, true, 0));
    let out = execute_job(
        &frozen,
        "alpha",
        Some("pack"),
        &engine,
        &config(),
        &docker,
        Interrupt::Never,
    );
    let finished = out.result.expect("the run finishes");

    let launch = out
        .invocations
        .iter()
        .find(|a| is_launch(a))
        .expect("a container was launched");
    let sources = bind_sources(launch);
    assert!(!sources.is_empty(), "the launch binds host paths");
    let receipt = &finished.receipt;
    let parent = receipt.parent().unwrap();
    assert!(
        receipt.is_file(),
        "receipt written at {}",
        receipt.display()
    );
    for src in sources {
        assert!(
            !receipt.starts_with(&src),
            "receipt under mount {}",
            src.display()
        );
        assert!(
            !parent.starts_with(&src),
            "receipt directory under mount {}",
            src.display()
        );
    }
    for tree in [&frozen.inputs, &frozen.outputs] {
        assert!(
            !file_tree(tree).keys().any(|k| k.ends_with("receipt.json")),
            "no receipt inside {}",
            tree.display()
        );
    }
}

#[test]
fn writes_only_allow_listed_receipt_fields() {
    let (fx, auth) = fixture();
    let brief = "TASK-BRIEF-CONTENT-7f3 design a page";
    fx.task_file("task.md", brief);
    let engine = TestEngine::new("alpha-engine", &auth);
    let frozen = freeze(&fx, Some("pack"));
    let mut script = reporting_script(&engine, true, 0);
    script.stdout.push("RAW-CONTAINER-LINE-91c".to_owned());
    script.stderr.push("RAW-CONTAINER-ERR-4d2".to_owned());
    let docker = FakeDocker::new(&script);
    let out = execute_job(
        &frozen,
        "alpha",
        Some("pack"),
        &engine,
        &config(),
        &docker,
        Interrupt::Never,
    );
    let finished = out.result.expect("the run finishes");

    let receipt = read_receipt(&finished.receipt);
    let keys: BTreeSet<&str> = receipt.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        FIELDS.iter().copied().collect::<BTreeSet<_>>(),
        "exactly the allow-list"
    );

    let text = fs::read_to_string(&finished.receipt).unwrap();
    let launch = out
        .invocations
        .iter()
        .find(|a| is_launch(a))
        .expect("launched");
    let name_at = launch.iter().position(|a| a == "--name").expect("--name");
    let container = &launch[name_at + 1];
    for (what, secret) in [
        ("task content", brief),
        ("raw stdout", "RAW-CONTAINER-LINE-91c"),
        ("raw stderr", "RAW-CONTAINER-ERR-4d2"),
        ("auth path", auth.to_str().unwrap()),
        ("credential", "fixture-secret-value"),
        ("container name", container.as_str()),
        ("image", engine.image.as_str()),
    ] {
        assert!(!text.contains(secret), "receipt leaks {what}: {text}");
    }
}
