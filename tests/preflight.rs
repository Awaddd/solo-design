mod common;

use common::Fixture;
use solo_design::inputs::{check_task, prepare};
use solo_design::root::resource_root;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

fn env(pairs: &[(&str, &Path)]) -> impl Fn(&str) -> Option<OsString> {
    let map: HashMap<String, OsString> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.as_os_str().to_owned()))
        .collect();
    move |key| map.get(key).cloned()
}

#[test]
fn rejects_a_missing_non_directory_linked_or_briefless_task_folder() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    let good = base.join("good");
    fs::create_dir(&good).unwrap();
    fs::write(good.join("task.md"), "Design a landing page.\n").unwrap();

    let a_file = base.join("a-file");
    fs::write(&a_file, "not a folder").unwrap();
    let linked = base.join("linked");
    symlink(&good, &linked).unwrap();
    let briefless = base.join("briefless");
    fs::create_dir(&briefless).unwrap();
    let empty_brief = base.join("empty-brief");
    fs::create_dir(&empty_brief).unwrap();
    fs::write(empty_brief.join("task.md"), "").unwrap();
    let linked_brief = base.join("linked-brief");
    fs::create_dir(&linked_brief).unwrap();
    symlink(good.join("task.md"), linked_brief.join("task.md")).unwrap();
    let folder_brief = base.join("folder-brief");
    fs::create_dir_all(folder_brief.join("task.md")).unwrap();
    let nested_brief = base.join("nested-brief");
    fs::create_dir_all(nested_brief.join("sub")).unwrap();
    fs::write(nested_brief.join("sub/task.md"), "Brief.\n").unwrap();

    assert_eq!(check_task(&good), Ok(good.canonicalize().unwrap()));
    for bad in [
        base.join("missing"),
        a_file,
        linked,
        briefless,
        empty_brief,
        linked_brief,
        folder_brief,
        nested_brief,
    ] {
        let message = check_task(&bad).expect_err(&format!("accepted {}", bad.display()));
        assert!(
            message.to_lowercase().contains("task"),
            "unclear message for {}: {message}",
            bad.display()
        );
    }
}

#[test]
fn resolves_resources_under_solo_design_root_or_the_documented_default() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let explicit = base.join("explicit");
    let xdg = base.join("xdg");
    let home = base.join("home");
    let empty = base.join("empty");
    for root in [
        explicit.clone(),
        xdg.join("solo-design"),
        home.join(".local/share/solo-design"),
    ] {
        fs::create_dir_all(root.join("agents")).unwrap();
    }
    fs::create_dir_all(explicit.join("lib")).unwrap();
    fs::create_dir_all(&empty).unwrap();
    let blank = Path::new("");

    let explicit_env = env(&[
        ("SOLO_DESIGN_ROOT", &explicit),
        ("XDG_DATA_HOME", &xdg),
        ("HOME", &home),
    ]);
    assert_eq!(
        resource_root(&explicit_env, false),
        Ok(explicit.canonicalize().unwrap()),
        "SOLO_DESIGN_ROOT wins"
    );
    assert_eq!(
        resource_root(&explicit_env, true),
        Ok(explicit.canonicalize().unwrap()),
        "root with lib/ serves a design"
    );

    let xdg_env = env(&[("XDG_DATA_HOME", &xdg), ("HOME", &home)]);
    assert_eq!(
        resource_root(&xdg_env, false),
        Ok(xdg.join("solo-design").canonicalize().unwrap()),
        "default under XDG_DATA_HOME"
    );
    assert!(
        resource_root(&xdg_env, true).is_err(),
        "a design needs lib/ under the root"
    );

    let home_default = home
        .join(".local/share/solo-design")
        .canonicalize()
        .unwrap();
    assert_eq!(
        resource_root(&env(&[("HOME", &home)]), false),
        Ok(home_default.clone()),
        "default under HOME/.local/share"
    );
    assert_eq!(
        resource_root(&env(&[("XDG_DATA_HOME", blank), ("HOME", &home)]), false),
        Ok(home_default),
        "an empty XDG_DATA_HOME falls back to HOME"
    );

    assert!(
        resource_root(
            &env(&[("SOLO_DESIGN_ROOT", &empty), ("HOME", &home)]),
            false
        )
        .is_err(),
        "a root without agents/ is rejected"
    );
    assert!(
        resource_root(&env(&[("SOLO_DESIGN_ROOT", &base.join("nope"))]), false).is_err(),
        "a missing root is rejected"
    );
}

#[test]
fn creates_no_run_directory_when_validation_fails() {
    let fx = Fixture::new();
    fx.agent("alpha");
    fx.agent_named("mismatch", "alpha");
    fx.design("pack");
    fx.put("lib/broken/DESIGN.md", "# broken\n");
    let briefless = fx.task.parent().unwrap().join("briefless");
    fs::create_dir_all(&briefless).unwrap();
    let linked_task = fx.task.parent().unwrap().join("linked-task");
    fs::create_dir_all(&linked_task).unwrap();
    fs::write(linked_task.join("task.md"), "Brief.\n").unwrap();
    symlink("/etc/hostname", linked_task.join("host")).unwrap();

    let failures: [(&str, &Path, &str, Option<&str>); 6] = [
        ("unknown agent", &fx.task, "absent", None),
        ("agent name mismatch", &fx.task, "mismatch", None),
        ("unknown design", &fx.task, "alpha", Some("absent")),
        (
            "design without globals.css",
            &fx.task,
            "alpha",
            Some("broken"),
        ),
        ("briefless task", &briefless, "alpha", None),
        ("task tree with a link", &linked_task, "alpha", Some("pack")),
    ];
    for (label, task, agent, design) in failures {
        assert!(
            prepare(&fx.root, task, agent, design).is_err(),
            "{label}: accepted"
        );
        assert_eq!(
            fx.runs(),
            Vec::<std::path::PathBuf>::new(),
            "{label}: run dir created"
        );
    }

    let run = prepare(&fx.root, &fx.task, "alpha", Some("pack"));
    assert!(run.is_ok(), "the valid selection must prepare: {run:?}");
    assert_eq!(
        fx.runs().len(),
        1,
        "exactly one run directory for the valid run"
    );
}
