mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Tracked (and new, not ignored) files under `prefixes`, relative to `root`.
fn listed_files(root: &Path, prefixes: &[&str]) -> Vec<PathBuf> {
    let out = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ])
        .args(prefixes)
        .current_dir(root)
        .output()
        .expect("run git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(PathBuf::from)
        .filter(|p| root.join(p).is_file())
        .collect()
}

fn tokens(line: &str) -> impl Iterator<Item = String> + '_ {
    line.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_ascii_lowercase)
}

// Built from parts so this file does not name them itself.
fn legacy_substrings() -> Vec<String> {
    vec![["anti", "gravity"].concat(), ["/Us", "ers/"].concat()]
}
fn legacy_tokens() -> Vec<String> {
    vec![["a", "gy"].concat()]
}

const REPO_SCAN: &[&str] = &[
    "src",
    "tests",
    ".github",
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "README.md",
    "AGENTS.md",
];

/// Lines carrying a legacy product name or personal path, as `path:line`.
fn legacy_hits(root: &Path) -> Vec<String> {
    let mut hits = Vec::new();
    for file in listed_files(root, REPO_SCAN) {
        let Ok(text) = fs::read_to_string(root.join(&file)) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            let sub = legacy_substrings()
                .iter()
                .any(|s| lower.contains(&s.to_ascii_lowercase()));
            let tok = tokens(line).any(|t| legacy_tokens().contains(&t));
            if sub || tok {
                hits.push(format!("{}:{}", file.display(), n + 1));
            }
        }
    }
    hits
}

// The guard's own lists: the only place model ids and provider names may
// appear outside, respectively, the config layer's docs and the provider adapter.
const MODEL_IDS: &[&str] = &[
    "claude", "sonnet", "opus", "haiku", "gpt", "gemini", "deepseek", "mistral", "llama", "qwen",
    "grok",
];
const PROVIDERS: &[&str] = &["anthropic", "openai", "openrouter", "google"];
const MODEL_SCAN: &[&str] = &["src", "tests", ".github", "Cargo.toml", "build.rs"];
const GUARD_FILE: &str = "tests/repo_hygiene.rs";
const CONFIG_LAYER: &str = "src/config.rs";
const PROVIDER_ADAPTER: &str = "src/providers.rs";

/// Lines naming one of `ids` in scanned files, as `path:line`, skipping lines
/// `allowed(file, line)` accepts and the guard file itself.
fn id_hits(root: &Path, ids: &[&str], allowed: &dyn Fn(&Path, &str) -> bool) -> Vec<String> {
    let mut hits = Vec::new();
    for file in listed_files(root, MODEL_SCAN) {
        if file == Path::new(GUARD_FILE) {
            continue;
        }
        let Ok(text) = fs::read_to_string(root.join(&file)) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            if allowed(&file, line) {
                continue;
            }
            if tokens(line).any(|t| ids.contains(&t.as_str())) {
                hits.push(format!("{}:{}", file.display(), n + 1));
            }
        }
    }
    hits
}

/// Model ids outside the config layer's docs.
fn model_id_hits(root: &Path) -> Vec<String> {
    id_hits(root, MODEL_IDS, &|file, line| {
        file == Path::new(CONFIG_LAYER) && line.trim_start().starts_with("//")
    })
}

/// Provider names (hence their key variables and endpoints) outside the adapter.
fn provider_hits(root: &Path) -> Vec<String> {
    id_hits(root, PROVIDERS, &|file, _| {
        file == Path::new(PROVIDER_ADAPTER)
    })
}

/// Both guards, sorted.
fn guard_hits(root: &Path) -> Vec<String> {
    let mut hits = model_id_hits(root);
    hits.extend(provider_hits(root));
    hits.sort();
    hits.dedup();
    hits
}

/// A throwaway git repo holding `files`, for checking the scanners themselves.
fn fixture_repo(files: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let target = dir.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text).unwrap();
    }
    let ok = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap()
        .success();
    assert!(ok, "git init fixture");
    dir
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn source_tests_manifests_ci_readme_and_agents_carry_no_legacy_product_names_or_personal_paths() {
    let product = ["Anti", "gravity"].concat();
    let fixture = fixture_repo(&[
        ("src/a.rs", format!("// {product}Designer\n")),
        (
            "src/b.rs",
            format!("let argv = [\"{}\"];\n", ["a", "gy"].concat()),
        ),
        (
            "README.md",
            format!("see {}me/x\n", ["/Us", "ers/"].concat()),
        ),
        ("src/ok.rs", "let strategy = \"agya\";\n".to_owned()),
        ("docs/history.md", format!("{product}\n")),
        ("specs/old.md", format!("{product}\n")),
    ]);
    assert_eq!(
        legacy_hits(fixture.path()),
        vec!["README.md:1", "src/a.rs:1", "src/b.rs:1"],
        "the scan finds each legacy form, matches whole tokens and skips docs/ and specs/"
    );

    assert_eq!(legacy_hits(repo_root()), Vec::<String>::new());
}

