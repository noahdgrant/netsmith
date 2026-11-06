// The problem marks in the gutter: the page's own severity icons, one for each line that has an
// error or a warning, the error winning. A note is not drawn on the source (the problems panel lists it).

import { type Diagnostic, forEachDiagnostic, setDiagnosticsEffect } from '@codemirror/lint';
import type { Extension } from '@codemirror/state';
import { EditorView, GutterMarker, gutter } from '@codemirror/view';

import { iconDom } from './dom';

class Mark extends GutterMarker {
  constructor(
    readonly severity: 'error' | 'warning',
    readonly message: string,
  ) {
    super();
  }
  eq(other: Mark) {
    return other.severity === this.severity && other.message === this.message;
  }
  toDOM() {
    const el = iconDom(this.severity, 14, `cm-sev ${this.severity}`);
    el.title = this.message;
    el.setAttribute('data-testid', `gutter-${this.severity}`);
    return el;
  }
}

/** The mark of a line: its worst problem. */
export function worst(list: Pick<Diagnostic, 'severity' | 'message'>[]): { severity: 'error' | 'warning'; message: string } | null {
  const e = list.find((d) => d.severity === 'error');
  const w = list.find((d) => d.severity === 'warning');
  const pick = e ?? w;
  return pick ? { severity: pick.severity === 'error' ? 'error' : 'warning', message: list.length > 1 ? `${pick.message.split('\n')[0]} (and ${list.length - 1} more on this line)` : pick.message.split('\n')[0] } : null;
}

export const severityGutter: Extension = gutter({
  class: 'cm-sev-gutter',
  lineMarker(view: EditorView, line) {
    const here: Diagnostic[] = [];
    forEachDiagnostic(view.state, (d, from, to) => {
      if (from <= line.to && to >= line.from && view.state.doc.lineAt(from).from === line.from) here.push(d);
    });
    const w = worst(here);
    return w ? new Mark(w.severity, w.message) : null;
  },
  lineMarkerChange: (u) => u.transactions.some((tr) => tr.effects.some((e) => e.is(setDiagnosticsEffect))),
  initialSpacer: () => new Mark('error', ''),
});
