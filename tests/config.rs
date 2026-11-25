use solo_design::config::{load, ModelConfig};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let map: HashMap<String, OsString> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), OsString::from(v)))
        .collect();
    move |key| map.get(key).cloned()
}

fn config_file(config_home: &Path) -> PathBuf {
    config_home.join("solo-design/config.json")
}

fn write_config(config_home: &Path, text: &str) -> PathBuf {
    let file = config_file(config_home);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, text).unwrap();
    file
}

fn cfg(model: &str, provider: &str) -> ModelConfig {
    ModelConfig {
        model: model.to_owned(),
        provider: provider.to_owned(),
    }
}

#[test]
fn reads_model_and_provider_from_the_config_file_an_env_var_overriding_it_per_field() {
    let dir = tempfile::tempdir().unwrap();
    let xdg = dir.path().join("xdg");
    let home = dir.path().join("home");
    write_config(
        &xdg,
        r#"{"model":"fixture-model","provider":"fixture-provider"}"#,
    );
    let xdg_s = xdg.to_str().unwrap();
    let home_s = home.to_str().unwrap();

    assert_eq!(
        load(&env(&[("XDG_CONFIG_HOME", xdg_s), ("HOME", home_s)])),
        Ok(cfg("fixture-model", "fixture-provider")),
        "file under XDG_CONFIG_HOME"
    );
    assert_eq!(
        load(&env(&[
            ("XDG_CONFIG_HOME", xdg_s),
            ("SOLO_DESIGN_MODEL", "env-model"),
        ])),
        Ok(cfg("env-model", "fixture-provider")),
        "env overrides the model only"
    );
    assert_eq!(
        load(&env(&[
            ("XDG_CONFIG_HOME", xdg_s),
            ("SOLO_DESIGN_PROVIDER", "env-provider"),
        ])),
        Ok(cfg("fixture-model", "env-provider")),
        "env overrides the provider only"
    );
    assert_eq!(
        load(&env(&[
            ("HOME", home_s),
            ("SOLO_DESIGN_MODEL", "env-model"),
            ("SOLO_DESIGN_PROVIDER", "env-provider"),
        ])),
        Ok(cfg("env-model", "env-provider")),
        "env alone suffices without a file"
    );

    let home_file_only = dir.path().join("home2");
    write_config(
        &home_file_only.join(".config"),
        r#"{"model":"home-model","provider":"home-provider"}"#,
    );
    assert_eq!(
        load(&env(&[("HOME", home_file_only.to_str().unwrap())])),
        Ok(cfg("home-model", "home-provider")),
        "default file under HOME/.config"
    );
}

#[test]
fn fails_preflight_naming_the_file_and_variables_when_the_model_or_provider_is_missing_or_the_file_is_invalid(
) {
    let dir = tempfile::tempdir().unwrap();
    let case = |name: &str, text: Option<&str>, extra: &[(&str, &str)]| {
        let xdg = dir.path().join(name);
        let file = match text {
            Some(text) => write_config(&xdg, text),
            None => config_file(&xdg),
        };
        let mut pairs = vec![("XDG_CONFIG_HOME", xdg.to_str().unwrap().to_owned())];
        pairs.extend(extra.iter().map(|(k, v)| (*k, v.to_string())));
        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        (file, load(&env(&borrowed)))
    };

    let cases = [
        ("no file, no env", case("none", None, &[])),
        (
            "model only in env",
            case("model-env", None, &[("SOLO_DESIGN_MODEL", "fixture-model")]),
        ),
        (
            "provider missing from file",
            case("partial", Some(r#"{"model":"fixture-model"}"#), &[]),
        ),
        ("invalid json", case("invalid", Some("{not json"), &[])),
        (
            "wrong field type",
            case(
                "typed",
                Some(r#"{"model":1,"provider":"fixture-provider"}"#),
                &[],
            ),
        ),
        (
            "empty values",
            case("blank", Some(r#"{"model":"","provider":""}"#), &[]),
        ),
    ];
    for (label, (file, result)) in cases {
        let message = result.expect_err(label);
        for needle in [
            file.to_str().unwrap(),
            "SOLO_DESIGN_MODEL",
            "SOLO_DESIGN_PROVIDER",
        ] {
            assert!(
                message.contains(needle),
                "{label}: missing {needle} in {message}"
            );
        }
    }

    // An unreadable config file (a directory in its place) is also a failure.
    let xdg = dir.path().join("unreadable");
    fs::create_dir_all(config_file(&xdg)).unwrap();
    let message = load(&env(&[("XDG_CONFIG_HOME", xdg.to_str().unwrap())]))
        .expect_err("unreadable file accepted");
    assert!(
        message.contains(config_file(&xdg).to_str().unwrap()),
        "unreadable: {message}"
    );
}
