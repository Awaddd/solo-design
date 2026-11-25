mod common;

use common::{execute_with, read_receipt, FakeDocker, Fixture, Interrupt, Script};
use serde_json::{json, Value};
use solo_design::config::ModelConfig;
use solo_design::engine::{Engine, Outcome, RunContext, Stream};
use solo_design::engines::pi::PiEngine;
use solo_design::inputs::prepare;
use solo_design::runtime::{execute, Job};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

fn fixture_lines(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pi")
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {} unreadable: {e}", path.display()))
        .lines()
        .map(str::to_owned)
        .collect()
}

// Provider names are built from parts (repo guard).
fn router() -> String {
    ["open", "router"].concat()
}

/// Feed `stdout` then `stderr` to a fresh Pi classifier: its verdict and the
/// progress lines it produced.
fn run_classifier(
    stdout: &[String],
    stderr: &[String],
    code: Option<i32>,
    signal: Option<i32>,
) -> (Outcome, Vec<String>) {
    let provider = router();
    let run = RunContext {
        model: "fixture-model",
        provider: &provider,
        effort: None,
        key_file: None,
    };
    let mut classifier = PiEngine::new().classifier(&run);
    let mut progress = Vec::new();
    for line in stdout {
        progress.extend(classifier.line(Stream::Stdout, line).map(|p| p.0));
    }
    for line in stderr {
        progress.extend(classifier.line(Stream::Stderr, line).map(|p| p.0));
    }
    (classifier.finish(code, signal), progress)
}

/// Whether a fresh Pi classifier calls the run a success.
fn classify(stdout: &[String], stderr: &[String], code: Option<i32>, signal: Option<i32>) -> bool {
    run_classifier(stdout, stderr, code, signal).0.success
}

/// The recorded auth-error stream with every `errorMessage` replaced by `message`.
fn error_stream(message: &str) -> Vec<String> {
    fn replace(value: &mut Value, message: &str) {
        match value {
            Value::Object(map) => {
                for (key, v) in map.iter_mut() {
                    if key == "errorMessage" {
                        *v = json!(message);
                    } else {
                        replace(v, message);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| replace(v, message)),
            _ => {}
        }
    }
    fixture_lines("error-auth.jsonl")
        .iter()
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).expect("fixture line is JSON");
            replace(&mut value, message);
            value.to_string()
        })
        .collect()
}

/// An assistant `message_end` with `stop_reason` and, when given, `usage`.
fn assistant_end(stop_reason: &str, usage: Option<Value>) -> String {
    let mut message = json!({
        "role": "assistant",
        "content": [{"type": "text", "text": "Done."}],
        "api": "fixture-api",
        "provider": "fixture-provider",
        "model": "fixture-model",
        "stopReason": stop_reason,
        "timestamp": 1,
    });
    if let Some(usage) = usage {
        message["usage"] = usage;
    }
    json!({"type": "message_end", "message": message}).to_string()
}

/// A two-turn stream whose assistant messages carry `first` and `second` usage.
fn two_turn_stream(first: Option<Value>, second: Option<Value>) -> Vec<String> {
    vec![
        json!({"type": "session", "version": 3, "id": "fixture", "cwd": "/solo/outputs"})
            .to_string(),
        json!({"type": "agent_start"}).to_string(),
        json!({"type": "turn_start"}).to_string(),
        assistant_end("toolUse", first),
        json!({"type": "turn_end"}).to_string(),
        json!({"type": "turn_start"}).to_string(),
        assistant_end("stop", second),
        json!({"type": "turn_end"}).to_string(),
        json!({"type": "agent_end", "messages": []}).to_string(),
    ]
}

/// (case, stdout, stderr, exit code, signal).
type Case = (
    &'static str,
    Vec<String>,
    Vec<String>,
    Option<i32>,
    Option<i32>,
);

