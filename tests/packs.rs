mod common;

use sha2::{Digest, Sha256};
use solo_design::inputs::{prepare, validate};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const PACKS: &[&str] = &["frontend-genjutsu"];

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every regular file under `dir`, as `/`-separated paths relative to `dir`.
fn files_under(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                walk(base, &path, out);
            } else {
                let rel = path.strip_prefix(base).unwrap();
                out.push(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn read_text(path: &Path) -> String {
    String::from_utf8_lossy(&fs::read(path).unwrap_or_default()).into_owned()
}

fn tokens(line: &str) -> impl Iterator<Item = String> + '_ {
    line.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_ascii_lowercase)
}

/// Fails (as an assertion) unless every bundled pack has its `agent.md`.
fn assert_packs_present(agents: &Path) {
    let missing: Vec<&str> = PACKS
        .iter()
        .copied()
        .filter(|p| !agents.join(p).join("agent.md").is_file())
        .collect();
    assert_eq!(
        missing,
        Vec::<&str>::new(),
        "bundled packs without agents/<name>/agent.md"
    );
}

fn copy_tree(from: &Path, to: &Path) {
    for rel in files_under(from) {
        let target = to.join(&rel);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(from.join(&rel), target).unwrap();
    }
}

// Built from parts so this file does not name them itself.
fn personal_paths() -> Vec<String> {
    vec![
        ["/ho", "me/"].concat(),
        ["/Us", "ers/"].concat(),
        ["~/pro", "jects"].concat(),
    ]
}
fn legacy_substrings() -> Vec<String> {
    vec![["anti", "gravity"].concat()]
}
fn legacy_tokens() -> Vec<String> {
    vec![["a", "gy"].concat()]
}

/// Lines in any file of any pack under `agents` naming a personal path, a
/// legacy product or a deny-listed name (by `hashes`), as `path:line`.
fn content_hits(agents: &Path, hashes: &[String]) -> Vec<String> {
    let mut hits = Vec::new();
    for pack in PACKS {
        let dir = agents.join(pack);
        for rel in files_under(&dir) {
            let text = read_text(&dir.join(&rel));
            for (n, line) in text.lines().enumerate() {
                let lower = line.to_ascii_lowercase();
                let personal = personal_paths()
                    .iter()
                    .any(|p| lower.contains(&p.to_ascii_lowercase()));
                let legacy = legacy_substrings().iter().any(|s| lower.contains(s))
                    || tokens(line).any(|t| legacy_tokens().contains(&t));
                let brand = common::names_denied(line, hashes);
                if personal || legacy || brand {
                    hits.push(format!("{pack}/{rel}:{}", n + 1));
                }
            }
        }
    }
    hits
}

/// Lines in our own pack files that make a supplied design authority
/// mandatory or point at a foreign workspace path, as `path:line`.
fn authority_hits(agents: &Path) -> Vec<String> {
    let mut hits = Vec::new();
    for pack in PACKS {
        let dir = agents.join(pack);
        for rel in files_under(&dir) {
            let text = read_text(&dir.join(&rel));
            for (n, line) in text.lines().enumerate() {
                let lower = line.to_ascii_lowercase();
                let names_authority = line.contains("DESIGN.md") || lower.contains("globals.css");
                let mandatory = [
                    "required",
                    "must",
                    "binding",
                    "mandatory",
                    "first",
                    "before",
                ]
                .iter()
                .any(|w| tokens(line).any(|t| t == *w));
                let conditional = ["present", "selected", "when", "if"]
                    .iter()
                    .any(|w| tokens(line).any(|t| t == *w));
                let names_design_mount = line.contains("/solo/design/");
                let foreign = lower.contains("/workspace/");
                if ((names_authority && mandatory) || names_design_mount) && !conditional || foreign
                {
                    hits.push(format!("{pack}/{rel}:{}", n + 1));
                }
            }
        }
    }
    hits
}

