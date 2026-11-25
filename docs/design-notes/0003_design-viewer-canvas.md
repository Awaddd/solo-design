# Design note 0003 — A design viewer: takes × states on a canvas

**Status:** proposed. Owner priority: third.

## Problem
Reviewing means opening run folders and screenshots one by one. The owner's
eye is the benchmark, so the review surface matters.

## Proposal
A viewer that lays out a job's outputs as artboards on a canvas:
- **Columns:** takes (pack × design system × model, or repeated runs).
- **Rows:** states of the same screen (empty, filled, error, selected, open
  panel) and viewports (1440, 390).
- **Each artboard:** the rendered page (live, not only a screenshot), with
  its run id, pack, design system and cost from the receipt.
- **Verdicts:** the owner marks picks and notes per artboard; they are saved
  beside the run, so the process record writes itself.

## Notes
- Generating states needs either several briefs or a brief that names the
  states; start with one state per run.
- Hosting: local first. A shared host (for example the owner's designs site
  or a future artboard host) must be on the egress allowlist
  ([0004](0004_key-proxy-and-egress-allowlist.md)) before anything previews
  from it.
