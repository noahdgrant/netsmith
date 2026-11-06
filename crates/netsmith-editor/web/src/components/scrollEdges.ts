// Where a scrolling box has more to show: which of its edges has content past it. A view uses it to fade that edge,
// as the file tabs do, so a clipped column or card is never clipped without a cue. The box is the one with
// `overflow: auto`; `changed` is what makes it measure again (the rows it holds, a filter).

import { useEffect, useRef, useState } from 'preact/hooks';

export interface Edges {
  l: boolean;
  r: boolean;
  t: boolean;
  b: boolean;
}

/** The edges of a box that have more beyond them (a couple of pixels of slack for rounding). */
export function edgesOf(el: Pick<HTMLElement, 'scrollLeft' | 'scrollTop' | 'clientWidth' | 'clientHeight' | 'scrollWidth' | 'scrollHeight'>): Edges {
  return {
    l: el.scrollLeft > 2,
    r: el.scrollLeft + el.clientWidth < el.scrollWidth - 2,
    t: el.scrollTop > 2,
    b: el.scrollTop + el.clientHeight < el.scrollHeight - 2,
  };
}

/** A ref for the scrolling box, its edges, and the handler for its scroll. */
export function useScrollEdges<T extends HTMLElement>(changed?: unknown) {
  const ref = useRef<T>(null);
  const [edges, setEdges] = useState<Edges>({ l: false, r: false, t: false, b: false });
  const measure = () => {
    const el = ref.current;
    if (!el) return;
    const next = edgesOf(el);
    setEdges((was) => (was.l === next.l && was.r === next.r && was.t === next.t && was.b === next.b ? was : next));
  };
  useEffect(measure, [changed]);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver !== 'function') return;
    const o = new ResizeObserver(measure);
    o.observe(el);
    if (el.firstElementChild) o.observe(el.firstElementChild);
    return () => o.disconnect();
  }, []);
  return { ref, edges, onScroll: measure };
}
