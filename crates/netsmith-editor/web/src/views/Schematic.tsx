import { signal } from '@preact/signals';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';

import { api as server } from '../client';
import { PanZoom, type PanZoomApi } from '../components/PanZoom';
import { type Drawing, loadDrawing } from '../schematic/load';
import {
  netNavOrder,
  netSheets,
  netSummary,
  orderSheets,
  otherSheets,
  sheetStats,
  stepRing,
  type Target,
} from '../schematic/nav';
import {
  type Box,
  K100,
  MIN_LEGIBLE_K,
  centreOnBox,
  isHeldForText,
  placeSheet,
  revealBox,
  zoomRange,
} from '../schematic/zoom';
import { ALL, resolvePick, stepPick } from '../sheets';
import { load, save } from '../storage';
import { analysis, board, detail, layout, problems, revealSection, visible } from '../store';
import { Inspector, type PinRef } from './SchematicInspector';
import { SheetNavPanel, SheetBar, rowOf } from './SheetNav';
import { SheetOverview } from './SheetOverview';
import {
  clearSelection,
  cursorParts,
  goToSource,
  selectNet,
  selectPart,
  selectedNet,
  selectedPart,
} from './schematic-selection';

/** The sheet each board shows (a module's name, or `*` for all), kept in localStorage. */
const picks = signal<Record<string, string>>(load('netsmith.sheets.v1', {}));
/** Whether the side panel is open: null until the person chooses, then open by the window's width. */
const detailsPref = signal<boolean | null>(load('netsmith.schematic.details', null));
/** The side panel is open when the view is wide enough to spare it, unless the person chose. */
const detailsOpen = (viewWidth: number) => detailsPref.value ?? viewWidth >= 1500;
/** Whether the sheet navigator is open: null until the person chooses, then open by the width. */
const navPref = signal<boolean | null>(load('netsmith.schematic.nav', null));
const navOpen = (viewWidth: number) => navPref.value ?? viewWidth >= 900;

function choose(board: string, pick: string) {
  picks.value = { ...picks.value, [board]: pick };
  save('netsmith.sheets.v1', picks.value);
}

/** Room around a sheet's box, so its frame and title are not cut. */
const PAD = 9;
const around = (s: Box): Box => ({ x: s.x - PAD, y: s.y - PAD, w: s.w + 2 * PAD, h: s.h + 2 * PAD });

/** Where to put the view once the sheet shown is the one asked for. */
interface Aim {
  seq: number;
  sheet: string;
  box: Box;
  /** Move no more than needed (stepping through pins), else centre and frame. */
  reveal?: boolean;
}

interface Hit {
  net?: string;
  part?: string;
  /** The click was on a flag, where a net leaves the sheet. */
  flag?: boolean;
}

const pickAt = (target: EventTarget | null): Hit | null => {
  if (!(target instanceof Element)) return null;
  const net = target.closest('[data-net]');
  if (net) return { net: net.getAttribute('data-net')!, flag: !!target.closest('.port') };
  const part = target.closest('.part');
  if (part) return { part: part.getAttribute('data-ref')! };
  return null;
};