/// Problems with a third-party notices text: it must name the example ADK's
/// `frontend-design` skill and no other ADK or skill (`others`).
fn notices_problems(text: &str, others: &[String]) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut problems = Vec::new();
    if !lower.contains("frontend-design") {
        problems.push("does not name frontend-design".to_string());
    }
    for other in others {
        if lower.contains(&other.to_ascii_lowercase()) {
            problems.push(format!("names {other}"));
        }
    }
    problems
}

/// Names the notices must not cover: the dropped ADKs, the private skill and
/// every example-ADK skill other than frontend-design.
fn other_adks_and_skills() -> Vec<String> {
    let mut others: Vec<String> = ["hallmark", "design-brief"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let skills = repo_root().join("agents/frontend-genjutsu/skills");
    if let Ok(entries) = fs::read_dir(skills) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() && name != "frontend-design" {
                others.push(name);
            }
        }
    }
    others.sort();
    others
}

#[test]
fn the_bundled_adks_are_the_example_adk_alone_and_the_third_party_notices_cover_only_its_frontend_design_skill(
) {
    let agents = repo_root().join("agents");
    let mut bundled: Vec<String> = fs::read_dir(&agents)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    bundled.sort();
    assert_eq!(
        bundled,
        vec!["frontend-genjutsu"],
        "the bundled set under agents/"
    );

    // Non-vacuity: the checker flags a hallmark section and a missing skill.
    let others = other_adks_and_skills();
    let good =
        "# Third-party notices\n\n## frontend-genjutsu: skills/frontend-design\nApache-2.0.\n";
    assert_eq!(
        notices_problems(good, &others),
        Vec::<String>::new(),
        "notices naming only frontend-design pass"
    );
    let hallmark = format!("{good}\n## hallmark-frontend\nMIT.\n");
    assert!(
        notices_problems(&hallmark, &others)
            .iter()
            .any(|p| p == "names hallmark"),
        "a hallmark section is flagged"
    );
    assert!(
        notices_problems("# Third-party notices\n", &others)
            .iter()
            .any(|p| p == "does not name frontend-design"),
        "notices without the skill are flagged"
    );

    let notices = repo_root().join("THIRD_PARTY_NOTICES.md");
    assert!(
        notices.is_file(),
        "THIRD_PARTY_NOTICES.md exists at the root"
    );
    assert_eq!(
        notices_problems(&read_text(&notices), &others),
        Vec::<String>::new(),
        "THIRD_PARTY_NOTICES.md covers only the example ADK's frontend-design skill"
    );
}

const SKILL_DIR: &str = "agents/frontend-genjutsu/skills/frontend-design";
const APACHE_SHA256: &str = "0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594";

#[test]
fn the_frontend_design_skill_ships_the_upstream_apache_licence_unchanged_and_the_refero_skill_mit_notice(
) {
    let skill = repo_root().join(SKILL_DIR);
    let apache = fs::read(skill.join("LICENSE.txt"));
    assert!(apache.is_ok(), "{SKILL_DIR}/LICENSE.txt exists");
    assert_eq!(
        sha256_hex(&apache.unwrap()),
        APACHE_SHA256,
        "LICENSE.txt is byte-identical to open-design's Apache-2.0 licence"
    );

    let refero = read_text(&skill.join("LICENSE-refero.txt"));
    let missing: Vec<&str> = [
        "Copyright (c) 2026 Refero",
        "Permission is hereby granted, free of charge",
        "The above copyright notice and this permission notice shall be included in all",
    ]
    .into_iter()
    .filter(|needle| !refero.contains(needle))
    .collect();
    assert_eq!(
        missing,
        Vec::<&str>::new(),
        "{SKILL_DIR}/LICENSE-refero.txt holds refero_skill's MIT copyright line and permission notice"
    );
}

const UPSTREAMS: &[(&str, &str)] = &[
    (
        "https://github.com/nexu-io/open-design",
        "0c237048cb078f95ab01c416924dcb255f7d5dfd",
    ),
    (
        "https://github.com/anthropics/skills",
        "00756142ab04c82a447693cf373c4e0c554d1005",
    ),
    (
        "https://github.com/referodesign/refero_skill",
        "2382e518f90096df607196b9a635da2d1bc142f9",
    ),
];

