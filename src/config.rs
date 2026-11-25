//! Model and provider configuration (config file + environment, no default).

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;

/// The configured model and provider identifiers, passed through unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelConfig {
    pub model: String,
    pub provider: String,
}

const MODEL_VAR: &str = "SOLO_DESIGN_MODEL";
const PROVIDER_VAR: &str = "SOLO_DESIGN_PROVIDER";

fn non_empty(env: &dyn Fn(&str) -> Option<OsString>, key: &str) -> Option<PathBuf> {
    env(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// `${XDG_CONFIG_HOME:-$HOME/.config}/solo-design/config.json`, when locatable.
fn config_path(env: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let base = non_empty(env, "XDG_CONFIG_HOME")
        .or_else(|| non_empty(env, "HOME").map(|home| home.join(".config")))?;
    Some(base.join("solo-design/config.json"))
}

fn env_value(env: &dyn Fn(&str) -> Option<OsString>, key: &str) -> Option<String> {
    env(key)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string_lossy().into_owned())
}

/// Load the model and provider, reading variables through `env`. A variable
/// overrides the file per field; there is no default for either.
pub fn load(env: &dyn Fn(&str) -> Option<OsString>) -> Result<ModelConfig, String> {
    let path = config_path(env);
    let shown = path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<config dir>/solo-design/config.json".to_owned());
    let hint =
        format!("set {MODEL_VAR} and {PROVIDER_VAR}, or put \"model\" and \"provider\" in {shown}");

    let mut file_model = None;
    let mut file_provider = None;
    if let Some(path) = &path {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let value: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| format!("config file {shown} is invalid ({e}); {hint}"))?;
                let object = value
                    .as_object()
                    .ok_or_else(|| format!("config file {shown} must be a JSON object; {hint}"))?;
                let field = |name: &str| -> Result<Option<String>, String> {
                    match object.get(name) {
                        None => Ok(None),
                        Some(serde_json::Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
                        Some(serde_json::Value::String(_)) => Ok(None),
                        Some(_) => Err(format!(
                            "config file {shown}: \"{name}\" must be a string; {hint}"
                        )),
                    }
                };
                file_model = field("model")?;
                file_provider = field("provider")?;
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("config file {shown} is unreadable ({e}); {hint}")),
        }
    }

    let model = env_value(env, MODEL_VAR).or(file_model);
    let provider = env_value(env, PROVIDER_VAR).or(file_provider);
    match (model, provider) {
        (Some(model), Some(provider)) => Ok(ModelConfig { model, provider }),
        (model, provider) => {
            let missing = match (model.is_none(), provider.is_none()) {
                (true, true) => "model and provider are not configured",
                (true, false) => "model is not configured",
                _ => "provider is not configured",
            };
            Err(format!("{missing}; {hint}"))
        }
    }
}

/// Everything a run is configured with besides its selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub model: ModelConfig,
    /// The engine name (default `pi`).
    pub engine: String,
    pub effort: Option<String>,
    pub timeout: Option<Duration>,
    /// The configured provider's key file.
    pub key_file: Option<PathBuf>,
}

/// Load the run settings (file fields `engine`, `effort`, `timeout_secs`,
/// `keys`; env `SOLO_DESIGN_ENGINE`, `SOLO_DESIGN_EFFORT`,
/// `SOLO_DESIGN_TIMEOUT_SECS`, `SOLO_DESIGN_KEY_FILE`).
pub fn load_settings(env: &dyn Fn(&str) -> Option<OsString>) -> Result<Settings, String> {
    let model = load(env)?;
    let path = config_path(env);
    let shown = path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<config dir>/solo-design/config.json".to_owned());
    let object = match path.as_ref().map(std::fs::read_to_string) {
        Some(Ok(text)) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(o)) => o,
            _ => return Err(format!("config file {shown} must be a JSON object")),
        },
        _ => serde_json::Map::new(),
    };
    let text_field = |name: &str| -> Result<Option<String>, String> {
        match object.get(name) {
            None => Ok(None),
            Some(serde_json::Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
            Some(serde_json::Value::String(_)) => Ok(None),
            Some(_) => Err(format!("config file {shown}: \"{name}\" must be a string")),
        }
    };

    let engine = env_value(env, "SOLO_DESIGN_ENGINE")
        .or(text_field("engine")?)
        .unwrap_or_else(|| "pi".to_owned());
    let effort = env_value(env, "SOLO_DESIGN_EFFORT").or(text_field("effort")?);

    let secs = match env_value(env, "SOLO_DESIGN_TIMEOUT_SECS") {
        Some(v) => Some(v.parse::<u64>().map_err(|_| {
            format!("SOLO_DESIGN_TIMEOUT_SECS must be a whole number of seconds, got {v:?}")
        })?),
        None => match object.get("timeout_secs") {
            None => None,
            Some(v) => Some(v.as_u64().ok_or_else(|| {
                format!("config file {shown}: \"timeout_secs\" must be a whole number of seconds")
            })?),
        },
    };
    if secs == Some(0) {
        return Err("the timeout must be at least one second".to_owned());
    }

    let key_file = match non_empty(env, "SOLO_DESIGN_KEY_FILE") {
        Some(p) => Some(p),
        None => match object.get("keys") {
            None => None,
            Some(serde_json::Value::Object(keys)) => match keys.get(&model.provider) {
                None => None,
                Some(serde_json::Value::String(p)) if !p.is_empty() => Some(PathBuf::from(p)),
                Some(_) => {
                    return Err(format!(
                        "config file {shown}: each \"keys\" entry must be a file path"
                    ))
                }
            },
            Some(_) => return Err(format!("config file {shown}: \"keys\" must be an object")),
        },
    };

    Ok(Settings {
        model,
        engine,
        effort,
        timeout: secs.map(Duration::from_secs),
        key_file,
    })
}
