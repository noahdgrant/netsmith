// "All modules": a grid of cards in source order, each with its title, its counts and a drawing of
// the sheet. A drawing is a copy of the sheet's group as it is in the page's SVG (a `<use>` would
// not take the style rules of the drawing in every browser), taken again when the selection changes
// so the highlight shows. A click on a card opens the sheet.

import { useLayoutEffect, useRef } from 'preact/hooks';

import type { Drawing } from '../schematic/load';
import { SCALE } from '../schematic/zoom';
import { plural } from '../util';
import type { SheetRow } from './SheetNav';

/** A card is two columns wide when its sheet is more than this much wider than it is tall. */
const WIDE = 2.4;

/** How roomy the cards are for `n` modules: `few` (up to 4) fills the window, `some` (up to 9) is in between, `many` is the dense grid. */
export const overviewSize = (n: number): 'few' | 'some' | 'many' => (n <= 4 ? 'few' : n <= 9 ? 'some' : 'many');

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
}) {
  // A few modules are given the room: two columns of large cards (three for a handful), where many stay as dense as they were.
  const size = overviewSize(rows.length);
  return (
    <div class={`sheet-overview schematic-host no-pan ov-${size}`} role="list" aria-label="All modules">
      {rows.map((r) => {
        const s = r.sheet;
        const pad = 1.5;
        const x = s.x / SCALE + drawing.mm.x - pad;
        const y = s.y / SCALE + drawing.mm.y - pad;
        const w = s.w / SCALE + 2 * pad;
        const h = s.h / SCALE + 2 * pad;
        const group = drawing.groupId.get(s.name);
        const aspect = w / h;
        const stats = r.stats;
        return (
          <div role="listitem" key={s.name} class={`ov-item ${aspect > WIDE ? 'wide' : ''} ${aspect > 4.2 ? 'wider' : ''}`}>
            <button class={`ov-card ${r.carries ? 'carries' : ''}`} onClick={() => open(s.name)} title={`Open ${s.title}`}>
              <span class="ov-head">
                <span class="ov-n">{r.n}</span>
                <span class="ov-name">{r.name}</span>
                {r.detail && <span class="ov-mod">{r.detail}</span>}
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
