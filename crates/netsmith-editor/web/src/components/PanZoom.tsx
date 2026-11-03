import type { ComponentChildren } from 'preact';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';

interface View {
  k: number;
  x: number;
  y: number;
}

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
}) {
  const box = useRef<HTMLDivElement>(null);
  const [view, setView] = useState<View>({ k: 1, x: 0, y: 0 });
  const fitK = useRef(1);
  const moved = useRef(false);
  const latest = useRef(view);
  latest.current = view;
  const primary = props.primaryPans !== false;
  useEffect(() => props.onView?.(view.k), [view.k]);

  const fit = useCallback(() => {
    const el = box.current;
    if (!el || !props.width || !props.height) return;
    const k = Math.min(el.clientWidth / props.width, el.clientHeight / props.height) * 0.97;
    fitK.current = k;
    setView({
      k,
      x: (el.clientWidth - props.width * k) / 2,
      y: (el.clientHeight - props.height * k) / 2,
    });
  }, [props.width, props.height]);

  useLayoutEffect(fit, [props.fitKey, fit]);

  useLayoutEffect(() => {
    const f = props.focus;
    const el = box.current;
    if (!f || !el) return;
    const k = Math.min(el.clientWidth / f.w, el.clientHeight / f.h) * 0.94;
    moved.current = true;
    setView({ k, x: el.clientWidth / 2 - (f.x + f.w / 2) * k, y: el.clientHeight / 2 - (f.y + f.h / 2) * k });
  }, [props.focus?.seq]);

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
    setView({ k, x: px - (px - v.x) * f, y: py - (py - v.y) * f });
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
      onPointerDown={(e) => {
        const pans = primary ? e.button === 0 : e.button === 1 || e.button === 2;
        if (!pans || (e.target as Element).closest('.no-pan')) return;
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
          setView({ ...d.view, x: d.view.x + dx, y: d.view.y + dy });
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
        if (!(e.target as Element).closest('.hit-target')) {
          moved.current = false;
          fit();
        }
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
        <button
          title="Fit to the window"
          onClick={() => {
            moved.current = false;
            fit();
          }}
        >
          Fit
        </button>
      </div>
    </div>
  );
}