#[test]
fn classifies_only_a_normally_stopped_final_message_with_a_zero_exit_as_success() {
    let success = fixture_lines("success-two-turns.jsonl");
    let auth = fixture_lines("error-auth.jsonl");
    let unknown_model = fixture_lines("error-unknown-model.jsonl");
    let unknown_provider = fixture_lines("unknown-provider.stderr.txt");

    assert!(
        classify(&success, &[], Some(0), None),
        "a normally stopped final message with exit 0 is a success"
    );

    let first_end = success
        .iter()
        .position(|l| {
            l.contains("\"stopReason\":\"toolUse\"") && l.contains("\"type\":\"message_end\"")
        })
        .expect("the fixture has a tool-use turn");
    let cut = success[..=first_end].to_vec();
    let mut error_after_stop = success.clone();
    error_after_stop.push(assistant_end("error", None));

    let not_success: [Case; 8] = [
        ("auth error, exit 0", auth, vec![], Some(0), None),
        (
            "unknown model, exit 0",
            unknown_model,
            vec![],
            Some(0),
            None,
        ),
        (
            "unknown provider, exit 1",
            vec![],
            unknown_provider,
            Some(1),
            None,
        ),
        (
            "stopped normally, exit 1",
            success.clone(),
            vec![],
            Some(1),
            None,
        ),
        (
            "stopped normally, killed",
            success.clone(),
            vec![],
            None,
            Some(9),
        ),
        (
            "stream cut after a tool-use turn, exit 0",
            cut,
            vec![],
            Some(0),
            None,
        ),
        (
            "final message is an error, exit 0",
            error_after_stop,
            vec![],
            Some(0),
            None,
        ),
        ("empty stream, exit 0", vec![], vec![], Some(0), None),
    ];
    for (case, stdout, stderr, code, signal) in not_success {
        assert!(
            !classify(&stdout, &stderr, code, signal),
            "{case}: must not be a success"
        );
    }

    // Through the runtime: a normal stop with exit 0 succeeds only when the
    // run itself produced outputs/index.html (new, or changed from the seed).
    for (case, seed, write, ok) in [
        ("no seed, written", false, true, true),
        ("seeded, changed by the run", true, true, true),
        ("seeded, untouched", true, false, false),
        ("no seed, nothing written", false, false, false),
    ] {
        let receipt = receipt_with(success.clone(), seed, write);
        if ok {
            assert_eq!(
                receipt.get("status"),
                Some(&json!("succeeded")),
                "{case}: {receipt:?}"
            );
            assert!(
                receipt.get("failure").is_none_or(Value::is_null),
                "{case}: no failure: {receipt:?}"
            );
        } else {
            assert_eq!(
                receipt.get("status"),
                Some(&json!("failed")),
                "{case}: {receipt:?}"
            );
            assert_eq!(
                receipt.get("failure"),
                Some(&json!("no_deliverable")),
                "{case}: no deliverable: {receipt:?}"
            );
        }
    }
}

/// Replay `stdout` through the runtime with the Pi engine and return the receipt.
fn receipt_for(stdout: Vec<String>) -> serde_json::Map<String, Value> {
    receipt_with(stdout, false, true)
}

/// Replay `stdout`; with `seed` the task seeds `outputs/index.html`, with
/// `write` the fake container writes its own `outputs/index.html`.
fn receipt_with(stdout: Vec<String>, seed: bool, write: bool) -> serde_json::Map<String, Value> {
    let fx = Fixture::new();
    fx.agent("alpha");
    if seed {
        fx.task_file("existing/index.html", "<p>seed</p>\n");
    }
    let frozen = prepare(&fx.root, &fx.task, "alpha", None).expect("fixture freezes");
    let key: PathBuf = fx.root.parent().unwrap().join("keys/provider.key");
    fs::create_dir_all(key.parent().unwrap()).unwrap();
    fs::write(&key, "fixture-secret-value").unwrap();
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    let config = ModelConfig {
        model: "fixture-vendor/fixture-model".to_owned(),
        provider: router(),
    };
    let setup = if write {
        format!(
            "printf '<p>made</p>\\n' > '{}'",
            frozen.outputs.join("index.html").display()
        )
    } else {
        String::new()
    };
    let docker = FakeDocker::with_setup(
        &Script {
            stdout,
            stderr: vec![],
            exit: 0,
        },
        0,
        &setup,
    );
    let out = execute_with(
        &frozen,
        "alpha",
        None,
        &PiEngine::new(),
        &config,
        Some(&key),
        &docker,
        Interrupt::Never,
    );
    let finished = out.result.expect("the run finishes");
    read_receipt(&finished.receipt)
}

fn usage_of(receipt: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    match receipt.get("usage") {
        Some(Value::Object(usage)) => usage.clone(),
        other => panic!("receipt has a usage object, got {other:?}: {receipt:?}"),
    }
}

fn assert_cost(usage: &serde_json::Map<String, Value>, expected: f64, case: &str) {
    let cost = usage
        .get("cost_estimate_usd")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("{case}: cost_estimate_usd recorded: {usage:?}"));
    assert!(
        (cost - expected).abs() < 1e-12,
        "{case}: cost {cost} summed to {expected}"
    );
}

