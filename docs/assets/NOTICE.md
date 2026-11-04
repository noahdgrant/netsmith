# Logo assets

The netsmith logo is drawn for this project and is covered by the project's licence. It is a
net-anvil (six round nodes joined by lines, in the shape of an anvil) with a hammer above it, and the
word "netsmith" drawn as traces.

- **No font is used.** The letters of the wordmark are polylines of horizontal, vertical and 45 degree
  segments, stroked, so every file renders the same everywhere and nothing needs a font licence or
  an attribution.
- **No third-party art.** Every shape is a circle, a rounded rectangle or a straight line.
- Colours: the editor's accent `#e8873a` and text colour `#d6d9de` on the dark background `#151618`
  (`logo-dark.svg`, `logo-mark.svg`); `#b4530a` and `#1c1d20` for white backgrounds
  (`logo-light.svg`); `#d4691a` and `#8a8f98` in `logo.svg`, which keeps at least 3:1 contrast on both
  white and `#151618`; pure black or white in the `logo-mono-*` files.
- `outline/` holds the same set with the hammer drawn as an outline (an alternative).
- `social-preview.png` is 1280 x 640, GitHub's size for a repository social preview. It is set by hand
  in the repository's settings (Settings, General, Social preview); the file does nothing until then.
  Its tagline is set in the system's sans-serif font when the image is rendered; no font file is part
  of the project.
- The editor's favicon and its fallbacks are in `crates/netsmith-editor/web/src/assets/`. They use
  heavier strokes than `logo-mark.svg` so the mark stays legible at 16 px.
