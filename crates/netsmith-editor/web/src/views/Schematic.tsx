import { signal } from '@preact/signals';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';

import { api as server } from '../client';
import { PanZoom, type PanZoomApi } from '../components/PanZoom';
import { EmptyState } from '../components/ui';
import { NoBoard } from '../components/ViewStatus';
import { busName, busTargets, litNets } from '../schematic/bus';
import { type Bus } from '../schematic/drawing';
import { type PartFacts, hintSheet, moduleNames, readFacts, readNotes } from '../schematic/blocks';
import { beginDrag, dragReadout } from '../schematic/dragdom';
import { addHalos, hintRectOf } from '../schematic/halo';
import { countAll, countIn, tablesOf } from '../schematic/hints';
import {
  type Ctx,
  clearAllEdit,
  clearPartEdit,
  clearSheetEdit,
  dragRefusal,
  dropEdit,
  entryState,
  hasEntry,
  mirrorEdit,
  noteDropEdit,
  nudgeEdit,
  turnEdit,
} from '../schematic/moves';
import { marksOf } from '../schematic/marks';
import { type Drawing, loadDrawing } from '../schematic/load';
import { litNotes, noteLocation, parseNoteAt, prepareNotes } from '../schematic/notes';
import {
  netNavOrder,
  netSheets,
  moduleOfSheet,
  neighbourBox,
  netSummary,
  orderSheets,
  otherSheets,
  revealStep,
  sheetStats,
  stepRing,
} from '../schematic/nav';
import {
  type Box,
  K100,
  MIN_LEGIBLE_K,
  centreOnBox,
  isHeldForText,
  placeSheet,
  revealBox,
  revealPart,
  zoomRange,
} from '../schematic/zoom';
import { ALL, resolvePick, stepPick } from '../sheets';
import { load, save } from '../storage';
import { toggleCheatSheet } from '../quickpick';
import { hoverEnter, hoverLeave, hoverOfRefs } from '../hover';
import { modulesOf, revealRequest, sameSelection, selection } from '../selection';
import { ask as confirmWith } from '../components/Confirm';
import { analysis, board, cancelAnalysis, detail, goTo, problems, revealSection } from '../store';
import { hintsState, lastWriteAt, loadHints, redrawing, submitEdit, writing } from './schematic-layout';
import { Inspector, type PinRef } from './SchematicInspector';
import { type SchematicActions, netFacts, schematicActions } from './schematic-actions';
import { SheetNavPanel, SheetBar, rowOf } from './SheetNav';
import { SheetOverview } from './SheetOverview';
import {
  clearSelection,
  cursorParts,
  drawnBuses,
  goToSource,
  moduleParts,
  selectBus,
  selectModule,
  selectNet,
  selectPart,
  selectedBus,
  selectedModule,
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
  /** A part to bring into view the way a pick from elsewhere does (`revealPart`): the zoom stays
   * when it reads and the part is on screen. `context` is the part and its neighbours. */
  part?: { ref: string; context: Box };
}

interface Hit {
  net?: string;
  part?: string;
  /** A bus (the path of its interface instance): its line, its writing or its flag. */
  bus?: string;
  /** The click was on a flag, where a net, or a bus, leaves the sheet. */
  flag?: boolean;
  /** A note (a doc comment or a relation): where it is written, `file:line`. A note is no part and no net, so nothing is picked or lit for it. */
  note?: string;
}

/** What the small menu under a flag offers: where a net or a bus can be followed to. */
interface FlagMenu {
  x: number;
  y: number;
  head: string;
  items: { key: string; name: string; detail: string; go: () => void }[];
}

const pickAt = (target: EventTarget | null): Hit | null => {
  if (!(target instanceof Element)) return null;
  const net = target.closest('[data-net]');
  if (net) return { net: net.getAttribute('data-net')!, flag: !!target.closest('.port') };
  // The thick line, its count and writing, or its flag; an entry is a net's wire and picks the net.
  const bus = target.closest('[data-bus]');
  if (bus) return { bus: bus.getAttribute('data-bus')!, flag: !!target.closest('.bus-flag') };
  const note = target.closest('.note');
  if (note) return { note: note.getAttribute('data-at') ?? '' };
  const part = target.closest('.part');
  if (part) return { part: part.getAttribute('data-ref')! };
  return null;
};

