import { signal } from '@preact/signals';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { api } from '../client';
import { PanZoom } from '../components/PanZoom';
import { ALL, canStep, resolvePick, stepPick } from '../sheets';
import { load, save } from '../storage';
import {
  analysis,
  board,
  detail,
  goTo,
  layout,
  lineParts,
  pickNet,
  pickPart,
  selectedRefdes,
  visible,
} from '../store';
import {
  declarationOf,
  modulesOf,
  moduleSel,
  partByPath,
  revealRequest,
  selection,
  setSelection,
} from '../selection';

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

/** The sheet each board shows (a module's name, or `*` for all), kept in localStorage. */
const picks = signal<Record<string, string>>(load('netsmith.sheets.v1', {}));
/** Whether the side panel is open: null until the person chooses, then open by the window's width. */
const detailsPref = signal<boolean | null>(load('netsmith.schematic.details', null));
/** The side panel is open when the view is wide enough to spare it, unless the person chose. */
const detailsOpen = (viewWidth: number) => detailsPref.value ?? viewWidth >= 1500;

function choose(board: string, pick: string) {
  picks.value = { ...picks.value, [board]: pick };
  save('netsmith.sheets.v1', picks.value);
}

/** A sheet's box with room around it, so its frame and title are not cut. */
const PAD = 1.5 * SCALE;
const around = (s: Sheet) => ({ x: s.x - PAD, y: s.y - PAD, w: s.w + 2 * PAD, h: s.h + 2 * PAD });

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
  const holder = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const [viewWidth, setViewWidth] = useState(0);
  /** A box of the drawing to zoom to, when the selection is asked to be brought into view. */
  const [focusBox, setFocusBox] = useState<{ x: number; y: number; w: number; h: number; seq: number } | undefined>();
  const [revealBox, setRevealBox] = useState<{ x: number; y: number; w: number; h: number; seq: number } | undefined>();
  const handled = useRef(0);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const watch = new ResizeObserver(() => setViewWidth(el.clientWidth));
    watch.observe(el);
    setViewWidth(el.clientWidth);
    return () => watch.disconnect();
  }, [!!drawing]);

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

  // Paint what the pointer is over, what is picked (a part, a net or a module: the shared
  // selection), and what the cursor's line declares.
  const sel = selection.value;
  const picked = sel?.kind === 'net' ? [] : [...selectedRefdes.value];
  const marked = [...new Set([...picked, ...lineParts.value])];
  const pickedNet = sel?.kind === 'net' ? sel.key : null;
  const wholeModule = sel?.kind === 'module';
  useEffect(() => {
    const svg = holder.current?.querySelector('svg.schematic');
    if (!svg) return;
    svg.querySelectorAll('.on,.touches,.mark,.linked').forEach((e) => e.classList.remove('on', 'touches', 'mark', 'linked'));
    // A module lights its parts and dims the rest; it does not light their nets (ground would light all).
    const focus = hover ?? (pickedNet ? { net: pickedNet } : wholeModule ? null : marked.length ? { part: marked[0] } : null);
    svg.classList.toggle('has-on', !!focus || (wholeModule && marked.length > 0));
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
      const el = svg.querySelector(`.part[data-ref="${CSS.escape(ref)}"]`);
      el?.classList.add('on', 'mark');
      // What only the cursor's line declares is marked less than what is selected.
      if (el && !picked.includes(ref)) el.classList.add('linked');
    }
  }, [drawing, hover, marked.join(','), netsOfPart, pickedNet, wholeModule]);

  const names = useMemo(() => (drawing?.sheets ?? []).map((s) => s.name), [drawing]);
  const current = name ? resolvePick(picks.value[name], names) : ALL;
  const open = (p: string) => name && choose(name, p);

  // Bring the selection into view when asked (go to anything, the chip): open the sheet that
  // has it, then zoom to it.
  const ask = revealRequest.value;
  useEffect(() => {
    if (!ask || !drawing || ask.nonce === handled.current) return;
    const svg = holder.current?.querySelector('svg.schematic') as SVGSVGElement | null;
    if (!svg) return;
    const { kind, key } = ask.sel;
    let el: Element | null = null;
    if (kind === 'part') {
      const ref = partByPath(detail.value, key)?.refdes;
      el = ref ? svg.querySelector(`.part[data-ref="${CSS.escape(ref)}"]`) : null;
    } else if (kind === 'module') {
      el = svg.querySelector(`.module-sheet[data-module="${CSS.escape(key.split('.')[0])}"]`);
    } else {
      el = svg.querySelector(`[data-net="${CSS.escape(key)}"]`);
    }
    if (!el) {
      handled.current = ask.nonce;
      return;
    }
    // The element's box in the drawing's pixels, and the sheet whose frame holds its middle.
    const box = el.getBoundingClientRect();
    const frame = svg.getBoundingClientRect();
    if (!frame.width) return;
    const to = drawing.width / frame.width;
    const rect = { x: (box.left - frame.left) * to, y: (box.top - frame.top) * to, w: box.width * to, h: box.height * to };
    const mid = { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
    const sheet = drawing.sheets.find((s) => mid.x >= s.x && mid.x <= s.x + s.w && mid.y >= s.y && mid.y <= s.y + s.h)?.name;
    if (sheet && current !== ALL && sheet !== current) {
      open(sheet);
      return;
    }
    handled.current = ask.nonce;
    // A net is a whole run of wires: it is lit, and the view stays where it is.
    if (kind === 'net') return;
    // One sheet is shown whole already: the view moves only if the thing is out of it. The
    // overview is too small to read: it zooms to the sheet that has the thing.
    const seq = (focusBox?.seq ?? revealBox?.seq ?? 0) + 1;
    if (current === ALL) {
      const at = drawing.sheets.find((s) => s.name === sheet);
      const r = at ? around(at) : rect;
      setFocusBox({ x: r.x, y: r.y, w: r.w, h: r.h, seq });
    } else setRevealBox({ ...rect, seq });
  }, [ask?.nonce, drawing, current]);

  // The keys step through the sheets while this view is the one the person is in.
  const stepRef = useRef<(dir: 1 | -1) => void>(() => {});
  stepRef.current = (dir) => name && open(stepPick(names, current, dir));
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      if (t?.closest('input, textarea, select, [contenteditable="true"], .cm-editor')) return;
      const here = visible.value.find((p) => p.index === layout.value.focus || visible.value.length === 1);
      if (here?.section !== 'schematic') return;
      const dir = e.key === 'ArrowLeft' || e.key === '[' ? -1 : e.key === 'ArrowRight' || e.key === ']' ? 1 : 0;
      if (!dir) return;
      e.preventDefault();
      stepRef.current(dir);
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, []);

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
  const sheet = drawing.sheets.find((s) => s.name === current);
  const box = sheet ? around(sheet) : undefined;
  // One sheet at a time: what is outside its frame is cut away, so it fills the view.
  const clip = box
    ? `inset(${box.y}px ${drawing.width - box.x - box.w}px ${drawing.height - box.y - box.h}px ${box.x}px)`
    : undefined;
  const side = detailsOpen(viewWidth);
  return (
    <div class="schematic-view" ref={root}>
      {stale && <div class="banner">The source has a mistake. This is the last drawing that built.</div>}
      <SheetBar
        sheets={drawing.sheets}
        pick={current}
        names={names}
        open={open}
        side={side}
        toggleSide={() => {
          detailsPref.value = !side;
          save('netsmith.schematic.details', detailsPref.value);
        }}
      />
      <div class={`schematic-body ${stale ? 'stale' : ''}`}>
      <PanZoom
        width={drawing.width}
        height={drawing.height}
        fitKey={`${name}:${drawing.width}x${drawing.height}:${current}`}
        fitBox={box}
        focus={focusBox}
        reveal={revealBox}
        memoKey={`schematic|${name}|${current}`}
        class="schematic-host"
      >
        <div
          ref={holder}
          class="hit-target"
          style={clip ? { clipPath: clip } : undefined}
          dangerouslySetInnerHTML={{ __html: drawing.html }}
          onPointerOver={(e) => setHover(pick(e.target))}
          onPointerLeave={() => setHover(null)}
          onClick={(e) => {
            const hit = pick(e.target);
            if (hit?.part) pickPart(hit.part, true);
            else if (hit?.net != null) pickNet(selection.peek()?.kind === 'net' && selection.peek()?.key === hit.net ? null : hit.net);
            else pickPart(null);
            // In the overview, a click on a module's box, and not on what is in it, opens its sheet
            // and selects the module.
            if (!hit && current === ALL && e.target instanceof Element) {
              const module = e.target.closest('.module-sheet')?.getAttribute('data-module');
              const found = drawing.sheets.find((s) => s.name === module);
              if (found) {
                open(found.name);
                if (modulesOf(detail.value).some((m) => m.path === found.name)) setSelection(moduleSel(found.name));
              }
            }
          }}
        />
      </PanZoom>
      {side && <Inspector hover={hover} sheets={drawing.sheets} pick={current} open={open} />}
      </div>
    </div>
  );
}

