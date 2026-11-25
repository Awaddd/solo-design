---
name: frontend-design
description: Implementation craft for a distinctive, working front-end page: direction, real states, typography, colour, layout, motion and a final self-review.
---

# Front-end design craft

Apply this whenever the deliverable is an interface: a page, a screen, a
component, a redesign. The aim is working code with a point of view, built for
the actual product and audience rather than a generic one.

## 1. Read the brief before picking a look

- Who is it for, what is the one job of this screen, what tone suits it?
- What is fixed: supplied copy, assets, screenshots, tokens, a selected design
  system, a viewport, a motion policy? Those win over taste. If two supplied
  rules collide and the task does not rank them, say so in your reply rather
  than quietly choosing.
- What is real in the screenshots (labels, data shapes, density)? Reuse it.

## 2. Commit to one direction

Name a single direction that fits the product (quiet and editorial, dense and
operational, warm and consumer, austere and technical) and make it concrete
through type, spacing, colour, hierarchy, motion and the shape of components.
Skip the stock look: violet-to-blue gradients, frosted cards, interchangeable
SaaS sections, pill-everything rounding, rows of stock icons, blobs with no job.

## 3. Build the real interface

- Ship what a user would need: navigation, controls, tables, filters, and the
  empty, loading, error, disabled and selected states.
- Use truthful content. Unknown data is labelled as sample or unavailable, never
  dressed up as a claim.
- Tools and dashboards should scan fast and tolerate repeated use. Marketing and
  portfolio pages may be more expressive.

## 4. Write sound code

- Self-contained HTML, CSS and a little JS unless a framework is requested.
- Semantic elements, keyboard-reachable controls, a visible focus style, text
  contrast that holds up, a layout that responds to width.
- Repeated colours, spacing, radii, shadows and type sizes live in CSS custom
  properties.

## 5. Finish the craft

- Type: pick faces on purpose; do not let a bare system stack be the idea unless
  utilitarian is the direction. Details in `references/typography.md`.
- Colour: neutrals carry the page, one accent leads, roles stay clear. Details in
  `references/color.md`.
- Layout: align to a grid, vary density, give space a purpose, keep panels from
  drifting.
- Motion: animate state changes and reveals with transform and opacity; keep it
  short; honour reduced-motion.
- Ornament (texture, borders, shadows, icons, imagery) only if it supports the
  concept.

## 6. Review before handing back

Check, by reading your source and, where a renderer exists, by looking:

- it holds up at phone and desktop widths;
- text fits its box and nothing overlaps;
- every interactive element has hover, focus, active and disabled treatment;
- none of the tells in `references/anti-slop.md` remain;
- someone could name one memorable thing about the page after closing it.
