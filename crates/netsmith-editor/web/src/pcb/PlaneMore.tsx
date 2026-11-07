// The chip `+N` of the PCB view: N planes whose label found no room at this zoom (planeLabels.ts hides it rather than lay it over a part).
// It is a button pinned to the corner of the canvas, so it is the same size at every zoom; it opens a list of the planes (swatch in the
// colour of the layer, then the label's own words). Pointing at or focusing a plane of the list outlines it on the board; choosing one
// zooms the board to it, which makes the room its label needed. The keys are the menu's: arrows, Home, End, Enter, Escape (back to the chip).

import { useEffect, useRef, useState } from 'preact/hooks';

import { Icon } from '../components/Icons';
import { type HiddenPlane, moreChip } from './planeLabels';

export function PlaneMore(props: {
  planes: HiddenPlane[];
  /** Zoom the board to this plane. */
  onZoom: (plane: HiddenPlane) => void;
  /** The plane the pointer or the focus is on in the list, or null. */
  onPeek: (plane: HiddenPlane | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const chip = useRef<HTMLButtonElement>(null);
  const info = moreChip(props.planes);
  const items = () => [...(root.current?.querySelectorAll<HTMLElement>('.pm-item') ?? [])];

  useEffect(() => {
    if (!open) return;
    items()[0]?.focus();
    const away = (e: PointerEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, [open]);
  // Nothing hidden any more (the view was zoomed in): the list goes with the chip, and the board's outline with it.
  useEffect(() => {
    if (!info) {
      setOpen(false);
      props.onPeek(null);
    }
  }, [!info]);
  if (!info) return null;

  const close = (back: boolean) => {
    setOpen(false);
    props.onPeek(null);
    if (back) chip.current?.focus();
  };
  return (
    <div class="plane-more no-pan" ref={root} data-testid="plane-more-root" data-toast-clear>
      <button
        type="button"
        ref={chip}
        class={`plane-more-chip ${open ? 'on' : ''}`}
        data-testid="plane-more"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls="plane-more-list"
        aria-label={info.label}
        title={info.title}
        onClick={() => (open ? close(false) : setOpen(true))}
        onKeyDown={(e) => {
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            // the canvas moves the picked parts with the arrows: not here
            e.stopPropagation();
            e.preventDefault();
            setOpen(true);
          }
        }}
      >
        <Icon name="layers" size={13} />
        <span class="pm-n">{info.text}</span>
      </button>
      {open && (
        <div
          id="plane-more-list"
          class="plane-more-list"
          role="menu"
          aria-label="Planes with no room for a label at this zoom"
          data-testid="plane-more-list"
          onKeyDown={(e) => {
            const all = items();
            const at = all.indexOf(document.activeElement as HTMLElement);
            let to: number | null = null;
            if (e.key === 'ArrowDown') to = (at + 1) % all.length;
            else if (e.key === 'ArrowUp') to = (at - 1 + all.length) % all.length;
            else if (e.key === 'Home') to = 0;
            else if (e.key === 'End') to = all.length - 1;
            else if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              close(true);
              return;
            } else if (e.key === 'Tab') {
              close(false);
              return;
            }
            // every key of a list the canvas also reads is the list's
            if (e.key.startsWith('Arrow') || e.key === 'Home' || e.key === 'End') e.stopPropagation();
            if (to !== null) {
              e.preventDefault();
              all[to]?.focus();
            }
          }}
        >
          <div class="pm-head">No room for a label at this zoom</div>
          {props.planes.map((p) => (
            <button
              key={`${p.net}|${p.layer ?? ''}`}
              type="button"
              role="menuitem"
              class="pm-item"
              data-testid="plane-more-item"
              data-net={p.net}
              data-layer={p.layer ?? undefined}
              onPointerEnter={() => props.onPeek(p)}
              onPointerLeave={() => props.onPeek(null)}
              onFocus={() => props.onPeek(p)}
              onBlur={() => props.onPeek(null)}
              onClick={() => {
                close(false);
                props.onZoom(p);
              }}
            >
              <i class="pm-sw" data-plane={p.tone} aria-hidden="true" />
              <span class="pm-name">{p.name}</span>
              <span class="pm-act">Zoom to it</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