export function Schematic() {
  const name = board.value;
  const version = analysis.value?.version;
  // The drawing is kept with the board it was made for: for a moment after the board is switched
  // the page still has the previous board's, which must not be drawn or revealed on.
  const [loaded, setLoaded] = useState<{ board: string; drawing: Drawing } | null>(null);
  const drawing = loaded && loaded.board === name ? loaded.drawing : null;
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<Hit | null>(null);
  const [menu, setMenu] = useState<FlagMenu | null>(null);
  const [aim, setAim] = useState<Aim | null>(null);
  const [navPin, setNavPin] = useState<PinRef | null>(null);
  /** The part that was brought into view, which is ringed for a moment. */
  const [pulse, setPulse] = useState<{ ref: string; seq: number } | null>(null);
  /** Goes up when the drawing was painted again while the overview shows (it copies the drawing). */
  const [painted, setPainted] = useState(0);
  const [vp, setVp] = useState<{ k: number; fitK: number } | null>(null);
  const holder = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const zoom = useRef<PanZoomApi | null>(null);
  const applied = useRef(0);
  const handled = useRef(0);
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
      setLoaded(null);
      return;
    }
    let live = true;
    // The layout file is read with the drawing: both are of the analysis the page has (a write of the file is one).
    void loadHints(name);
    server
      .schematic(name)
      .then((text) => {
        if (!live) return;
        const d = loadDrawing(text);
        setLoaded(d ? { board: name, drawing: d } : null);
        setError(d ? null : 'The server sent no drawing.');
      })
      .catch((e: Error) => live && (setLoaded(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  const d = detail.value && detail.value.name === name ? detail.value : null;
  // The layout file (ADR 0061): what the server read of it for this board, the board's table in it, and the parts it can move.
  const hs = hintsState.value;
  const hintData = hs && hs.board === name ? hs.data : null;
  const definition = analysis.value?.boards.find((b) => b.name === name)?.definition ?? null;
  const refPaths = useMemo(() => new Map((d?.parts ?? []).map((p) => [p.refdes, p.path] as const)), [d]);
  const modules = useMemo(() => moduleNames((d?.parts ?? []).map((p) => p.path)), [d]);
  /** Where the view was, and a part that does not move, when a write of the file began: the drawing that comes back is put so that part
   * is where it was on the screen (the page's frame shifts when a block was the leftmost or topmost thing of its sheet). */
  const steady = useRef<{ view: { k: number; x: number; y: number }; ref: string; box: Box } | null>(null);
  const layoutCtx = (): Ctx | null => {
    const root = holder.current;
    if (!root || !hintData || !definition || !name) return null;
    return { board: name, definition, data: hintData, facts: readFacts(root, (r) => refPaths.get(r)), modules };
  };

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
  const selModule = selectedModule.value;
  const inModule = moduleParts.value;
  const netInfo = selNet ? d?.nets.find((n) => n.name === selNet) : undefined;
  const pickedBus = selectedBus.value;
  // The bus as the drawing has it (its sheet and box), when it is the picked one.
  const selBus = pickedBus && drawing ? drawing.index.buses.find((b) => b.path === pickedBus.path) : undefined;
  const carried = useMemo(() => {
    if (!drawing) return new Set<string>();
    // A bus is carried by every sheet that has a pin of any of its nets.
    const nets = selBus ? selBus.nets : selNet ? [selNet] : [];
    const out = new Set<string>();
    for (const n of nets) for (const s of netSheets(d?.nets.find((x) => x.name === n)?.pins ?? [], drawing.index.partSheet).keys()) out.add(s);
    return out;
  }, [selBus, selNet, d, drawing]);
  const rows = useMemo(
    () => ordered.map((s, i) => rowOf(s, i + 1, stats.get(s.name), carried.has(s.name), partsBySheet.get(s.name) ?? [])),
    [ordered, stats, carried, partsBySheet],
  );

  const current = name ? resolvePick(picks.value[name], names) : ALL;
  const overview = current === ALL && rows.length > 1;
  const open = (p: string) => name && choose(name, p);
  const modulePaths = useMemo(() => modulesOf(d).map((m) => m.path), [d]);
  /** A click on a card of the overview: the module the sheet draws is selected (the editor's shared
   * selection, so every view shows it) and its sheet opens. */
  const openCard = (sheet: string) => {
    const module = moduleOfSheet(sheet, modulePaths);
    if (module) selectModule(module);
    open(sheet);
  };

  /** A part's path from the board to its designator: what a relation note's words name. */
  const partRefs = useMemo(() => new Map((d?.parts ?? []).map((p) => [p.path, p.refdes] as const)), [d]);

  // Which sheet's drawing is in the page: the open one (all of them hide in the overview, which
  // draws them again from their groups).
  useLayoutEffect(() => {
    const svg = holder.current?.querySelector('svg.schematic');
    if (!svg) return;
    for (const slot of svg.querySelectorAll<SVGGElement>(':scope > g.sheet-slot')) {
      const sheet = slot.getAttribute('data-sheet');
      slot.style.display = sheet === '' || sheet === current ? '' : 'none';
    }
    // The overlay of a part (schematic/halo.ts) needs a part that is laid out: the sheet on show.
    addHalos(svg);
    // The notes take Tab and name their parts (the server writes only the first).
    prepareNotes(svg, partRefs);
  }, [drawing, current, partRefs]);

  // Paint what is selected, what the pointer is over, and what the cursor's line declares. With
  // a net selected nothing else lights, so the rest stays dimmed while the pointer moves.
  // A module that is picked marks its parts, except on its own sheet when that is the one on screen: every part would wear the halo.
  const shownModule = overview ? null : moduleOfSheet(current, modulePaths);
  const { picked, marked } = marksOf({ part: selPart, module: selModule, moduleParts: inModule, shownModule, cursor: cursorParts.value });
  useEffect(() => {
    const svg = holder.current?.querySelector('svg.schematic');
    if (!svg) return;
    addHalos(svg);
    svg.querySelectorAll('.on,.touches,.mark,.cur,.linked').forEach((e) => e.classList.remove('on', 'touches', 'mark', 'cur', 'linked'));
    // A module lights its parts and dims the rest; it does not light their nets (ground would light all).
    const focus: Hit | null = selBus
      ? { bus: selBus.path }
      : selNet
        ? { net: selNet }
        : (hover ?? (selModule || !marked.length ? null : { part: marked[0] }));
    svg.classList.toggle('has-on', !!focus || (!!selModule && marked.length > 0));
    if (focus) {
      // A bus lights all its members; a net, or a part's nets, light the line they run through.
      const nets =
        focus.bus != null
          ? litNets(drawing?.index.buses.find((b) => b.path === focus.bus))
          : focus.net != null
            ? new Set([focus.net])
            : (netsOfPart.get(focus.part!) ?? new Set<string>());
      for (const b of drawing?.index.buses ?? []) {
        if (b.nets.some((n) => nets.has(n))) svg.querySelector(`.bus-group[data-bus="${CSS.escape(b.path)}"]`)?.classList.add('on');
      }
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
    // The note of a part that is lit is lit with it (the rest dim with the drawing).
    const noteEls = [...svg.querySelectorAll('g.note')];
    const litRefs = new Set([...marked, ...(hover?.part ? [hover.part] : [])]);
    // A relation about several parts is lit when any of them is (`data-anchors`: every part the note names, `data-ref` is only the first).
    for (const i of litNotes(noteEls.map((n) => ({ anchors: (n.getAttribute('data-anchors') ?? n.getAttribute('data-ref') ?? '').split(' ').filter(Boolean) })), litRefs)) noteEls[i].classList.add('on');
    if (navPin && selNet) {
      svg.querySelector(`.pin[data-ref="${CSS.escape(navPin.ref)}"][data-pin="${CSS.escape(navPin.pin)}"]`)?.classList.add('cur');
    }
    if (overview) setPainted((n) => n + 1);
  }, [drawing, hover, selNet, selBus, selModule, marked.join(','), picked.join(','), netsOfPart, navPin, current, overview]);

  // Which parts the layout file has an entry for, which can be dragged, and which is the sheet's anchor (it cannot: its offset moves
  // the whole sheet and no distance, so the drawing says why).
  useLayoutEffect(() => {
    const svg = holder.current?.querySelector('svg.schematic');
    if (!svg) return;
    addHalos(svg);
    const ctx = layoutCtx();
    for (const g of svg.querySelectorAll('g.part[data-ref]')) {
      const f = ctx?.facts.get(g.getAttribute('data-ref')!);
      const state = ctx && f ? entryState(ctx, f) : { has: false, refused: false };
      const hint = hintRectOf(g);
      hint?.classList.toggle('on', state.has);
      hint?.classList.toggle('refused', state.refused);
      const refusal = ctx && f ? dragRefusal(ctx, f.ref) : 'not drawn';
      g.classList.toggle('movable', refusal === null);
      g.classList.toggle('anchor', !!f?.anchor);
      const title = g.querySelector(':scope > title');
      if (title && f?.anchor && !title.hasAttribute('data-anchor-note')) {
        title.textContent = `${title.textContent ?? ''}\nThe sheet's anchor: drag a neighbour.`;
        title.setAttribute('data-anchor-note', '');
      }
    }
  }, [drawing, hintData, current, refPaths, definition]);

  // A drawing that comes back from a write of the layout file: the part that did not move is where it was on the screen, and the zoom
  // is what it was.
  useLayoutEffect(() => {
    const st = steady.current;
    const z = zoom.current;
    if (!st || !drawing || !z) return;
    if (!writing() && Date.now() - lastWriteAt() > 4000) {
      steady.current = null;
      return;
    }
    const b = drawing.index.partBox.get(st.ref);
    if (!b) return;
    const dx = b.x - st.box.x;
    const dy = b.y - st.box.y;
    const kept = { k: st.view.k, x: st.view.x - dx * st.view.k, y: st.view.y - dy * st.view.k };
    z.set(kept);
    steady.current = { view: kept, ref: st.ref, box: b };
  }, [drawing]);

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
  /** Pick a bus on the sheet that draws it, and bring its line into view. */
  const goBus = (bus: Bus) => {
    selectBus(bus);
    if (bus.sheet !== '' && bus.sheet !== current) open(bus.sheet);
    aimAt(bus.sheet || current, bus.box);
  };
  /** A bundle flag was clicked: the module that carries the same member nets opens with them lit
   * (the bus drawn there is picked, else the first net they share); several modules offer the same
   * small menu as for a net's flag. */
  const followBusFlag = (bus: Bus, x: number, y: number) => {
    if (!drawing) return;
    const targets = busTargets(bus, d?.nets ?? [], drawing.index.partSheet, names, drawing.index.buses);
    const there = (t: (typeof targets)[number]) => {
      if (t.bus) goBus(t.bus);
      else {
        selectNet(t.nets[0]);
        open(t.sheet);
        aimAt(t.sheet, netBox(t.nets[0], t.sheet));
      }
    };
    if (targets.length === 1) there(targets[0]);
    else if (targets.length > 1) {
      const r = stage.current?.getBoundingClientRect();
      setMenu({
        x: x - (r?.left ?? 0),
        y: y - (r?.top ?? 0),
        head: `${busName(bus.path)} also on`,
        items: targets.map((t) => ({
          key: t.sheet,
          name: rows.find((r) => r.sheet.name === t.sheet)?.name ?? t.sheet,
          detail: `${t.carried} of ${bus.nets.length} nets`,
          go: () => there(t),
        })),
      });
    }
  };
  /** Show a part: its sheet opens, and the view moves only if it must (`revealPart`), with a ring on the part. */
  const showPart = (ref: string) => {
    const sheet = drawing?.index.partSheet.get(ref);
    const own = drawing?.index.partBox.get(ref);
    if (sheet == null || !own || !drawing) return;
    if (sheet !== '' && sheet !== current) open(sheet);
    const context = neighbourBox(ref, d?.nets ?? [], drawing.index.partSheet, drawing.index.partBox) ?? own;
    setAim({ seq: ++seq.current, sheet: sheet || current, box: own, part: { ref, context } });
  };
  const goPart = (ref: string) => {
    selectPart(ref);
    showPart(ref);
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
      setMenu({
        x: x - (r?.left ?? 0),
        y: y - (r?.top ?? 0),
        head: `${net} also on`,
        items: targets.map((t) => ({
          key: t.sheet,
          name: rows.find((r) => r.sheet.name === t.sheet)?.name ?? t.sheet,
          detail: `${t.pins
            .slice(0, 3)
            .map((p) => `${p.refdes}.${p.pin}`)
            .join(', ')}${t.pins.length > 3 ? ` +${t.pins.length - 3}` : ''}`,
          go: () => goNetOn(t.sheet, net),
        })),
      });
    }
  };
  /** Zoom to the selection: a net's box, or a part with the parts it is wired to. */
  const frameSelection = () => {
    if (selBus) {
      aimAt(selBus.sheet || current, selBus.box);
    } else if (selNet) {
      const b = netBox(selNet, current);
      if (b) aimAt(current, b);
    } else if (selPart && drawing) {
      const b = neighbourBox(selPart, d?.nets ?? [], drawing.index.partSheet, drawing.index.partBox) ?? drawing.index.partBox.get(selPart);
      if (b) aimAt(drawing.index.partSheet.get(selPart) || current, b);
    }
  };

  // A pick made in another view and asked to be shown (go to anything, a problem, the chip): open
  // the sheet that has it, and move the view only as far as it takes to see it.
  const ask = revealRequest.value;
  useEffect(() => {
    if (!ask || handled.current === ask.nonce) return;
    // It waits for the drawing and the detail of the board it was asked on; it is dropped when
    // the board in front is another one by now.
    const step = revealStep(ask, name, loaded?.board ?? null, detail.value?.name ?? null);
    if (step === 'wait') return;
    handled.current = ask.nonce;
    if (step === 'drop' || !drawing || !d) return;
    // Something else was picked since: that is not asked for any more.
    if (!sameSelection(selection.peek(), ask.sel)) return;
    const { kind, key } = ask.sel;
    if (kind === 'part') {
      const ref = d.parts.find((p) => p.path === key)?.refdes;
      if (ref) showPart(ref);
    } else if (kind === 'net') {
      // A bus is picked as its first net: the line is what is brought into view.
      const bus = selectedBus.peek() ? drawing.index.buses.find((b) => b.path === selectedBus.peek()!.path) : undefined;
      if (bus) {
        if (bus.sheet !== '' && bus.sheet !== current) open(bus.sheet);
        aimAt(bus.sheet || current, bus.box, true);
        return;
      }
      const places = drawing.index.netPlaces.get(key);
      if (!places) return;
      const sheet = places.has(current) ? current : (names.find((n) => places.has(n)) ?? current);
      if (sheet !== current) open(sheet);
      aimAt(sheet, netBox(key, sheet), true);
    } else {
      // A module: the sheet that has its parts; a module inside a sheet is framed, a whole sheet is not moved.
      const refs = d.parts.filter((p) => p.path.startsWith(`${key}.`)).map((p) => p.refdes);
      const sheet = refs.map((r) => drawing.index.partSheet.get(r)).find((x) => x);
      if (!sheet) return;
      if (sheet !== current) open(sheet);
      if (!key.includes('.')) return;
      const boxes = refs.filter((r) => drawing.index.partSheet.get(r) === sheet).map((r) => drawing.index.partBox.get(r)!).filter(Boolean);
      if (boxes.length === 0) return;
      const x0 = Math.min(...boxes.map((b) => b.x));
      const y0 = Math.min(...boxes.map((b) => b.y));
      aimAt(sheet, { x: x0, y: y0, w: Math.max(...boxes.map((b) => b.x + b.w)) - x0, h: Math.max(...boxes.map((b) => b.y + b.h)) - y0 });
    }
  }, [ask?.nonce, drawing, d, name]);

  // Apply the aim once the sheet it is for is the one shown and the canvas has been placed.
  useLayoutEffect(() => {
    const z = zoom.current;
    if (!aim || !z || applied.current === aim.seq || aim.sheet !== current || overview) return;
    const { w, h } = z.size();
    if (!w || !h) return;
    applied.current = aim.seq;
    const view = z.view();
    if (aim.part) {
      const r = revealPart(w, h, aim.box, aim.part.context, view);
      if (!r.kept) z.set(r.view);
      setPulse({ ref: aim.part.ref, seq: aim.seq });
      return;
    }
    z.set(
      aim.reveal
        ? revealBox(w, h, aim.box, view, MIN_LEGIBLE_K, 70)
        : centreOnBox(w, h, aim.box, view, { minK: MIN_LEGIBLE_K, fitBox: true }),
    );
  }, [aim, current, overview, drawing, viewWidth]);

  // The ring on a part that was brought into view goes after a moment.
  useEffect(() => {
    if (!pulse) return;
    const t = setTimeout(() => setPulse(null), 1700);
    return () => clearTimeout(t);
  }, [pulse?.seq]);

  // ---- arranging by hand (ADR 0061, stage 4) ----
  /** Make the view steady across the drawing a write brings back: a part that does not move stays where it is on the screen. */
  const holdView = (ctx: Ctx, ref: string) => {
    const f = ctx.facts.get(ref);
    const z = zoom.current;
    if (!f || !z || !drawing) return;
    const fixed = [...ctx.facts.values()].find((o) => o.slot === f.slot && o.ref !== ref && (o.anchor || o.address.sheet === f.address.sheet));
    const box = fixed && drawing.index.partBox.get(fixed.ref);
    steady.current = fixed && box ? { view: z.view(), ref: fixed.ref, box } : null;
  };
  const submitFor = (ctx: Ctx, ref: string, edit: ReturnType<typeof dropEdit>) => {
    if (!edit) return;
    holdView(ctx, ref);
    submitEdit(edit);
  };
  const startDrag = (e: PointerEvent) => {
    if (e.button !== 0 || overview) return;
    const el = e.target instanceof Element ? e.target : null;
    const svg = holder.current?.querySelector('svg.schematic');
    const z = zoom.current;
    if (!el || !svg || !z) return;
    const ctx = layoutCtx();
    if (!ctx) return;
    const part = el.closest('g.part[data-ref]');
    const noteEl = part ? null : el.closest('g.note[data-n]');
    let moving: Element[];
    let label: string;
    let drop: (r: { sx: number; sy: number; pin: boolean }) => void;
    if (part) {
      const ref = part.getAttribute('data-ref')!;
      // The sheet's anchor, or a file with a fault: the press pans, as it did before.
      if (dragRefusal(ctx, ref)) return;
      moving = [...svg.querySelectorAll(`g.part[data-ref="${CSS.escape(ref)}"]`)];
      label = ref;
      drop = (r) => {
        const now = layoutCtx();
        if (!now) return;
        // The dragged part is the pick, so the keys that follow are about it.
        selectPart(ref);
        submitFor(now, ref, dropEdit(now, ref, r.sx, r.sy, r.pin));
      };
    } else if (noteEl) {
      const note = readNotes(svg).get(Number(noteEl.getAttribute('data-n')));
      if (!note || !hintData || hintData.errors.length > 0) return;
      moving = [noteEl];
      label = 'note';
      drop = (r) => {
        const now = layoutCtx();
        const edit = now && noteDropEdit(now, note, r.sx, r.sy);
        if (now && edit) {
          const first = [...now.facts.keys()][0];
          if (first) holdView(now, first);
          submitEdit(edit);
        }
      };
    } else return;
    // A press that moves is a drag: it is not the canvas's pan. One that does not move is a click, as it was.
    e.stopPropagation();
    (el.closest('.panzoom') as HTMLElement | null)?.focus({ preventScroll: true });
    beginDrag({ svg, moving, x: e.clientX, y: e.clientY, k: z.view().k, label, onDrop: drop });
  };
  const selectedFacts = (): { ctx: Ctx; f: PartFacts } | null => {
    const ctx = layoutCtx();
    const f = ctx && selPart ? ctx.facts.get(selPart) : undefined;
    return ctx && f ? { ctx, f } : null;
  };
  const here = (f: PartFacts) => f.slot === current || f.slot === '' || overview;
  const sheetTitle = () => rows.find((r) => r.sheet.name === current)?.name ?? current;
  const entries = () => {
    const t = hintData ? tablesOf(hintData.boards) : {};
    return t;
  };
  const layoutActions = {
    canMove: () => {
      const s = selectedFacts();
      return !!s && !overview && here(s.f) && dragRefusal(s.ctx, s.f.ref) === null;
    },
    canTurn: () => {
      const s = selectedFacts();
      return !!s && !overview && here(s.f) && !!s.f.orient && !!hintData && hintData.errors.length === 0;
    },
    nudge: (sx: number, sy: number) => {
      const s = selectedFacts();
      if (s) submitFor(s.ctx, s.f.ref, nudgeEdit(s.ctx, s.f.ref, sx, sy));
    },
    turn: (dir: 1 | -1) => {
      const s = selectedFacts();
      if (s) submitFor(s.ctx, s.f.ref, turnEdit(s.ctx, s.f.ref, dir));
    },
    mirror: () => {
      const s = selectedFacts();
      if (s) submitFor(s.ctx, s.f.ref, mirrorEdit(s.ctx, s.f.ref));
    },
    hinted: () => {
      const s = selectedFacts();
      return !!s && hasEntry(s.ctx, s.f.ref);
    },
    clearPart: () => {
      const s = selectedFacts();
      if (s) submitFor(s.ctx, s.f.ref, clearPartEdit(s.ctx, s.f.ref));
    },
    sheetEntries: () => (overview ? 0 : countIn(entries(), definition ?? '', hintSheet(current, modules))),
    allEntries: () => countAll(entries()),
    clearSheet: () => {
      const ctx = layoutCtx();
      const edit = ctx && clearSheetEdit(ctx, hintSheet(current, modules));
      if (!ctx || !edit) return;
      const n = countIn(entries(), ctx.definition, hintSheet(current, modules));
      confirmWith({
        title: `Clear the layout of ${sheetTitle()}?`,
        lines: [`${n === 1 ? 'The 1 entry' : `The ${n} entries`} of this module in ${ctx.data.file} will be removed, and what they moved goes back where the drawing puts it.`, 'The file is the only place they are kept; git has them if the file is committed.'],
        yes: 'Clear the module',
        run: () => submitEdit(edit),
      });
    },
    clearAll: () => {
      const ctx = layoutCtx();
      const edit = ctx && clearAllEdit(ctx);
      if (!ctx || !edit) return;
      const n = countAll(entries());
      confirmWith({
        title: 'Clear every layout entry?',
        lines: [`${n === 1 ? 'The 1 entry' : `All ${n} entries`} in ${ctx.data.file}, for every board and module in it, will be removed. The file is kept, with nothing in it.`, 'The file is the only place they are kept; git has them if the file is committed.'],
        yes: 'Clear the file',
        run: () => submitEdit(edit),
      });
    },
  };

  // The keys of this view are commands (`commandset.ts`, group Schematic): they act through these.
  // The handle is made again on each render, so what they do is always about what is shown.
  const live = useRef<SchematicActions | null>(null);
  live.current = {
    overview: () => overview,
    sheets: () => rows.length,
    sheet: (dir) => {
      if (name && rows.length > 1) open(stepPick(names, current, dir));
    },
    fit: () => zoom.current?.fit(),
    fitWidth: () => zoom.current?.fitWidth(),
    actual: () => zoom.current?.zoomTo(K100),
    zoomSelection: frameSelection,
    zoom: (factor) => zoom.current?.zoomTo(zoom.current.view().k * factor),
    pan: (dx, dy) => zoom.current?.panBy(dx, dy),
    pin: stepPins,
    source: () => selPart && goToSource(selPart),
    layout: layoutActions,
  };
  useEffect(() => {
    const now = () => live.current!;
    const stable: SchematicActions = {
      overview: () => now().overview(),
      sheets: () => now().sheets(),
      sheet: (dir) => now().sheet(dir),
      fit: () => now().fit(),
      fitWidth: () => now().fitWidth(),
      actual: () => now().actual(),
      zoomSelection: () => now().zoomSelection(),
      zoom: (f) => now().zoom(f),
      pan: (dx, dy) => now().pan(dx, dy),
      pin: (dir) => now().pin(dir),
      source: () => now().source(),
      layout: {
        canMove: () => now().layout.canMove(),
        canTurn: () => now().layout.canTurn(),
        nudge: (sx, sy) => now().layout.nudge(sx, sy),
        turn: (dir) => now().layout.turn(dir),
        mirror: () => now().layout.mirror(),
        hinted: () => now().layout.hinted(),
        clearPart: () => now().layout.clearPart(),
        sheetEntries: () => now().layout.sheetEntries(),
        allEntries: () => now().layout.allEntries(),
        clearSheet: () => now().layout.clearSheet(),
        clearAll: () => now().layout.clearAll(),
      },
    };
    schematicActions.current = stable;
    return () => {
      if (schematicActions.current === stable) schematicActions.current = null;
    };
  }, []);
  // What the top bar's chip says of the selected net, which only the drawing knows.
  useEffect(() => {
    // A bus has no walk of pins: the chip says the bus, and the inspector lists its members' pins.
    netFacts.value =
      selNet && netInfo && drawing && !selBus ? { net: selNet, text: netSummary(netInfo, drawing.index.partSheet).text, canStep: walk.length > 1 } : null;
  }, [selNet, netInfo, drawing, walk.length, selBus]);
  // Go to anything lists the buses the drawing has.
  useEffect(() => {
    drawnBuses.value = { board: loaded?.board ?? '', buses: drawing?.index.buses ?? [] };
  }, [drawing, loaded?.board]);
  useEffect(() => () => void (netFacts.value = null), []);
  // Esc closes the flag menu first (`data-esc-closes` holds the Clear command back); clearing the
  // selection is that command's.
  useEffect(() => {
    const esc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || !menu) return;
      setMenu(null);
      e.preventDefault();
    };
    window.addEventListener('keydown', esc);
    return () => window.removeEventListener('keydown', esc);
  }, [menu]);

  // A menu of sheets closes on a click anywhere else.
  useEffect(() => {
    if (!menu) return;
    const away = (e: PointerEvent) => {
      if (!(e.target as Element).closest('.flag-menu')) setMenu(null);
    };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, [menu]);

  if (!name) return <NoBoard what="draw" />;
  if (d && d.parts.length === 0) {
    return (
      <SchematicEmpty
        title={`${d.name} has no parts yet`}
        text="Declare a part in the source and it is drawn here, with its wires and nets."
        action="Open the source"
      />
    );
  }
  if (error)
    return (
      <EmptyState
        icon="error"
        tone="bad"
        title="The schematic could not be drawn"
        help="The server answered with an error. Fix the source, or try again."
        actions={
          <button class="tool" onClick={() => revealSection('source')}>
            Open the source
          </button>
        }
      >
        <div class="empty-error">
          <span class="text">{error}</span>
        </div>
      </EmptyState>
    );
  if (!drawing) return <Skeleton />;

  const sheet = drawing.sheets.find((s) => s.name === current);
  const box = sheet ? around(sheet) : undefined;
  const page: Box = box ?? { x: 0, y: 0, w: drawing.width, h: drawing.height };
  const side = detailsOpen(viewWidth);
  const nav = rows.length > 1 && navOpen(viewWidth);
  const held = !overview && vp ? isHeldForText(vp.fitK, vp.k) : false;
  const total = { parts: d?.parts.length ?? rows.reduce((n, r) => n + (r.stats?.parts ?? 0), 0) };

  return (
    <div class="schematic-view" ref={root}>
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
        help={() => toggleCheatSheet(true, 'Schematic')}
      />
      <div class="schematic-body">
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
            class={`schematic-host ${overview ? 'is-hidden' : ''} ${redrawing.value?.board === name ? 'redrawing' : ''}`}
            scale100={K100}
            presets
            arrowKeys
            // The keys are the Schematic group's commands (commandset.ts), not the canvas's own.
            keys={false}
            label="Schematic module"
            range={zoomRange}
            placeView={(w, h) => placeSheet(w, h, page).view}
            onViewport={(v) => setVp((o) => (o && o.k === v.k && o.fitK === v.fitK ? o : { k: v.k, fitK: v.fitK }))}
            onApi={(a) => (zoom.current = a)}
          >
            <div
              ref={holder}
              class="hit-target"
              dangerouslySetInnerHTML={{ __html: drawing.html }}
              onPointerDown={(e) => startDrag(e)}
              onPointerOver={(e) => {
                // A section that was away when the drawing came has no halos yet.
                if (holder.current) addHalos(holder.current);
                const hit = pickAt(e.target);
                // A note lifts by its own CSS and lights nothing else.
                setHover(hit?.note != null ? null : hit);
                if (hit?.note != null) return hoverLeave();
                if (e.pointerType === 'touch') return;
                // The same thing is lit in every other view (hover.ts): a part, a net, a bus's members.
                const bus = hit?.bus ? drawing.index.buses.find((b) => b.path === hit.bus) : undefined;
                const shared = hit?.net ? { kind: 'net' as const, key: hit.net } : bus ? { kind: 'bus' as const, key: bus.path, nets: bus.nets } : hit?.part ? hoverOfRefs(detail.peek(), [hit.part]) : null;
                if (shared) hoverEnter(shared);
                else hoverLeave();
              }}
              onPointerLeave={() => {
                setHover(null);
                hoverLeave();
              }}
              onKeyDown={(e) => {
                // A note is a button for the keyboard: Enter or Space opens the source at its line, as a click does.
                const note = e.target instanceof Element ? e.target.closest('g.note') : null;
                if (!note || e.target !== note || (e.key !== 'Enter' && e.key !== ' ')) return;
                e.preventDefault();
                const at = parseNoteAt(note.getAttribute('data-at'));
                if (at) void goTo(noteLocation(at));
              }}
              onFocusIn={(e) => {
                // The parts a note is about light in every view while it has the focus, as they do under the pointer.
                const note = e.target instanceof Element ? e.target.closest('g.note') : null;
                const refs = (note?.getAttribute('data-anchors') ?? '').split(' ').filter(Boolean);
                const shared = refs.length ? hoverOfRefs(detail.peek(), refs) : null;
                if (shared) hoverEnter(shared);
              }}
              onFocusOut={(e) => {
                if (e.target instanceof Element && e.target.closest('g.note')) hoverLeave();
              }}
              onClick={(e) => {
                setMenu(null);
                const hit = pickAt(e.target);
                if (hit?.note != null) {
                  // Not a part: nothing is selected. The source opens at the line the note is written at, as a row of the problems dock does.
                  const at = parseNoteAt(hit.note);
                  if (at) void goTo(noteLocation(at));
                  return;
                }
                const bus = hit?.bus ? drawing.index.buses.find((b) => b.path === hit.bus) : undefined;
                if (hit?.net) {
                  selectNet(hit.net);
                  if (hit.flag) followFlag(hit.net, e.clientX, e.clientY);
                } else if (bus) {
                  selectBus(bus);
                  if (hit?.flag) followBusFlag(bus, e.clientX, e.clientY);
                } else if (hit?.part) selectPart(hit.part);
                else clearSelection();
              }}
              onDblClick={(e) => {
                const hit = pickAt(e.target);
                if (hit?.part) goToSource(hit.part);
              }}
            />
            {pulse && !overview && drawing.index.partBox.get(pulse.ref) && <PulseRing key={pulse.seq} box={drawing.index.partBox.get(pulse.ref)!} />}
          </PanZoom>
          {overview && (
            <SheetOverview
              drawing={drawing}
              rows={rows}
              open={openCard}
              source={() => holder.current}
              lit={!!selNet || !!selPart}
              stamp={painted}
            />
          )}
          <RedrawChip board={name} />
          <DragReadout />
          {held && (
            <div class="hold-chip no-pan" role="status">
              <span>Zoomed in to keep the text readable</span>
              <button onClick={() => zoom.current?.fit()}>Fit</button>
            </div>
          )}
          {menu && (
            <div class="flag-menu no-pan" role="menu" data-esc-closes style={{ left: Math.max(8, menu.x - 20), top: Math.max(8, menu.y + 12) }}>
              <div class="menu-head">{menu.head}</div>
              {menu.items.map((t) => (
                <button
                  key={t.key}
                  role="menuitem"
                  class="menu-item"
                  onClick={() => {
                    setMenu(null);
                    t.go();
                  }}
                >
                  <span class="mi-name">{t.name}</span>
                  <span class="mi-pins">{t.detail}</span>
                </button>
              ))}
            </div>
          )}
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
            goBus={goBus}
            navPin={navPin}
            stepPins={stepPins}
            hover={hover}
          />
        )}
      </div>
    </div>
  );
}