/// Problems with an attribution text: each upstream URL with its commit on one
/// line, one line saying the files were rewritten, no text copied and the
/// substance derived, and no word calling the upstream unknown or unconfirmed.
fn attribution_problems(text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for (url, commit) in UPSTREAMS {
        if !text.lines().any(|l| l.contains(url) && l.contains(commit)) {
            problems.push(format!("no line with {url} and {commit}"));
        }
    }
    let states_change = text.lines().any(|l| {
        let lower = l.to_ascii_lowercase();
        lower.contains("rewritten")
            && (lower.contains("no text copied") || lower.contains("no text was copied"))
            && lower.contains("derived")
    });
    if !states_change {
        problems.push("no line saying rewritten, no text copied, derived".to_string());
    }
    let lower = text.to_ascii_lowercase();
    for word in ["unknown", "unconfirmed", "unverified", "not confirmed"] {
        if lower.contains(word) {
            problems.push(format!("says {word}"));
        }
    }
    problems
}

#[test]
fn the_frontend_design_attribution_names_its_three_upstreams_with_their_commits_and_says_the_text_was_rewritten(
) {
    let entries: String = UPSTREAMS
        .iter()
        .map(|(url, commit)| format!("- {url} at commit {commit}\n"))
        .collect();
    let change = "Changed: rewritten; no text copied; substance derived.\n";
    let good = format!("{entries}{change}");
    assert_eq!(
        attribution_problems(&good),
        Vec::<String>::new(),
        "each URL with its commit and the change statement on one line pass"
    );

    let (url, commit) = UPSTREAMS[1];
    let scattered_commit = good.replace(
        &format!("{url} at commit {commit}"),
        &format!("{url}\n  pinned at commit {commit}"),
    );
    assert!(
        attribution_problems(&scattered_commit)
            .iter()
            .any(|p| p.contains(url)),
        "a URL and its commit on different lines are flagged"
    );
    let scattered_change =
        format!("{entries}The skill was rewritten.\nNo text copied.\nIts substance is derived.\n");
    assert!(
        attribution_problems(&scattered_change)
            .iter()
            .any(|p| p.starts_with("no line saying")),
        "the three change words on different lines are flagged"
    );
    let doubtful = format!("{good}The upstream of one reference remains Unknown.\n");
    assert!(
        attribution_problems(&doubtful)
            .iter()
            .any(|p| p == "says unknown"),
        "an upstream described as unknown is flagged"
    );

    let mut problems = Vec::new();
    for rel in [
        "agents/frontend-genjutsu/PROVENANCE.md",
        "THIRD_PARTY_NOTICES.md",
    ] {
        let path = repo_root().join(rel);
        if !path.is_file() {
            problems.push(format!("{rel}: missing"));
            continue;
        }
        for p in attribution_problems(&read_text(&path)) {
            problems.push(format!("{rel}: {p}"));
        }
    }
    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn every_bundled_pack_passes_agent_validation_and_freezes() {
    let agents = repo_root().join("agents");
    assert_packs_present(&agents);
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("resources");
    let task = tmp.path().join("caller/task");
    copy_tree(&agents, &root.join("agents"));
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir_all(&task).unwrap();
    fs::write(task.join("task.md"), "Design a landing page.\n").unwrap();
    let root = root.canonicalize().unwrap();
    let task = task.canonicalize().unwrap();

    for pack in PACKS {
        let selected = validate(&root, &task, pack, None);
        assert_eq!(
            selected.as_ref().map(|s| s.agent.clone()),
            Ok(root.join("agents").join(pack)),
            "{pack} passes agent validation"
        );
        let run = match prepare(&root, &task, pack, None) {
            Ok(run) => run,
            Err(message) => panic!("{pack} must freeze, got: {message}"),
        };
        let frozen: BTreeMap<String, String> = run
            .hashes
            .into_iter()
            .filter(|(k, _)| k.starts_with("agent/"))
            .collect();
        let expected: BTreeMap<String, String> = files_under(&agents.join(pack))
            .into_iter()
            .map(|rel| {
                let hash = sha256_hex(&fs::read(agents.join(pack).join(&rel)).unwrap());
                (format!("agent/{rel}"), hash)
            })
            .collect();
        assert_eq!(frozen, expected, "{pack} freezes every file unchanged");
    }
}

#[test]
fn every_bundled_pack_declares_a_headless_run_and_maps_design_md_to_the_active_design_system() {
    let scattered = "Never ask.\n\
        If your prompt has a `## Active design system` section, use it.\n\
        It means the look is fixed.\n\
        The supplied design.md is in that prompt.\n";
    let one_line = "Never ask.\n\
        Where a skill mentions a project DESIGN.md, it means that section, when present.\n";
    assert!(
        !maps_design_md(scattered),
        "design.md, the section and `means` on separate lines are not a mapping"
    );
    assert!(
        maps_design_md(one_line),
        "one line naming design.md and saying it means that section is a mapping"
    );

    let agents = repo_root().join("agents");
    assert_packs_present(&agents);
    let mut problems = Vec::new();
    for pack in PACKS {
        let text = read_text(&agents.join(pack).join("agent.md"));
        let lower = text.to_ascii_lowercase();
        if !(lower.contains("non-interactive") || lower.contains("never ask")) {
            problems.push(format!("{pack}: no non-interactive statement"));
        }
        if !text.contains("## Active design system") {
            problems.push(format!("{pack}: does not name `## Active design system`"));
        }
        if !maps_design_md(&text) {
            problems.push(format!("{pack}: does not map design.md"));
        }
    }
    assert_eq!(problems, Vec::<String>::new());
}

/// One line names design.md, the `## Active design system` section (or "that
/// section") and says the one *means* the other.
fn maps_design_md(text: &str) -> bool {
    text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("design.md")
            && (lower.contains("active design system") || lower.contains("that section"))
            && lower.contains(" means ")
    })
}