#[test]
fn source_tests_manifests_and_ci_name_no_model_or_provider_outside_the_config_layer() {
    let fixture = fixture_repo(&[
        ("src/run.rs", "let model = \"gemini-3-flash\";\n".to_owned()),
        (
            ".github/workflows/ci.yml",
            "model: deepseek/deepseek-chat\n".to_owned(),
        ),
        (
            "src/config.rs",
            "//! e.g. SOLO_DESIGN_MODEL=gpt-5\n//! or openrouter\nlet x = 1;\n".to_owned(),
        ),
        ("src/fine.rs", "let gptx = \"fixture-model\";\n".to_owned()),
        (
            "src/net.rs",
            "let url = \"https://api.openai.com\";\n".to_owned(),
        ),
        ("Cargo.toml", "# anthropic\n".to_owned()),
        (PROVIDER_ADAPTER, "name: \"openrouter\",\n".to_owned()),
        (GUARD_FILE, "claude openrouter\n".to_owned()),
        ("README.md", "claude anthropic\n".to_owned()),
    ]);
    assert_eq!(
        guard_hits(fixture.path()),
        vec![
            ".github/workflows/ci.yml:1",
            "Cargo.toml:1",
            "src/config.rs:2",
            "src/net.rs:1",
            "src/run.rs:1",
        ],
        "model ids are allowed only in config-layer docs, provider names only in the adapter, \
         both in the guard's own lists; README is not scanned"
    );

    assert_eq!(guard_hits(repo_root()), Vec::<String>::new());
}

#[test]
fn names_providers_only_in_the_provider_adapter_and_model_ids_nowhere() {
    let fixture = fixture_repo(&[
        (
            PROVIDER_ADAPTER,
            "name: \"anthropic\", key_env: \"ANTHROPIC_API_KEY\",\n\
             endpoint: \"https://openrouter.ai/api\",\n\
             // default: claude-x\n"
                .to_owned(),
        ),
        (
            "src/engines/pi.rs",
            "let var = \"OPENROUTER_API_KEY\";\n".to_owned(),
        ),
        ("tests/pi.rs", "let p = \"anthropic\";\n".to_owned()),
        (
            "tests/parts.rs",
            "let p = [\"anthro\", \"pic\"].concat();\n".to_owned(),
        ),
        (
            "tests/model.rs",
            "let m = \"vendor/sonnet-4\";\n".to_owned(),
        ),
    ]);
    assert_eq!(
        provider_hits(fixture.path()),
        vec!["src/engines/pi.rs:1", "tests/pi.rs:1"],
        "provider names, key variables and endpoints only in the adapter"
    );
    assert_eq!(
        model_id_hits(fixture.path()),
        vec!["src/providers.rs:3", "tests/model.rs:1"],
        "model ids nowhere, the adapter included"
    );

    assert_eq!(provider_hits(repo_root()), Vec::<String>::new());
    assert_eq!(model_id_hits(repo_root()), Vec::<String>::new());
}

const OVERLAY: &str = "scripts/public-overlay/";

/// Lines in the public manifest's files (and public-overlay files) at `root` naming a denied name (by
/// `hashes`) or an absolute home path, as `path:line`.
fn public_hits(root: &Path, hashes: &[String]) -> Vec<String> {
    let mut hits = Vec::new();
    // Public-overlay files replace exported files, so they are public too.
    let mut files = common::manifest_files(root);
    files.extend(
        common::git_listed(root)
            .into_iter()
            .filter(|f| f.starts_with(OVERLAY)),
    );
    files.sort();
    files.dedup();
    for file in files {
        let text =
            String::from_utf8_lossy(&fs::read(root.join(&file)).unwrap_or_default()).into_owned();
        for (n, line) in text.lines().enumerate() {
            if common::names_denied(line, hashes) || common::names_home_path(line) {
                hits.push(format!("{file}:{}", n + 1));
            }
        }
    }
    hits
}

#[test]
fn the_public_scan_flags_a_hashed_token_a_hashed_pair_and_a_home_path_in_a_fixture_tree() {
    let home = ["/ho", "me/"].concat();
    let fixture = fixture_repo(&[
        (
            common::MANIFEST,
            "# public\nsrc/\nREADME.md\n!src/skip.md\n".to_owned(),
        ),
        ("src/token.md", "Built in the Zorblax style.\n".to_owned()),
        (
            "src/pair.md",
            "ok\nDeploy to frob_Nitz tonight.\n".to_owned(),
        ),
        ("README.md", format!("Notes at {home}someone/x.\n")),
        (
            "src/clean.md",
            format!(
                "zorblaxy, frob alone, nitz frob, frob-nitzy, {}stead/x/, {home}/x, {home}\n",
                ["/ho", "me"].concat()
            ),
        ),
        ("src/skip.md", "Zorblax\n".to_owned()),
        (
            "scripts/public-overlay/README.md",
            "ok\nok\nA Zorblax note.\n".to_owned(),
        ),
        ("docs/private.md", "zorblax frob-nitz\n".to_owned()),
    ]);
    let hashes = vec![common::deny_hash("zorblax"), common::deny_hash("frob-nitz")];
    assert_eq!(
        public_hits(fixture.path(), &hashes),
        vec![
            "README.md:1",
            "scripts/public-overlay/README.md:3",
            "src/pair.md:2",
            "src/token.md:1"
        ],
        "the scan flags a hashed whole token, a hashed adjacent pair and a home path, \
         in the manifest's files and public-overlay files, not in excluded ones"
    );
}

#[test]
fn the_public_manifests_files_carry_no_deny_listed_names_or_home_paths() {
    let root = repo_root();
    assert!(
        !common::manifest_files(root).is_empty(),
        "{} lists git-tracked files",
        common::MANIFEST
    );
    let hashes = common::deny_hashes(root);
    assert_eq!(public_hits(root, &hashes), Vec::<String>::new());
}