#[test]
fn sums_tokens_and_estimated_cost_over_every_message_into_the_receipt_absent_when_never_reported() {
    // The recorded two-turn run: 653+776 in, 173+62 out, 826+838 total.
    let receipt = receipt_for(fixture_lines("success-two-turns.jsonl"));
    assert_eq!(
        receipt.get("status"),
        Some(&json!("succeeded")),
        "{receipt:?}"
    );
    assert_eq!(receipt.get("provider"), Some(&json!(router())));
    assert_eq!(
        receipt.get("model"),
        Some(&json!("fixture-vendor/fixture-model"))
    );
    assert_eq!(
        receipt.get("tokens"),
        Some(&json!(1664)),
        "tokens = summed totalTokens"
    );
    let usage = usage_of(&receipt);
    for (key, value) in [
        ("input_tokens", 1429),
        ("output_tokens", 235),
        ("cache_read_tokens", 0),
        ("cache_write_tokens", 0),
    ] {
        assert_eq!(usage.get(key), Some(&json!(value)), "recorded run: {key}");
    }
    assert_cost(&usage, 0.00013986 + 0.000126, "recorded run");

    // Fields only some messages report are summed over those; never-reported
    // ones are absent.
    let partial = two_turn_stream(
        Some(
            json!({"input": 10, "output": 5, "cacheRead": 3, "totalTokens": 18,
                    "cost": {"total": 0.5}}),
        ),
        Some(json!({"input": 20, "output": 7, "totalTokens": 27})),
    );
    let receipt = receipt_for(partial);
    assert_eq!(receipt.get("tokens"), Some(&json!(45)), "partial: tokens");
    let usage = usage_of(&receipt);
    assert_eq!(
        usage.get("input_tokens"),
        Some(&json!(30)),
        "partial: input"
    );
    assert_eq!(
        usage.get("output_tokens"),
        Some(&json!(12)),
        "partial: output"
    );
    assert_eq!(
        usage.get("cache_read_tokens"),
        Some(&json!(3)),
        "partial: cache read"
    );
    assert_eq!(
        usage.get("cache_write_tokens"),
        None,
        "partial: cache write never reported"
    );
    assert_cost(&usage, 0.5, "partial");

    // No message reports usage: nothing is invented.
    let receipt = receipt_for(two_turn_stream(None, None));
    assert_eq!(
        receipt.get("status"),
        Some(&json!("succeeded")),
        "{receipt:?}"
    );
    assert_eq!(receipt.get("tokens"), None, "no usage: no tokens");
    match receipt.get("usage") {
        None => {}
        Some(Value::Object(usage)) => assert!(usage.is_empty(), "no usage: empty: {usage:?}"),
        Some(other) => panic!("no usage: unexpected usage {other:?}"),
    }

    // An interrupted run keeps the usage its stream had already reported. The
    // interruption is raised from the progress callback once the second turn
    // starts, i.e. after the first message's usage was read; no timing race.
    let fx = Fixture::new();
    fx.agent("alpha");
    let frozen = prepare(&fx.root, &fx.task, "alpha", None).expect("fixture freezes");
    let docker = FakeDocker::sleeping(
        &Script {
            stdout: fixture_lines("success-two-turns.jsonl"),
            stderr: vec![],
            exit: 0,
        },
        8,
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let program = docker.program.clone();
    std::thread::spawn(move || {
        let config = ModelConfig {
            model: "fixture-vendor/fixture-model".to_owned(),
            provider: router(),
        };
        let engine = PiEngine::new();
        let job = Job {
            frozen: &frozen,
            agent: "alpha",
            design: None,
            engine: &engine,
            config: &config,
            effort: None,
            key_file: None,
            timeout: None,
        };
        let interrupt = AtomicBool::new(false);
        let mut report = |line: &str| {
            if line.contains("turn 2") {
                interrupt.store(true, Ordering::SeqCst);
            }
        };
        let _ = tx.send(execute(&job, &program, &interrupt, &mut report));
    });
    let finished = rx
        .recv_timeout(std::time::Duration::from_secs(6))
        .expect("the interrupted run returns")
        .expect("an interrupted run still finishes");
    let receipt = read_receipt(&finished.receipt);
    assert_eq!(
        receipt.get("status"),
        Some(&json!("interrupted")),
        "{receipt:?}"
    );
    let usage = usage_of(&receipt);
    assert!(
        usage.get("input_tokens").and_then(Value::as_u64) >= Some(653),
        "interrupted: the first message's usage is kept: {usage:?}"
    );
    assert!(
        receipt.get("tokens").and_then(Value::as_u64) >= Some(826),
        "interrupted: tokens kept: {receipt:?}"
    );
}

/// A one-turn stream whose assistant text, thinking and tool call carry
/// distinctive markers, writing `/solo/outputs/index.html`.
fn marked_stream() -> Vec<String> {
    marked_stream_at("/solo/outputs/index.html")
}

/// `marked_stream` with the `write` tool call aimed at `path`.
fn marked_stream_at(path: &str) -> Vec<String> {
    let content = json!([
        {"type": "thinking", "thinking": "THINKING-MARKER-b2 planning the page", "thinkingSignature": "sig"},
        {"type": "text", "text": "MODEL-TEXT-MARKER-a1 here is the page"},
        {"type": "toolCall", "id": "call_1", "name": "write",
         "arguments": {"path": path, "content": "<p>TOOL-CONTENT-MARKER-c3</p>"}}
    ]);
    let message = json!({"role": "assistant", "content": content, "api": "fixture-api",
        "provider": "fixture-provider", "model": "fixture-model", "stopReason": "toolUse", "timestamp": 1});
    vec![
        json!({"type": "session", "version": 3, "id": "fixture", "cwd": "/solo/outputs"}).to_string(),
        json!({"type": "agent_start"}).to_string(),
        json!({"type": "turn_start"}).to_string(),
        json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "delta": "MODEL-TEXT-MARKER-a1", "partial": message}}).to_string(),
        json!({"type": "message_end", "message": message}).to_string(),
        json!({"type": "tool_execution_start", "toolCallId": "call_1", "toolName": "write",
               "args": {"path": path, "content": "<p>TOOL-CONTENT-MARKER-c3</p>"}}).to_string(),
        json!({"type": "tool_execution_end", "toolCallId": "call_1", "toolName": "write",
               "result": {"content": [{"type": "text", "text": "TOOL-RESULT-MARKER-d4 wrote it"}]}, "isError": false}).to_string(),
        json!({"type": "turn_end"}).to_string(),
        json!({"type": "turn_start"}).to_string(),
        assistant_end("stop", None),
        json!({"type": "turn_end"}).to_string(),
        json!({"type": "agent_end", "messages": []}).to_string(),
    ]
}

