# Typography rules

The brief or the active design system decides which typefaces to use. These rules
decide how type behaves once chosen.

## Scale
Build the sizes from one ratio (about 1.2 to 1.25) and keep the whole artifact to
roughly six to eight sizes. Typical ranges: display 48-72px, h1 32-48, h2 24-32,
h3 20-24, body 15-18, small 13-14, caption 11-12. Above the fold, show no more
than three distinct sizes.

## Leading
Large headings (32px and up) sit at 1.0-1.2. Body copy at 1.5-1.6. Small text
about 1.5.

CJK text needs more room, because the glyphs fill the whole em box. Headings use
1.3-1.4 and body 1.7-1.8, at every size including a giant cover title, and
negative tracking is dropped (use 0). In a mixed block such as a Latin kicker above
a CJK headline, apply the tight Latin values to the Latin element only.

## Tracking
- Body: default (0). Small text 11-13px: +0.01 to +0.02em. Labels and buttons: +0.02em.
- ALL CAPS: always +0.06 to +0.1em. Uppercase set tight looks cramped.
- Headings from 32px: -0.01 to -0.02em. Display from 48px: -0.02 to -0.03em.

## Pairing and fallbacks
At most two typefaces (or one variable family across weights). Always list a
fallback stack that still looks coherent. Do not leave a heading on a bare
system-ui.

## Measure and weight
Keep paragraphs to 50-75 characters (`max-width: 65ch` is a fine start). Three
weights are usually enough: regular for reading, medium for UI text, semibold for
headlines and buttons. Weights of 700+ are seldom needed.

## Lint list
- uppercase text without wide tracking
- large Latin display text without tightening
- CJK display text at Latin leading, or with negative tracking
- more than three sizes above the fold
- serif and slab mixed without a clear job each
- justified body text on the web (it creates rivers)
