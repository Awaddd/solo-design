# Colour rules

Use the palette and roles that the brief or the design system supplies. These
rules shape how to spend it.

## Four layers
Plan them before any CSS: neutrals (roughly 70-90% of the pixels: backgrounds,
surfaces, text, borders), one accent (5-10%), semantic colours for success,
warning and danger (up to 5%), and effects such as gradients or glows (under 1%,
rarely justified). Never invent a second accent.

## Accent discipline
Accent overuse is the commonest failure. Allow about two visible accent moments
per screen, for example one eyebrow plus the primary action. Links, hover states
and focus rings count toward that. If the screen already has a primary action,
render ordinary links in the text colour with an underline.

## Contrast floors
Body text under 18px against its background: 4.5:1. Large or bold-large text: 3:1.
Controls and graphical parts against what is next to them: 3:1. When a brand
colour fails as text, use a darker step for text and keep the bright step for fills.

## Dark themes
Do not use pure black or pure white as ground or text; a near-black and an
off-white vibrate less. On dark surfaces, hairline borders in translucent white
(around 8% opacity) suggest structure quietly.

## Naming
Name tokens for their job (surface, text-muted, accent, danger), not their hue.

## Anti-defaults
- Tailwind's violet and indigo accents (#6366f1 and its neighbours) read as machine-made.
- A two-stop hero gradient (violet to blue, blue to cyan) is the next tell.
- A gradient should separate levels of hierarchy, not fill empty space.