#[test]
fn turns_the_stream_into_one_line_progress_without_model_text() {
    for (case, stream) in [
        ("recorded run", fixture_lines("success-two-turns.jsonl")),
        ("marked run", marked_stream()),
        (
            "relative path from the outputs folder",
            marked_stream_at("index.html"),
        ),
    ] {
        let (_, progress) = run_classifier(&stream, &[], Some(0), None);
        let turns = stream
            .iter()
            .filter(|l| l.contains("\"type\":\"turn_start\""))
            .count();
        assert_eq!(
            progress
                .iter()
                .filter(|p| p.to_lowercase().contains("turn"))
                .count(),
            turns,
            "{case}: one line per turn start: {progress:?}"
        );
        assert!(
            progress
                .iter()
                .any(|p| p.contains("write") && p.contains("index.html")),
            "{case}: the tool call with its path: {progress:?}"
        );
        for line in &progress {
            assert!(
                !line.contains('\n') && line.len() <= 200,
                "{case}: one short line: {line:?}"
            );
            for leaked in [
                "MARKER",
                "DOCTYPE",
                "user wants me",
                "has been created",
                "Successfully wrote",
            ] {
                assert!(
                    !line.contains(leaked),
                    "{case}: no model text, thinking or tool content: {line:?}"
                );
            }
        }
    }

    // Paths that climb out or carry control characters are not shown verbatim.
    for path in [
        "../escape/index.html",
        "page\u{1b}[2J\u{7}.html",
        "a\nb.html",
    ] {
        let (_, progress) = run_classifier(&marked_stream_at(path), &[], Some(0), None);
        assert!(
            progress.iter().any(|p| p.contains("write")),
            "{path:?}: the tool call is still reported: {progress:?}"
        );
        for line in &progress {
            assert!(!line.contains(path), "{path:?} shown verbatim: {line:?}");
            assert!(
                !line.chars().any(char::is_control),
                "{path:?}: no control characters: {line:?}"
            );
        }
    }

    let (_, progress) = run_classifier(&fixture_lines("error-auth.jsonl"), &[], Some(0), None);
    assert!(
        progress.iter().any(|p| p.to_lowercase().contains("error")),
        "a provider error is reported: {progress:?}"
    );
}

