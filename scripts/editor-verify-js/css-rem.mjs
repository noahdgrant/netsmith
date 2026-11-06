#!/usr/bin/env node
// Rewrites the editor's stylesheets so that sizes are rem (and tokens), which is what lets the settings' size (90 to 125 %)
// change the whole page by changing the root font size. It is a tool, not part of the build: run it on a stylesheet that has
// new px in it, and commit the result.
//
//   node scripts/editor-verify-js/css-rem.mjs crates/netsmith-editor/web/src/style.css crates/netsmith-editor/web/src/views.css ...
//
// What it converts: a length of more than 2 px in a declaration of the page's boxes (font-size, width, height, min and max of
// them, padding, margin, gap, top, right, bottom, left, inset, flex-basis, grid templates, and the custom properties of the shared
// scale in themes.css: --fs-*, --sp-*, --h-*, --row, --ctl*, --bar, --tab, --status, --gap), as that number over 16 in rem.
// What it leaves as px, on purpose: lengths up to 2 px (hairlines, outlines, a focus ring); borders, outlines, shadows, radii,
// transforms and strokes; every media and container query; and the rules of the drawings and the graph cards (the board, the
// schematic, the power tree and the system view): those have a zoom of their own, and the sizes of a card are also computed in
// script, so they stay one size whatever the page's size is (`responsive.css` sets the tokens back to px inside a canvas).
// Running it twice changes nothing.

import fs from 'node:fs';

const BOXES = /^(font-size|line-height|width|height|min-width|max-width|min-height|max-height|padding(-[a-z]+)*|margin(-[a-z]+)*|gap|row-gap|column-gap|top|right|bottom|left|inset(-[a-z]+)*|flex-basis|grid-template-columns|grid-template-rows|grid-auto-columns|grid-auto-rows|scroll-margin(-[a-z]+)*|scroll-padding(-[a-z]+)*|text-indent|columns|--(fs|sp|h|icon|ft)-[a-z0-9]+|--row|--ctl|--ctl-lg|--bar|--tab|--status|--gap)$/;
const SKIP_SELECTOR = /(\bpt-|\bsy-|\.card\b|\.canvas\b|\.graph|\.edge|\.arrowhead|svg\.schematic|\.schematic-host svg|\.pcb-host svg|\.pcb-overlay|\.pcb-grid|\.lsw|@keyframes|@font-face)/;

function convert(value) {
  return value.replace(/(?<![\w.#-])(-?\d*\.?\d+)px\b/g, (all, n) => {
    const v = Number(n);
    if (Math.abs(v) <= 2) return all;
    return `${String(Number((v / 16).toFixed(4)))}rem`;
  });
}

function body(decls) {
  // A comment may hold a colon or a semicolon: set each aside while the declarations are read.
  const kept = [];
  const bare = decls.replace(/\/\*[\s\S]*?\*\//g, (c) => `\u0000${kept.push(c) - 1}\u0000`);
  const done = bare
    .split(';')
    .map((d) => {
      const i = d.indexOf(':');
      if (i < 0) return d;
      const prop = d.slice(0, i).replace(/\u0000\d+\u0000/g, '').trim();
      if (!BOXES.test(prop)) return d;
      return `${d.slice(0, i + 1)}${convert(d.slice(i + 1))}`;
    })
    .join(';');
  return done.replace(/\u0000(\d+)\u0000/g, (_, n) => kept[Number(n)]);
}

for (const file of process.argv.slice(2)) {
  const css = fs.readFileSync(file, 'utf8');
  const out = css.replace(/([^{}]+)\{([^{}]*)\}/g, (all, selector, decls) => {
    // Only the last line of a prelude is the selector (a comment may stand before it).
    const sel = selector.replace(/\/\*[\s\S]*?\*\//g, '');
    if (SKIP_SELECTOR.test(sel)) return all;
    return `${selector}{${body(decls)}}`;
  });
  if (out !== css) fs.writeFileSync(file, out);
  console.log(`${file}: ${css === out ? 'unchanged' : 'converted'}`);
}