#[test]
fn bundled_packs_carry_no_brand_authority_personal_path_or_legacy_product_wording() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = tmp.path().join("agents");
    let put = |path: &str, text: String| {
        let target = fixture.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text).unwrap();
    };
    put(
        "frontend-genjutsu/rules/brand.md",
        "Copy the Zorblax look.\n".into(),
    );
    put(
        "frontend-genjutsu/rules/pair.md",
        "ok\nPublish on Frob Nitz.\n".into(),
    );
    put(
        "frontend-genjutsu/rules/ok.md",
        "Use a sidezorblax layout, frob alone.\n".into(),
    );
    put(
        "frontend-genjutsu/agent.md",
        format!("See {}me/notes.\n", personal_paths()[0]),
    );
    put(
        "frontend-genjutsu/skills/up/SKILL.md",
        format!("Ported from {}.\n", legacy_substrings()[0]),
    );
    put(
        "frontend-genjutsu/skills/up/rules/cli.md",
        format!("Run {} first.\n", legacy_tokens()[0]),
    );
    let dummy = vec![common::deny_hash("zorblax"), common::deny_hash("frob-nitz")];
    assert_eq!(
        content_hits(&fixture, &dummy),
        vec![
            "frontend-genjutsu/agent.md:1",
            "frontend-genjutsu/rules/brand.md:1",
            "frontend-genjutsu/rules/pair.md:2",
            "frontend-genjutsu/skills/up/SKILL.md:1",
            "frontend-genjutsu/skills/up/rules/cli.md:1",
        ],
        "the scan finds each form and matches deny-listed names as whole tokens or pairs"
    );

    let agents = repo_root().join("agents");
    assert_packs_present(&agents);
    let hashes = common::deny_hashes(repo_root());
    assert_eq!(content_hits(&agents, &hashes), Vec::<String>::new());
}