/** The sheets as a strip of tabs, with Previous and Next, the overview first. */
function SheetBar({
  sheets,
  pick,
  names,
  open,
  side,
  toggleSide,
}: {
  sheets: Sheet[];
  pick: string;
  names: string[];
  open: (p: string) => void;
  side: boolean;
  toggleSide: () => void;
}) {
  const can = canStep(names, pick);
  const place = pick === ALL ? 'All sheets' : `${names.indexOf(pick) + 1} of ${names.length}`;
  const active = useRef<HTMLButtonElement>(null);
  useEffect(() => active.current?.scrollIntoView({ block: 'nearest', inline: 'nearest' }), [pick]);
  return (
    <div class="sheetbar">
      <button
        class="step"
        disabled={!can.prev}
        title="Previous sheet (Left arrow or [)"
        aria-label="Previous sheet"
        onClick={() => open(stepPick(names, pick, -1))}
      >
        ‹
      </button>
      <button
        class="step"
        disabled={!can.next}
        title="Next sheet (Right arrow or ])"
        aria-label="Next sheet"
        onClick={() => open(stepPick(names, pick, 1))}
      >
        ›
      </button>
      <div class="sheet-tabs" role="tablist" aria-label="Sheets">
        <button
          role="tab"
          aria-selected={pick === ALL}
          ref={pick === ALL ? active : undefined}
          class={`sheet-tab ${pick === ALL ? 'active' : ''}`}
          title="Every sheet at once; click one to open it"
          onClick={() => open(ALL)}
        >
          All sheets
        </button>
        {sheets.map((s) => (
          <button
            key={s.name}
            role="tab"
            aria-selected={pick === s.name}
            ref={pick === s.name ? active : undefined}
            class={`sheet-tab ${pick === s.name ? 'active' : ''}`}
            title={s.title}
            onClick={() => open(s.name)}
          >
            {s.title}
          </button>
        ))}
      </div>
      <span class="sheet-place">{place}</span>
      <button
        class={`tool small ${side ? 'on' : ''}`}
        aria-pressed={side}
        title="Show or hide the panel with the selection, the modules and the parts"
        onClick={toggleSide}
      >
        Details
      </button>
    </div>
  );
}

