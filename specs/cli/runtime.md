### Requirement: A sandboxed, supervised container per run, parameterised by an engine
last-changed-by: SD-1-repo-setup

The system SHALL run one hardened container per run through the Docker CLI, with all engine-specific values supplied by an `Engine`.

#### AC cli.runtime.1 — Engine boundary: THE SYSTEM SHALL take the image, auth (a read-only mount and/or environment variable names passed through by name, never values), in-container workspace layout, argv and stream classification only from an `Engine` implementation, with no engine-specific values in the core
verified by: tests/runtime.rs › "takes image, auth, layout, argv and classification only from the engine, which receives the configured model and provider unchanged"

#### AC cli.runtime.2 — Mounts: WHEN a run's container starts THE SYSTEM SHALL mount the frozen inputs read-only and the run's `outputs/` as the only writable workspace mount, plus the engine's read-only auth mount when it supplies one
verified by: tests/runtime.rs › "mounts frozen inputs read-only and only outputs writable"

#### AC cli.runtime.3 — Hardening: WHEN a run's container starts THE SYSTEM SHALL run it non-root with a read-only root filesystem, all capabilities dropped, no-new-privileges, a tmpfs `/tmp`, and without the Docker socket or the host home
verified by: tests/runtime.rs › "starts a hardened non-root container without the socket or home"

#### AC cli.runtime.4 — Owned container: WHEN a container is started THE SYSTEM SHALL give it a unique `solo-design-` name owned by this invocation and remove only that container when the run ends
verified by: tests/runtime.rs › "names the container uniquely and removes only that container"

#### AC cli.runtime.5 — Signals: WHEN the host receives SIGINT or SIGTERM during a run THE SYSTEM SHALL stop and remove the owned container, keep the run's outputs, write the receipt and exit nonzero
verified by: tests/runtime.rs › "on interruption stops and removes the container, keeps outputs and writes the receipt"

#### AC cli.runtime.6 — Host failure cleanup: IF the host fails after the container starts (error or panic in supervision) THEN THE SYSTEM SHALL still remove the owned container
verified by: tests/runtime.rs › "removes the container when supervision fails after launch"

#### AC cli.runtime.7 — Interrupted before launch: WHEN the host receives SIGINT or SIGTERM after the run directory exists but before the container starts THE SYSTEM SHALL start no container, write an `interrupted` receipt and exit nonzero
verified by: tests/runtime.rs › "an interruption before launch starts no container and writes an interrupted receipt"
