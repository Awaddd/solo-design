# Testing notes

One line per failure pattern seen while writing tests.

- Placeholder `main` exits 2 with usage for every argv, so a usage-error e2e test passes vacuously: pair it with a well-formed argv that must not get the usage exit/text.
- Repo guard tests pass on a clean tree: assert the scanner on a temp `git init` fixture tree holding each forbidden form (built from parts) as well as on the real tree.
- A "no engine" e2e passes vacuously while `main` prints that message after partial validation: add a control with an invalid agent that must fail *without* the no-engine message.
- Clippy `-D warnings` flags `type_complexity` on `[(&str, fn(&Fixture)); N]` case tables in tests: name the tuple with a `type` alias.
- `tests/common/mod.rs` is compiled into each test crate separately; keep `#![allow(dead_code)]` at its top or helpers unused by one file break clippy.
- "Printed before launch" ordering: the test reporter reads the fake `docker` log at each reported line and records whether a `run` was already logged; no sleeps or timestamps.
- Runtime tests run `run(...)` on a spawned thread and `recv_timeout` the result, so a hung supervisor fails the test instead of hanging CI.
- Docker launch assertions parse both `-v src:dst[:ro]` and `--mount type=…,source=…,target=…,readonly` (and `--flag value` / `--flag=value`), so tests pin behaviour, not one argv spelling.
- A fake `docker` that must "keep running" uses `exec sleep N` so killing the client kills the sleeper; a plain `sleep` child would keep the stdout pipe open and block readers.
- Host-failure cleanup is provoked with a classifier that panics on a marker line; run the job inside `catch_unwind` on a worker thread and assert on the docker log, not on the panic.
- Signal tests raise real SIGINT/SIGTERM: keep them in their own test binary with one test, and assert the handler installed *before* raising (an uninstalled SIGINT kills the test process instead of failing an assertion).
- Never `fs::write` + chmod a script and exec it in a multi-threaded test binary: another thread's fork() inherits the write fd until its exec and the exec fails with ETXTBSY ("Text file busy", 54/1200 in a stress probe). Use `common::write_executable` (a waited-on `sh` child writes it).
- Dockerfile pin checks parse instructions (continuations joined, `ARG` defaults substituted) rather than grepping, and a parser self-test on an inline good Dockerfile plus loose forms (`latest`, `^x.y.z`, `node:22`) keeps the real-file test from passing or failing for spelling reasons.
- "Without reading the key" is proved with a mode-0o000 key file (any host open fails) plus the secret's absence from argv, reports and the run dir outside `outputs/`; tests never run as root, where 0o000 is readable.
- Preflight-failure e2e cases (unknown provider/engine, bad key file) each need a valid control that must reach `docker run`, or a binary failing early for any reason passes them.
- Pi repeats each assistant `usage` in `message_start` (zeros), `message_update` partials, `message_end`, `turn_end` and `agent_end.messages`: usage-sum tests use the recorded stream so double counting changes the total.
- e2e runs set PATH to the fake-bin folder only: a fake `docker` that runs any external program (`cat`, `sleep`) must name it in full (`/bin/cat`), or the "container" prints nothing and the run fails for a fixture reason.
- A leak test passes vacuously while the surface it guards is silent (no progress printed yet): assert the surface was exercised (a progress line, a staged dir, a receipt) before asserting the secret's absence.
- Fixtures that always build absolute paths miss cwd-relative settings (a relative key path became a Docker named volume): add a case run from a chosen `current_dir` with the setting relative, and assert what the launch actually mounts.
- A "burst wedges the timeout" case needs lines still queued when the timeout fires: a few thousand lines drain in milliseconds, so the fake container sleeps just under the timeout and then cats a 200k-line file. Verified red by applying the fix temporarily.
- To guarantee output is still queued when a timeout fires, stall the supervisor in the test's `report` callback past the deadline instead of racing a `sleep` against it, and have the fake mark the end of its burst: an absent marker proves the case was not vacuous.
- A fake's "finished" marker must be gated on success (`cat … && : > marker`): with `;`, a cat killed by SIGPIPE still lets the shell write the marker, and a non-vacuity check fails on a correct run (CI, PR #9).
- A new success precondition (e.g. `outputs/index.html` must exist) silently invalidates every success-path fixture that never produces it: seed the deliverable (`task/existing/index.html`) in those fixtures in the same change.
- Pi composes its first message from the `@` file blocks in argv order and then the positional message, joined with no separator (0.83.0 `dist/cli/initial-message.js`): argv position of the positional message is irrelevant, so assert the `@` order and the message's leading newline instead.
- Presence-only substring checks ("mentions X", "mentions Y") pass when X and Y occur in unrelated sentences: for a *relation* between terms, require them on one line with the linking verb and add a scattered-terms fixture that must fail.
- An export-from-HEAD test that clones the repo misses uncommitted Green work (the new script, manifest, example files): build the fixture as a fresh `git init` of the git-listed working tree, committed once, then add the uncommitted edit and untracked file on top.
- A real-tree scan over a manifest passes vacuously while the manifest is empty or lists only clean paths: assert the expansion is non-empty, and stub the manifest with the planned path list so the scan covers the files still to be scrubbed.
- Public-overlay files sit under an exported folder (`scripts/`), so a manifest-only hygiene scan covers them only by accident and stops once the overlay is excluded: scan them explicitly, with a fixture whose manifest leaves the overlay out.
- An export guard (no excluded skill, no dangling ADK reference) passes on a correct tree and proves nothing alone: in the same temp clone, commit a variant that breaks each guard (an overlay naming an excluded file; the `!` line dropped), re-export and assert each is flagged.
- The provider-name scan (`tests/repo_hygiene.rs`) reads tokens, so a path literal like the agent-config folder (`.c…/`) in test code trips it: build such paths from parts in a helper.
