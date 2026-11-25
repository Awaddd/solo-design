# Agent guide

Rust crate `solo-design`. Checks: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --locked`.

## Never commit

- Design packs (`lib/`), references, run outputs (`runs/`) and
  snapshots. `.gitignore` covers them; do not work around it.

## Model and provider are configuration

No model id, provider or subscription appears in code, tests or defaults.
They come from `SOLO_DESIGN_MODEL` / `SOLO_DESIGN_PROVIDER` or the config
file; tests use fixture values only.

## Tests

Tests never need real Docker, credentials or a model: use a test engine and a
fake docker script. Test titles are behaviour names.

## Designing with Solo Design

Design briefs, skills and the design process live in the private kits folder
that `SOLO_DESIGN_ROOT` points at (see README › Private kits). The owner's eye
is the judge, not LLM judges.
