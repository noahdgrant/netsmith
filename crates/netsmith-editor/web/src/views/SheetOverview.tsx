// "All modules": a grid of cards in the order the drawing sets the sheets out (row by row, so the
// modules most tied by nets are side by side, `setOut`), each with its title, its number in the
// source (the order of the keys and the navigator), its counts and a drawing of the sheet. A drawing is a copy of the sheet's group as it is in the page's SVG (a `<use>` would
// not take the style rules of the drawing in every browser), taken again when the selection changes
// so the highlight shows. A click on a card opens the sheet.
//
// A card can be dragged onto another (ADR 0139, `schematic/sheetplace.ts`): a press that moves 4 pixels is a drag, the card follows
// the pointer and the card under it is marked, and the drop asks for the dragged sheet to take that card's place on the page. A
// press that does not move is a click. Esc cancels. `Backspace` or `Delete` on a placed card puts it back to the arrangement.

import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';

import type { Drawing } from '../schematic/load';
import { setOut } from '../schematic/setout';
import { SCALE } from '../schematic/zoom';
import { plural } from '../util';
import type { SheetRow } from './SheetNav';

/** A card is two columns wide when its sheet is more than this much wider than it is tall. */
const WIDE = 2.4;

/** How roomy the cards are for `n` modules: `few` (up to 4) fills the window, `some` (up to 9) is in between, `many` is the dense grid. */
export const overviewSize = (n: number): 'few' | 'some' | 'many' => (n <= 4 ? 'few' : n <= 9 ? 'some' : 'many');

/** How far, in screen pixels, a press must move to be a drag and not a click. */
const DRAG_PX = 4;

/** What the overview may do with the layout file; absent when it cannot be written (no file read yet, or a fault in it). */
export interface Placing {
  /** Whether the card's sheet can be dragged (a sheet cut into numbered boxes cannot). */
  can: (sheet: string) => boolean;
  /** Whether the file gives the card's sheet a place. */
  placed: (sheet: string) => boolean;
  /** The card `from` was dropped on the card `to`. */
  drop: (from: string, to: string) => void;
  /** The card's sheet goes back to the arrangement. */
  clear: (sheet: string) => void;
}

interface Dragging {
  from: string;
  x0: number;
  y0: number;
  dx: number;
  dy: number;
  moved: boolean;
  over: string | null;
}

/** The card under a point of the screen, by its sheet's name, other than the dragged one (which is under the pointer too). */
function cardAt(x: number, y: number, from: string): string | null {
  for (const el of document.elementsFromPoint(x, y)) {
    const name = el.closest('[data-sheet-card]')?.getAttribute('data-sheet-card');
    if (name && name !== from) return name;
  }
  return null;
}

/** A thumbnail: the group of a sheet, copied from the page's drawing. */
function Thumb({ source, group, view, lit, stamp }: { source: () => Element | null; group: string | undefined; view: string; lit: boolean; stamp: number }) {
  const svg = useRef<SVGSVGElement>(null);
  useLayoutEffect(() => {
    const el = svg.current;
    if (!el) return;
    const from = group ? source()?.querySelector(`#${group}`) : null;
    // The copy is a drawing of its own, so its ids are not the page's.
    el.innerHTML = from ? from.outerHTML.replace(/ id="nsm-sheet-\d+"/, '') : '';
  }, [group, stamp]);
  return <svg ref={svg} class={`schematic ${lit ? 'has-on' : ''}`} viewBox={view} preserveAspectRatio="xMidYMid meet" aria-hidden="true" />;
}

