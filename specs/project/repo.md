### Requirement: A clean, checked repository
last-changed-by: SD-1-repo-setup

The repository SHALL carry no identifiers of the previous product or personal paths, and CI SHALL enforce the checks.

#### AC project.repo.1 — Renamed: THE SYSTEM SHALL contain no identifiers of the previous product (its name and its command, built from parts in `tests/repo_hygiene.rs`) or `/Users/` paths in the tracked files under `src/`, `tests/`, `.github/` and in `Cargo.toml`, `Cargo.lock`, `build.rs`, `README.md` and `AGENTS.md` (`specs/` and `docs/` are not scanned: they hold provenance history)
verified by: tests/repo_hygiene.rs › "source, tests, manifests, CI, README and AGENTS carry no legacy product names or personal paths"

#### AC project.repo.2 — CI: WHEN a commit is pushed or a PR opened THE SYSTEM SHALL run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --locked`, failing on any error
verified by: glue — .github/workflows/ci.yml runs fmt --check, clippy -D warnings and test --locked on push and pull_request; first run green at 585164a (GitHub Actions run 43898592765)
