use solo_design::cli::{self, CliError};
use solo_design::runtime::{self, Job};
use solo_design::{config, engine, inputs, providers, root, signals};
use std::ffi::OsString;
use std::path::Path;
use std::process::exit;

fn main() {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let args = match cli::parse(&argv) {
        Ok(args) => args,
        Err(CliError::Usage(message)) => {
            eprintln!("{message}");
            exit(2);
        }
        Err(CliError::Selector(message)) => fail(&message),
    };
    signals::install().unwrap_or_else(|e| fail(&e));
    let env = |key: &str| std::env::var_os(key);
    let root = root::resource_root(&env, args.design.is_some()).unwrap_or_else(|e| fail(&e));
    let settings = config::load_settings(&env).unwrap_or_else(|e| fail(&e));
    let selected = inputs::validate(&root, &args.task, &args.agent, args.design.as_deref())
        .unwrap_or_else(|e| fail(&e));
    // Read-only checks are done; nothing is frozen until all of them pass.
    let engines = engine::available();
    let Some(engine) = engines.iter().find(|e| e.name() == settings.engine) else {
        let names: Vec<&str> = engines.iter().map(|e| e.name()).collect();
        fail(&format!(
            "unknown engine {:?}; available engines: {}",
            settings.engine,
            names.join(", ")
        ));
    };
    providers::lookup(&settings.model.provider).unwrap_or_else(|e| fail(&e));
    let key_file =
        providers::check_key_file(settings.key_file.as_deref()).unwrap_or_else(|e| fail(&e));
    check_image(engine.as_ref());
    let frozen = inputs::freeze(&selected).unwrap_or_else(|e| fail(&e));
    let job = Job {
        frozen: &frozen,
        agent: &args.agent,
        design: args.design.as_deref(),
        engine: engine.as_ref(),
        config: &settings.model,
        effort: settings.effort.as_deref(),
        key_file: Some(key_file.as_path()),
        timeout: settings.timeout,
    };
    let mut report = |line: &str| println!("{line}");
    match runtime::execute(&job, Path::new("docker"), signals::flag(), &mut report) {
        Ok(finished) => exit(finished.exit_code),
        Err(message) => fail(&message),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("solo-design: {message}");
    exit(1);
}

/// Preflight: the engine's image must exist locally (it is never pulled).
fn check_image(engine: &dyn engine::Engine) {
    let image = engine.image();
    let present = std::process::Command::new("docker")
        .args(["image", "inspect", &image])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !present {
        fail(&format!(
            "the {} image {image} is not available locally; build it with: {}",
            engine.name(),
            engine.build_hint()
        ));
    }
}
