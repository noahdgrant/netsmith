// A read-only CodeMirror view of a comparison of one file: either one view with the removed and the
// added lines in turn (inline), or two views that scroll together (side by side). The document is the
// text of the rows, so selecting and copying work; the colours of added and removed lines, the marks
// inside a line and the two columns of line numbers are decorations and a gutter.

import { EditorState, RangeSetBuilder, StateEffect, StateField } from '@codemirror/state';
import { BlockType, Decoration, type DecorationSet, EditorView, GutterMarker, WidgetType, gutter } from '@codemirror/view';

import { colouring } from '../languages';
import { cmTheme } from '../source/cm-theme';
import { barText, type Region } from './collapse';
import type { Span } from './linediff';

export interface ViewRow {
  text: string;
  kind: 'equal' | 'add' | 'del' | 'filler';
  /** The line number at the ref, and the one on disk (1-based). */
  a?: number;
  b?: number;
  marks: Span[];
}

class Nums extends GutterMarker {
  constructor(
    readonly a: number | undefined,
    readonly b: number | undefined,
    readonly columns: 'both' | 'a' | 'b',
    readonly sign: string,
  ) {
    super();
  }
  eq(o: Nums) {
    return o.a === this.a && o.b === this.b && o.columns === this.columns && o.sign === this.sign;
  }
  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-dn';
    const col = (n: number | undefined, cls: string) => {
      const s = document.createElement('span');
      s.className = `cm-dn-${cls}`;
      s.textContent = n === undefined ? '' : String(n);
      el.appendChild(s);
    };
    if (this.columns !== 'b') col(this.a, 'a');
    if (this.columns !== 'a') col(this.b, 'b');
    const sign = document.createElement('span');
    sign.className = 'cm-dn-sign';
    sign.setAttribute('aria-hidden', 'true');
    sign.textContent = this.sign;
    el.appendChild(sign);
    return el;
  }
}

const SIGN = { equal: '', add: '+', del: '−', filler: '' } as const;

/** A hidden run of unchanged rows: the rows, and the number the bar is named by (the base line the run starts at: the same in both ways of showing the changes). */
export interface Collapsed extends Region {
  key: number;
}

export interface DiffViewOptions {
  columns: 'both' | 'a' | 'b';
  /** The index of a row to bring into view and outline. */
  focus?: number;
  label: string;
  /** The runs of unchanged rows to hide behind a bar, and the keys of those already opened. */
  collapsed?: Collapsed[];
  opened?: ReadonlySet<number>;
  /** A bar was asked to open (a click, Enter or Space on it): the caller opens it in its other views too. */
  onOpen?: (key: number) => void;
}

/** Open the hidden run with this key. */
export const openRegion = StateEffect.define<number>();

/** A slim bar in the place of a run of unchanged lines: a button, so Tab reaches it and Enter or Space opens it. */
class Bar extends WidgetType {
  constructor(
    readonly count: number,
    readonly key: number,
    readonly first: string,
    readonly onOpen: ((key: number) => void) | undefined,
  ) {
    super();
  }
  eq(o: Bar) {
    return o.count === this.count && o.key === this.key && o.first === this.first;
  }
  toDOM() {
    const el = document.createElement('button');
    el.type = 'button';
    el.className = 'cm-diff-bar';
    el.setAttribute('data-testid', 'diff-bar');
    el.setAttribute('data-count', String(this.count));
    el.setAttribute('aria-label', `Show ${barText(this.count)}, ${this.first}`);
    el.title = `Show ${barText(this.count)} (${this.first})`;
    const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    icon.setAttribute('viewBox', '0 0 16 16');
    icon.setAttribute('class', 'icon cm-diff-bar-icon');
    icon.setAttribute('aria-hidden', 'true');
    icon.innerHTML = '<path d="M4.5 6L8 2.8 11.5 6M4.5 10L8 13.2 11.5 10"/>';
    const label = document.createElement('span');
    label.textContent = barText(this.count);
    el.append(icon, label);
    el.addEventListener('click', () => this.onOpen?.(this.key));
    return el;
  }
  // The button takes its own clicks and keys.
  ignoreEvent() {
    return true;
  }
}

/** The decorations of a view: the colours of the lines, the marks in them, and a bar over each run that is hidden and not opened. */
function build(rows: ViewRow[], collapsed: Collapsed[], opened: ReadonlySet<number>, focus: number | undefined, onOpen: ((key: number) => void) | undefined): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const starts: number[] = [];
  let pos = 0;
  for (const r of rows) {
    starts.push(pos);
    pos += r.text.length + 1;
  }
  const hidden = collapsed.filter((c) => !opened.has(c.key));
  let next = 0;
  for (let i = 0; i < rows.length; i++) {
    const h = hidden[next];
    if (h && i === h.from) {
      const from = starts[h.from];
      const to = starts[h.to - 1] + rows[h.to - 1].text.length;
      const num = (r: ViewRow) => r.a ?? r.b ?? '';
      const where = `lines ${num(rows[h.from])} to ${num(rows[h.to - 1])}`;
      builder.add(from, to, Decoration.replace({ block: true, widget: new Bar(h.to - h.from, h.key, where, onOpen) }));
      i = h.to - 1;
      next++;
      continue;
    }
    const r = rows[i];
    builder.add(starts[i], starts[i], Decoration.line({ class: `cm-dl cm-dl-${r.kind}${i === focus ? ' cm-dl-focus' : ''}` }));
    for (const [from, to] of r.marks) {
      const f = Math.min(from, r.text.length);
      const t = Math.min(to, r.text.length);
      if (t > f) builder.add(starts[i] + f, starts[i] + t, Decoration.mark({ class: r.kind === 'add' ? 'cm-di cm-di-add' : 'cm-di cm-di-del' }));
    }
  }
  return builder.finish();
}

