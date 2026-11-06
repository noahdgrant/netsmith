// The source's margin when the comparison's marks are on: a bar beside each line that is new or
// replaces others, and a small triangle where lines are gone, from the line diff of the file at the
// ref against the text in the editor (so it follows typing). It also says how many changes there are,
// for the status bar. The base text is put in with an effect when it arrives; null turns the marks off.

import { type Extension, StateEffect, StateField } from '@codemirror/state';
import { EditorView, GutterMarker, gutter } from '@codemirror/view';
import { signal } from '@preact/signals';

import { type GutterMarks, gutterMarks, splitLines } from '../compare/linediff';

/** The base text of the file, or null when the marks are off (or the file is not in the comparison). */
export const setBaseText = StateEffect.define<string | null>();

interface Marked {
  added: Set<number>;
  changed: Set<number>;
  deleted: Set<number>;
  hunks: number;
}

interface Held {
  base: string[] | null;
  marks: Marked | null;
}

function marked(base: string[], head: string[], lines: number): Marked {
  const m: GutterMarks = gutterMarks(base, head);
  // Lines that are gone sit above the line that follows them (below the last line, if none does).
  return { added: new Set(m.added), changed: new Set(m.changed), deleted: new Set(m.deletedAfter.map((after) => Math.min(after + 1, lines))), hunks: m.hunks };
}

const field = StateField.define<Held>({
  create: () => ({ base: null, marks: null }),
  update(held, tr) {
    let base = held.base;
    let changed = false;
    for (const e of tr.effects) {
      if (e.is(setBaseText)) {
        base = e.value === null ? null : splitLines(e.value);
        changed = true;
      }
    }
    if (!changed && !tr.docChanged) return held;
    if (!base) return { base, marks: null };
    return { base, marks: marked(base, splitLines(tr.state.doc.toString()), tr.state.doc.lines) };
  },
});

class Bar extends GutterMarker {
  constructor(readonly kind: 'added' | 'changed' | 'deleted') {
    super();
  }
  eq(o: Bar) {
    return o.kind === this.kind;
  }
  toDOM() {
    const el = document.createElement('span');
    el.className = `cm-chg cm-chg-${this.kind}`;
    el.setAttribute('data-testid', `chg-${this.kind}`);
    el.title = this.kind === 'added' ? 'New since the ref' : this.kind === 'changed' ? 'Changed since the ref' : 'Lines were removed here since the ref';
    return el;
  }
}

const bars = { added: new Bar('added'), changed: new Bar('changed'), deleted: new Bar('deleted') };

/** The changes of the file the editor shows, as the status bar counts them. */
export const sourceChanges = signal<{ file: string; hunks: number } | null>(null);

export function changesExtension(path: string): Extension {
  return [
    field,
    gutter({
      class: 'cm-chg-gutter',
      lineMarker(view, line) {
        const m = view.state.field(field).marks;
        if (!m) return null;
        const n = view.state.doc.lineAt(line.from).number;
        if (m.added.has(n)) return bars.added;
        if (m.changed.has(n)) return bars.changed;
        return m.deleted.has(n) ? bars.deleted : null;
      },
      lineMarkerChange: (u) => u.startState.field(field) !== u.state.field(field),
      initialSpacer: () => bars.added,
    }),
    EditorView.updateListener.of((u) => {
      if (u.startState.field(field) === u.state.field(field)) return;
      const m = u.state.field(field).marks;
      const next = m ? { file: path, hunks: m.hunks } : null;
      const now = sourceChanges.peek();
      if (now?.file === next?.file && now?.hunks === next?.hunks) return;
      // Another file's count is not this file's: only the file on show writes.
      if (next || now?.file === path) sourceChanges.value = next;
    }),
  ];
}
