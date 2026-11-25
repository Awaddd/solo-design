mod common;

use common::{file_tree, snapshot, Fixture};
use sha2::{Digest, Sha256};
use solo_design::inputs::{prepare, validate, FrozenRun, Selected};
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::Path;

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn mkfifo(path: &Path) {
    let name = CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0, "mkfifo");
}

type Spoil = (&'static str, fn(&Fixture));

fn frozen(fx: &Fixture, agent: &str, design: Option<&str>) -> FrozenRun {
    match prepare(&fx.root, &fx.task, agent, design) {
        Ok(run) => run,
        Err(message) => panic!("a valid selection must freeze, got: {message}"),
    }
}

#[test]
fn resolves_only_the_named_agent_and_requires_its_front_matter_name_to_match() {
    let fx = Fixture::new();
    let alpha = fx.agent("alpha");
    fx.agent("beta");
    fx.agent_named("gamma", "beta");
    fs::create_dir_all(fx.root.join("agents/no-brief/skills")).unwrap();
    fx.put(
        "agents/no-front/agent.md",
        "name: no-front\nNo front-matter.\n",
    );
    fx.put(
        "agents/bad-yaml/agent.md",
        "---\nname: [bad-yaml\n---\nBody.\n",
    );
    fx.put(
        "agents/no-name/agent.md",
        "---\ndescription: x\n---\nBody.\n",
    );
    fx.put(
        "agents/quoted/agent.md",
        "---\nname: \"quoted\"\ndescription: Fixture.\n---\nBody.\n",
    );
    symlink(&alpha, fx.root.join("agents/linked")).unwrap();

    assert_eq!(
        validate(&fx.root, &fx.task, "alpha", None),
        Ok(Selected {
            root: fx.root.clone(),
            task: fx.task.clone(),
            agent: alpha,
            design: None,
        })
    );
    assert_eq!(
        validate(&fx.root, &fx.task, "quoted", None).map(|s| s.agent),
        Ok(fx.root.join("agents/quoted")),
        "a quoted YAML name still matches"
    );
    for (agent, why) in [
        ("missing", "no such agent, even though others exist"),
        ("gamma", "front-matter names another agent"),
        ("no-brief", "agent folder without agent.md"),
        ("no-front", "agent.md without front-matter"),
        ("bad-yaml", "front-matter is not valid YAML"),
        ("no-name", "front-matter without a name"),
        ("linked", "agent folder is a symbolic link"),
    ] {
        let message = validate(&fx.root, &fx.task, agent, None)
            .expect_err(&format!("accepted agent {agent}: {why}"));
        assert!(
            message.contains(agent),
            "{why}: message must name the agent: {message}"
        );
    }
}

#[test]
fn requires_a_design_pack_with_regular_design_md_and_globals_css_only_when_selected() {
    let fx = Fixture::new();
    fx.agent("alpha");
    let pack = fx.design("pack");
    fx.put("lib/no-css/DESIGN.md", "# x\n");
    fx.put("lib/no-doc/globals.css", "a{}\n");
    let linked_css = fx.design("linked-css");
    fs::remove_file(linked_css.join("globals.css")).unwrap();
    symlink(pack.join("globals.css"), linked_css.join("globals.css")).unwrap();
    fx.put("lib/dir-doc/globals.css", "a{}\n");
    fs::create_dir_all(fx.root.join("lib/dir-doc/DESIGN.md")).unwrap();
    symlink(&pack, fx.root.join("lib/linked-pack")).unwrap();

    assert_eq!(
        validate(&fx.root, &fx.task, "alpha", Some("pack")).map(|s| s.design),
        Ok(Some(pack)),
        "the selected pack resolves"
    );
    assert_eq!(
        validate(&fx.root, &fx.task, "alpha", None).map(|s| s.design),
        Ok(None),
        "no design when omitted"
    );
    for slug in [
        "missing",
        "no-css",
        "no-doc",
        "linked-css",
        "dir-doc",
        "linked-pack",
    ] {
        let message = validate(&fx.root, &fx.task, "alpha", Some(slug))
            .expect_err(&format!("accepted design {slug}"));
        assert!(
            message.contains(slug),
            "message must name the design {slug}: {message}"
        );
    }

    let run = frozen(&fx, "alpha", None);
    assert!(
        fs::symlink_metadata(run.inputs.join("design")).is_err(),
        "no design is staged when omitted"
    );
}

#[test]
fn rejects_selected_trees_containing_links_or_special_files() {
    let cases: [Spoil; 6] = [
        ("link in task", |fx| {
            fs::create_dir_all(fx.task.join("refs")).unwrap();
            symlink("/etc/hostname", fx.task.join("refs/host")).unwrap();
        }),
        ("fifo in task", |fx| mkfifo(&fx.task.join("pipe"))),
        ("link in agent", |fx| {
            symlink("/etc/hostname", fx.root.join("agents/alpha/skills/host")).unwrap();
        }),
        ("fifo in agent", |fx| {
            mkfifo(&fx.root.join("agents/alpha/rules/pipe"))
        }),
        ("link in design", |fx| {
            symlink("/etc", fx.root.join("lib/pack/fonts/etc")).unwrap();
        }),
        ("fifo in design", |fx| {
            mkfifo(&fx.root.join("lib/pack/pipe"))
        }),
    ];
    for (label, spoil) in cases {
        let fx = Fixture::new();
        fx.agent("alpha");
        fx.design("pack");
        assert!(
            validate(&fx.root, &fx.task, "alpha", Some("pack")).is_ok(),
            "{label}: the clean fixture must validate"
        );
        spoil(&fx);
        assert!(
            validate(&fx.root, &fx.task, "alpha", Some("pack")).is_err(),
            "{label}: accepted"
        );
    }
}

#[test]
fn freezes_the_selected_task_agent_and_design_with_a_sha_256_per_file() {
    let fx = Fixture::new();
    fx.agent("alpha");
    fx.design("pack");
    fx.task_file("refs/brief-image.png", [9u8, 8, 7]);

    let run = frozen(&fx, "alpha", Some("pack"));
    assert_eq!(run.run_dir.parent(), Some(fx.root.join("runs").as_path()));
    assert_eq!(run.inputs, run.run_dir.join("inputs"));
    assert_eq!(run.outputs, run.run_dir.join("outputs"));

    assert_eq!(file_tree(&run.inputs.join("task")), file_tree(&fx.task));
    assert_eq!(
        file_tree(&run.inputs.join("agent")),
        file_tree(&fx.root.join("agents/alpha"))
    );
    assert_eq!(
        file_tree(&run.inputs.join("design")),
        file_tree(&fx.root.join("lib/pack"))
    );
    let frozen_files = file_tree(&run.inputs);
    for key in [
        "task/task.md",
        "agent/agent.md",
        "agent/skills/layout.md",
        "agent/rules/style.md",
        "design/DESIGN.md",
        "design/globals.css",
    ] {
        assert!(frozen_files.contains_key(key), "{key} not frozen");
    }
    let expected: std::collections::BTreeMap<String, String> = frozen_files
        .iter()
        .map(|(path, bytes)| (path.clone(), sha256_hex(bytes)))
        .collect();
    assert_eq!(run.hashes, expected, "one SHA-256 per frozen file");
}

#[test]
fn keeps_the_frozen_brief_byte_identical() {
    let fx = Fixture::new();
    fx.agent("alpha");
    let brief: &[u8] = b"\xef\xbb\xbf# Brief\r\nNo trailing newline \xe2\x80\x94 caf\xc3\xa9\t";
    fx.task_file("task.md", brief);

    let run = frozen(&fx, "alpha", None);
    assert_eq!(
        fs::read(run.inputs.join("task/task.md")).unwrap(),
        brief,
        "frozen task.md must be the caller's exact bytes"
    );
}

#[test]
fn stages_nothing_that_was_not_selected() {
    let fx = Fixture::new();
    fx.agent("alpha");
    fx.agent("beta");
    fx.design("pack");
    fx.design("other");
    fx.put("README.md", "root notes\n");
    fx.put("lib/loose.md", "not a pack\n");

    let run = frozen(&fx, "alpha", Some("pack"));
    let top: Vec<String> = fs::read_dir(&run.inputs)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        top,
        ["agent", "design", "task"],
        "inputs holds only the selection"
    );
    let all = file_tree(&run.run_dir);
    for path in all.keys() {
        assert!(
            path.starts_with("inputs/") || path.starts_with("outputs/"),
            "unexpected staged file {path}"
        );
    }
    assert_eq!(
        file_tree(&run.inputs.join("agent")),
        file_tree(&fx.root.join("agents/alpha"))
    );
    assert_eq!(
        file_tree(&run.inputs.join("design")),
        file_tree(&fx.root.join("lib/pack"))
    );
    assert_eq!(fx.runs().len(), 1, "one run directory");
}