/** What a drag says beside the pointer: the offset, and that it is a pin. */
function DragReadout() {
  const r = dragReadout.value;
  return r ? (
    <div class="drag-readout" role="status" data-testid="drag-readout" style={{ left: r.x, top: r.y }}>
      {r.text}
    </div>
  ) : null;
}

/** While the layout file is written the whole project is analysed again: the drawing says so, and the analysis can be stopped. */
function RedrawChip({ board }: { board: string }) {
  const r = redrawing.value;
  if (!r || r.board !== board) return null;
  return (
    <div class="redraw-chip no-pan" role="status" data-testid="redrawing">
      <span>
        Redrawing the schematic: {r.label}
        {r.queued ? ' (one more is waiting)' : ''}
      </span>
      <button onClick={() => void cancelAnalysis()}>Cancel</button>
    </div>
  );
}

/** A ring round the part that was brought into view; it fades, and is gone after a moment. */
function PulseRing({ box }: { box: Box }) {
  const grow = 14;
  return (
    <div
      class="pulse-ring"
      aria-hidden="true"
      style={{ left: box.x - grow, top: box.y - grow, width: box.w + 2 * grow, height: box.h + 2 * grow }}
    />
  );
}

function SchematicEmpty({ title, text, action }: { title: string; text: string; action: string }) {
  return (
    <EmptyState
      icon="schematic"
      title={title}
      help={text}
      actions={
        <button class="tool" onClick={() => revealSection('source')}>
          {action}
        </button>
      }
    />
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
