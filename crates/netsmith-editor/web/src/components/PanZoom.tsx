import type { ComponentChildren } from 'preact';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';

import { ViewMemo, type ViewState as View, fitView } from '../sheets';

/** The view each canvas had when it was left, for the session, by `memoKey`. */
export const viewMemo = new ViewMemo();

const MIN = 0.05;
const MAX = 80;

/** A canvas of `width` by `height` pixels that is dragged to pan and scrolled to zoom. It
 * fits itself to its box when `fitKey` changes. */
export function PanZoom(props: {
  width: number;
  height: number;
  fitKey: string;
  /** A box of the canvas (in its pixels) to zoom to when `seq` changes. */
  focus?: { x: number; y: number; w: number; h: number; seq: number };
  children: ComponentChildren;
  class?: string;
  /** Whether a drag with the main button pans (the default). Where it is `false` the main
   * button belongs to the children (selecting, moving), and the middle or right button pans. */
  primaryPans?: boolean;
  /** Told the zoom (canvas pixels to screen pixels) whenever it changes. */
  onView?: (k: number) => void;
  /** Told the whole view whenever it or the box changes: the zoom, where the canvas is, and the
   * box's size in screen pixels. A minimap draws from it. */
  onViewport?: (v: { k: number; x: number; y: number; w: number; h: number }) => void;
  /** A box of the canvas (its pixels) to bring into view when `seq` changes: the view moves no
   * more than it must, and the zoom stays. */
  reveal?: { x: number; y: number; w: number; h: number; seq: number };
  /** A point of the canvas (its pixels) to put in the middle of the box when `seq` changes. */
  centre?: { x: number; y: number; seq: number };
  /** The part of the canvas that "fit" shows (canvas pixels); the whole canvas when absent. */
  fitBox?: { x: number; y: number; w: number; h: number };
  /** Where the view is remembered for the session: coming back to this key shows the zoom and
   * place it was left at, until Fit. */
  memoKey?: string;
  /** Whether the keys F and 0 (fit), + and - (zoom) work while the canvas has the focus. */
  keys?: boolean;
  /** Whether the zoom bar has a button for actual size (one canvas pixel to one screen pixel). */
  actualSize?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [view, setView] = useState<View>({ k: 1, x: 0, y: 0 });
  const fitK = useRef(1);
  const moved = useRef(false);
  const latest = useRef(view);
  latest.current = view;
  const primary = props.primaryPans !== false;
  useEffect(() => props.onView?.(view.k), [view.k]);
  useEffect(() => {
    const el = box.current;
    if (el) props.onViewport?.({ k: view.k, x: view.x, y: view.y, w: el.clientWidth, h: el.clientHeight });
  }, [view.k, view.x, view.y, box.current?.clientWidth, box.current?.clientHeight]);

  const fb = props.fitBox;
  const fitBoxOf = () => fb ?? { x: 0, y: 0, w: props.width, h: props.height };
  const memoKey = props.memoKey;
  const memoRef = useRef(memoKey);
  memoRef.current = memoKey;

  /** Set the view; one the person made (not a fit) is remembered under `memoKey`. */
  const put = (v: View, remember = true) => {
    setView(v);
    const key = memoRef.current;
    if (key) {
      if (remember) viewMemo.set(key, v);
      else viewMemo.drop(key);
    }
  };

  const fit = useCallback(() => {
    const el = box.current;
    const b = fitBoxOf();
    if (!el || !b.w || !b.h || !el.clientWidth || !el.clientHeight) return;
    const v = fitView(el.clientWidth, el.clientHeight, b);
    fitK.current = v.k;
    put(v, false);
  }, [props.width, props.height, fb?.x, fb?.y, fb?.w, fb?.h]);

  useLayoutEffect(() => {
    const el = box.current;
    const kept = memoKey ? viewMemo.get(memoKey) : undefined;
    if (kept && el && el.clientWidth) {
      const b = fitBoxOf();
      fitK.current = fitView(el.clientWidth, el.clientHeight, b).k;
      moved.current = true;
      setView(kept);
    } else {
      moved.current = false;
      fit();
    }
  }, [props.fitKey, memoKey, fit]);

  useLayoutEffect(() => {
    const f = props.focus;
    const el = box.current;
    if (!f || !el) return;
    const k = Math.min(el.clientWidth / f.w, el.clientHeight / f.h) * 0.94;
    moved.current = true;
    put({ k, x: el.clientWidth / 2 - (f.x + f.w / 2) * k, y: el.clientHeight / 2 - (f.y + f.h / 2) * k });
  }, [props.focus?.seq]);

  useLayoutEffect(() => {
    const r = props.reveal;
    const el = box.current;
    if (!r || !el) return;
    const v = latest.current;
    const margin = 24;
    const shift = (from: number, size: number, at: number, room: number) => {
      const lo = from * v.k + at;
      const hi = (from + size) * v.k + at;
      if (hi - lo + 2 * margin > room) return room / 2 - ((from + size / 2) * v.k + at);
      if (lo < margin) return margin - lo;
      if (hi > room - margin) return room - margin - hi;
      return 0;
    };
    const dx = shift(r.x, r.w, v.x, el.clientWidth);
    const dy = shift(r.y, r.h, v.y, el.clientHeight);
    if (dx === 0 && dy === 0) return;
    moved.current = true;
    put({ ...v, x: v.x + dx, y: v.y + dy });
  }, [props.reveal?.seq]);

  useLayoutEffect(() => {
    const c = props.centre;
    const el = box.current;
    if (!c || !el) return;
    const v = latest.current;
    moved.current = true;
    put({ ...v, x: el.clientWidth / 2 - c.x * v.k, y: el.clientHeight / 2 - c.y * v.k });
  }, [props.centre?.seq]);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      if (!moved.current) fit();
    });
    observer.observe(el);
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      const unit = e.deltaMode === 1 ? 16 : 1;
      zoomAt(Math.exp(-e.deltaY * unit * (e.ctrlKey ? 0.01 : 0.0016)), e.clientX - r.left, e.clientY - r.top);
    };
    el.addEventListener('wheel', wheel, { passive: false });
    return () => {
      observer.disconnect();
      el.removeEventListener('wheel', wheel);
    };
  }, [fit]);

  function zoomAt(factor: number, px: number, py: number) {
    const v = latest.current;
    const k = Math.min(Math.max(v.k * factor, fitK.current * MIN), fitK.current * MAX);
    const f = k / v.k;
    moved.current = true;
    put({ k, x: px - (px - v.x) * f, y: py - (py - v.y) * f });
  }

  function refit() {
    moved.current = false;
    fit();
  }

  function zoomCentre(factor: number) {
    const el = box.current!;
    zoomAt(factor, el.clientWidth / 2, el.clientHeight / 2);
  }

  const drag = useRef<{ x: number; y: number; id: number; view: View; active: boolean } | null>(null);

  return (
    <div
      class={`panzoom ${props.class ?? ''}`}
      ref={box}
      tabIndex={-1}
      onKeyDown={(e) => {
        if (props.keys === false || e.ctrlKey || e.metaKey || e.altKey) return;
        if (e.key === 'f' || e.key === 'F' || e.key === '0') {
          e.preventDefault();
          refit();
        } else if (e.key === '+' || e.key === '=') {
          e.preventDefault();
          zoomCentre(1.25);
        } else if (e.key === '-' || e.key === '_') {
          e.preventDefault();
          zoomCentre(0.8);
        }
      }}
      onPointerDown={(e) => {
        const pans = primary ? e.button === 0 : e.button === 1 || e.button === 2;
        if ((e.target as Element).closest('.no-pan')) return;
        // The canvas takes the focus, so its keys work; the hit targets keep their own clicks.
        box.current?.focus({ preventScroll: true });
        if (!pans) return;
        drag.current = { x: e.clientX, y: e.clientY, id: e.pointerId, view: latest.current, active: false };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || d.id !== e.pointerId) return;
        const dx = e.clientX - d.x;
        const dy = e.clientY - d.y;
        if (!d.active && Math.hypot(dx, dy) > 4) {
          d.active = true;
          box.current!.setPointerCapture(e.pointerId);
        }
        if (d.active) {
          moved.current = true;
          put({ ...d.view, x: d.view.x + dx, y: d.view.y + dy });
        }
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        if (d?.active) {
          // The click that ends a drag is not a click on what is under it.
          const stop = (ev: Event) => ev.stopPropagation();
          window.addEventListener('click', stop, { capture: true, once: true });
          setTimeout(() => window.removeEventListener('click', stop, { capture: true }), 0);
          box.current!.releasePointerCapture(e.pointerId);
        }
      }}
      onContextMenu={(e) => {
        if (!primary) e.preventDefault();
      }}
      onDblClick={(e) => {
        if (!(e.target as Element).closest('.hit-target')) refit();
      }}
    >
      <div
        class="canvas"
        style={{
          width: props.width,
          height: props.height,
          transform: `translate(${view.x}px, ${view.y}px) scale(${view.k})`,
        }}
      >
        {props.children}
      </div>
      <div class="zoombar no-pan">
        <button title="Zoom out" onClick={() => zoomCentre(0.8)}>
          −
        </button>
        <span>{Math.round((view.k / (fitK.current || 1)) * 100)}%</span>
        <button title="Zoom in" onClick={() => zoomCentre(1.25)}>
          +
        </button>
        {props.actualSize && (
          <button title="Actual size: one pixel of the drawing to one pixel of the screen" onClick={() => zoomCentre(1 / latest.current.k)}>
            1:1
          </button>
        )}
        <button title="Fit to the window (F)" onClick={refit}>
          Fit
        </button>
      </div>
    </div>
  );
}