function Inspector({
  hover,
  sheets,
  pick,
  open,
}: {
  hover: { net?: string; part?: string } | null;
  sheets: Sheet[];
  pick: string;
  open: (p: string) => void;
}) {
  const d = detail.value;
  const sel = selection.value;
  const picked = selectedRefdes.value;
  // What the pointer is over comes first; else what is selected, in any view.
  const ref = hover?.part ?? (!hover?.net && sel?.kind === 'part' ? partByPath(d, sel.key)?.refdes : undefined);
  const part = d?.parts.find((p) => p.refdes === ref);
  const netName = hover?.part ? undefined : (hover?.net ?? (sel?.kind === 'net' ? sel.key : undefined));
  const net = netName ? d?.nets.find((n) => n.name === netName) : undefined;
  const module = !hover && sel?.kind === 'module' ? modulesOf(d).find((m) => m.path === sel.key) : undefined;
  const moduleAt = module && sel ? declarationOf(d, sel) : null;
  return (
    <aside class="inspector no-pan">
      <h3>
        Selection
        {sel && !hover && (
          <button class="link clear" title="Clear the selection (Esc)" onClick={() => setSelection(null)}>
            clear
          </button>
        )}
      </h3>
      {module ? (
        <div>
          <div class="title">{module.path}</div>
          <div class="sub">
            module · {module.parts.length} part{module.parts.length === 1 ? '' : 's'}
          </div>
          <div class="chips">
            {module.parts.map((p) => (
              <button key={p.path} class="chip" title={`${p.value} · ${p.path}`} onClick={() => pickPart(p.refdes, true)}>
                {p.refdes}
              </button>
            ))}
          </div>
          {moduleAt && (
            <button class="link" onClick={() => void goTo(moduleAt)}>
              {moduleAt.file}:{moduleAt.line}
            </button>
          )}
          {sheets.some((x) => x.name === module.path.split('.')[0]) && pick !== module.path.split('.')[0] && (
            <button class="link" onClick={() => open(module.path.split('.')[0])}>
              open sheet
            </button>
          )}
        </div>
      ) : net ? (
        <div>
          <div class="title">{net.name}</div>
          <div class="sub">
            net · {net.pins.length} pins{net.named ? '' : ', unnamed'} · no line in the source
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
        <p class="hint">
          Click a part to select it everywhere and go to its source; click a wire for the net. Ctrl+P finds anything by name.
        </p>
      )}
      {sheets.length > 0 && (
        <>
          <h3>
            Modules <span class="count">{sheets.length}</span>
          </h3>
          <ul class="outline">
            {sheets.map((s) => (
              <li key={s.name}>
                <button class={`link ${pick === s.name ? 'sel' : ''}`} title="Open this sheet" onClick={() => open(s.name)}>
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
              class={picked.has(p.refdes) ? 'sel' : ''}
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
