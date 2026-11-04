# Logo assets

The netsmith logo is drawn for this project and is covered by the project's licence, except for
the letters of the wordmark, which are derived from a font under its own licence (below). The mark
is a net-anvil (six round nodes joined by lines, in the shape of an anvil) with a hammer above it.
Every shape of the mark is a circle, a rounded rectangle or a straight line, drawn for this project.

## The wordmark's font

The word "netsmith" in `logo.svg`, `logo-dark.svg`, `logo-light.svg`, `logo-mono-black.svg`,
`logo-mono-white.svg` (and in the editor's top bar, `crates/netsmith-editor/web/src/components/Logo.tsx`,
and `social-preview.png`) is set in **Zilla Slab Bold** (weight 700). These files contain **outlines
derived from the font**: the text was converted to SVG paths, so nothing renders with a font and no
font file is part of this repository.

- Font: Zilla Slab, Bold, `ZillaSlab-Bold.ttf`, "Version 1.1; 2017; ttfautohint (v1.6)".
- Designer: Typotheque (typotheque.com), made for Mozilla.
- Copyright, as in the font's own `OFL.txt`: `Copyright 2017, The Mozilla Foundation`.
- Licence: SIL Open Font License, Version 1.1 (26 February 2007), text and FAQ at
  <https://openfontlicense.org> (the font's licence file also points to <http://scripts.sil.org/OFL>).
  The Font Software is not modified or redistributed here; the licence allows its use to make
  artwork, and the outlines are shown with this notice.
- "Zilla is a trademark of The Mozilla Corporation" (from the font's name table). Nothing here
  implies a connection with Mozilla.
- Source: the Google Fonts repository, `github.com/google/fonts`, directory `ofl/zillaslab`.
- Made with opentype.js 1.3.4: the glyphs of "netsmith" at a fixed size with the font's kerning,
  laid out by hand into one path for "net" and one for "smith".

## Colours and files

There is one accent for each mode, used in every asset of that mode: the editor's orange `#e8873a`
for a dark background, and `#d4691a` for a light one (the same hue, darker: 3.6:1 on white, 3.4:1
on GitHub's light canvas `#f6f8fa`). Text is `#d6d9de` on dark and `#1c1d20` on light.

| File | For | Accent | Text ("net") |
| --- | --- | --- | --- |
| `logo-mark-dark.svg`, `logo-dark.svg` | dark backgrounds | `#e8873a` | `#d6d9de` |
| `logo-mark-light.svg`, `logo-light.svg` | light backgrounds | `#d4691a` | `#1c1d20` |
| `logo.svg` | follows the reader's `prefers-color-scheme` (dark when there is none) | `#e8873a`, `#d4691a` in light | `#d6d9de`, `#1c1d20` in light |
| `logo-mono-black.svg`, `logo-mono-white.svg` | one colour | black, white | the same |
| `social-preview.png` | dark by design | `#e8873a` | `#d6d9de`, tagline `#9aa1ab` |
| favicon (`crates/netsmith-editor/web/src/assets/favicon.svg`) | follows `prefers-color-scheme` (dark when there is none) | `#e8873a`, `#d4691a` in light | none |
| `favicon-32.png`, `apple-touch-icon.png` (same folder) | dark accent | `#e8873a` | none |

The README shows `logo-dark.svg` and `logo-light.svg` through a `<picture>`.

- The mark has no ambiguous file: `logo-mark-dark.svg` and `logo-mark-light.svg`.
- The favicon has heavier strokes than the mark so it stays legible at 16 px. The 180 px icon has
  a `#151618` background.
- `social-preview.png` is 1280 x 640, GitHub's size for a repository social preview. It is set by hand
  in the repository's settings (Settings, General, Social preview); the file does nothing until then.
  It is rendered from `social-preview.html`, where the tagline is the one text to change, by
  `node docs/assets/render-social.mjs` (Node 20 or newer; `cd scripts/editor-verify-js && npm ci` once
  for the headless Chromium). The tagline is set in the system's sans-serif font; it is a picture, so no
  font file is part of the project.
