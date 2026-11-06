// Inline hints in the editor: widgets at the end of a line. They are decorations, not text: the
// document, a copy and a selection do not have them, and a line draws no differently for them
// (the selection is drawn from the text's own boxes). They are mapped through edits, so they stay on
// their line while the text is typed and are chosen again when the board is.

import { type Extension, StateEffect, StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, WidgetType } from '@codemirror/view';

import type { Hint } from './hints';

class HintWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  eq(other: HintWidget) {
    return other.text === this.text;
  }
  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-hint';
    el.textContent = this.text;
    el.setAttribute('aria-hidden', 'true');
    el.setAttribute('data-testid', 'inline-hint');
    return el;
  }
  ignoreEvent() {
    return true;
  }
}

/** Replace the hints of the editor (an empty list takes them away). */
export const setHints = StateEffect.define<Hint[]>();

function build(state: { doc: { lines: number; line(n: number): { to: number } } }, hints: Hint[]): DecorationSet {
  const ranges = hints
    .filter((h) => h.line >= 1 && h.line <= state.doc.lines)
    .sort((a, b) => a.line - b.line)
    .map((h) => Decoration.widget({ widget: new HintWidget(h.text), side: 1 }).range(state.doc.line(h.line).to));
  return Decoration.set(ranges, true);
}

const field = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(set, tr) {
    set = set.map(tr.changes);
    for (const e of tr.effects) if (e.is(setHints)) set = build(tr.state, e.value);
    return set;
  },
  provide: (f) => EditorView.decorations.from(f),
});

export const hintsExtension: Extension = field;
