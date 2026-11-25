### Requirement: The Pi engine
last-changed-by: SD-2-pi-engine

The system SHALL run designs with the Pi coding agent inside the sandboxed container, driven only by configuration.

#### AC engine.pi.1 — Pi engine: WHEN a run is prepared THE SYSTEM SHALL drive Pi with the configured model and provider, implementing the `Engine` trait without changing the core
verified by: tests/pi_engine.rs › "drives Pi with the configured model and provider"

#### AC engine.pi.2 — Pinned image: THE SYSTEM SHALL run Pi from a container image built from the repository's Dockerfile with an exact pinned Pi version, as a non-root user
verified by: tests/pi_image.rs › "builds Pi from an exact pinned version as a non-root user"

#### AC engine.pi.3 — Missing image: IF the engine's image is not present locally THEN THE SYSTEM SHALL fail during preflight, creating no run directory, with a message naming the build command
verified by: tests/command.rs › "fails preflight naming the build command when the image is missing and creates no run"

#### AC engine.pi.4 — System prompt staging: WHEN a run is staged THE SYSTEM SHALL compose one Pi system prompt from the selected agent's `agent.md` body, then its `rules/*.md`, then the selected design pack's `DESIGN.md` (as `## Active design system`), then each `skills/<s>/SKILL.md` under a `## Active skill — <s>` header (v1), joined with `\n\n---\n\n` (v1), followed by a fixed engine section stating the in-container roots (task, agent, skills references, design, outputs), that inputs are read-only, and that the deliverable is `index.html` in the outputs folder; other skill files are not inlined
verified by: tests/pi_engine.rs › "composes the agent, rules, design pack and skills into one system prompt in v1s order with the engines filesystem section"

#### AC engine.pi.5 — Brief as the task: WHEN Pi starts THE SYSTEM SHALL give it the frozen brief byte-identical as the task (an `@` attachment of `task.md`, which Pi wraps in its file block, as v1 did), with images attached, followed by a list of the available files as absolute in-container paths (seeded outputs, task assets, the design pack's `globals.css`, and every skill file other than `SKILL.md`) starting on a new line, adding no other instructions
verified by: tests/pi_engine.rs › "gives Pi the byte-identical brief as the task after the list of available files, images attached"

#### AC engine.pi.6 — Output location: WHILE Pi runs THE SYSTEM SHALL run it with the writable outputs folder as its working directory, with the task, agent, design and staged prompt read-only
verified by: tests/pi_engine.rs › "runs Pi in the writable outputs folder with every other mount read-only"

#### AC engine.pi.7 — Progress: WHILE Pi runs THE SYSTEM SHALL turn its event stream into sanitised one-line progress (turn starts, tool calls with paths, files written, errors), never printing model text in full or any key
verified by: tests/pi_stream.rs › "turns the stream into one-line progress without model text"

#### AC engine.pi.8 — Success: WHEN Pi ends with a normal terminal result (the last assistant message stops normally) and exits zero and `outputs/index.html` is a regular file that was absent before launch or differs from the seeded copy THE SYSTEM SHALL classify the run as success, and never on a zero exit alone; IF the deliverable is missing THEN the run fails with failure `no_deliverable`
verified by: tests/pi_stream.rs › "classifies only a normally stopped final message with a zero exit as success"

#### AC engine.pi.9 — Specific failures: IF Pi reports an authentication failure, an unknown model, a rate or credit limit, or a provider error THEN THE SYSTEM SHALL classify the run with that specific failure and exit nonzero
verified by: tests/pi_stream.rs › "classifies authentication, unknown model, rate or credit limit and provider errors"

#### AC engine.pi.10 — Incomplete stream: IF the stream is malformed or ends without a terminal result THEN THE SYSTEM SHALL classify the run as incomplete, never success
verified by: tests/pi_stream.rs › "classifies a malformed or unterminated stream as incomplete"

#### AC engine.pi.11 — Usage in the receipt: WHEN Pi reports token usage or cost THE SYSTEM SHALL record in the receipt the provider, model, and the input, output, cache-read and cache-write tokens and the estimated cost each summed over every message of the run, set `tokens` to the summed total tokens, and leave a field absent when no message reported it
verified by: tests/pi_stream.rs › "sums tokens and estimated cost over every message into the receipt, absent when never reported"

#### AC engine.pi.12 — Effort: WHERE an effort or thinking level is configured THE SYSTEM SHALL pass it to Pi and record it in the receipt, and pass none otherwise
verified by: tests/pi_engine.rs › "passes the configured thinking level and none otherwise"

#### AC engine.pi.13 — Run timeout: WHERE a run timeout is configured THE SYSTEM SHALL stop and remove the owned container when it elapses, keep the outputs, and record the run as timed out
verified by: tests/runtime.rs › "stops the container at the run timeout, keeps outputs and records a timeout"

#### AC engine.pi.14 — Real smoke run: WHEN the manual smoke run executes once per provider THE SYSTEM SHALL produce `outputs/index.html` and a receipt with tokens and cost
manual: the SD-2 check.md › Smoke runs (2026-11-19), kept in the archive
