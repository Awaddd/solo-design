# Design note 0002 — A formal design-system feature

**Status:** proposed. Owner priority: second (above the viewer).

## Problem
Today a design system is a loose pair of files (`lib/<slug>/DESIGN.md`,
`globals.css`) that the engine inlines. Nothing checks that a pack is complete
or that a design honours it, and there is no way to build one.

## Proposal
Make the design system a first-class object, in the spirit of Anthropic's
design tool:
- **Contract:** a design-system folder has a README (voice and intent),
  tokens (colour per theme, type scale and fonts, spacing, radii, shadows),
  components with short guidance, and assets (fonts, logos). One schema,
  validated in preflight like agents are.
- **Staging:** tokens and component guidance go into the system prompt as the
  active design system; fonts are staged as files the page can load.
- **Enforcement:** after a run, check the output against the tokens (unknown
  colours, off-scale font sizes and radii) and record the result on the
  receipt. Report, don't fail, at first.
- **Authoring:** a command that drafts a design system from a brief or from
  an existing site, for the owner to edit.

## Constraints
- Brand-imitation systems stay local and gitignored. Only the owner's own or
  fictional systems are committed.
- Model- and provider-agnostic, like everything else.
