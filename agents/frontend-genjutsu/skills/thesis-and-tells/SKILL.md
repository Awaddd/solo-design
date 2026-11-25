---
name: thesis-and-tells
description: A short written thesis before building, and a four-family check for invented, decorative, habitual and hollow content before handing back.
---

# Thesis first, tells last

This sits on top of Front-end design and any active design system. It adds two
disciplines and no new visual style.

## A. Write the thesis before the page

Put it in the first comment of your source file, not in the rendered page.

- Visual thesis: colour direction; which typeface the design system or brief
  names and why it suits this product; spacing; how components are treated.
- Interaction thesis: timing and easing, hover behaviour, scroll behaviour,
  motion you will not use, what happens under reduced motion.
- Do not invent a token or behaviour the system or brief did not supply.
- Guessability test: if the thesis could be written from the product category
  alone, tie it to a fact from this brief instead of another mood word.
- Write `Allowed patterns:` followed by each deliberate device and the product or
  authority reason for it, or `none`. An adjective like "premium" does not count as a pattern.
- Nobody approves the thesis in a headless run, so mark it unvalidated. An
  unvalidated thesis excuses no tell, and a required token is not a licence to
  decorate with it.

## B. Check four families, in markup and in strings

While writing, and again before handing back, read the visible markup and also
the strings inside script arrays and objects, alt text and labels.

1. Invented information. Every figure, person, quotation, version or status line, urgency claim,
   image credit, place, time display and little imitation product picture must trace
   back to a supplied fact or asset. Keep only what traces.
2. Decorative filler. Ask what each of these tells the reader: a numbered eyebrow,
   a pill, a status dot, a scroll hint, a keyword ribbon, a crosshair, a glow, a
   blob, a looping animation. Nothing? Remove it.
3. Habit over reason. Repeated identical cards, zigzag rows, empty bento cells,
   floating corner paragraphs, ruled lines, outsized display type, warm-paper
   grounds, glass, mono labels, gradient text: did the brief or system call for
   them, or did habit?
4. Hollow copy. Does each sentence say something true and specific? Look for filler
   verbs, poetic labels, mannered proof, the same action under several button names,
   shifts of register, and every visible string you have not actually read.

These are questions, not bans, and not a house style. Do not "fix" a choice the
design system requires by swapping its tokens.

For each finding note the place (file line and the text), the reason it counts as a tell, and what the thesis question yields. It is acceptable only if it matches a named allowed
pattern, or you can point at source evidence that it is not that tell. Otherwise fix
the line, or report it. Re-read lines after changing them. Reading source is not a
rendering or an automated audit; say which checks were not run.