#[test]
fn seeds_existing_index_html_and_assets_into_outputs_byte_identical() {
    let fx = Fixture::new();
    fx.agent("alpha");
    let index: &[u8] = b"<!doctype html>\r\n<p>caf\xc3\xa9</p>";
    fx.task_file("existing/index.html", index);
    fx.task_file("existing/notes.md", "not seeded\n");
    fx.task_file("assets/logo.svg", "<svg/>");
    fx.task_file("assets/img/photo.bin", [0u8, 255, 1, 254]);

    let run = frozen(&fx, "alpha", None);
    let outputs = file_tree(&run.outputs);
    let mut expected = std::collections::BTreeMap::new();
    expected.insert("index.html".to_owned(), index.to_vec());
    for (path, bytes) in file_tree(&fx.task.join("assets")) {
        expected.insert(format!("assets/{path}"), bytes);
    }
    assert_eq!(
        outputs, expected,
        "outputs seeded with index.html and assets only"
    );

    let fx = Fixture::new();
    fx.agent("alpha");
    let run = frozen(&fx, "alpha", None);
    assert!(
        run.outputs.is_dir(),
        "outputs exists even with nothing to seed"
    );
    assert!(file_tree(&run.outputs).is_empty(), "nothing seeded");
}

#[test]
fn leaves_the_source_task_agents_and_designs_unchanged() {
    let fx = Fixture::new();
    fx.agent("alpha");
    fx.agent("beta");
    fx.design("pack");
    fx.task_file("existing/index.html", "<p>old</p>");
    fx.task_file("assets/a.txt", "a");
    let before_task = snapshot(&fx.task);
    let before_agents = snapshot(&fx.root.join("agents"));
    let before_lib = snapshot(&fx.root.join("lib"));

    let run = frozen(&fx, "alpha", Some("pack"));
    for (frozen_path, source) in [
        (run.inputs.join("task/task.md"), fx.task.join("task.md")),
        (
            run.inputs.join("agent/agent.md"),
            fx.root.join("agents/alpha/agent.md"),
        ),
        (
            run.inputs.join("design/DESIGN.md"),
            fx.root.join("lib/pack/DESIGN.md"),
        ),
        (
            run.outputs.join("index.html"),
            fx.task.join("existing/index.html"),
        ),
    ] {
        assert_ne!(
            fs::metadata(&frozen_path).map(|m| m.ino()).ok(),
            Some(fs::metadata(&source).unwrap().ino()),
            "{} must be a copy, not a link to the source",
            frozen_path.display()
        );
    }
    // A failing selection must not touch the sources either.
    assert!(prepare(&fx.root, &fx.task, "alpha", Some("absent")).is_err());

    assert_eq!(snapshot(&fx.task), before_task, "task folder changed");
    assert_eq!(
        snapshot(&fx.root.join("agents")),
        before_agents,
        "agents changed"
    );
    assert_eq!(
        snapshot(&fx.root.join("lib")),
        before_lib,
        "design packs changed"
    );
}
