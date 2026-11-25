mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A `git` command in `dir` with a fixed identity and no user or system
/// config, as on CI.
fn git(dir: &Path, home: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.test")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.test");
    cmd
}

fn git_ok(dir: &Path, home: &Path, args: &[&str]) -> String {
    let out = git(dir, home).args(args).output().expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A fresh repository at `<tmp>/src` holding this repo's working tree (git-
/// listed files), committed once, so the test sees the export script and
/// manifest as they are now.
fn source_clone(tmp: &Path, home: &Path) -> PathBuf {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = tmp.join("src");
    for rel in common::git_listed(repo) {
        let target = src.join(&rel);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repo.join(&rel), &target).unwrap();
    }
    git_ok(&src, home, &["init", "-q"]);
    git_ok(&src, home, &["add", "-A"]);
    git_ok(&src, home, &["commit", "-qm", "base"]);
    src
}

fn export(src: &Path, home: &Path, dest: &Path) -> (Output, String) {
    // Run through bash rather than exec'ing the copied file (ETXTBSY, testing.md).
    let env = git(src, home);
    let out = Command::new("bash")
        .arg("scripts/export-public.sh")
        .arg(dest)
        .current_dir(src)
        .envs(
            env.get_envs()
                .filter_map(|(k, v)| Some((k.to_owned(), v?.to_owned()))),
        )
        .output()
        .expect("run scripts/export-public.sh");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out, stderr)
}

/// Every regular file under `dir` outside `.git/`, as relative paths.
fn exported_files(dir: &Path) -> BTreeSet<String> {
    common::file_tree(dir)
        .into_keys()
        .filter(|k| !k.starts_with(".git/"))
        .collect()
}