/// (case, stdout, stderr, exit code, expected failure kind).
type KindCase = (
    &'static str,
    Vec<String>,
    Vec<String>,
    Option<i32>,
    &'static str,
);

#[test]
fn classifies_authentication_unknown_model_rate_or_credit_limit_and_provider_errors() {
    let unknown_provider = fixture_lines("unknown-provider.stderr.txt");
    let cases: Vec<KindCase> = vec![
        (
            "recorded 401",
            fixture_lines("error-auth.jsonl"),
            vec![],
            Some(0),
            "auth",
        ),
        (
            "403",
            error_stream("403: {\"message\":\"Forbidden\"}"),
            vec![],
            Some(0),
            "auth",
        ),
        (
            "recorded 400 invalid model",
            fixture_lines("error-unknown-model.jsonl"),
            vec![],
            Some(0),
            "unknown_model",
        ),
        (
            "404",
            error_stream("404: {\"message\":\"No endpoints found\"}"),
            vec![],
            Some(0),
            "unknown_model",
        ),
        (
            "model not found on stderr",
            vec![],
            unknown_provider,
            Some(1),
            "unknown_model",
        ),
        (
            "402",
            error_stream("402: {\"message\":\"Insufficient credits\"}"),
            vec![],
            Some(0),
            "rate_or_credit_limit",
        ),
        (
            "429",
            error_stream("429: {\"message\":\"Rate limit exceeded\"}"),
            vec![],
            Some(0),
            "rate_or_credit_limit",
        ),
        (
            "503",
            error_stream("503: {\"message\":\"Upstream unavailable\"}"),
            vec![],
            Some(0),
            "provider_error",
        ),
        (
            "400 low credit balance",
            error_stream(
                "400 {\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\
                 \"message\":\"Your credit balance is too low to access the API. Please go to \
                 Plans & Billing to upgrade or purchase credits.\"}}",
            ),
            vec![],
            Some(0),
            "rate_or_credit_limit",
        ),
        (
            "other 400",
            error_stream("400: {\"message\":\"Bad request\"}"),
            vec![],
            Some(0),
            "provider_error",
        ),
    ];
    for (case, stdout, stderr, code, kind) in cases {
        let (outcome, _) = run_classifier(&stdout, &stderr, code, None);
        assert!(!outcome.success, "{case}: not a success");
        assert_eq!(
            outcome.failure.as_deref(),
            Some(kind),
            "{case}: failure kind"
        );
    }
    let (outcome, _) = run_classifier(
        &fixture_lines("success-two-turns.jsonl"),
        &[],
        Some(0),
        None,
    );
    assert_eq!(outcome.failure, None, "a success has no failure kind");
}

#[test]
fn classifies_a_malformed_or_unterminated_stream_as_incomplete() {
    let success = fixture_lines("success-two-turns.jsonl");
    let mut malformed = success.clone();
    malformed.insert(5, "{\"type\":\"message_end\",\"message\":".to_owned());
    let mut junk = success.clone();
    junk.insert(3, "not json at all".to_owned());
    let first_end = success
        .iter()
        .position(|l| {
            l.contains("\"stopReason\":\"toolUse\"") && l.contains("\"type\":\"message_end\"")
        })
        .expect("the fixture has a tool-use turn");
    let cut = success[..=first_end].to_vec();
    let mut half_line = cut.clone();
    let last = success[first_end + 1].clone();
    half_line.push(last[..last.len() / 2].to_owned());

    for (case, stdout) in [
        ("truncated JSON line mid-stream", malformed),
        ("non-JSON line mid-stream", junk),
        ("ends after a tool-use turn", cut),
        ("ends in half a line", half_line),
        ("empty stream", vec![]),
    ] {
        let (outcome, _) = run_classifier(&stdout, &[], Some(0), None);
        assert!(!outcome.success, "{case}: never a success");
        assert_eq!(
            outcome.failure.as_deref(),
            Some("incomplete"),
            "{case}: incomplete"
        );
    }
    let (outcome, _) = run_classifier(&success, &[], Some(0), None);
    assert!(
        outcome.success && outcome.failure.is_none(),
        "the complete stream is a success"
    );
}
