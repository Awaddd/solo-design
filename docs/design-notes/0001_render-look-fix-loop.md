# Design note 0001 — Render → look → fix loop

**Status:** proposed. Owner priority: highest (2026-11-19).

## Problem
The agent writes `index.html` without ever seeing it. Defects a glance would
catch ship anyway: in exp2, a capture clipped inside a drawn phone frame.

## Proposal
Give the agent a browser inside the sandbox and one tool: screenshot.
- **Image:** bake headless Chromium (Playwright) into the Pi image, pinned.
  No new runtime: the NVIDIA OpenShell research found it has no browser or
  computer-use support (see [0005](0005_openshell-verdict.md)).
- **Tool:** `screenshot(viewport, full_page)` renders `/solo/outputs/index.html`
  with the task's `assets/` served beside it, and returns the PNG to the model
  as an image.
- **Rendering policy:** local files only by default. Google Fonts are allowed
  (the briefs allow web fonts); every other request is blocked and listed back
  to the model. The browser never reaches the network beyond that (see
  [0004](0004_key-proxy-and-egress-allowlist.md)).
- **Loop:** write → screenshot → fix, **at most 2 passes**. The agent is told
  what to look for: clipping, overflow, broken images, unreadable contrast,
  layout collapse at the narrow viewport, copy that differs from the brief.
- **Receipt:** the cost and tokens of each pass are recorded, plus the pass
  count, so the owner can see what the loop costs per design.

## Open points
- Whether the screenshot goes in as an image content part or as a file the
  model reads; Pi's read tool already handles images.
- A per-pass time cap inside the run timeout.

## Effort
Small: image change, one tool, a staging line in the prompt, receipt fields.