export function Schematic() {
  const name = board.value;
  const version = analysis.value?.version;
  const [drawing, setDrawing] = useState<Drawing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<Hit | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; net: string; targets: Target[] } | null>(null);
  const [aim, setAim] = useState<Aim | null>(null);
  const [navPin, setNavPin] = useState<PinRef | null>(null);
  const [help, setHelp] = useState(false);
  /** Goes up when the drawing was painted again while the overview shows (it copies the drawing). */
  const [painted, setPainted] = useState(0);
  const [vp, setVp] = useState<{ k: number; fitK: number } | null>(null);
  const holder = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const zoom = useRef<PanZoomApi | null>(null);
  const applied = useRef(0);
  const seq = useRef(0);
  const [viewWidth, setViewWidth] = useState(0);
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
    server
      .schematic(name)
      .then((text) => {
        if (!live) return;
        const d = loadDrawing(text);
        setDrawing(d);
        setError(d ? null : 'The server sent no drawing.');
      })
      .catch((e: Error) => live && (setDrawing(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  const d = detail.value && detail.value.name === name ? detail.value : null;

  const netsOfPart = useMemo(() => {
    const map = new Map<string, Set<string>>();
    for (const n of d?.nets ?? []) {
      for (const p of n.pins) {
        if (!map.has(p.refdes)) map.set(p.refdes, new Set());
        map.get(p.refdes)!.add(n.name);
      }
    }
    return map;
  }, [d]);

  // The sheets in the order their modules are declared in the source, with what each holds.
  const ordered = useMemo(
    () => (drawing ? orderSheets(drawing.sheets, drawing.index.partSheet, d?.parts ?? [], d?.file ?? null) : []),
    [drawing, d],
  );
  const names = useMemo(() => ordered.map((s) => s.name), [ordered]);
  const stats = useMemo(
    () => (drawing ? sheetStats(ordered, drawing.index, d?.parts ?? [], problems.value) : new Map()),
    [drawing, ordered, d, problems.value],
  );
  const partsBySheet = useMemo(() => {
    const out = new Map<string, string[]>();
    for (const [ref, sheet] of drawing?.index.partSheet ?? []) {
      if (!out.has(sheet)) out.set(sheet, []);
      out.get(sheet)!.push(ref);
    }
    return out;
  }, [drawing]);
  const selNet = selectedNet.value;
  const selPart = selectedPart.value;
  const netInfo = selNet ? d?.nets.find((n) => n.name === selNet) : undefined;
  const carried = useMemo(
    () => new Set(netInfo && drawing ? netSheets(netInfo.pins, drawing.index.partSheet).keys() : []),
    [netInfo, drawing],
  );
  const rows = useMemo(
    () => ordered.map((s, i) => rowOf(s, i + 1, stats.get(s.name), carried.has(s.name), partsBySheet.get(s.name) ?? [])),
    [ordered, stats, carried, partsBySheet],
  );

  const current = name ? resolvePick(picks.value[name], names) : ALL;
  const overview = current === ALL && rows.length > 1;
  const open = (p: string) => name && choose(name, p);

  // Which sheet's drawing is in the page: the open one (all of them hide in the overview, which
  // draws them again from their groups).
  useLayoutEffect(() => {
    const svg = holder.current?.querySelector('svg.schematic');
    if (!svg) return;
    for (const slot of svg.querySelectorAll<SVGGElement>(':scope > g.sheet-slot')) {
      const sheet = slot.getAttribute('data-sheet');
      slot.style.display = sheet === '' || sheet === current ? '' : 'none';
    }
  }, [drawing, current]);

  // Paint what is selected, what the pointer is over, and what the cursor's line declares. With
  // a net selected nothing else lights, so the rest stays dimmed while the pointer moves.
  const marked = [...new Set([...(selPart ? [selPart] : []), ...cursorParts.value])];
  useEffect(() => {
    const svg = holder.current?.querySelector('svg.schematic');
    if (!svg) return;
    svg.querySelectorAll('.on,.touches,.mark,.cur').forEach((e) => e.classList.remove('on', 'touches', 'mark', 'cur'));
    const focus: Hit | null = selNet ? { net: selNet } : (hover ?? (marked.length ? { part: marked[0] } : null));
    svg.classList.toggle('has-on', !!focus);
    if (focus) {
      const nets = focus.net != null ? new Set([focus.net]) : (netsOfPart.get(focus.part!) ?? new Set<string>());
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
    if (navPin && selNet) {
      svg.querySelector(`.pin[data-ref="${CSS.escape(navPin.ref)}"][data-pin="${CSS.escape(navPin.pin)}"]`)?.classList.add('cur');
    }
    if (overview) setPainted((n) => n + 1);
  }, [drawing, hover, selNet, marked.join(','), netsOfPart, navPin, current, overview]);

  // The pin the net navigator is on belongs to the net that is selected.
  useEffect(() => setNavPin(null), [selNet]);

  const geometry = (sheet: string, box: Box | undefined): Box | undefined => (box ? { ...box } : drawing?.sheets.find((s) => s.name === sheet));
  const aimAt = (sheet: string, box: Box | undefined, reveal = false) => {
    const b = geometry(sheet, box);
    if (!b) return;
    setAim({ seq: ++seq.current, sheet, box: b, reveal });
  };

  const netBox = (net: string, sheet: string) => drawing?.index.netPlaces.get(net)?.get(sheet)?.box;
  /** Select a net and bring it into view: on this sheet when it is drawn here, else on the first sheet that draws it. */
  const goNet = (net: string) => {
    selectNet(net);
    const places = drawing?.index.netPlaces.get(net);
    if (!places) return;
    const sheet = places.has(current) ? current : (names.find((n) => places.has(n)) ?? current);
    if (sheet !== current) open(sheet);
    aimAt(sheet, netBox(net, sheet));
  };
  /** Follow a net to another sheet, selected and centred. */
  const goNetOn = (sheet: string, net: string) => {
    selectNet(net);
    open(sheet);
    aimAt(sheet, netBox(net, sheet));
  };
  const goPart = (ref: string) => {
    selectPart(ref);
    const sheet = drawing?.index.partSheet.get(ref);
    if (sheet == null) return;
    if (sheet !== '' && sheet !== current) open(sheet);
    aimAt(sheet || current, drawing?.index.partBox.get(ref));
  };
  const goPin = (ref: string, pin: string) => {
    const at = drawing?.index.pinPoint.get(`${ref}.${pin}`);
    if (!at) return goPart(ref);
    setNavPin({ ref, pin });
    if (at.sheet !== '' && at.sheet !== current) open(at.sheet);
    aimAt(at.sheet || current, { x: at.x, y: at.y, w: 0, h: 0 }, at.sheet === current);
  };
  const walk = useMemo(
    () => (netInfo && drawing ? netNavOrder(netInfo.pins, drawing.index.partSheet, names) : []),
    [netInfo, drawing, names],
  );
  const stepPins = (dir: 1 | -1) => {
    if (walk.length === 0) return;
    const at = walk.findIndex((p) => navPin && p.refdes === navPin.ref && p.pin === navPin.pin);
    const next = walk[stepRing(walk.length, at, dir)];
    if (next) goPin(next.refdes, next.pin);
  };
  const followFlag = (net: string, x: number, y: number) => {
    const targets = otherSheets(d?.nets.find((n) => n.name === net), current, drawing?.index.partSheet ?? new Map(), names);
    if (targets.length === 1) goNetOn(targets[0].sheet, net);
    else if (targets.length > 1) {
      const r = stage.current?.getBoundingClientRect();
      setMenu({ x: x - (r?.left ?? 0), y: y - (r?.top ?? 0), net, targets });
    }
  };
  const frameSelection = () => {
    if (selNet) {
      const b = netBox(selNet, current);
      if (b) aimAt(current, b);
    } else if (selPart) {
      const b = drawing?.index.partBox.get(selPart);
      if (b) aimAt(current, b);
    }
  };

  // Apply the aim once the sheet it is for is the one shown and the canvas has been placed.
  useLayoutEffect(() => {
    const z = zoom.current;
    if (!aim || !z || applied.current === aim.seq || aim.sheet !== current || overview) return;
    const { w, h } = z.size();
    if (!w || !h) return;
    applied.current = aim.seq;
    const view = z.view();
    z.set(
      aim.reveal
        ? revealBox(w, h, aim.box, view, MIN_LEGIBLE_K, 70)
        : centreOnBox(w, h, aim.box, view, { minK: MIN_LEGIBLE_K, fitBox: true }),
    );
  }, [aim, current, overview, drawing]);

  // The keys, while this view is the one the person is in.
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e) => {
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
    const t = e.target as HTMLElement | null;
    if (t?.closest('input, textarea, select, [contenteditable="true"], .cm-editor')) return;
    const here = visible.value.find((p) => p.index === layout.value.focus || visible.value.length === 1);
    if (here?.section !== 'schematic') return;
    const onCanvas = !t || t === document.body || t.classList.contains('panzoom') || !!t.closest('.panzoom');
    if (e.key === '[' || e.key === 'PageUp' || e.key === ']' || e.key === 'PageDown') {
      if (!name || rows.length < 2) return;
      e.preventDefault();
      open(stepPick(names, current, e.key === '[' || e.key === 'PageUp' ? -1 : 1));
    } else if (e.key === 'Escape') {
      if (help) setHelp(false);
      else if (menu) setMenu(null);
      else if (selNet || selPart) clearSelection();
      else return;
      e.preventDefault();
    } else if (e.key === 'Tab' && selNet && onCanvas) {
      e.preventDefault();
      stepPins(e.shiftKey ? -1 : 1);
    } else if (e.key === 'Enter' && selPart && onCanvas) {
      e.preventDefault();
      goToSource(selPart);
    } else if (e.code === 'Digit2' && e.shiftKey && (selNet || selPart)) {
      e.preventDefault();
      frameSelection();
    } else if (e.key === '?') {
      e.preventDefault();
      setHelp(!help);
    }
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, []);

  // A menu of sheets closes on a click anywhere else.
  useEffect(() => {
    if (!menu) return;
    const away = (e: PointerEvent) => {
      if (!(e.target as Element).closest('.flag-menu')) setMenu(null);
    };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, [menu]);

  if (!name) return <EmptyState title="There is no board to draw" text="Fix the problems in the source, then the board is drawn here." action="Open the source" />;
  if (d && d.parts.length === 0) {
    return (
      <EmptyState
        title={`${d.name} has no parts yet`}
        text="Declare a part in the source and it is drawn here, with its wires and nets."
        action="Open the source"
      />
    );
  }
  if (error) return <div class="empty bad">{error}</div>;
  if (!drawing) return <Skeleton />;

  const stale = analysis.value?.boards.find((b) => b.name === name)?.stale;
  const sheet = drawing.sheets.find((s) => s.name === current);
  const box = sheet ? around(sheet) : undefined;
  const page: Box = box ?? { x: 0, y: 0, w: drawing.width, h: drawing.height };
  const side = detailsOpen(viewWidth);
  const nav = rows.length > 1 && navOpen(viewWidth);
  const held = !overview && vp ? isHeldForText(vp.fitK, vp.k) : false;
  const total = { parts: d?.parts.length ?? rows.reduce((n, r) => n + (r.stats?.parts ?? 0), 0) };

  return (
    <div class="schematic-view" ref={root}>
      {stale && <div class="banner">The source has a mistake. This is the last drawing that built.</div>}
      <SheetBar
        rows={rows}
        names={names}
        current={current}
        boardName={drawing ? (d?.name ?? name) : name}
        onOpen={open}
        navOpen={nav}
        toggleNav={() => {
          navPref.value = !nav;
          save('netsmith.schematic.nav', navPref.value);
        }}
        side={side}
        toggleSide={() => {
          detailsPref.value = !side;
          save('netsmith.schematic.details', detailsPref.value);
        }}
        total={total}
        help={() => setHelp(!help)}
      />
      <div class={`schematic-body ${stale ? 'stale' : ''}`}>
        {nav && (
          <SheetNavPanel rows={rows} current={current} onOpen={open} total={total} onCollapse={() => {
            navPref.value = false;
            save('netsmith.schematic.nav', false);
          }} />
        )}
        <div class="stage" ref={stage}>
          <PanZoom
            width={drawing.width}
            height={drawing.height}
            fitKey={`${name}:${drawing.width}x${drawing.height}:${current}`}
            fitBox={box}
            memoKey={`schematic|${name}|${current}`}
            class={`schematic-host ${overview ? 'is-hidden' : ''}`}
            scale100={K100}
            presets
            arrowKeys
            label="Schematic sheet"
            range={zoomRange}
            placeView={(w, h) => placeSheet(w, h, page).view}
            onViewport={(v) => setVp((o) => (o && o.k === v.k && o.fitK === v.fitK ? o : { k: v.k, fitK: v.fitK }))}
            onApi={(a) => (zoom.current = a)}
          >
            <div
              ref={holder}
              class="hit-target"
              dangerouslySetInnerHTML={{ __html: drawing.html }}
              onPointerOver={(e) => setHover(pickAt(e.target))}
              onPointerLeave={() => setHover(null)}
              onClick={(e) => {
                setMenu(null);
                const hit = pickAt(e.target);
                if (hit?.net) {
                  selectNet(hit.net);
                  if (hit.flag) followFlag(hit.net, e.clientX, e.clientY);
                } else if (hit?.part) selectPart(hit.part);
                else clearSelection();
              }}
              onDblClick={(e) => {
                const hit = pickAt(e.target);
                if (hit?.part) goToSource(hit.part);
              }}
            />
          </PanZoom>
          {overview && (
            <SheetOverview
              drawing={drawing}
              rows={rows}
              open={open}
              source={() => holder.current}
              lit={!!selNet || !!selPart}
              stamp={painted}
            />
          )}
          {held && (
            <div class="hold-chip no-pan" role="status">
              <span>Zoomed in to keep the text readable</span>
              <button onClick={() => zoom.current?.fit()}>Fit</button>
            </div>
          )}
          {!overview && (selNet || selPart) && (
            <SelectionChip
              net={netInfo ? netSummary(netInfo, drawing.index.partSheet).text : undefined}
              netName={selNet}
              part={selPart}
              partValue={d?.parts.find((p) => p.refdes === selPart)?.value}
              onStep={stepPins}
              canStep={walk.length > 1}
              onSource={() => selPart && goToSource(selPart)}
              onFrame={frameSelection}
            />
          )}
          {menu && (
            <div class="flag-menu no-pan" role="menu" style={{ left: Math.max(8, menu.x - 20), top: Math.max(8, menu.y + 12) }}>
              <div class="menu-head">{menu.net} also on</div>
              {menu.targets.map((t) => (
                <button
                  key={t.sheet}
                  role="menuitem"
                  class="menu-item"
                  onClick={() => {
                    setMenu(null);
                    goNetOn(t.sheet, menu.net);
                  }}
                >
                  <span class="mi-name">{rows.find((r) => r.sheet.name === t.sheet)?.name ?? t.sheet}</span>
                  <span class="mi-pins">
                    {t.pins
                      .slice(0, 3)
                      .map((p) => `${p.refdes}.${p.pin}`)
                      .join(', ')}
                    {t.pins.length > 3 ? ` +${t.pins.length - 3}` : ''}
                  </span>
                </button>
              ))}
            </div>
          )}
          {help && <Shortcuts onClose={() => setHelp(false)} />}
        </div>
        {side && (
          <Inspector
            drawing={drawing}
            rows={rows}
            current={current}
            order={names}
            openSheet={open}
            goPart={goPart}
            goPin={goPin}
            goNet={goNet}
            navPin={navPin}
            stepPins={stepPins}
            hover={hover}
          />
        )}
      </div>
    </div>
  );
}

/** What is selected, small, on the canvas: where the inspector is closed this is the net navigator. */
function SelectionChip({
  net,
  netName,
  part,
  partValue,
  onStep,
  canStep,
  onSource,
  onFrame,
}: {
  net?: string;
  netName: string | null;
  part: string | null;
  partValue?: string;
  onStep: (dir: 1 | -1) => void;
  canStep: boolean;
  onSource: () => void;
  onFrame: () => void;
}) {
  return (
    <div class="sel-chip no-pan" role="status">
      {netName ? (
        <>
          <span class="sc-kind">Net</span>
          <span class="sc-name mono" title={netName}>
            {netName}
          </span>
          <span class="sc-sub">{net}</span>
          <button class="sc-btn" disabled={!canStep} title="Previous pin (Shift+Tab)" aria-label="Previous pin" onClick={() => onStep(-1)}>
            ‹
          </button>
          <button class="sc-btn" disabled={!canStep} title="Next pin (Tab)" aria-label="Next pin" onClick={() => onStep(1)}>
            ›
          </button>
        </>
      ) : (
        <>
          <span class="sc-kind">Part</span>
          <span class="sc-name mono">{part}</span>
          {partValue && <span class="sc-sub">{partValue}</span>}
          <button class="sc-btn text" title="Open the source at this part (Enter, or double-click)" onClick={onSource}>
            Source
          </button>
        </>
      )}
      <button class="sc-btn text" title="Zoom to the selection (Shift+2)" onClick={onFrame}>
        Zoom to
      </button>
      <button class="sc-btn" title="Clear the selection (Esc)" aria-label="Clear the selection" onClick={clearSelection}>
        ×
      </button>
    </div>
  );
}

const KEYS: [string, string][] = [
  ['Click', 'Select a part, or a net (wire, label, flag, pin)'],
  ['Click a flag', 'Follow the net to the sheet that carries it'],
  ['Double-click / Enter', 'Open the part in the source'],
  ['Tab / Shift+Tab', 'Walk the pins of the selected net'],
  ['Esc', 'Clear the selection'],
  ['[  ]  or  Page Up / Down', 'Previous / next sheet'],
  ['0 or F', 'Fit the sheet'],
  ['W', 'Fit the width'],
  ['Shift+1', '100%: the drawing at its size on paper'],
  ['Shift+2', 'Zoom to the selection'],
  ['+  −', 'Zoom'],
  ['Arrow keys', 'Pan'],
  ['?', 'This list'],
];

function Shortcuts({ onClose }: { onClose: () => void }) {
  return (
    <div class="shortcuts no-pan" role="dialog" aria-label="Keyboard shortcuts">
      <div class="sh-head">
        <b>Schematic keys</b>
        <button class="icon-btn" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <dl>
        {KEYS.map(([k, v]) => (
          <>
            <dt>
              <kbd>{k}</kbd>
            </dt>
            <dd>{v}</dd>
          </>
        ))}
      </dl>
    </div>
  );
}

function EmptyState({ title, text, action }: { title: string; text: string; action: string }) {
  return (
    <div class="empty-state" role="status">
      <div class="es-icon" aria-hidden="true">
        ⌁
      </div>
      <h2>{title}</h2>
      <p>{text}</p>
      <button class="tool" onClick={() => revealSection('source')}>
        {action}
      </button>
    </div>
  );
}

/** What shows while the first analysis is on its way: the shape of the view, in grey. */
function Skeleton() {
  return (
    <div class="schematic-view skeleton" aria-busy="true" aria-label="Loading the schematic">
      <div class="sheetbar">
        <span class="sk sk-btn" />
        <span class="sk sk-btn" />
        <span class="sk sk-pick" />
        <span class="spacer" />
        <span class="sk sk-btn wide" />
      </div>
      <div class="schematic-body">
        <div class="sheetnav sk-nav">
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <span key={i} class="sk sk-row" />
          ))}
        </div>
        <div class="stage">
          <div class="sk-canvas">
            <span class="sk sk-block a" />
            <span class="sk sk-block b" />
            <span class="sk sk-block c" />
          </div>
        </div>
        <div class="sinspector sk-side">
          <span class="sk sk-line w60" />
          <span class="sk sk-line w40" />
          <span class="sk sk-line w80" />
          <span class="sk sk-line w50" />
        </div>
      </div>
    </div>
  );
}
