mod common;

use common::{file_tree, Fixture};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const SENTINEL: &str = "sk-fixture-KEY-SENTINEL-5e1b";

/// A Pi stream in which a misbehaving model echoes the key in its text,
/// thinking, a tool call and a tool result, then writes index.html and stops.
fn leaky_stream() -> String {
    let content = json!([
        {"type": "thinking", "thinking": format!("the key is {SENTINEL}"), "thinkingSignature": "sig"},
        {"type": "text", "text": format!("Key: {SENTINEL}")},
        {"type": "toolCall", "id": "call_1", "name": "write",
         "arguments": {"path": "/solo/outputs/index.html", "content": format!("<p>{SENTINEL}</p>")}}
    ]);
    let tool_turn = json!({"role": "assistant", "content": content, "api": "fixture-api",
        "provider": "fixture-provider", "model": "fixture-model", "stopReason": "toolUse",
        "usage": {"input": 10, "output": 5, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 15,
                  "cost": {"total": 0.001}},
        "timestamp": 1});
    let final_turn = json!({"role": "assistant",
        "content": [{"type": "text", "text": format!("Done, key {SENTINEL}")}],
        "api": "fixture-api", "provider": "fixture-provider", "model": "fixture-model",
        "stopReason": "stop",
        "usage": {"input": 20, "output": 3, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 23,
                  "cost": {"total": 0.002}},
        "timestamp": 2});
    let lines = [
        json!({"type": "session", "version": 3, "id": "fixture", "cwd": "/solo/outputs"}),
        json!({"type": "agent_start"}),
        json!({"type": "turn_start"}),
        json!({"type": "message_update", "assistantMessageEvent":
            {"type": "text_delta", "delta": SENTINEL, "partial": tool_turn}}),
        json!({"type": "message_end", "message": tool_turn}),
        json!({"type": "tool_execution_start", "toolCallId": "call_1", "toolName": "write",
               "args": {"path": "/solo/outputs/index.html", "content": format!("<p>{SENTINEL}</p>")}}),
        json!({"type": "tool_execution_end", "toolCallId": "call_1", "toolName": "write",
               "result": {"content": [{"type": "text", "text": format!("wrote {SENTINEL}")}]},
               "isError": false}),
        json!({"type": "turn_end"}),
        json!({"type": "turn_start"}),
        json!({"type": "message_end", "message": final_turn}),
        json!({"type": "turn_end"}),
        json!({"type": "agent_end", "messages": [tool_turn, final_turn]}),
    ];
    lines.iter().map(|l| format!("{l}\n")).collect()
}

#[test]
fn keeps_the_key_value_out_of_argv_progress_the_receipt_and_the_run_directory() {
    let fx = Fixture::new();
    fx.agent("alpha");
    fx.design("pack");
    let base = fx.root.parent().unwrap().to_path_buf();
    let key = base.join("keys/provider.key");
    fs::create_dir_all(key.parent().unwrap()).unwrap();
    fs::write(&key, format!("{SENTINEL}\n")).unwrap();
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();

    // A fake docker: logs every argv; `run` writes the deliverable into the
    // folder mounted at /solo/outputs (so a normal stop is a success) and
    // replays the leaky stream. PATH holds only this folder, so the script
    // names /bin/cat in full.
    let bin = base.join("fake-bin");
    fs::create_dir_all(&bin).unwrap();
    let log = bin.join("docker.log");
    let stream = bin.join("stream.jsonl");
    fs::write(&stream, leaky_stream()).unwrap();
    common::write_executable(
        &bin.join("docker"),
        &format!(
            "#!/bin/sh\necho \"$@\" >> '{log}'\n\
             if [ \"$1\" = run ]; then\n\
               for a in \"$@\"; do case \"$a\" in *:/solo/outputs) out=\"${{a%:/solo/outputs}}\";; esac; done\n\
               printf '<p>made</p>\\n' > \"$out/index.html\"\n\
               /bin/cat '{stream}'; exit 0\n\
             fi\nexit 0\n",
            log = log.display(),
            stream = stream.display()
        ),
    );

    let provider = ["open", "router"].concat();
    let out = Command::new(env!("CARGO_BIN_EXE_solo-design"))
        .args(["run", "--agent", "alpha", "--task"])
        .arg(&fx.task)
        .args(["--design", "pack"])
        .env_clear()
        .env("PATH", &bin)
        .env("HOME", &base)
        .env("SOLO_DESIGN_ROOT", &fx.root)
        .env("SOLO_DESIGN_MODEL", "fixture-vendor/fixture-model")
        .env("SOLO_DESIGN_PROVIDER", &provider)
        .env("SOLO_DESIGN_KEY_FILE", &key)
        .output()
        .expect("spawn the solo-design binary");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // Every surface is exercised: the run launched, succeeded and printed progress.
    assert!(out.status.success(), "the run succeeds: {stderr}");
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("write") && l.contains("index.html")),
        "progress was printed: {stdout}"
    );
    let runs = fx.runs();
    assert_eq!(runs.len(), 1, "one run directory");
    let run_dir = &runs[0];
    assert!(
        run_dir.join("receipt.json").is_file(),
        "a receipt was written"
    );
    assert!(run_dir.join("stage").is_dir(), "a prompt was staged");

    let argv = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        argv.lines().any(|l| l.starts_with("run ")),
        "docker run was invoked: {argv}"
    );
    assert!(
        !argv.contains(SENTINEL),
        "the key value is not in docker argv"
    );
    assert!(
        !stdout.contains(SENTINEL),
        "the key value is not in progress"
    );
    assert!(!stderr.contains(SENTINEL), "the key value is not on stderr");
    for (path, bytes) in file_tree(run_dir) {
        if path.starts_with("outputs/") {
            continue;
        }
        assert!(
            !String::from_utf8_lossy(&bytes).contains(SENTINEL),
            "the key value is not in {path}"
        );
    }
}
