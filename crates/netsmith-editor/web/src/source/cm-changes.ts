// The source's margin when the comparison's marks are on: a bar beside each line that is new or
// replaces others, and a small triangle where lines are gone, from the line diff of the file at the
// ref against the text in the editor (so it follows typing). It also says how many changes there are,
// for the status bar. The base text is put in with an effect when it arrives; null turns the marks off.

import { type Extension, RangeSetBuilder, StateEffect, StateField } from '@codemirror/state';
import { Decoration, EditorView, GutterMarker, ViewPlugin, type ViewUpdate, gutter } from '@codemirror/view';
import { signal } from '@preact/signals';

import { type GutterMarks, gutterMarks, splitLines } from '../compare/linediff';

/** The base text of the file, or null when the marks are off (or the file is not in the comparison). */
export const setBaseText = StateEffect.define<string | null>();

interface Marked {
  added: Set<number>;
  changed: Set<number>;
  deleted: Set<number>;
  hunks: number;
  lines: number;
}

interface Held {
  base: string[] | null;
  marks: Marked | null;
}

function marked(base: string[], head: string[], lines: number): Marked {
  const m: GutterMarks = gutterMarks(base, head);
  // Lines that are gone sit above the line that follows them (below the last line, if none does).
  return { added: new Set(m.added), changed: new Set(m.changed), deleted: new Set(m.deletedAfter.map((after) => Math.min(after + 1, lines))), hunks: m.hunks, lines: m.lines };
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

/** The lines that differ get a faint tint of their colour across the line, so a change is seen without reading the margin. */
const lineTints = EditorView.decorations.compute([field], (state) => {
  const m = state.field(field).marks;
  const b = new RangeSetBuilder<Decoration>();
  if (!m) return b.finish();
  const numbers = [...new Set([...m.added, ...m.changed])].filter((n) => n >= 1 && n <= state.doc.lines).sort((x, y) => x - y);
  for (const n of numbers) b.add(state.doc.line(n).from, state.doc.line(n).from, Decoration.line({ class: m.added.has(n) ? 'cm-chg-line cm-chg-line-added' : 'cm-chg-line cm-chg-line-changed' }));
  return b.finish();
});

/** An overview ruler: a tick in the track at the right edge for each run of changed lines, where the file's whole length is the
 * track's, so the changes of a long file can be seen (and found) without scrolling through it. The ticks are drawn again when the
 * marks, the text or the size change. They are not read by a screen reader (the status bar says how many lines changed). */
const ruler = ViewPlugin.fromClass(
  class {
    dom: HTMLDivElement;
    constructor(readonly view: EditorView) {
      this.dom = document.createElement('div');
      this.dom.className = 'cm-chg-ruler';
      this.dom.setAttribute('aria-hidden', 'true');
      this.dom.setAttribute('data-testid', 'chg-ruler');
      view.dom.appendChild(this.dom);
      this.draw();
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.geometryChanged || u.startState.field(field) !== u.state.field(field)) this.draw();
    }
    draw() {
      const view = this.view;
      const m = view.state.field(field).marks;
      this.dom.replaceChildren();
      if (!m) return;
      const track = view.scrollDOM.clientHeight;
      const whole = Math.max(view.contentHeight, track);
      if (track <= 0) return;
      const kinds: [number, 'added' | 'changed' | 'deleted'][] = [
        ...[...m.added].map((n) => [n, 'added'] as [number, 'added']),
        ...[...m.changed].map((n) => [n, 'changed'] as [number, 'changed']),
        ...[...m.deleted].map((n) => [n, 'deleted'] as [number, 'deleted']),
      ];
      kinds.sort((x, y) => x[0] - y[0]);
      const lines = view.state.doc.lines;
      let run: { from: number; to: number; kind: string } | null = null;
      const flush = () => {
        if (!run) return;
        const top = view.lineBlockAt(view.state.doc.line(run.from).from).top;
        const bottom = view.lineBlockAt(view.state.doc.line(run.to).from).bottom;
        const tick = document.createElement('span');
        tick.className = `cm-chg-tick cm-chg-tick-${run.kind}`;
        tick.style.top = `${((top * track) / whole).toFixed(1)}px`;
        tick.style.height = `${Math.max(3, ((bottom - top) * track) / whole).toFixed(1)}px`;
        this.dom.appendChild(tick);
        run = null;
      };
      for (const [n, kind] of kinds) {
        if (n < 1 || n > lines) continue;
        if (run && run.kind === kind && n <= run.to + 1) run.to = n;
        else {
          flush();
          run = { from: n, to: n, kind };
        }
      }
      flush();
    }
    destroy() {
      this.dom.remove();
    }
  },
);

/** The changes of the file the editor shows, as the status bar counts them. */
export const sourceChanges = signal<{ file: string; hunks: number; lines: number } | null>(null);

export function changesExtension(path: string): Extension {
  return [
    field,
    lineTints,
    ruler,
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
      const next = m ? { file: path, hunks: m.hunks, lines: m.lines } : null;
      const now = sourceChanges.peek();
      if (now?.file === next?.file && now?.hunks === next?.hunks && now?.lines === next?.lines) return;
      // Another file's count is not this file's: only the file on show writes.
      if (next || now?.file === path) sourceChanges.value = next;
    }),
  ];
}
