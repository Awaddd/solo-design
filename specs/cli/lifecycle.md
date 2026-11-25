### Requirement: An observable run with a host-owned receipt
last-changed-by: SD-1-repo-setup

The system SHALL report the run's progress and outcome and record it in a sanitised receipt.

#### AC cli.lifecycle.1 — Output path first: WHEN preflight succeeds THE SYSTEM SHALL print the run's absolute output path before starting the container
verified by: tests/runtime.rs › "prints the absolute output path before the container starts"

#### AC cli.lifecycle.2 — Progress: WHILE the container runs THE SYSTEM SHALL print the engine's sanitised progress for each stdout/stderr line it classifies
verified by: tests/runtime.rs › "prints the engine's progress for classified lines"

#### AC cli.lifecycle.3 — Outcome: WHEN the container exits THE SYSTEM SHALL exit zero only if the engine classifies the run as success and the container exited zero, and nonzero otherwise
verified by: tests/runtime.rs › "succeeds only on engine success with a zero container exit"

#### AC cli.lifecycle.4 — Receipt: WHEN a run that started a container ends THE SYSTEM SHALL write `receipt.json` with status, exit code or signal, output path, agent, design, engine, start and end times, input hashes, output hashes, model and provider (from configuration), and effort and tokens when the engine reports them
verified by: tests/receipt.rs › "records status, exit, paths, selection, configured model and provider, times and hashes in the receipt"

#### AC cli.lifecycle.5 — Receipt outside the sandbox: WHILE the container runs THE SYSTEM SHALL keep the receipt and its parent directory outside every container mount
verified by: tests/receipt.rs › "keeps the receipt outside every container mount"

#### AC cli.lifecycle.6 — Receipt sanitised: WHEN the receipt is written THE SYSTEM SHALL include only its allow-listed fields, never environment values, credentials, auth paths, container ids or task content
verified by: tests/receipt.rs › "writes only allow-listed receipt fields"