/** Build the view in `parent`. The caller destroys it. */
export function createDiffView(parent: HTMLElement, rows: ViewRow[], path: string, o: DiffViewOptions): EditorView {
  const doc = rows.map((r) => r.text).join('\n');
  const markers = rows.map((r) => new Nums(r.a, r.b, o.columns, SIGN[r.kind]));
  const widest = new Nums(99999, 99999, o.columns, '+');
  const collapsed = o.collapsed ?? [];
  const field = StateField.define<{ opened: ReadonlySet<number>; deco: DecorationSet }>({
    create: () => {
      const opened = o.opened ?? new Set<number>();
      return { opened, deco: build(rows, collapsed, opened, o.focus, o.onOpen) };
    },
    update(value, tr) {
      let opened = value.opened;
      for (const e of tr.effects) if (e.is(openRegion) && !opened.has(e.value)) opened = new Set([...opened, e.value]);
      return opened === value.opened ? value : { opened, deco: build(rows, collapsed, opened, o.focus, o.onOpen) };
    },
    provide: (f) => EditorView.decorations.from(f, (v) => v.deco),
  });
  const state = EditorState.create({
    doc,
    extensions: [
      EditorState.readOnly.of(true),
      EditorView.contentAttributes.of({ 'aria-readonly': 'true', 'aria-label': o.label }),
      cmTheme,
      ...colouring(path),
      gutter({
        class: 'cm-diff-gutter',
        // The rows a bar hides have no numbers: the bar is one block, whose line is the first of the run.
        lineMarker: (view, line) => (line.type !== BlockType.Text ? null : (markers[view.state.doc.lineAt(line.from).number - 1] ?? null)),
        initialSpacer: () => widest,
      }),
      field,
    ],
  });
  const view = new EditorView({ state, parent });
  view.dom.setAttribute('data-testid', 'diff-view');
  return view;
}

/** Bring row `index` to the top third of the view. */
export function scrollToRow(view: EditorView, index: number, margin = 80) {
  const line = view.state.doc.line(Math.min(Math.max(index + 1, 1), view.state.doc.lines));
  view.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: 'start', yMargin: margin }) });
  // The scroll lands between two lines; a line cut in half under the header reads as a fault. Once it has landed, move to the line
  // below the cut (a line is never cut when the view's top is on a line's top).
  // (The scroll is made when CodeMirror next measures, which can be a few frames on: it is looked at twice, and a second look changes nothing.)
  const settle = () => snapToLine(view);
  for (const ms of [60, 220]) setTimeout(settle, ms);
}

/** Put the top of the view on the top of a line: the line cut by the edge is scrolled out, never left half shown. It looks at the
 * lines as laid out (CodeMirror's own estimate of a line it has not drawn yet can be off by a few pixels). */
export function snapToLine(view: EditorView) {
  const sc = view.scrollDOM;
  const edge = sc.getBoundingClientRect().top;
  for (const l of view.contentDOM.querySelectorAll('.cm-line')) {
    const r = l.getBoundingClientRect();
    if (r.bottom <= edge + 0.5) continue;
    if (r.top >= edge - 0.5) break;
    // Scroll the cut line out (down) when that is the shorter way and there is room, else bring all of it in (up).
    const down = r.bottom - edge;
    const up = edge - r.top;
    const room = sc.scrollHeight - sc.clientHeight - sc.scrollTop;
    sc.scrollTop += down <= up && down <= room + 0.5 ? down : -up;
    break;
  }
}

/** Two views that scroll as one: moving either moves the other, in both directions. */
export function syncScroll(a: EditorView, b: EditorView): () => void {
  let busy = false;
  const link = (from: EditorView, to: EditorView) => () => {
    if (busy) return;
    busy = true;
    to.scrollDOM.scrollTop = from.scrollDOM.scrollTop;
    to.scrollDOM.scrollLeft = from.scrollDOM.scrollLeft;
    requestAnimationFrame(() => (busy = false));
  };
  const ab = link(a, b);
  const ba = link(b, a);
  a.scrollDOM.addEventListener('scroll', ab, { passive: true });
  b.scrollDOM.addEventListener('scroll', ba, { passive: true });
  return () => {
    a.scrollDOM.removeEventListener('scroll', ab);
    b.scrollDOM.removeEventListener('scroll', ba);
  };
}