#[test]
fn no_bundled_pack_requires_a_supplied_design_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = tmp.path().join("agents");
    let put = |path: &str, text: &str| {
        let target = fixture.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text).unwrap();
    };
    put(
        "frontend-genjutsu/agent.md",
        "Read DESIGN.md first; it is required.\n\
         When one is present, /solo/design/DESIGN.md must be followed.\n\
         Read /solo/design/DESIGN.md before you start.\n\
         Tokens go in globals.css before you start.\n",
    );
    put(
        "frontend-genjutsu/rules/a.md",
        "Write to /workspace/out.\nTokens live in globals.css.\n",
    );
    put(
        "frontend-genjutsu/skills/up/SKILL.md",
        "DESIGN.md is binding.\nWhen DESIGN.md is present it is binding.\n",
    );
    assert_eq!(
        authority_hits(&fixture),
        vec![
            "frontend-genjutsu/agent.md:1",
            "frontend-genjutsu/agent.md:3",
            "frontend-genjutsu/agent.md:4",
            "frontend-genjutsu/rules/a.md:1",
            "frontend-genjutsu/skills/up/SKILL.md:1",
        ],
        "the scan flags mandatory authority and foreign paths in every pack file"
    );

    let agents = repo_root().join("agents");
    assert_packs_present(&agents);
    assert_eq!(authority_hits(&agents), Vec::<String>::new());
}

/// The bullets of a Markdown file: each runs from a line starting `- ` to the
/// next such line or a blank line, continuation lines joined with a space.
fn bullets(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = line.strip_prefix("- ") {
            out.extend(current.take());
            current = Some(rest.trim().to_string());
        } else if trimmed.is_empty() {
            out.extend(current.take());
        } else if let Some(bullet) = current.as_mut() {
            bullet.push(' ');
            bullet.push_str(trimmed);
        }
    }
    out.extend(current);
    out
}

fn is_img_height_auto_bullet(bullet: &str) -> bool {
    let lower = bullet.to_ascii_lowercase();
    bullet.contains("img { height: auto; }")
        && lower.contains("base css")
        && lower.contains("width")
        && lower.contains("height")
        && lower.contains("attribute")
        && lower.contains("aspect-ratio")
}

#[test]
fn every_bundled_pack_tells_the_agent_to_give_images_height_auto_in_its_base_css() {
    let agents = repo_root().join("agents");
    assert_packs_present(&agents);
    let missing: Vec<&str> = PACKS
        .iter()
        .copied()
        .filter(|pack| {
            let text = read_text(&agents.join(pack).join("rules/headless-output.md"));
            !bullets(&text).iter().any(|b| is_img_height_auto_bullet(b))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "packs whose rules/headless-output.md has no bullet telling the agent to put \
         `img {{ height: auto; }}` in its base CSS (because of the width/height attributes \
         and aspect-ratio): {missing:?}"
    );
}

#[test]
fn every_file_an_adks_agent_md_names_under_its_agent_folder_exists_in_the_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = tmp.path().join("agents");
    let put = |path: &str, text: &str| {
        let target = fixture.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text).unwrap();
    };
    put(
        "demo/agent.md",
        "Use the skill (`/solo/agent/skills/kept/`) and its notes.\n\
         Read `/solo/agent/skills/gone/references/a.md`.\n\
         Follow /solo/agent/rules/style.md. Do not write into /solo/agent.\n",
    );
    put("demo/rules/style.md", "Rule.\n");
    put(
        "demo/skills/kept/SKILL.md",
        "See /solo/agent/skills/kept/references/b.md, and /solo/agent/skills/kept/missing.md.\n",
    );
    put("demo/skills/kept/references/b.md", "B.\n");
    put(
        "demo/skills/kept/references/notes.md",
        "/solo/agent/nowhere.md\n",
    );
    assert_eq!(
        common::dangling_agent_refs(&fixture),
        vec![
            "demo/agent.md:2 skills/gone/references/a.md",
            "demo/skills/kept/SKILL.md:1 skills/kept/missing.md",
        ],
        "the check flags each /solo/agent/ path that is missing from the pack, \
         in agent.md and skill SKILL.md files only"
    );

    let agents = repo_root().join("agents");
    assert_packs_present(&agents);
    assert_eq!(common::dangling_agent_refs(&agents), Vec::<String>::new());
}
