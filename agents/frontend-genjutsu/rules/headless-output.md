# Headless output rules

- Write only `/solo/outputs/index.html`. Do not write into `/solo/task`, `/solo/agent`
  or `/solo/design`; those are read-only.
- No network use with one exception: Google Fonts (`fonts.googleapis.com`,
  `fonts.gstatic.com`), only for a family DESIGN.md names. The viewer's browser
  loads them; the run itself stays offline. No CDN, no remote image, no other
  external script or stylesheet.
- Supplied images go in by relative path, `assets/<name>`. Use no other imagery.
- Never invent metrics, quotes, customer names, prices or dates. A missing fact
  gets a labelled placeholder or a layout that does not need it.
- Report honestly: checks you could not run (rendering, contrast tools,
  performance) are reported as not run.
- Put `img { height: auto; }` in your base CSS: the HTML `width` and `height`
  attributes on an image otherwise override the CSS `aspect-ratio` and
  distort it.
