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

- Colours: the editor's accent `#e8873a` and text colour `#d6d9de` on the dark background `#151618`
  (`logo-dark.svg`, `logo-mark.svg`); `#b4530a` and `#1c1d20` for white backgrounds
  (`logo-light.svg`); `#d4691a` and `#8a8f98` in `logo.svg`, which keeps at least 3:1 contrast on both
  white and `#151618`; pure black or white in the `logo-mono-*` files.
- `social-preview.png` is 1280 x 640, GitHub's size for a repository social preview. It is set by hand
  in the repository's settings (Settings, General, Social preview); the file does nothing until then.
  Its tagline is set in the system's sans-serif font when the image is rendered (a picture, so no
  font file is part of the project).
- The editor's favicon and its fallbacks are in `crates/netsmith-editor/web/src/assets/`. They use
  heavier strokes than `logo-mark.svg` so the mark stays legible at 16 px.
