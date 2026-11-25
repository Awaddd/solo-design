### Requirement: The run command and preflight
last-changed-by: SD-2-pi-engine

The system SHALL accept one run surface and validate everything before any container work.

#### AC cli.command.1 — Run surface: WHEN the operator invokes `solo-design run` THE SYSTEM SHALL accept required `--agent <name>`, required `--task <folder>` and optional `--design <slug>`
verified by: tests/command.rs › "accepts agent and task with or without a design"

#### AC cli.command.2 — Usage failure: IF a required argument is missing, repeated or unknown THEN THE SYSTEM SHALL exit nonzero with a usage error
verified by: tests/command.rs › "exits with a usage error for missing, repeated or unknown options"

#### AC cli.command.3 — Selector syntax: IF an agent or design selector does not match `^[a-z0-9]+(-[a-z0-9]+)*$` THEN THE SYSTEM SHALL reject it before touching the filesystem
verified by: tests/command.rs › "rejects unsafe selectors before touching the filesystem"

#### AC cli.command.4 — Task folder: IF the task path is missing, not a directory, a symbolic link, or lacks a non-empty regular root `task.md` THEN THE SYSTEM SHALL fail with a clear message
verified by: tests/preflight.rs › "rejects a missing, non-directory, linked or briefless task folder"

#### AC cli.command.5 — No work on failure: IF any preflight or validation step fails THEN THE SYSTEM SHALL start no container and create no run directory
verified by: tests/preflight.rs › "creates no run directory when validation fails"

#### AC cli.command.6 — Engine selection: IF the configured engine has no implementation THEN THE SYSTEM SHALL exit nonzero naming the available engines, after validation and without starting a container or creating a run
verified by: tests/command.rs › "fails preflight naming the available engines for an unknown engine and creates no run"

#### AC cli.command.7 — Resource root: WHEN `SOLO_DESIGN_ROOT` is set THE SYSTEM SHALL resolve agents, design packs and runs under it, and otherwise under the documented default
verified by: tests/preflight.rs › "resolves resources under SOLO_DESIGN_ROOT or the documented default"
