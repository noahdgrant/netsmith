// The one subscriber of the shared hover (hover.ts): when the hover changes it finds everything in the page
// that stands for the same thing and gives it the class `hover-lit`, and takes the class off what was lit
// before. No view re-renders: the schematic's and the board's drawings are not touched but for a class, and
// a list (the bill of materials, an inspector, the tree) keeps its own state. A drawing that is replaced while
// something is lit (a rebuild) is lit again, and so is a card whose view was rendered again and set its own `class`
// (only the difference is written: domMarks.ts). Escape and the window losing focus put the light out.

import { effect } from '@preact/signals';

import { reconcile, want, type WantedMap } from './domMarks';
import { clearHover, hover, modulesLit, resolveHover, selectorOf } from './hover';
import { detail } from './store';

export const LIT = 'hover-lit';

/** Install the subscriber; the returned function removes it. */
export function installHoverDom(root: Document = document): () => void {
  let lit: WantedMap<Element> = new Map();
  let frame = 0;
  let redraws: MutationObserver | null = null;

  const apply = () => {
    frame = 0;
    const next: WantedMap<Element> = new Map();
    const h = hover.value;
    root.documentElement.classList.toggle('has-hover', !!h);
    if (h) {
      const stands = resolveHover(detail.value, h);
      const keys = new Set<string>();
      root.querySelectorAll('[data-hv^="module:"]').forEach((e) => keys.add(e.getAttribute('data-hv')!.slice(7)));
      const selector = selectorOf(stands, modulesLit([...keys], stands.at));
      if (selector) root.querySelectorAll(selector).forEach((e) => want(next, e, [LIT]));
    }
    reconcile(lit, next);
    lit = next;
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(apply);
  };

  const stop = effect(() => {
    // Read both, so a board that is read while something is lit lights it.
    void hover.value;
    void detail.value;
    schedule();
    if (hover.value && !redraws && typeof MutationObserver === 'function') {
      redraws = new MutationObserver(schedule);
      redraws.observe(root.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] });
    } else if (!hover.value && redraws) {
      redraws.disconnect();
      redraws = null;
    }
  });
  const key = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && hover.peek()) clearHover();
  };
  const away = () => clearHover();
  root.addEventListener('keydown', key, true);
  window.addEventListener('blur', away);
  return () => {
    stop();
    redraws?.disconnect();
    if (frame) cancelAnimationFrame(frame);
    root.removeEventListener('keydown', key, true);
    window.removeEventListener('blur', away);
    reconcile(lit, new Map());
    lit = new Map();
  };
}
