### Requirement: Model and provider are configuration only
last-changed-by: SD-2-pi-engine

The system SHALL take the model and provider only from configuration, never from code or defaults.

#### AC project.config.1 — Configured model: WHEN a run is prepared THE SYSTEM SHALL take the model and provider identifiers only from the documented config file or environment variables, an environment variable overriding the file per field, and pass them to the engine and receipt unchanged
verified by: tests/config.rs › "reads model and provider from the config file, an env var overriding it per field"

#### AC project.config.2 — No default model: IF the model or provider is not configured, or the config file is unreadable or invalid, THEN THE SYSTEM SHALL fail preflight with a clear message naming the config file and variables, and start no container
verified by: tests/config.rs › "fails preflight naming the file and variables when the model or provider is missing or the file is invalid"

#### AC project.config.3 — Model-id guard: IF a known model id string (e.g. deepseek, gemini, claude, gpt) appears as a whole token in `src/`, `tests/`, `.github/`, `Cargo.toml` or `build.rs` outside the config layer's documentation and the guard's own list, OR a known provider name (e.g. openrouter, anthropic, openai) appears there outside the provider adapter `src/providers.rs` and the guard's own list, THEN THE SYSTEM's checks SHALL fail (a test run by `cargo test`, hence CI)
verified by: tests/repo_hygiene.rs › "source, tests, manifests and CI name no model or provider outside the config layer"
