### Requirement: Providers are configuration
last-changed-by: SD-2-pi-engine

The system SHALL select the provider, model and key file only from configuration, keeping every provider-specific value in its adapter.

#### AC engine.providers.1 — Two providers: WHEN the configured provider is OpenRouter or Anthropic THE SYSTEM SHALL run Pi against that provider with the configured model
verified by: tests/providers.rs › "maps each supported provider to its Pi provider and key variable"

#### AC engine.providers.2 — Unknown provider: IF the configured provider has no adapter THEN THE SYSTEM SHALL fail preflight listing the supported providers, creating no run directory and starting no container
verified by: tests/command.rs › "fails preflight listing the supported providers for an unknown provider and creates no run"

#### AC engine.providers.3 — Key file by path: WHEN a run starts THE SYSTEM SHALL mount the configured provider's key file read-only into the container, without the host reading its contents
verified by: tests/pi_engine.rs › "mounts the configured key file read-only without reading it"

#### AC engine.providers.4 — Key file checks: IF the provider's key file is not configured, missing, not a regular file, or accessible (any permission bit) to group or others THEN THE SYSTEM SHALL fail preflight naming the path (or the config field), never its contents, creating no run directory and starting no container
verified by: tests/command.rs › "fails preflight naming the key path when it is unconfigured, missing, not a file or group or world readable, and creates no run"

#### AC engine.providers.5 — Key never leaks: WHILE preparing, running and recording a run THE SYSTEM SHALL keep the key's value out of every surface the host writes: argv, progress, logs, the receipt, `stage/` and the run directory's files outside `outputs/` (the agent's own `outputs/` are the README's stated residual risk)
verified by: tests/key_safety.rs › "keeps the key value out of argv, progress, the receipt and the run directory"

#### AC engine.providers.6 — Adapter-only names: THE SYSTEM SHALL keep provider names, env var names and endpoints only in the provider adapter, and model ids nowhere in `src/` or `tests/` (the model-id guard enforces both)
verified by: tests/repo_hygiene.rs › "names providers only in the provider adapter and model ids nowhere"

#### AC engine.providers.7 — No fallback: IF the provider or model fails THEN THE SYSTEM SHALL return that failure without trying another provider or model
verified by: tests/pi_engine.rs › "returns a provider failure without trying another provider or model"
