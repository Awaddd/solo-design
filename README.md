# Solo Design

Sandboxed AI design agents, in Rust. You hand a task folder and a named agent
to `solo-design`; it freezes those inputs, runs the agent in a hardened
container, and leaves the generated design in an output folder next to a
receipt of what happened.

## Install

```sh
cargo install --path .
```

Requires a Rust toolchain and Docker.

## Quickstart

```sh
solo-design run --agent <name> --task <folder> [--design <slug>]
```

`<folder>` must hold a non-empty `task.md`. Before anything is frozen, a run
checks the engine, provider, key file and engine image; a failed check creates
no run directory and starts no container.

Try the bundled example task with the example ADK, from the repo root (the
root holds `agents/`; runs land in `./runs/`, which is git-ignored):

```sh
SOLO_DESIGN_ROOT=. solo-design run --agent frontend-genjutsu --task examples/task
```

## Private kits

The public tree ships one example ADK (`agents/frontend-genjutsu`). Your own
ADKs, design packs and briefs live in a separate kits folder. Point
`SOLO_DESIGN_ROOT` at it; it holds `agents/<name>/` and `lib/<slug>/`, and
runs land in its `runs/`:

```sh
export SOLO_DESIGN_ROOT=~/solo-design-kits
solo-design run --agent my-adk --design my-design --task ~/briefs/landing
```

## Engine

The default engine is [Pi](https://www.npmjs.com/package/@earendil-works/pi-coding-agent)
0.83.0, run in a container from a pinned, non-root image. Build the image once
(it is never pulled):

```sh
docker build -t solo-design-pi:0.83.0 docker/pi
```

## Configuration

**Resource root.** `SOLO_DESIGN_ROOT`, else
`${XDG_DATA_HOME:-$HOME/.local/share}/solo-design`. It holds
`agents/<name>/agent.md` (its front-matter `name` must equal `<name>`) and,
when `--design` is used, `lib/<slug>/` with `DESIGN.md` and `globals.css`.
Runs land in `<root>/runs/<run-id>/`.

**Model and provider.** Set `SOLO_DESIGN_MODEL` and `SOLO_DESIGN_PROVIDER`, or
write `${XDG_CONFIG_HOME:-$HOME/.config}/solo-design/config.json`:

```json
{ "model": "<model-id>", "provider": "<provider>" }
```

An environment variable overrides the file per field. Both are required and
there is no default. `model` is the provider's own id, without the provider
prefix.

All fields of `config.json`, with their overriding variables:

| Field | Variable | Meaning |
|---|---|---|
| `model` | `SOLO_DESIGN_MODEL` | model id (required) |
| `provider` | `SOLO_DESIGN_PROVIDER` | provider name (required) |
| `engine` | `SOLO_DESIGN_ENGINE` | engine name (default `pi`) |
| `effort` | `SOLO_DESIGN_EFFORT` | thinking level passed to the engine (optional) |
| `timeout_secs` | `SOLO_DESIGN_TIMEOUT_SECS` | stop the run after this many seconds (optional) |
| `keys` | `SOLO_DESIGN_KEY_FILE` | `keys` maps provider to key-file path; the variable overrides the entry for the configured provider |

## Providers

The supported providers are listed in `src/providers.rs`, the only place that
names providers and their key variables. An unsupported provider fails
preflight and lists the supported ones. Adding a provider is one adapter entry.

## Keys

Each provider has one key file: mode `0600`, a regular file, outside any
repository. Configure its path only (`keys` or `SOLO_DESIGN_KEY_FILE`); the key
itself is never configured, logged or placed in argv. The host never reads the
file: it checks the path and mode, then mounts it read-only into the container
at `/run/solo/key`, where the image's entrypoint hands it to Pi.

## In-container layout

| Path | Contents |
|---|---|
| `/solo/task` | the frozen task folder; the brief is `/solo/task/task.md` (read-only) |
| `/solo/agent` | the frozen agent; skills are under `/solo/agent/skills` (read-only) |
| `/solo/design` | the selected design pack (read-only) |
| `/solo/outputs` | the working directory and the only writable folder; the deliverable is `index.html` |
| `/solo/stage/system-prompt.md` | the composed system prompt (read-only) |

## Receipt

`receipt.json` records `status` (`succeeded`, `failed`, `interrupted` or
`timed_out`), exit code and signal, selection, model, provider, times and file
hashes, plus `tokens` (the summed total) and a `usage` object (`input_tokens`,
`output_tokens`, `cache_read_tokens`, `cache_write_tokens`,
`cost_estimate_usd`), each present only when reported. `cost_estimate_usd` is
Pi's catalog estimate, not a bill. `failure` is `null` unless the run failed,
then one of `auth`, `unknown_model`, `rate_or_credit_limit`, `provider_error`,
`incomplete` or `no_deliverable` (the run stopped normally but left no regular
`outputs/index.html`).

## Architecture

- `solo-design` is a small Rust CLI (`cli`, `root`, `inputs`, `config`,
  `engine`, `runtime`, `signals`, `receipt`).
- Each run lives in `<root>/runs/<run-id>/` with frozen `inputs/`
  (task, agent, design; SHA-256 per file), writable `outputs/`, and a
  `receipt.json` kept outside every container mount.
- An *engine* (a trait) supplies the image, auth provision, container layout,
  argv and progress classification. The binary ships the Pi engine; the model
  and provider are configuration, never code.

## Safety model

The container is the boundary. The agent runs as a non-root user in a
container with no Docker socket and no host home; only the frozen inputs
(read-only) and the outputs folder (read-write) are mounted. The CLI removes
only the container it started and keeps outputs on interruption or timeout.

The container is read-only (`--read-only`, tmpfs `/tmp`), drops all
capabilities, sets `no-new-privileges` and runs as the host user's uid.

- **Egress is unrestricted.** Pi must reach the provider, so the container has
  network access.
- **The key is reachable inside the container.** Pi's own read tool runs in the
  same container and could read the key (the mounted file or the process
  environment) if the model were induced to, and a manipulated model could
  write it into `outputs/`, or name a file after it, which the progress lines
  then print (tool paths are model-chosen). Treat `outputs/` and the progress
  output as untrusted.
- **Root hosts.** A root host runs the container as uid 65534, which cannot
  read a root-owned `0600` key file; run as a regular user.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT licence ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 licence, shall be
dual licensed as above, without any additional terms or conditions.
