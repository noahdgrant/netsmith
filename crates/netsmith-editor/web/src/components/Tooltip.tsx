// One tooltip for the whole page, in the page's own style. An element with `data-tip="text"`
// (and, optionally, `data-tip-keys="Ctrl+K"`) shows it after a short pause under the pointer or
// on keyboard focus; it hides at once on a press, a scroll or Escape. Elements that recipes
// find by their `title` keep it: the browser's tip is the fallback there, never both on one element.

import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';

interface Tip {
  text: string;
  keys: string[];
  x: number;
  y: number;
  above: boolean;
}

const SHOW_AFTER_MS = 380;
const GAP = 7;

export function TooltipLayer() {
  const [tip, setTip] = useState<Tip | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const owner = useRef<Element | null>(null);

  useEffect(() => {
    const hide = () => {
      window.clearTimeout(timer.current);
      owner.current = null;
      setTip(null);
    };
    const show = (el: Element, now: boolean) => {
      window.clearTimeout(timer.current);
      const open = () => {
        if (!el.isConnected) return;
        const r = el.getBoundingClientRect();
        const text = el.getAttribute('data-tip') ?? '';
        if (!text) return;
        const keys = (el.getAttribute('data-tip-keys') ?? '').split('+').filter(Boolean);
        const above = r.bottom + 44 > window.innerHeight;
        setTip({ text, keys, x: r.left + r.width / 2, y: above ? r.top - GAP : r.bottom + GAP, above });
      };
      owner.current = el;
      if (now) open();
      else timer.current = window.setTimeout(open, SHOW_AFTER_MS);
    };
    const target = (e: Event) => (e.target instanceof Element ? e.target.closest('[data-tip]') : null);
    const over = (e: PointerEvent) => {
      if (e.pointerType === 'touch') return;
      const el = target(e);
      if (!el) return;
      if (el === owner.current) return;
      show(el, !!document.querySelector('.tip.on'));
    };
    const out = (e: PointerEvent) => {
      const el = target(e);
      if (el && el === owner.current && !(e.relatedTarget instanceof Node && el.contains(e.relatedTarget))) hide();
    };
    const focus = (e: FocusEvent) => {
      const el = target(e);
      if (el && (el as HTMLElement).matches(':focus-visible')) show(el, true);
    };
    const blur = (e: FocusEvent) => {
      if (target(e) === owner.current) hide();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') hide();
    };
    document.addEventListener('pointerover', over);
    document.addEventListener('pointerout', out);
    document.addEventListener('focusin', focus);
    document.addEventListener('focusout', blur);
    document.addEventListener('pointerdown', hide, true);
    document.addEventListener('scroll', hide, true);
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('pointerover', over);
      document.removeEventListener('pointerout', out);
      document.removeEventListener('focusin', focus);
      document.removeEventListener('focusout', blur);
      document.removeEventListener('pointerdown', hide, true);
      document.removeEventListener('scroll', hide, true);
      document.removeEventListener('keydown', key, true);
      window.clearTimeout(timer.current);
    };
  }, []);

  // Keep the tip inside the window: it is centred on its element, then pushed back in. It is measured at the window's left edge (its width is
  // then its own, not what is left of the window to the right of its element) and moved before it is painted.
  useLayoutEffect(() => {
    const el = box.current;
    if (!tip || !el) return;
    const w = el.offsetWidth;
    const left = Math.max(8, Math.min(tip.x - w / 2, window.innerWidth - w - 8));
    el.style.left = `${left}px`;
  }, [tip]);

  if (!tip) return null;
  return (
    <div
      ref={box}
      class={`tip on ${tip.above ? 'above' : 'below'}`}
      role="tooltip"
      style={{ top: tip.y, left: 0, transform: tip.above ? 'translateY(-100%)' : undefined }}
    >
      <span class="tip-text">{tip.text}</span>
      {tip.keys.length > 0 && (
        <span class="tip-keys">
          {tip.keys.map((k) => (
            <kbd key={k}>{k}</kbd>
          ))}
        </span>
      )}
    </div>
  );
}
