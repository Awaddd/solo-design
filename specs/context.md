# context.md — Solo Design

## Stack
| Aspect | Value |
|---|---|
| Language | Rust (edition per Cargo.toml), single crate, binary `solo-design` |
| Runtime | Docker CLI as a child process; one sandboxed container per run |
| Test runner | `cargo test` |
| Checks | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked` |
| CI | GitHub Actions (`.github/workflows/ci.yml`) |

## Conventions
- Tests never need real Docker, credentials or a model: integration tests pass the runtime a fake `docker` script path (written by a child process; see docs/testing.md) and a test-only engine, with an injected interruption flag.
- Numbered project notes: `docs/NNNN_topic.md`.
- Design process, design-pack and font conventions are kept with the private kits, outside this repo. Design packs and run outputs are never committed (see AGENTS.md, .gitignore).

## Glossary
| Term | Meaning |
|---|---|
| Engine | An implementation of the `Engine` trait: everything specific to one coding agent (image, auth, workspace layout, argv, stream classification). Pi is the only one so far. |
| Task folder | Caller-supplied directory with a non-empty root `task.md` (the product brief) and optional `existing/index.html`, `assets/`. |
| Agent | `agents/<name>/`: `agent.md` plus optional `skills/` and `rules/`. Also called an agent pack; to be renamed ADK (see below). |
| ADK | Agent design kit: the owner's name (2026-11-19) for what code, docs and specs still call an agent pack (`agents/<name>/`). The rename itself waits for a scheduled epic. |
| Design pack | Optional `lib/<slug>/` with `DESIGN.md` and `globals.css`. |
| Run | One invocation: `runs/<run-id>/` holding the frozen inputs, `outputs/` and `receipt.json`. |
| Receipt | Host-owned, sanitised JSON record of a run; never visible to the container. |