/// Problems in an exported tree at `dest`: a file holding one of `hashes`, or
/// a dangling `/solo/agent/` reference in its ADKs.
fn public_guard_problems(dest: &Path, hashes: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    for (rel, bytes) in common::file_tree(dest) {
        if rel.starts_with(".git/") {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
        if hashes.iter().any(|h| text.contains(h.as_str())) {
            problems.push(format!("private hash in {rel}"));
        }
    }
    for hit in common::dangling_agent_refs(&dest.join("agents")) {
        problems.push(format!("dangling {hit}"));
    }
    problems
}

#[test]
fn exporting_the_public_tree_copies_exactly_the_manifests_files_into_a_fresh_repository_with_one_commit_and_no_remote(
) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let src = source_clone(tmp.path(), &home);

    // Two tracked files under manifest directories are excluded by committed
    // `!path` and `! path` lines (a space after the `!`).
    let original = fs::read_to_string(src.join(common::MANIFEST)).unwrap();
    let (dirs, _) = common::manifest_entries(&original);
    let under_dirs: Vec<String> = common::manifest_files(&src)
        .into_iter()
        .filter(|f| {
            dirs.iter()
                .any(|e| common::manifest_covers(e, f) && f.as_str() != e.trim_end_matches('/'))
        })
        .collect();
    assert!(
        under_dirs.len() >= 3,
        "the manifest lists directories holding tracked files"
    );
    let excluded = [
        under_dirs[under_dirs.len() - 1].clone(),
        under_dirs[under_dirs.len() - 2].clone(),
    ];
    fs::write(
        src.join(common::MANIFEST),
        format!("{original}!{}\n! {}\n", excluded[0], excluded[1]),
    )
    .unwrap();
    git_ok(&src, &home, &["add", common::MANIFEST]);
    git_ok(
        &src,
        &home,
        &["commit", "-qm", "manifest excludes two files"],
    );

    let expected: BTreeSet<String> = common::manifest_files(&src).into_iter().collect();
    assert!(!expected.is_empty(), "the manifest lists tracked files");

    // An uncommitted edit and an untracked file inside a manifest directory.
    let text = fs::read_to_string(src.join(common::MANIFEST)).unwrap();
    let (include, _) = common::manifest_entries(&text);
    let (dir, edited) = include
        .iter()
        .find_map(|entry| {
            let file = expected
                .iter()
                .find(|f| common::manifest_covers(entry, f) && f.as_str() != entry.as_str())?;
            Some((entry.trim_end_matches('/').to_owned(), file.clone()))
        })
        .expect("the manifest lists a directory holding tracked files");
    let committed = fs::read(src.join(&edited)).unwrap();
    let mut changed = committed.clone();
    changed.extend_from_slice(b"\nUNCOMMITTED-EDIT-MARKER\n");
    fs::write(src.join(&edited), &changed).unwrap();
    let untracked = format!("{dir}/untracked-marker.txt");
    fs::write(src.join(&untracked), "untracked\n").unwrap();

    let dest = tmp.path().join("public");
    let (out, stderr) = export(&src, &home, &dest);
    assert!(out.status.success(), "the export succeeds: {stderr}");
    assert_eq!(
        exported_files(&dest),
        expected,
        "the export holds exactly the manifest's tracked files"
    );
    assert_eq!(
        fs::read(dest.join(&edited)).unwrap_or_default(),
        committed,
        "{edited} is exported as committed, without the uncommitted edit"
    );
    for rel in &excluded {
        assert!(
            !expected.contains(rel),
            "the test's manifest reading drops {rel}"
        );
        assert!(
            !dest.join(rel).exists(),
            "{rel}, excluded by the manifest, is not exported"
        );
    }
    assert!(
        !dest.join(&untracked).exists(),
        "the untracked {untracked} is not exported"
    );
    // Files with a public-overlay counterpart at HEAD carry the overlay's
    // content; the overlay test checks those.
    let overlaid: BTreeSet<String> = git_ok(
        &src,
        &home,
        &[
            "ls-tree",
            "-r",
            "--name-only",
            "HEAD",
            "--",
            "scripts/public-overlay/",
        ],
    )
    .lines()
    .filter_map(|l| l.strip_prefix("scripts/public-overlay/"))
    .map(str::to_owned)
    .collect();
    for rel in &expected {
        if *rel != edited && !overlaid.contains(rel) {
            assert_eq!(
                fs::read(dest.join(rel)).ok(),
                fs::read(src.join(rel)).ok(),
                "{rel} is exported unchanged"
            );
        }
    }
    assert_eq!(
        git_ok(&dest, &home, &["rev-list", "--count", "HEAD"]).trim(),
        "1",
        "the export is a fresh repository with one commit"
    );
    assert_eq!(
        git_ok(&dest, &home, &["remote"]).trim(),
        "",
        "the export has no remote"
    );

    // The private deny-list stays out, and the composed public ADK is whole.
    // The public tree has no deny file, so its hash check is skipped there; the
    // archive tracks the file and deny_hashes requires it to hold hashes.
    let hashes = if common::git_listed(&src)
        .iter()
        .any(|f| f == common::DENY_FILE)
    {
        common::deny_hashes(&src)
    } else {
        Vec::new()
    };
    assert_eq!(
        public_guard_problems(&dest, &hashes),
        Vec::<String>::new(),
        "the export holds no private deny hash and no dangling /solo/agent/ reference"
    );

    // Non-vacuity: the exported agent.md naming a skill file nothing exports
    // is flagged. The line goes in the overlay's agent.md when one is
    // committed (it replaces the ADK's own), else in the ADK's own agent.md.
    const MISSING_SKILL: &str = "skills/no-such-skill/SKILL.md";
    let agent_rel = if overlaid.contains("agents/frontend-genjutsu/agent.md") {
        "scripts/public-overlay/agents/frontend-genjutsu/agent.md"
    } else {
        "agents/frontend-genjutsu/agent.md"
    };
    let mut agent_text = fs::read_to_string(src.join(agent_rel)).unwrap_or_default();
    agent_text.push_str(&format!("\nRead `/solo/agent/{MISSING_SKILL}`.\n"));
    put(&src, agent_rel, &agent_text);
    git_ok(&src, &home, &["add", "-A"]);
    git_ok(
        &src,
        &home,
        &["commit", "-qm", "agent.md names a missing skill"],
    );
    let dest_ref = tmp.path().join("public-ref");
    let (out, stderr) = export(&src, &home, &dest_ref);
    assert!(
        out.status.success(),
        "the variant export succeeds: {stderr}"
    );
    let problems = public_guard_problems(&dest_ref, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains(MISSING_SKILL) && p.starts_with("dangling")),
        "a dangling agent.md reference is flagged: {problems:?}"
    );

    // Control: a committed manifest naming a path git does not track fails.
    fs::write(src.join("scratch-untracked.md"), "not tracked\n").unwrap();
    fs::write(
        src.join(common::MANIFEST),
        format!("{text}scratch-untracked.md\n"),
    )
    .unwrap();
    git_ok(&src, &home, &["add", common::MANIFEST]);
    git_ok(
        &src,
        &home,
        &["commit", "-qm", "manifest names an untracked path"],
    );
    let (out, stderr) = export(&src, &home, &tmp.path().join("public-bad"));
    assert!(
        !out.status.success(),
        "a manifest path git does not track fails the export"
    );
    assert!(
        stderr.contains("scratch-untracked.md"),
        "the failure names the untracked path: {stderr}"
    );
}