export function SheetOverview({
  drawing,
  rows,
  open,
  source,
  lit,
  stamp,
  placing,
}: {
  drawing: Drawing;
  rows: SheetRow[];
  open: (sheet: string) => void;
  /** The element that holds the page's drawing. */
  source: () => Element | null;
  /** Whether something is selected, so the rest is dimmed. */
  lit: boolean;
  /** Goes up when the page's drawing was painted again. */
  stamp: number;
  placing?: Placing;
}) {
  // A few modules are given the room: two columns of large cards (three for a handful), where many stay as dense as they were.
  const size = overviewSize(rows.length);
  const [drag, setDrag] = useState<Dragging | null>(null);
  const live = useRef<Dragging | null>(null);
  /** Set by a drop, so the click the browser sends after it does not open the sheet. */
  const swallow = useRef(false);
  const update = (d: Dragging | null) => {
    live.current = d;
    setDrag(d);
  };
  useEffect(() => {
    if (!drag) return;
    const esc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      swallow.current = live.current?.moved ?? false;
      update(null);
    };
    window.addEventListener('keydown', esc, true);
    return () => window.removeEventListener('keydown', esc, true);
  }, [!!drag]);
  const down = (name: string) => (e: PointerEvent) => {
    if (e.button !== 0 || !placing?.can(name)) return;
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
    update({ from: name, x0: e.clientX, y0: e.clientY, dx: 0, dy: 0, moved: false, over: null });
  };
  const move = (e: PointerEvent) => {
    const d = live.current;
    if (!d) return;
    const dx = e.clientX - d.x0;
    const dy = e.clientY - d.y0;
    const moved = d.moved || Math.hypot(dx, dy) >= DRAG_PX;
    if (!moved) return;
    update({ ...d, dx, dy, moved, over: cardAt(e.clientX, e.clientY, d.from) });
  };
  const up = (e: PointerEvent) => {
    const d = live.current;
    if (!d) return;
    update(null);
    if (!d.moved) return;
    swallow.current = true;
    const to = cardAt(e.clientX, e.clientY, d.from);
    if (to) placing?.drop(d.from, to);
  };
  return (
    <div class={`sheet-overview schematic-host no-pan ov-${size}`} role="list" aria-label="All modules">
      {setOut(rows, (r) => r.sheet).map((r) => {
        const s = r.sheet;
        const pad = 1.5;
        const x = s.x / SCALE + drawing.mm.x - pad;
        const y = s.y / SCALE + drawing.mm.y - pad;
        const w = s.w / SCALE + 2 * pad;
        const h = s.h / SCALE + 2 * pad;
        const group = drawing.groupId.get(s.name);
        const aspect = w / h;
        const stats = r.stats;
        const dragged = drag?.moved && drag.from === s.name;
        const target = drag?.moved && drag.over === s.name && drag.from !== s.name;
        const placed = placing?.placed(s.name) ?? false;
        const movable = placing?.can(s.name) ?? false;
        return (
          <div
            role="listitem"
            key={s.name}
            data-sheet-card={s.name}
            class={`ov-item ${aspect > WIDE ? 'wide' : ''} ${aspect > 4.2 ? 'wider' : ''} ${dragged ? 'ov-dragging' : ''} ${target ? 'ov-target' : ''}`}
            style={dragged ? { transform: `translate(${drag.dx}px, ${drag.dy}px)` } : undefined}
          >
            <button
              class={`ov-card ${r.carries ? 'carries' : ''} ${movable ? 'movable' : ''}`}
              onClick={() => {
                if (swallow.current) {
                  swallow.current = false;
                  return;
                }
                open(s.name);
              }}
              onPointerDown={down(s.name)}
              onPointerMove={move}
              onPointerUp={up}
              onPointerCancel={() => update(null)}
              onKeyDown={(e) => {
                if ((e.key === 'Backspace' || e.key === 'Delete') && placed) {
                  e.preventDefault();
                  e.stopPropagation();
                  placing?.clear(s.name);
                }
              }}
              title={`Open ${s.title}${movable ? (placed ? ' (placed by hand: drag it onto another card to move it, Delete puts it back)' : ' (drag it onto another card to put it there)') : ''}`}
            >
              <span class="ov-head">
                <span class="ov-n">{r.n}</span>
                <span class="ov-name">{r.name}</span>
                {r.detail && <span class="ov-mod">{r.detail}</span>}
                {placed && (
                  <span class="ov-placed" title="Placed by hand in the layout file">
                    placed
                  </span>
                )}
              </span>
              <span class="ov-thumb" style={{ aspectRatio: `${w} / ${h}` }}>
                <Thumb source={source} group={group} view={`${x} ${y} ${w} ${h}`} lit={lit} stamp={stamp} />
              </span>
              {stats && (
                <span class="ov-foot">
                  <span class="ov-count">{plural(stats.parts, 'part')}</span>
                  {stats.leaving > 0 && <span class="ov-count leave">{plural(stats.leaving, 'net')} leave</span>}
                  {stats.errors + stats.warnings > 0 && <span class="ov-count bad">{plural(stats.errors + stats.warnings, 'problem')}</span>}
                  {r.carries && <span class="ov-count carry">carries the net</span>}
                </span>
              )}
            </button>
          </div>
        );
      })}
    </div>
  );
}
