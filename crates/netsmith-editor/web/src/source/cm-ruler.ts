// The overview ruler: a strip at the editor's right edge that holds the whole file. It has a tick for each problem the checker found
// in it (the lane on the right) and for each run of lines the comparison with a ref changed (the lane on the left), a faint thumb
// where the window is, and a press on it scrolls there: on a tick to that line (the cursor goes to it), on the strip to that share
// of the file. A tick says `line N: message` in the page's own tooltip. A problem is a glyph (a disc, a triangle) and a change a thin bar, so
// colour is not the only difference. The strip is not for a screen reader or the keyboard: the Problems dock, F8, Next change (Alt+F5) and the
// status bar say the same, and the strip is a way to see and reach by the pointer.

import { forEachDiagnostic } from '@codemirror/lint';
import { type Extension } from '@codemirror/state';
import { EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

import { changeLines } from './cm-changes';
import { heightAt, place, rulerTicks } from './rulerMarks';

const ruler = ViewPlugin.fromClass(
  class {
    dom: HTMLDivElement;
    thumb: HTMLDivElement;
    ticks: HTMLDivElement;
    signature = '';
    frame = 0;
    constructor(readonly view: EditorView) {
      this.dom = document.createElement('div');
      this.dom.className = 'cm-ruler';
      this.dom.setAttribute('aria-hidden', 'true');
      this.dom.setAttribute('data-testid', 'ruler');
      this.thumb = document.createElement('div');
      this.thumb.className = 'cm-ruler-thumb';
      this.thumb.setAttribute('data-testid', 'ruler-thumb');
      this.ticks = document.createElement('div');
      this.ticks.className = 'cm-ruler-ticks';
      this.dom.append(this.thumb, this.ticks);
      view.dom.appendChild(this.dom);
      this.dom.addEventListener('click', this.press);
      view.scrollDOM.addEventListener('scroll', this.onScroll, { passive: true });
      this.draw();
    }
    onScroll = () => {
      if (!this.frame) this.frame = requestAnimationFrame(() => ((this.frame = 0), this.moveThumb()));
    };
    update(u: ViewUpdate) {
      if (u.docChanged || u.geometryChanged || u.viewportChanged || u.transactions.length > 0 || this.signature === '') this.draw();
    }
    marks() {
      const state = this.view.state;
      const problems: { line: number; severity: 'error' | 'warning'; message: string }[] = [];
      forEachDiagnostic(state, (d, from) => {
        if (d.severity === 'error' || d.severity === 'warning') problems.push({ line: state.doc.lineAt(Math.min(from, state.doc.length)).number, severity: d.severity, message: d.message });
      });
      return rulerTicks(state.doc.lines, problems, changeLines(state));
    }
    draw() {
      const view = this.view;
      const marks = this.marks();
      const sig = `${view.state.doc.lines}|${view.contentHeight | 0}|${view.scrollDOM.clientHeight}|${marks.map((m) => `${m.lane}${m.kind}${m.from}-${m.to}${m.label}`).join(';')}`;
      const had = sig === this.signature;
      this.signature = sig;
      // The strip is there when there is something to find in it.
      this.dom.hidden = marks.length === 0;
      // It stands beside the scrollbar, not over it: the scrollbar keeps its own presses.
      this.dom.style.right = `${view.scrollDOM.offsetWidth - view.scrollDOM.clientWidth}px`;
      if (marks.length === 0) {
        this.ticks.replaceChildren();
        return;
      }
      if (!had) {
        const track = view.scrollDOM.clientHeight;
        const whole = Math.max(view.contentHeight, track);
        this.ticks.replaceChildren(
          ...marks.map((m) => {
            const top = view.lineBlockAt(view.state.doc.line(m.from).from).top;
            const bottom = view.lineBlockAt(view.state.doc.line(m.to).from).bottom;
            const tick = document.createElement('span');
            tick.className = `cm-ruler-tick lane-${m.lane} kind-${m.kind}`;
            if (m.lane === 'problem') {
              // A glyph of a fixed size, centred on its line: the stylesheet keeps it inside the strip.
              const at = place(top, bottom, whole, track, 0);
              tick.style.setProperty('--y', `${(at.top + at.height / 2).toFixed(1)}px`);
            } else {
              // A bar as tall as its lines; the lines a deletion left are a thin line at the place they were.
              const at = place(top, m.kind === 'deleted' ? top : bottom, whole, track, m.kind === 'deleted' ? 2 : 4);
              tick.style.top = `${at.top.toFixed(1)}px`;
              tick.style.height = `${at.height.toFixed(1)}px`;
            }
            tick.setAttribute('data-line', String(m.from));
            tick.setAttribute('data-tip', m.label);
            tick.setAttribute('data-kind', m.kind);
            return tick;
          }),
        );
      }
      this.moveThumb();
    }
    moveThumb() {
      const sc = this.view.scrollDOM;
      const track = sc.clientHeight;
      const whole = Math.max(this.view.contentHeight, track);
      const at = place(sc.scrollTop, sc.scrollTop + track, whole, track, 8);
      this.thumb.style.top = `${at.top.toFixed(1)}px`;
      this.thumb.style.height = `${at.height.toFixed(1)}px`;
      // A file that fits the window has no thumb to show: it would fill the strip.
      this.thumb.hidden = whole <= track + 1;
    }
    press = (e: MouseEvent) => {
      const view = this.view;
      const tick = e.target instanceof Element ? e.target.closest<HTMLElement>('.cm-ruler-tick') : null;
      if (tick) {
        const line = view.state.doc.line(Math.min(Number(tick.getAttribute('data-line')) || 1, view.state.doc.lines));
        view.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: 'center' }), scrollIntoView: false });
        view.focus();
        return;
      }
      const rect = this.dom.getBoundingClientRect();
      const track = view.scrollDOM.clientHeight;
      const whole = Math.max(view.contentHeight, track);
      // The middle of the window goes to where the press was.
      const target = heightAt(e.clientY - rect.top, whole, track) - track / 2;
      view.scrollDOM.scrollTo({ top: Math.max(0, target) });
    };
    destroy() {
      cancelAnimationFrame(this.frame);
      this.view.scrollDOM.removeEventListener('scroll', this.onScroll);
      this.dom.remove();
    }
  },
);

export const rulerExtension: Extension = ruler;
