use solo_design::engine::Engine;
use solo_design::engines::pi::PiEngine;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const PI_PACKAGE: &str = "@earendil-works/pi-coding-agent";

fn pi_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docker/pi")
}

/// Dockerfile instructions as (KEYWORD, args), with continuations joined,
/// comments dropped and `ARG` defaults substituted into later lines.
fn instructions(text: &str) -> Vec<(String, String)> {
    let mut logical = Vec::new();
    let mut current = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if current.is_empty() && (line.is_empty() || line.starts_with('#')) {
            continue;
        }
        if let Some(head) = line.strip_suffix('\\') {
            current.push_str(head);
            current.push(' ');
        } else {
            current.push_str(line);
            logical.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        logical.push(current);
    }

    let mut args: HashMap<String, String> = HashMap::new();
    let mut out = Vec::new();
    for line in logical {
        let line = substitute(&line, &args);
        let (kw, rest) = line.split_once(char::is_whitespace).unwrap_or((&line, ""));
        let kw = kw.to_ascii_uppercase();
        let rest = rest.trim().to_owned();
        if kw == "ARG" {
            if let Some((name, value)) = rest.split_once('=') {
                args.insert(
                    name.trim().to_owned(),
                    value.trim().trim_matches('"').to_owned(),
                );
            }
        }
        out.push((kw, rest));
    }
    out
}

fn substitute(line: &str, args: &HashMap<String, String>) -> String {
    let mut s = line.to_owned();
    for (name, value) in args {
        s = s.replace(&format!("${{{name}}}"), value);
        s = s.replace(&format!("${name}"), value);
    }
    s
}

fn is_exact_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// A base image reference is pinned when it carries a digest or a tag that
/// starts with an exact x.y.z version.
fn base_is_pinned(image: &str) -> bool {
    if image.contains("@sha256:") {
        return true;
    }
    let Some((_, tag)) = image.rsplit_once(':') else {
        return false;
    };
    let version = tag.split('-').next().unwrap_or("");
    !tag.contains('/') && is_exact_version(version)
}

fn is_root_user(user: &str) -> bool {
    let name = user.split(':').next().unwrap_or("").trim();
    name.is_empty() || name == "root" || name == "0"
}

/// Versions the Dockerfile installs the Pi package at, from `RUN` lines.
fn pi_versions(instrs: &[(String, String)]) -> Vec<String> {
    let prefix = format!("{PI_PACKAGE}@");
    instrs
        .iter()
        .filter(|(kw, _)| kw == "RUN")
        .flat_map(|(_, run)| {
            run.split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .filter_map(|tok| tok.strip_prefix(prefix.as_str()).map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The program each `exec` line of a shell script runs.
fn exec_targets(script: &str) -> Vec<String> {
    script
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("exec "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(|p| p.trim_matches('"').trim_matches('\'').to_owned())
        .collect()
}

#[test]
fn builds_pi_from_an_exact_pinned_version_as_a_non_root_user() {
    let dockerfile = fs::read_to_string(pi_dir().join("Dockerfile")).expect("read Dockerfile");
    let entrypoint =
        fs::read_to_string(pi_dir().join("entrypoint.sh")).expect("read entrypoint.sh");
    let instrs = instructions(&dockerfile);
    let mut problems = Vec::new();

    let bases: Vec<&str> = instrs
        .iter()
        .filter(|(kw, _)| kw == "FROM")
        .filter_map(|(_, rest)| rest.split_whitespace().find(|t| !t.starts_with("--")))
        .collect();
    if bases.is_empty() {
        problems.push("no FROM instruction".to_owned());
    }
    for base in &bases {
        if !base_is_pinned(base) {
            problems.push(format!(
                "base image {base:?} is not pinned to an exact version or digest"
            ));
        }
    }

    let versions = pi_versions(&instrs);
    if versions.is_empty() {
        problems.push(format!("no RUN installs {PI_PACKAGE}@<version>"));
    }
    for v in &versions {
        if !is_exact_version(v) {
            problems.push(format!("Pi installed at {v:?}, not an exact x.y.z version"));
        }
    }
    let image = PiEngine::new().image();
    let tag = image.rsplit_once(':').map(|(_, t)| t.to_owned());
    if versions.iter().any(|v| Some(v) != tag.as_ref()) {
        problems.push(format!(
            "the engine runs image {image:?} but the Dockerfile installs Pi {versions:?}"
        ));
    }

    match instrs.iter().rev().find(|(kw, _)| kw == "USER") {
        None => problems.push("no USER instruction: the image runs as root".to_owned()),
        Some((_, user)) if is_root_user(user) => {
            problems.push(format!("final USER is root ({user:?})"))
        }
        Some(_) => {}
    }

    match instrs.iter().rev().find(|(kw, _)| kw == "ENTRYPOINT") {
        Some((_, ep)) if ep.contains("entrypoint.sh") => {}
        other => problems.push(format!("ENTRYPOINT does not run entrypoint.sh: {other:?}")),
    }

    let targets = exec_targets(&entrypoint);
    if !targets.iter().any(|t| t == "pi" || t.ends_with("/pi")) {
        problems.push(format!(
            "entrypoint.sh does not exec pi (exec targets: {targets:?})"
        ));
    }

    assert!(
        problems.is_empty(),
        "Pi image problems:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn image_parser_accepts_a_pinned_non_root_pi_image_and_rejects_loose_ones() {
    let good = "ARG PI_VERSION=0.83.0\n\
                FROM node:22.19.0-bookworm-slim\n\
                # comment\n\
                RUN npm install -g \\\n    \"@earendil-works/pi-coding-agent@${PI_VERSION}\"\n\
                USER 10001:10001\n\
                ENTRYPOINT [\"/usr/local/bin/entrypoint.sh\"]\n";
    let instrs = instructions(good);
    assert_eq!(pi_versions(&instrs), vec!["0.83.0".to_owned()]);
    assert!(base_is_pinned("node:22.19.0-bookworm-slim"));
    assert!(base_is_pinned("node@sha256:abc"));
    for loose in ["node", "node:latest", "node:22", "node:22-slim", "scratch"] {
        assert!(!base_is_pinned(loose), "{loose} must not count as pinned");
    }
    for loose in ["^0.83.0", "~0.83.0", "latest", "0.83", "0.83.x"] {
        assert!(!is_exact_version(loose), "{loose} must not count as exact");
    }
    assert!(is_root_user("root") && is_root_user("0:0") && !is_root_user("pi"));
    assert_eq!(
        exec_targets("#!/bin/sh\n# exec nothing\nexec pi \"$@\"\n"),
        vec!["pi".to_owned()]
    );
}