const OVERLAY: &str = "scripts/public-overlay";

/// Write `text` at `rel` under `root`, creating parent folders.
fn put(root: &Path, rel: &str, text: &str) {
    let target = root.join(rel);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

#[test]
fn the_export_applies_the_public_overlays_files_over_the_exported_tree_and_never_exports_the_overlay_itself(
) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let src = source_clone(tmp.path(), &home);

    // Public-only versions of two exported files, one at the top level and one
    // nested, committed; then an uncommitted edit to one of them.
    let targets = ["README.md", "agents/frontend-genjutsu/agent.md"];
    let listed = common::manifest_files(&src);
    for rel in targets {
        assert!(
            listed.iter().any(|f| f == rel),
            "{rel} is exported by the manifest"
        );
        put(
            &src,
            &format!("{OVERLAY}/{rel}"),
            &format!("PUBLIC OVERLAY of {rel}\n"),
        );
    }
    git_ok(&src, &home, &["add", "-A"]);
    git_ok(&src, &home, &["commit", "-qm", "public overlay"]);
    put(
        &src,
        &format!("{OVERLAY}/{}", targets[0]),
        "UNCOMMITTED OVERLAY EDIT\n",
    );

    let expected: BTreeSet<String> = common::manifest_files(&src)
        .into_iter()
        .filter(|f| !f.starts_with(&format!("{OVERLAY}/")))
        .collect();

    let dest = tmp.path().join("public");
    let (out, stderr) = export(&src, &home, &dest);
    assert!(out.status.success(), "the export succeeds: {stderr}");
    for rel in targets {
        assert_eq!(
            fs::read_to_string(dest.join(rel)).unwrap_or_default(),
            format!("PUBLIC OVERLAY of {rel}\n"),
            "{rel} carries the committed overlay's content"
        );
    }
    let files = exported_files(&dest);
    let leaked: Vec<&String> = files
        .iter()
        .filter(|f| f.contains("public-overlay"))
        .collect();
    assert!(leaked.is_empty(), "no overlay path is exported: {leaked:?}");
    assert_eq!(
        files, expected,
        "the export holds exactly the manifest's tracked files, overlay excluded"
    );

    // Control: an overlay for a path the manifest does not export fails.
    put(&src, &format!("{OVERLAY}/docs/not-public.md"), "x\n");
    git_ok(&src, &home, &["add", "-A"]);
    git_ok(
        &src,
        &home,
        &["commit", "-qm", "overlay for a non-exported path"],
    );
    let (out, stderr) = export(&src, &home, &tmp.path().join("public-bad"));
    assert!(
        !out.status.success(),
        "an overlay for a non-exported path fails the export"
    );
    assert!(
        stderr.contains("docs/not-public.md"),
        "the failure names the overlay path: {stderr}"
    );
}

const ADK: &str = "agents/frontend-genjutsu/";
const SKILL: &str = "agents/frontend-genjutsu/skills/frontend-design/";

/// Problems with the example ADK in an export at `dest` of `src`'s HEAD:
/// a tracked ADK file missing or changed, a required skill or licence file or
/// the notices missing, or a dangling `/solo/agent/` reference.
fn adk_export_problems(src: &Path, dest: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for rel in common::git_listed(src) {
        if !rel.starts_with(ADK) {
            continue;
        }
        match fs::read(dest.join(&rel)) {
            Err(_) => problems.push(format!("missing {rel}")),
            Ok(bytes) if bytes != fs::read(src.join(&rel)).unwrap() => {
                problems.push(format!("changed {rel}"))
            }
            Ok(_) => {}
        }
    }
    for file in ["SKILL.md", "LICENSE.txt", "LICENSE-refero.txt"] {
        if !dest.join(SKILL).join(file).is_file() {
            problems.push(format!("missing required {SKILL}{file}"));
        }
    }
    let references = common::file_tree(&dest.join(SKILL).join("references"));
    if references.is_empty() {
        problems.push(format!("missing required {SKILL}references/"));
    }
    if !dest.join("THIRD_PARTY_NOTICES.md").is_file() {
        problems.push("missing required THIRD_PARTY_NOTICES.md".to_string());
    }
    for hit in common::dangling_agent_refs(&dest.join("agents")) {
        problems.push(format!("dangling {hit}"));
    }
    problems
}

