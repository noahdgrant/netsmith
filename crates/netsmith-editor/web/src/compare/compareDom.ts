// The one subscriber of the comparison's marks, like hoverDom.ts for the hover: when the switch is on
// and a comparison is ready it finds what stands for each changed part, net, rail, module and board in
// the page (the schematic's `data-ref` and `data-net`, the cards' `data-hv`, the board's `data-path`,
// the system's boards) and gives it a class (`cmp-added`, `cmp-changed`, `cmp-renumbered`, `cmp-net-*`),
// and takes the classes off what lost them. No view re-renders and no drawing is replaced; a drawing that
// is replaced meanwhile is marked again. A rail's badge is an attribute (`data-cmp-delta`) that CSS draws.
// A view that is rendered again sets the `class` of its own elements (a card that is picked) and takes the marks
// off; the subscriber sees that (the page's `class` attributes) and puts them back, writing only the difference
// (domMarks.ts), so it never answers its own writes and never takes another subscriber's classes (`hover-lit`).

import { effect } from '@preact/signals';

import { reconcile, want, type WantedMap } from '../domMarks';
import { partByPath } from '../selection';
import { board, detail } from '../store';
import { boardMarks, marksOf, markTargets, otherBoardTargets } from './model';
import { diskCompare, marks, showChanges } from './store';

export const ROOT_CLASS = 'cmp-on';

const q = (v: string) => `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;

/** Install the subscriber; the returned function removes it (and every mark). */
export function installCompareDom(root: Document = document): () => void {
  let marked: WantedMap<Element> = new Map();
  let frame = 0;
  let redraws: MutationObserver | null = null;

  const apply = () => {
    frame = 0;
    const next: WantedMap<Element> = new Map();
    const put = (selector: string, cls: string, delta?: string) => {
      let found: NodeListOf<Element>;
      try {
        found = root.querySelectorAll(selector);
      } catch {
        return;
      }
      found.forEach((el) => want(next, el, cls.split(' '), delta ? ['data-cmp-delta', delta] : undefined));
    };
    collect(put);
    reconcile(marked, next);
    marked = next;
  };
  const collect = (put: (selector: string, cls: string, delta?: string) => void) => {
    const m = marks.value;
    const c = diskCompare.value;
    root.documentElement.classList.toggle(ROOT_CLASS, showChanges.value && !!c);
    if (!showChanges.value || !c) return;
    const short = (name: string) => name.slice(name.lastIndexOf('/') + 1);
    const here = board.value;
    const d = detail.value;
    if (m) {
      // The board in front: parts, nets, modules and rails; a system's rail cards are told apart by their board.
      const named = c.boards.length > 1 && here ? short(here) : undefined;
      for (const t of markTargets(m, (path) => partByPath(d, path)?.refdes, named)) put(t.selector, t.cls, t.delta);
    }
    // The power tree draws the rails of every board of a system: the other boards' rails are marked too.
    for (const b of c.boards) if (b.name !== here) for (const t of otherBoardTargets(marksOf(b), short(b.name))) put(t.selector, t.cls, t.delta);
    for (const [name, status] of boardMarks(c)) put(`[data-cmp-board=${q(name)}]`, `cmp-board-${status}`);
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(apply);
  };
  const stop = effect(() => {
    void marks.value;
    void board.value;
    void diskCompare.value;
    void showChanges.value;
    void detail.value;
    schedule();
    const on = showChanges.value && !!diskCompare.value;
    if (on && !redraws && typeof MutationObserver === 'function') {
      // What is added or removed, and the `class` of what is drawn again; a write that changes nothing is not made (reconcile).
      redraws = new MutationObserver(schedule);
      redraws.observe(root.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] });
    } else if (!on && redraws) {
      redraws.disconnect();
      redraws = null;
    }
  });
  return () => {
    stop();
    redraws?.disconnect();
    if (frame) cancelAnimationFrame(frame);
    reconcile(marked, new Map());
    marked = new Map();
    root.documentElement.classList.remove(ROOT_CLASS);
  };
}
