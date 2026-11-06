// The one subscriber of the comparison's marks, like hoverDom.ts for the hover: when the switch is on
// and a comparison is ready it finds what stands for each changed part, net, rail, module and board in
// the page (the schematic's `data-ref` and `data-net`, the cards' `data-hv`, the board's `data-path`,
// the system's boards) and gives it a class (`cmp-added`, `cmp-changed`, `cmp-renumbered`, `cmp-net-*`),
// and takes the classes off what lost them. No view re-renders and no drawing is replaced; a drawing that
// is replaced meanwhile is marked again. A rail's badge is an attribute (`data-cmp-delta`) that CSS draws.

import { effect } from '@preact/signals';

import { partByPath } from '../selection';
import { board, detail } from '../store';
import { boardMarks, marksOf, markTargets, otherBoardTargets } from './model';
import { compare, marks, showChanges } from './store';

export const ROOT_CLASS = 'cmp-on';

const q = (v: string) => `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;

/** Install the subscriber; the returned function removes it (and every mark). */
export function installCompareDom(root: Document = document): () => void {
  let marked: { el: Element; classes: string[]; delta: boolean }[] = [];
  let frame = 0;
  let redraws: MutationObserver | null = null;

  const clear = () => {
    for (const m of marked) {
      m.el.classList.remove(...m.classes);
      if (m.delta) m.el.removeAttribute('data-cmp-delta');
    }
    marked = [];
  };
  const put = (selector: string, cls: string, delta?: string) => {
    const classes = cls.split(' ');
    let found: NodeListOf<Element>;
    try {
      found = root.querySelectorAll(selector);
    } catch {
      return;
    }
    found.forEach((el) => {
      el.classList.add(...classes);
      if (delta) el.setAttribute('data-cmp-delta', delta);
      marked.push({ el, classes, delta: !!delta });
    });
  };
  const apply = () => {
    frame = 0;
    clear();
    const m = marks.value;
    const c = compare.value;
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
    void compare.value;
    void showChanges.value;
    void detail.value;
    schedule();
    const on = showChanges.value && !!compare.value;
    if (on && !redraws && typeof MutationObserver === 'function') {
      // Only what is added or removed: our own classes are attribute changes and do not come back here.
      redraws = new MutationObserver(schedule);
      redraws.observe(root.body, { childList: true, subtree: true });
    } else if (!on && redraws) {
      redraws.disconnect();
      redraws = null;
    }
  });
  return () => {
    stop();
    redraws?.disconnect();
    if (frame) cancelAnimationFrame(frame);
    clear();
    root.documentElement.classList.remove(ROOT_CLASS);
  };
}
