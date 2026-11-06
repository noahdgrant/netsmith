// Indent guides: a hairline for each level of indentation, drawn as the background of the line
// (`cm-theme.ts`), so there is nothing to select or copy and nothing that shifts the text.

import { RangeSetBuilder } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

const STEP = 4;

/** How many levels of guide a line has: its leading spaces (a tab counts as a level) over the step. */
export function levelsOf(text: string): number {
  let spaces = 0;
  for (const c of text) {
    if (c === ' ') spaces++;
    else if (c === '\t') spaces += STEP;
    else break;
  }
  return Math.floor(spaces / STEP);
}

function build(view: EditorView): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  let last = -1;
  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to; ) {
      const line = view.state.doc.lineAt(pos);
      if (line.from > last) {
        last = line.from;
        // A blank line takes the depth of the line before it, so a guide is not broken by a gap.
        let n = levelsOf(line.text);
        if (line.text.trim() === '' && line.number > 1) n = Math.min(levelsOf(view.state.doc.line(line.number - 1).text), levelsOf(view.state.doc.line(Math.min(line.number + 1, view.state.doc.lines)).text));
        if (n > 0) b.add(line.from, line.from, Decoration.line({ class: 'cm-ig', attributes: { style: `--ig:${n}` } }));
      }
      pos = line.to + 1;
    }
  }
  return b.finish();
}

export const guidesExtension = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged) this.decorations = build(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
