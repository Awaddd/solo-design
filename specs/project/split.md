### Requirement: A public engine tree and a private kits tree
last-changed-by: SD-6-frontend-design-public

The project SHALL be exportable as a self-contained public engine tree with no private content, and its private content SHALL live in a separate kits tree.

#### AC project.split.1 — Manifest export: WHEN the export script runs THE SYSTEM SHALL produce a fresh tree containing exactly the paths listed in the public manifest, with contents from HEAD and the public overlay's files written over their paths
verified by: tests/public_export.rs › "exporting the public tree copies exactly the manifest's files into a fresh repository with one commit and no remote"

#### AC project.split.2 — Public tree passes: WHEN the exported public tree is built THE SYSTEM SHALL pass `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --locked` with only the example ADK present
last-changed-by: SD-6-frontend-design-public
manual: specs/history/2026-11-25-SD-6-frontend-design-public/check.md › public tree run (fmt, clippy, test --locked in the exported tree at its HEAD) (2026-11-25)

#### AC project.split.3 — Public hygiene: IF a file in the public manifest contains a name on the private deny-list (the source brands of the private design packs, the private server and host names, the private designs host) or an absolute home path (`/home/<user>/`) THEN THE SYSTEM's checks SHALL fail
verified by: tests/repo_hygiene.rs › "the public manifest's files carry no deny-listed names or home paths"

#### AC project.split.4 — Example task runs: WHEN `solo-design run --agent frontend-genjutsu --task <the example task>` is staged THE SYSTEM SHALL pass validation and freeze the inputs (no real model needed)
verified by: tests/command.rs › "the bundled example task with the example ADK passes validation and freezes its inputs"

#### AC project.split.5 — Kits via root: THE README SHALL document that private ADKs and design packs are found under `SOLO_DESIGN_ROOT` (`agents/`, `lib/`), with a worked example
verified by: glue — README "Private kits" section: SOLO_DESIGN_ROOT holding agents/ and lib/, with a worked run line

#### AC project.split.6 — Hallmark dropped: THE SYSTEM SHALL contain no base-hallmark or hallmark-frontend ADK, and its `THIRD_PARTY_NOTICES.md` SHALL cover only the example ADK's frontend-design skill
last-changed-by: SD-6-frontend-design-public
verified by: tests/packs.rs › "the bundled ADKs are the example ADK alone, and the third-party notices cover only its frontend-design skill"

#### AC project.split.7 — Example ADK provenance: THE public example ADK SHALL contain only text that is the project's own or derived under a licence that allows redistribution, with that licence and notice shipped beside it
last-changed-by: SD-6-frontend-design-public
manual: specs/history/2026-11-25-SD-6-frontend-design-public/evidence/licence-finding.md (upstream licence verified at the pinned commits) and specs/history/2026-11-21-SD-5-repo-split/g1-provenance.md (the rest of the ADK) (2026-11-25)

#### AC project.split.8 — Kits tree: WHEN the kits tree is assembled THE SYSTEM SHALL hold every item in Decision R3 at its stated path, as a local git repository
manual: specs/history/2026-11-21-SD-5-repo-split/evidence/kits-inventory.md (2026-11-21)

#### AC project.split.9 — No secrets in kits: WHEN the kits tree is assembled THE SYSTEM SHALL contain no key values and no key-shaped strings (a scan over every file reports none)
manual: specs/history/2026-11-21-SD-5-repo-split/evidence/kits-secret-scan.txt (2026-11-21)

#### AC project.split.10 — Archive untouched: WHILE splitting THE SYSTEM SHALL not rewrite this repository's history and SHALL create no GitHub repository and publish nothing without the owner (G2, G3)
glue — orchestrator STOP gates G2/G3; the local trees have no remote; main is an ancestor of the epic head

#### AC project.split.11 — Licence beside the skill: THE frontend-design skill folder SHALL hold `LICENSE.txt`, byte-identical to open-design's Apache-2.0 `skills/frontend-design/LICENSE.txt` (SHA-256 `0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594`), and `LICENSE-refero.txt`, holding refero_skill's MIT licence with its "Copyright (c) 2026 Refero" line and permission notice
last-changed-by: SD-6-frontend-design-public
verified by: tests/packs.rs › "the frontend-design skill ships the upstream Apache licence unchanged and the refero_skill MIT notice"

#### AC project.split.12 — Attribution: THE example ADK's `PROVENANCE.md` and the root `THIRD_PARTY_NOTICES.md` SHALL each name, on one line per upstream, `https://github.com/nexu-io/open-design` with commit `0c237048cb078f95ab01c416924dcb255f7d5dfd`, `https://github.com/anthropics/skills` with commit `00756142ab04c82a447693cf373c4e0c554d1005` (the origin of open-design's skill and of its `LICENSE.txt`) and `https://github.com/referodesign/refero_skill` with commit `2382e518f90096df607196b9a635da2d1bc142f9`, and SHALL state on one line that the skill's files were rewritten, with no text copied and the substance derived from those upstreams; neither SHALL describe the skill's upstream as unknown, unconfirmed or unverified
last-changed-by: SD-6-frontend-design-public
verified by: tests/packs.rs › "the frontend-design attribution names its three upstreams with their commits and says the text was rewritten"

#### AC project.split.13 — Skill in the public export: WHEN the export script runs THE SYSTEM SHALL export every tracked file under `agents/frontend-genjutsu/` unchanged from HEAD (including `skills/frontend-design/` and both licence files, with no public overlay replacing an ADK file) and `THIRD_PARTY_NOTICES.md`, with no dangling `/solo/agent/` reference
last-changed-by: SD-6-frontend-design-public
verified by: tests/public_export.rs › "the public export carries the whole example ADK with the frontend-design skill, its licence files and the third-party notices"

#### AC project.split.14 — design-brief stays private: IF the public manifest would export a file of the `design-brief` skill THEN THE SYSTEM's checks SHALL fail
last-changed-by: SD-6-frontend-design-public
verified by: tests/public_export.rs › "the public export holds no design-brief skill"
