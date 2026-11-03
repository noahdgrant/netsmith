import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { api } from '../client';
import { PanZoom } from '../components/PanZoom';
import {
  analysis,
  board,
  detail,
  goTo,
  lineParts,
  pickPart,
  selected,
} from '../store';

/** Pixels in the canvas for each millimetre of the drawing. */
const SCALE = 6;

/** A module's box in the drawing, in canvas pixels. */
interface Sheet {
  name: string;
  title: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Drawing {
  html: string;
  width: number;
  height: number;
  sheets: Sheet[];
}

/** The SVG with its size taken out, so the canvas decides it, and its bounds. */
function prepare(text: string): Drawing | null {
  const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
  const svg = doc.documentElement;
  const bounds = svg.getAttribute('data-bounds');
  if (svg.nodeName !== 'svg' || !bounds) return null;
  const [x0, y0, w, h] = bounds.split(' ').map(Number);
  const sheets = [...svg.querySelectorAll('.module-sheet')].map((g) => {
    const [x, y, sw, sh] = (g.getAttribute('data-bounds') ?? '0 0 0 0').split(' ').map(Number);
    return {
      name: g.getAttribute('data-module') ?? '',
      title: g.querySelector('.module-title')?.textContent ?? '',
      x: (x - x0) * SCALE,
      y: (y - y0) * SCALE,
      w: sw * SCALE,
      h: sh * SCALE,
    };
  });
  svg.removeAttribute('width');
  svg.removeAttribute('height');
  svg.setAttribute('width', String(w * SCALE));
  svg.setAttribute('height', String(h * SCALE));
  return { html: svg.outerHTML, width: w * SCALE, height: h * SCALE, sheets };
}

export function Schematic() {
  const name = board.value;
  const version = analysis.value?.version;
  const [drawing, setDrawing] = useState<Drawing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<{ net?: string; part?: string } | null>(null);
  const [focus, setFocus] = useState<{ x: number; y: number; w: number; h: number; seq: number } | undefined>();
  const holder = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!name) {
      setDrawing(null);
      return;
    }
    let live = true;
    api
      .schematic(name)
      .then((text) => {
        if (!live) return;
        const d = prepare(text);
        setDrawing(d);
        setError(d ? null : 'The server sent no drawing.');
      })
      .catch((e: Error) => live && (setDrawing(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  const netsOfPart = useMemo(() => {
    const map = new Map<string, Set<string>>();
    for (const n of detail.value?.nets ?? []) {
      for (const p of n.pins) {
        if (!map.has(p.refdes)) map.set(p.refdes, new Set());
        map.get(p.refdes)!.add(n.name);
      }
    }
    return map;
  }, [detail.value]);

  // Paint what the pointer is over, what is picked, and what the cursor's line declares.
  const marked = [...new Set([...(selected.value ? [selected.value] : []), ...lineParts.value])];
  useEffect(() => {
    const svg = holder.current?.querySelector('svg.schematic');
    if (!svg) return;
    svg.querySelectorAll('.on,.touches,.mark').forEach((e) => e.classList.remove('on', 'touches', 'mark'));
    const focus = hover ?? (marked.length ? { part: marked[0] } : null);
    svg.classList.toggle('has-on', !!focus);
    if (focus) {
      const nets =
        focus.net != null ? new Set([focus.net]) : (netsOfPart.get(focus.part!) ?? new Set<string>());
      const touching = new Set<Element>();
      for (const net of nets) {
        svg.querySelectorAll(`[data-net="${CSS.escape(net)}"]`).forEach((e) => {
          e.classList.add('on');
          const part = e.closest('.part');
          if (part) touching.add(part);
        });
      }
      touching.forEach((p) => p.classList.add('touches'));
    }
    for (const ref of marked) {
      svg.querySelector(`.part[data-ref="${CSS.escape(ref)}"]`)?.classList.add('on', 'mark');
    }
  }, [drawing, hover, marked.join(','), netsOfPart]);

  const jump = (sheet: Sheet) => setFocus({ ...sheet, seq: (focus?.seq ?? 0) + 1 });

  const pick = (target: EventTarget | null) => {
    if (!(target instanceof Element)) return null;
    const net = target.closest('[data-net]');
    if (net) return { net: net.getAttribute('data-net')! };
    const part = target.closest('.part');
    if (part) return { part: part.getAttribute('data-ref')! };
    return null;
  };

  if (!name) return <div class="empty">There is no board to draw. See the problems.</div>;
  if (error) return <div class="empty bad">{error}</div>;
  if (!drawing) return <div class="empty">Drawing…</div>;

  const stale = analysis.value?.boards.find((b) => b.name === name)?.stale;
  return (
    <div class="schematic-view">
      {stale && <div class="banner">The source has a mistake. This is the last drawing that built.</div>}
      <div class={`schematic-body ${stale ? 'stale' : ''}`}>
      <PanZoom width={drawing.width} height={drawing.height} fitKey={`${name}:${drawing.width}x${drawing.height}`} focus={focus} class="schematic-host">
        <div
          ref={holder}
          class="hit-target"
          dangerouslySetInnerHTML={{ __html: drawing.html }}
          onPointerOver={(e) => setHover(pick(e.target))}
          onPointerLeave={() => setHover(null)}
          onClick={(e) => {
            const hit = pick(e.target);
            if (hit?.part) pickPart(hit.part, true);
            else pickPart(null);
            // A click on a module's box, and not on what is in it, zooms into the module.
            if (!hit && e.target instanceof Element) {
              const module = e.target.closest('.module-sheet')?.getAttribute('data-module');
              const sheet = drawing.sheets.find((s) => s.name === module);
              if (sheet) jump(sheet);
            }
          }}
        />
      </PanZoom>
      <Inspector hover={hover} sheets={drawing.sheets} jump={jump} />
      </div>
    </div>
  );
}

function Inspector({
  hover,
  sheets,
  jump,
}: {
  hover: { net?: string; part?: string } | null;
  sheets: Sheet[];
  jump: (s: Sheet) => void;
}) {
  const d = detail.value;
  const ref = hover?.part ?? selected.value;
  const part = d?.parts.find((p) => p.refdes === ref);
  const net = hover?.net ? d?.nets.find((n) => n.name === hover.net) : undefined;
  return (
    <aside class="inspector no-pan">
      <h3>Selection</h3>
      {net ? (
        <div>
          <div class="title">{net.name}</div>
          <div class="sub">
            {net.pins.length} pins{net.named ? '' : ', unnamed'}
          </div>
          <div class="chips">
            {net.pins.map((p) => (
              <button key={`${p.refdes}.${p.pin}`} class="chip" onClick={() => pickPart(p.refdes, true)}>
                {p.refdes}.{p.pin}
              </button>
            ))}
          </div>
        </div>
      ) : part ? (
        <div>
          <div class="title">
            {part.refdes} <span class="sub">{part.path}</span>
          </div>
          <dl>
            <dt>Kind</dt>
            <dd>{part.kind}</dd>
            <dt>Value</dt>
            <dd>{part.value}</dd>
            <dt>Package</dt>
            <dd>{part.package ?? '-'}</dd>
            <dt>LCSC</dt>
            <dd>{part.lcsc ?? (part.assembled ? '-' : 'hand-soldered')}</dd>
          </dl>
          {part.decl && (
            <button class="link" onClick={() => void goTo(part.decl!)}>
              {part.decl.file}:{part.decl.line}
            </button>
          )}
          <table class="pins">
            <tbody>
              {(d?.nets ?? [])
                .flatMap((n) => n.pins.filter((p) => p.refdes === part.refdes).map((p) => ({ pin: p.pin, net: n.name })))
                .sort((a, b) => a.pin.localeCompare(b.pin, undefined, { numeric: true }))
                .map((p) => (
                  <tr key={p.pin}>
                    <td class="num">{p.pin}</td>
                    <td>{p.net}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p class="hint">Click a part to go to its source. Move the cursor in the source to see what the line declares.</p>
      )}
      {sheets.length > 0 && (
        <>
          <h3>
            Modules <span class="count">{sheets.length}</span>
          </h3>
          <ul class="outline">
            {sheets.map((s) => (
              <li key={s.name}>
                <button class="link" title="Zoom to this module" onClick={() => jump(s)}>
                  {s.title}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
      <h3>Parts {d ? <span class="count">{d.parts.length}</span> : null}</h3>
      <table class="parts">
        <tbody>
          {(d?.parts ?? []).map((p) => (
            <tr
              key={p.refdes}
              class={selected.value === p.refdes ? 'sel' : ''}
              onClick={() => pickPart(p.refdes, true)}
            >
              <td class="ref">{p.refdes}</td>
              <td>{p.value}</td>
              <td class="faint">{p.kind}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {d && d.notes.length > 0 && (
        <>
          <h3>Notes</h3>
          <ul class="notes">
            {d.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </>
      )}
    </aside>
  );
}