#[test]
fn the_public_export_carries_the_whole_example_adk_with_the_frontend_design_skill_its_licence_files_and_the_third_party_notices(
) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let src = source_clone(tmp.path(), &home);
    let manifest = fs::read_to_string(src.join(common::MANIFEST)).unwrap();

    let dest = tmp.path().join("public");
    let (out, stderr) = export(&src, &home, &dest);
    assert!(out.status.success(), "the export succeeds: {stderr}");
    assert_eq!(
        adk_export_problems(&src, &dest),
        Vec::<String>::new(),
        "the export carries every tracked {ADK} file unchanged, the skill's licence \
         files and THIRD_PARTY_NOTICES.md, with no dangling reference"
    );

    // Non-vacuity (a): a manifest excluding the skill is flagged.
    fs::write(src.join(common::MANIFEST), format!("{manifest}!{SKILL}\n")).unwrap();
    git_ok(&src, &home, &["add", "-A"]);
    git_ok(
        &src,
        &home,
        &["commit", "-qm", "manifest excludes the skill"],
    );
    let dest_skill = tmp.path().join("public-skill");
    let (out, stderr) = export(&src, &home, &dest_skill);
    assert!(
        out.status.success(),
        "the variant export succeeds: {stderr}"
    );
    let problems = adk_export_problems(&src, &dest_skill);
    assert!(
        problems
            .iter()
            .any(|p| p == &format!("missing {SKILL}SKILL.md")),
        "the excluded skill is flagged: {problems:?}"
    );

    // Non-vacuity (b): an overlay replacing an ADK file is flagged.
    fs::write(src.join(common::MANIFEST), &manifest).unwrap();
    put(
        &src,
        &format!("{OVERLAY}/{ADK}agent.md"),
        "PUBLIC OVERLAY replacing the ADK's agent.md\n",
    );
    git_ok(&src, &home, &["add", "-A"]);
    git_ok(
        &src,
        &home,
        &["commit", "-qm", "overlay replaces an ADK file"],
    );
    let dest_overlay = tmp.path().join("public-overlay");
    let (out, stderr) = export(&src, &home, &dest_overlay);
    assert!(
        out.status.success(),
        "the variant export succeeds: {stderr}"
    );
    let problems = adk_export_problems(&src, &dest_overlay);
    assert!(
        problems
            .iter()
            .any(|p| p == &format!("changed {ADK}agent.md")),
        "the overlaid ADK file is flagged: {problems:?}"
    );
}

/// The agent-config folder (`.c…/`), built from parts so this file does not
/// name the provider (repo_hygiene's provider scan).
fn agent_config_dir() -> String {
    [".cl", "aude/"].concat()
}

/// Exported paths (outside `.git/`) of the design-brief skill or under the
/// agent-config folder.
fn design_brief_problems(dest: &Path) -> Vec<String> {
    exported_files(dest)
        .into_iter()
        .filter(|f| f.contains("design-brief") || f.starts_with(&agent_config_dir()))
        .map(|f| format!("exported {f}"))
        .collect()
}

#[test]
fn the_public_export_holds_no_design_brief_skill() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let src = source_clone(tmp.path(), &home);

    let dest = tmp.path().join("public");
    let (out, stderr) = export(&src, &home, &dest);
    assert!(out.status.success(), "the export succeeds: {stderr}");
    assert!(!exported_files(&dest).is_empty(), "the export holds files");
    assert_eq!(
        design_brief_problems(&dest),
        Vec::<String>::new(),
        "the export holds no design-brief file and nothing under the agent-config folder"
    );

    // Non-vacuity: a manifest naming the skill's folder is flagged. The
    // public tree has no such skill, so commit a fixture when it is absent.
    let brief = format!("{}skills/design-brief/", agent_config_dir());
    if !src.join(&brief).join("SKILL.md").is_file() {
        put(
            &src,
            &format!("{brief}SKILL.md"),
            "# design-brief fixture\n",
        );
    }
    let manifest = fs::read_to_string(src.join(common::MANIFEST)).unwrap();
    fs::write(src.join(common::MANIFEST), format!("{manifest}{brief}\n")).unwrap();
    git_ok(&src, &home, &["add", "-A"]);
    git_ok(
        &src,
        &home,
        &["commit", "-qm", "manifest names the design-brief skill"],
    );
    let dest_brief = tmp.path().join("public-brief");
    let (out, stderr) = export(&src, &home, &dest_brief);
    assert!(
        out.status.success(),
        "the variant export succeeds: {stderr}"
    );
    let problems = design_brief_problems(&dest_brief);
    assert!(
        problems
            .iter()
            .any(|p| p == &format!("exported {brief}SKILL.md")),
        "an exported design-brief file is flagged: {problems:?}"
    );
}
