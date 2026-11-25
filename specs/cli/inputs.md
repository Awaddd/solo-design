### Requirement: Validated, frozen run inputs
last-changed-by: SD-1-repo-setup

The system SHALL freeze exactly the selected inputs into a per-run snapshot with a hash of every file.

#### AC cli.inputs.1 — Agent resolution: WHEN `--agent <name>` is given THE SYSTEM SHALL resolve exactly `agents/<name>/agent.md` and require its YAML front-matter `name` to equal the selector, failing otherwise without choosing another agent
verified by: tests/inputs.rs › "resolves only the named agent and requires its front-matter name to match"

#### AC cli.inputs.2 — Design resolution: WHEN `--design <slug>` is given THE SYSTEM SHALL require `lib/<slug>/` with regular `DESIGN.md` and `globals.css`, failing otherwise; WHEN it is omitted no design is staged
verified by: tests/inputs.rs › "requires a design pack with regular DESIGN.md and globals.css only when selected"

#### AC cli.inputs.3 — Regular trees only: IF a selected task, agent or design tree contains a symbolic link or special file THEN THE SYSTEM SHALL reject the run
verified by: tests/inputs.rs › "rejects selected trees containing links or special files"

#### AC cli.inputs.4 — Freeze with hashes: WHEN preflight succeeds THE SYSTEM SHALL copy the task folder, the selected agent (`agent.md`, `skills/`, `rules/`) and the selected design into a new per-run directory and record a SHA-256 for every frozen file
verified by: tests/inputs.rs › "freezes the selected task, agent and design with a SHA-256 per file"

#### AC cli.inputs.5 — Brief byte identity: WHEN `task.md` is frozen THE SYSTEM SHALL preserve its bytes exactly and add no runtime text to it
verified by: tests/inputs.rs › "keeps the frozen brief byte-identical"

#### AC cli.inputs.6 — Selected only: WHILE staging a run THE SYSTEM SHALL exclude every unselected agent, design pack and any other repository content
verified by: tests/inputs.rs › "stages nothing that was not selected"

#### AC cli.inputs.7 — Output seeding: WHEN the task has a regular `existing/index.html` or `assets/` THE SYSTEM SHALL seed them byte-identical into the run's `outputs/` before launch
verified by: tests/inputs.rs › "seeds existing index.html and assets into outputs byte-identical"

#### AC cli.inputs.8 — Sources untouched: WHEN a run ends in any outcome THE SYSTEM SHALL leave the caller's task folder, agents and design packs unchanged
verified by: tests/inputs.rs › "leaves the source task, agents and designs unchanged"
