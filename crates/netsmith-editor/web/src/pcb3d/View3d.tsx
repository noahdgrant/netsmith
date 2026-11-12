// The PCB view in 3D (ADR 0105): the board's scene as the server writes it (`GET /api/pcb/3d`, the
// file `netsmith board3d --glb` writes), drawn by three.js. It is read-only: a part is picked and
// hovered here through the page's shared selection and hover, and moved in the 2D view. A part
// with no known height is a flat hatched outline, never a box of a guessed height, and the chip
// says how many parts have a known height. This component and three.js are a chunk of their own,
// loaded the first time 3D opens.

import { useEffect, useRef, useState } from 'preact/hooks';

import type { ViewChange } from '../api';

import { focusedSection } from '../commandset';
import { CopyButton, EmptyState, Popover, Segmented, Spinner, copyText } from '../components/ui';
import { notify } from '../feedback/state';
import { theme as pageTheme } from '../theme';
import { Icon } from '../components/Icons';
import { type Hover, hover as sharedHover, hoverEnter, hoverLeave, resolveHover, sameHover } from '../hover';
import { scene as scene2d, select, selection as pcbSelection } from '../pcbstore';
import { partPathsOf, revealRequest, selection as sharedSelection } from '../selection';
import { reducedMotion, settings } from '../settings';
import { analysis, detail, pcbStamp, pickNet, pickPart } from '../store';
import { InspectBar, InspectLegend, PreviewLabel } from './InspectBar';
import { KINDS, MARGIN_TOKENS, findingTargets, litNets } from './inspect';
import { type Ids, litIds, planChange, sectionText } from './control';
import { colourBy, focus3d, heightPreview, xray } from './inspectMode';
import { MeasurePanel, Overlay3d, type Readout, SectionPanel } from './Tools3d';
import { type Axis, type Measurement, type Snap, axisRange, defaultCut, faceZ, nearestView, partReadout, renderCommand } from './tools';
import { type LimitInfo, type PartInfo3d, coverageLine, limitTip, partTip, provenanceCounts, unknownParts } from './model';
import { view3d } from './mode';
import { LOOKS, type Role, TOKENS } from './palette';
import { PRESETS, type Preset } from './presets';
import { type Colours, type Hit, type PadPoint, Viewer } from './viewer';
import './view3d.css';

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** A token's colour as `#rrggbb`, whatever CSS it is written in (a `var()`, a `color-mix()`). */
function resolver(probe: HTMLElement): (token: string) => string {
  const c = document.createElement('canvas');
  c.width = c.height = 1;
  const g = c.getContext('2d', { willReadFrequently: true })!;
  return (token: string) => {
    probe.style.color = `var(${token})`;
    const css = getComputedStyle(probe).color;
    g.clearRect(0, 0, 1, 1);
    g.fillStyle = 'black';
    g.fillStyle = css;
    g.fillRect(0, 0, 1, 1);
    const [r, gr, b] = g.getImageData(0, 0, 1, 1).data;
    return `#${((1 << 24) | (r << 16) | (gr << 8) | b).toString(16).slice(1)}`;
  };
}

function readColours(): Colours {
  const probe = document.createElement('span');
  probe.style.display = 'none';
  document.body.appendChild(probe);
  const get = resolver(probe);
  const out: Partial<Colours> = {};
  for (const role of Object.keys(LOOKS) as Role[]) out[role] = get(LOOKS[role].token);
  out.background = get(TOKENS.background);
  out.edge = get(TOKENS.edge);
  out.select = get(TOKENS.select);
  out.hover = get(TOKENS.hover);
  out.kinds = Object.fromEntries(KINDS.map((k) => [k.id, get(k.token)])) as Colours['kinds'];
  out.margin = { easy: get(MARGIN_TOKENS.easy), tight: get(MARGIN_TOKENS.tight), over: get(MARGIN_TOKENS.over), none: get(MARGIN_TOKENS.none) };
  out.preview = get('--pcb3d-preview');
  probe.remove();
  return out as Colours;
}

interface Tip {
  x: number;
  y: number;
  title: string;
  lines: string[];
}

type Status = { state: 'loading' } | { state: 'ready' } | { state: 'error'; message: string; webgl?: boolean };

const typing = (t: EventTarget | null) => t instanceof Element && !!t.closest('input, textarea, select, [contenteditable="true"], .cm-editor');

export function View3d({ board }: { board: string }) {
  const host = useRef<HTMLDivElement>(null);
  const viewer = useRef<Viewer | null>(null);
  const [status, setStatus] = useState<Status>({ state: 'loading' });
  const [parts, setParts] = useState<PartInfo3d[]>([]);
  const [limits, setLimits] = useState<LimitInfo[]>([]);
  const [coverage, setCoverage] = useState<{ known: number; total: number } | null>(null);
  const [tip, setTip] = useState<Tip | null>(null);
  const [preset, setPreset] = useState<Preset | null>('iso');
  const [ortho, setOrtho] = useState(false);
  const [listOpen, setListOpen] = useState(false);
  const [previewed, setPreviewed] = useState(0);
  const pointed = useRef<Hover | null>(null);
  const focusSeen = useRef(focus3d.peek()?.nonce ?? 0);
  // The tools (tools.ts): the measure tool and its measurements, the section plane, the stackup apart.
  const [measuring, setMeasuring] = useState(false);
  const [measures, setMeasures] = useState<Measurement[]>([]);
  const [pending, setPending] = useState<Snap | null>(null);
  const [snapAt, setSnapAt] = useState<Snap | null>(null);
  const [cut, setCut] = useState<{ axis: Axis; at: number } | null>(null);
  const [exploded, setExploded] = useState(false);
  const [canExplode, setCanExplode] = useState(false);
  const nextId = useRef(1);
  const boardRef = useRef(board);
  boardRef.current = board;
  const tools = useRef({ measuring, pending, cut });
  tools.current = { measuring, pending, cut };
  // For `/api/view` (agent.ts): the board whose scene is drawn, and why the view cannot draw.
  const loadedBoard = useRef<string | null>(null);
  const failure = useRef<string | null>(null);

  const stamp = scene2d.value?.board === board ? scene2d.value.layoutStamp : null;
  const routed = pcbStamp.value;
  const version = analysis.value?.version;

  // The drawing: made once, given back when the view goes.
  useEffect(() => {
    const el = host.current!;
    let v: Viewer;
    try {
      v = new Viewer(el, {
        onFirstFrame: () => {
          el.dataset.ready = '1';
          setStatus({ state: 'ready' });
        },
        onCamera: () => {
          setPreset(v.currentPreset());
        },
      });
    } catch (e) {
      failure.current = e instanceof Error ? e.message : String(e);
      setStatus({ state: 'error', webgl: true, message: failure.current });
      return;
    }
    viewer.current = v;
    v.still = reducedMotion();
    v.setColours(readColours());
    const size = () => v.resize(el.clientWidth, el.clientHeight);
    size();
    const watch = new ResizeObserver(size);
    watch.observe(el);
    const lost = (e: Event) => {
      e.preventDefault();
      failure.current = 'the graphics context was lost';
      setStatus({ state: 'error', webgl: true, message: 'The graphics context was lost' });
    };
    v.canvas.addEventListener('webglcontextlost', lost);
    // For the recipes: where a part is on screen, and what the view drew, asked of the view itself.
    (el as unknown as { view3d: unknown }).view3d = {
      project: (path: string) => v.project(path),
      sample: () => v.sample(),
      toScreen: (p: [number, number, number]) => {
        const s = v.toScreen(p);
        const r = v.canvas.getBoundingClientRect();
        return { x: Math.round(r.left + s.x), y: Math.round(r.top + s.y), front: s.front };
      },
      snap: (x: number, y: number) => v.snap(x, y),
      slots: () => v.slots(),
      exploded: () => v.isExploded(),
      command: () => commandText(),
      board: () => v.board,
      colourAt: (x: number, y: number) => v.colourAt(x, y),
      parts: () => v.partList(),
      preset: () => v.currentPreset(),
      ortho: () => v.isOrtho(),
      nets: () => v.netList(),
      previewCount: () => v.previewCount(),
      focused: () => v.focusedList(),
      litReport: () => v.litReport(),
      caps: () => v.capList(),
    };
    view3d.current = {
      preset: (p) => v.preset(p),
      flip: () => v.flip(),
      toggleProjection: () => {
        v.toggleProjection();
        setOrtho(v.isOrtho());
      },
      fit: () => v.fit(),
      toggleMeasure: () => toggleMeasure(),
      clearMeasures: () => {
        setMeasures([]);
        setPending(null);
      },
      toggleSection: () => changeCut(tools.current.cut ? null : (lastCut.current ?? startCut('x'))),
      section: (axis) => changeCut(tools.current.cut?.axis === axis ? { axis, at: tools.current.cut.at } : startCut(axis)),
      toggleExplode: () => {
        if (!v.canExplode()) return;
        const on = !v.isExploded();
        v.setExploded(on);
        setExploded(on);
        if (on) setPending(null);
      },
      copyCommand: () => void copyCommand(),
      agent: {
        loaded: () => (v.board && el.dataset.ready ? loadedBoard.current : null),
        failed: () => failure.current,
        apply: (change) => applyChange(v, change),
        state: () => {
          // The page's own isometric view (the key I) is a few degrees from render's: it is `iso` too.
          const near = nearestView(v.towardEye());
          return {
            view: near.exact ? near.view : v.currentPreset() === 'iso' ? 'iso' : undefined,
            camera: v.cameraPlace(),
            ortho: v.isOrtho(),
            section: sectionText(tools.current.cut) ?? undefined,
            exploded: v.isExploded(),
            command: commandText(),
            limits: litLimits(v),
          };
        },
        picture: () => v.picture(),
      },
    };
    return () => {
      view3d.current = null;
      watch.disconnect();
      v.canvas.removeEventListener('webglcontextlost', lost);
      v.dispose();
      viewer.current = null;
      delete el.dataset.ready;
    };
  }, []);

  // The scene: fetched again when the board is edited, routed or built again; the camera stays.
  useEffect(() => {
    const v = viewer.current;
    if (!v) return;
    const ask = new AbortController();
    void (async () => {
      try {
        const r = await fetch(`/api/pcb/3d?board=${encodeURIComponent(board)}`, { cache: 'no-store', signal: ask.signal });
        if (!r.ok) throw new Error((await r.text()) || `the server answered ${r.status}`);
        const bytes = await r.arrayBuffer();
        if (ask.signal.aborted) return;
        await v.load(bytes);
        setParts(v.partList());
        setLimits(v.limitList());
        setCoverage(v.board?.coverage ?? null);
        setCanExplode(v.canExplode());
        loadedBoard.current = board;
        failure.current = null;
        v.setPads(padsOf());
        v.highlight(pcbSelection.peek(), []);
        if (host.current) host.current.dataset.stamp = r.headers.get('X-Netsmith-Layout-Stamp') ?? '';
      } catch (e) {
        if (!ask.signal.aborted) {
          failure.current = e instanceof Error ? e.message : String(e);
          setStatus({ state: 'error', message: failure.current });
        }
      }
    })();
    return () => ask.abort();
  }, [board, stamp, routed, version]);

  // The theme's colours, again when the theme changes; reduced motion when the setting does.
  // The page's theme is the `data-theme` of <html>: watched, so any way it changes repaints the view.
  useEffect(() => {
    const again = () => viewer.current?.setColours(readColours());
    const watch = new MutationObserver(again);
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    return () => watch.disconnect();
  }, []);
  const motion = settings.value.motion;
  useEffect(() => {
    if (viewer.current) viewer.current.still = reducedMotion();
  }, [motion]);

  // The selection and the shared hover, from every view: the parts, and the nets (a row of the
  // nets list, a pad here) lit on every layer.
  const picked = pcbSelection.value;
  const h = sharedHover.value;
  const d = detail.value;
  const shared = sharedSelection.value;
  const pickedNet = shared?.kind === 'net' ? shared.key : null;
  useEffect(() => {
    const lit = h ? resolveHover(d, h) : null;
    viewer.current?.highlight(picked, lit ? [...lit.paths] : []);
    viewer.current?.setLook({ nets: litNets(pickedNet, lit ? lit.nets : []) });
  }, [picked, h, d, parts, pickedNet]);

  // The pads' nets, from the 2D scene: a pad's barrel lights with its net.
  const sc2 = scene2d.value;
  useEffect(() => {
    if (!sc2 || sc2.board !== board) return;
    const nets = new Map<string, string>();
    for (const p of sc2.parts) for (const pad of p.pads) if (pad.net) nets.set(`${p.path}:${pad.number}`, pad.net);
    viewer.current?.setPadNets(nets);
  }, [sc2, board, parts]);

  // X-ray, the colouring and the height preview (inspectMode.ts): the picture only.
  const xr = xray.value;
  const cb = colourBy.value;
  const pv = heightPreview.value;
  useEffect(() => {
    const v = viewer.current;
    if (!v) return;
    v.setLook({ xray: xr, colourBy: cb, previewMm: pv.on ? pv.mm : null });
    setPreviewed(v.previewCount());
  }, [xr, cb, pv.on, pv.mm, parts]);

  // A finding picked in the problems dock (or the findings list): fly to it and light it. One
  // asked before this view opened is the 2D board's.
  const asked = focus3d.value;
  useEffect(() => {
    const v = viewer.current;
    if (!asked || !v || !parts.length || asked.nonce <= focusSeen.current) return;
    focusSeen.current = asked.nonce;
    const t = findingTargets(asked.refs, asked.keepout, v.partList(), v.limitList());
    v.focusFinding(t.paths, t.limits);
  }, [asked?.nonce, parts]);

  // Asked to show the selection (go to anything, a click on the status chip): frame its parts.
  const ask = revealRequest.value;
  useEffect(() => {
    if (!ask || ask.board !== board) return;
    const paths = partPathsOf(detail.peek(), ask.sel);
    if (paths.length) viewer.current?.frameParts(paths);
  }, [ask?.nonce]);

  // ---- the tools ----

  /** The pads' centres in the scene's frame, from the 2D scene (a sheet point is from the outline's top-left corner, y down). */
  const padsOf = (): PadPoint[] => {
    const s = scene2d.peek();
    const b = viewer.current?.board;
    if (!s || s.board !== board || !b) return [];
    const o = s.outline;
    // A pad's centre is on its copper's outer surface (under the mask's opening), else on the face.
    const coppers = b.layers.filter((l) => l.kind === 'copper');
    const padZ = (side: 'top' | 'bottom') => (side === 'top' ? (coppers[0]?.zTopMm ?? 0) : (coppers[coppers.length - 1]?.zBottomMm ?? faceZ('bottom', b.thicknessMm)));
    return s.parts.flatMap((p) => p.pads.map((pad) => ({ at: [pad.x - o.x, o.y + o.height - pad.y, padZ(p.side)] as [number, number, number], refdes: p.refdes, pad: pad.number, side: p.side })));
  };
  const lastCut = useRef<{ axis: Axis; at: number } | null>(null);
  const startCut = (axis: Axis) => {
    const b = viewer.current?.board;
    return { axis, at: b ? defaultCut(axis, b) : 0 };
  };
  const changeCut = (c: { axis: Axis; at: number } | null) => {
    if (c) lastCut.current = c;
    setCut(c);
    viewer.current?.setSection(c);
  };
  const toggleMeasure = () => {
    const on = !tools.current.measuring;
    setMeasuring(on);
    setPending(null);
    setSnapAt(null);
    if (on) setTip(null);
  };
  const commandText = () => {
    const v = viewer.current;
    if (!v) return '';
    const name = boardRef.current;
    const file = analysis.peek()?.boards.find((b) => b.name === name)?.file ?? '<file.nsm>';
    // What is lit, named as `/api/view` and `render --highlight` name it: the parts picked, the net picked, the limits lit.
    const sel = sharedSelection.peek();
    const lit = litIds(pcbSelection.peek(), sel?.kind === 'net' ? sel.key : null, litLimits(v), v.partList());
    const t = pageTheme.peek();
    return renderCommand({
      file,
      board: name,
      view: nearestView(v.towardEye()).view,
      section: tools.current.cut,
      highlight: lit.highlight,
      dark: t === 'dark' || t === 'hc-dark',
      perspective: !v.isOrtho(),
    });
  };
  /** The limits lit (by a finding, or by `/api/view`), by name. */
  const litLimits = (v: Viewer) => [...new Set(v.focusedList().map((k) => k.slice(0, k.lastIndexOf(':'))))];

  /** A change of `/api/view`: checked against the scene first (a refusal changes nothing), then
   * made with what the keys and the palette run, and ended once the view has drawn it. A camera
   * move is eased as a person's is, at once with reduced motion or in a window that draws no frames. */
  const applyChange = async (v: Viewer, change: ViewChange) => {
    const have: Ids = { parts: v.partList(), nets: v.netList(), limits: v.limitList().map((l) => l.name) };
    const plan = planChange(change, have, v.canExplode());
    const still = v.still;
    if (document.hidden) v.still = true;
    try {
      if (plan.ortho !== undefined && plan.ortho !== v.isOrtho()) view3d.current?.toggleProjection();
      if (plan.view) {
        // The views the page has a preset for are its presets, as the keys and the bar move to them (`iso` is the key I's); `iso-bottom` is render's direction.
        const preset = PRESETS.find((p) => p.id === plan.view);
        if (preset) v.preset(preset.id);
        else v.lookFrom(plan.towardEye!);
      }
      if (plan.camera) v.place(plan.camera.position, plan.camera.target);
      if (plan.section !== undefined) changeCut(plan.section);
      if (plan.exploded !== undefined && plan.exploded !== v.isExploded()) view3d.current?.toggleExplode();
      if (plan.selection) {
        const { parts: paths, net } = plan.selection;
        if (net) pickNet(net);
        else {
          // A net picked goes first, and the views follow it before the parts are picked.
          if (sharedSelection.peek()?.kind === 'net') {
            pickNet(null);
            await sleep(150);
          }
          select(paths);
        }
      }
      if (plan.limits) v.lightLimits(plan.limits);
      if (plan.xray !== undefined) xray.value = plan.xray;
      if (plan.colourBy !== undefined) colourBy.value = plan.colourBy;
      if (plan.preview) heightPreview.value = { on: plan.preview.on, mm: plan.preview.mm ?? heightPreview.peek().mm };
      // The page's effects run after a paint (or 100 ms); then the camera and the layers finish moving.
      await sleep(160);
      const end = performance.now() + 3000;
      while (v.busy() && performance.now() < end) await sleep(30);
      v.drawNow();
    } finally {
      v.still = still;
    }
  };

  const copyCommand = async () => {
    const text = commandText();
    if (await copyText(text)) notify({ kind: 'ok', title: 'Copied the netsmith render command', detail: text });
    else notify({ kind: 'bad', title: 'Could not copy the command', detail: text });
  };

  // The pads follow the 2D scene (a part moved in 2D moves its pads).
  useEffect(() => {
    viewer.current?.setPads(padsOf());
  }, [scene2d.value, parts]);

  // The keys of the drawing (the commands of mode.ts have the presets, the flip and the projection).
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey || typing(e.target)) return;
      if (focusedSection() !== 'pcb') return;
      const v = viewer.current;
      if (!v) return;
      const k = e.key;
      const used = () => e.preventDefault();
      const step = Math.PI / 12;
      if (k === 'f' || k === 'F' || k === '0') {
        if (e.shiftKey && k !== '0') return;
        used();
        v.fit();
      } else if (k === '+' || k === '=') {
        used();
        v.zoomBy(1.25);
      } else if (k === '-' || k === '_') {
        used();
        v.zoomBy(0.8);
      } else if (k.startsWith('Arrow')) {
        used();
        const dx = k === 'ArrowLeft' ? -1 : k === 'ArrowRight' ? 1 : 0;
        const dy = k === 'ArrowUp' ? -1 : k === 'ArrowDown' ? 1 : 0;
        if (e.shiftKey) v.panBy(-dx * 0.1, -dy * 0.1);
        else v.orbitBy(-dx * step, dy * step);
      } else if ((k === 'm' || k === 'M') && !e.shiftKey) {
        used();
        toggleMeasure();
      } else if (k === 'Escape') {
        if (tools.current.measuring) {
          used();
          if (tools.current.pending) setPending(null);
          else toggleMeasure();
        } else if (pcbSelection.peek().length) {
          used();
          select([]);
        } else if (sharedSelection.peek()?.kind === 'net') {
          used();
          pickNet(null);
        }
        v.clearFocus();
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, []);

  // The pointer: hover lights and names a part, a click picks it, a double click shows it in the source.
  const down = useRef<{ x: number; y: number; button: number } | null>(null);
  const moveFrame = useRef(0);
  const hoverOf = (path: string): Hover => ({ kind: 'part', key: path });
  const hoverOfHit = (hit: Hit | null | undefined): Hover | null =>
    hit?.kind === 'part' ? hoverOf(hit.info.path) : hit?.kind === 'copper' && hit.info.net ? { kind: 'net', key: hit.info.net } : null;
  const tipFor = (hit: Hit, x: number, y: number): Tip => {
    if (hit.kind === 'copper') {
      const net = hit.info.net ?? '';
      const lit = sharedSelection.peek()?.kind === 'net' && sharedSelection.peek()?.key === net;
      return { x, y, title: `Net ${net}`, lines: [hit.info.kind === 'hole' ? 'A plated barrel through the board' : `Copper on ${hit.info.layer ?? 'a layer'}`, lit ? 'Lit on every layer: click to clear' : 'Click to light it on every layer'] };
    }
    const words = hit.kind === 'part' ? partTip(hit.info, viewer.current?.board?.findings ?? [], detail.peek()?.parts.find((p) => p.path === hit.info.path)?.value ?? '') : limitTip(hit.info);
    if (hit.kind === 'part' && hit.info.provenance === 'unknown' && heightPreview.peek().on) words.lines.splice(1, 0, `Preview: drawn ${heightPreview.peek().mm} mm tall in the picture only, not a height`);
    return { x, y, ...words };
  };
  const onMove = (e: PointerEvent) => {
    if (e.pointerType === 'touch') return;
    const at = { x: e.clientX, y: e.clientY, buttons: e.buttons };
    cancelAnimationFrame(moveFrame.current);
    moveFrame.current = requestAnimationFrame(() => {
      const v = viewer.current;
      const el = host.current;
      if (!v || !el) return;
      if (at.buttons) {
        setTip(null);
        return;
      }
      if (tools.current.measuring && !v.isExploded()) {
        const s = v.snap(at.x, at.y);
        setSnapAt(s);
        el.style.cursor = s ? 'crosshair' : '';
        return;
      }
      const hit = v.pick(at.x, at.y);
      const now = hoverOfHit(hit);
      if (!sameHover(now, pointed.current)) {
        if (pointed.current) hoverLeave(pointed.current);
        if (now) hoverEnter(now);
        pointed.current = now;
      }
      el.style.cursor = now ? 'pointer' : '';
      const r = el.getBoundingClientRect();
      setTip(hit ? tipFor(hit, at.x - r.left + 14, at.y - r.top + 16) : null);
    });
  };
  const onLeave = () => {
    cancelAnimationFrame(moveFrame.current);
    setSnapAt(null);
    if (pointed.current) hoverLeave(pointed.current);
    pointed.current = null;
    setTip(null);
  };

  // The picked part's height and margin, when the measure tool is on and one part is picked.
  let readout: Readout | null = null;
  const v0 = viewer.current;
  if (measuring && v0 && picked.length === 1) {
    const part = parts.find((p) => p.path === picked[0]);
    const box = part ? v0.partBox(part.path) : null;
    if (part && box) {
      const z = faceZ(part.side, v0.board?.thicknessMm ?? 0);
      readout = { part, words: partReadout(part, { min: [box.min[0], box.min[1]], max: [box.max[0], box.max[1]] }, v0.limitPlans()), foot: [box.max[0], box.min[1], z] };
    }
  }
  const range = cut && v0 ? axisRange(cut.axis, v0.sceneBox()) : ([0, 1] as [number, number]);

  const unknown = unknownParts(parts);
  const counts = provenanceCounts(parts);
  const over = limits.some((l) => l.violated);
  const choose = (path: string) => {
    select([path]);
    viewer.current?.frameParts([path]);
  };

  return (
    <div class="v3-wrap">
      <div
        ref={host}
        class="v3-host"
        data-testid="pcb-3d"
        tabIndex={0}
        role="img"
        aria-label={`The board ${board} in 3D. ${coverage ? coverageLine(coverage) : ''}. Read-only: move parts in the 2D view.`}
        onPointerDown={(e) => {
          down.current = { x: e.clientX, y: e.clientY, button: e.button };
          setTip(null);
        }}
        onPointerUp={(e) => {
          const was = down.current;
          down.current = null;
          if (!was || was.button !== 0 || Math.hypot(e.clientX - was.x, e.clientY - was.y) > 4) return;
          const v = viewer.current;
          if (measuring && v && !e.shiftKey) {
            if (v.isExploded()) return;
            const s = v.snap(e.clientX, e.clientY);
            if (!s) return;
            if (!pending) setPending(s);
            else {
              const a = pending;
              setMeasures((ms) => [...ms, { id: nextId.current++, a, b: s }]);
              setPending(null);
            }
            return;
          }
          const hit = v?.pick(e.clientX, e.clientY);
          v?.clearFocus();
          if (hit?.kind === 'part') select(e.shiftKey ? [...new Set([...pcbSelection.peek(), hit.info.path])] : [hit.info.path]);
          else if (hit?.kind === 'copper' && hit.info.net) pickNet(pickedNet === hit.info.net ? null : hit.info.net);
          else if (!e.shiftKey) select([]);
        }}
        onDblClick={(e) => {
          if (measuring) return;
          const hit = viewer.current?.pick(e.clientX, e.clientY);
          if (hit?.kind === 'part') pickPart(hit.info.refdes, true);
          else viewer.current?.fit();
        }}
        onPointerMove={onMove}
        onPointerLeave={onLeave}
        onContextMenu={(e) => e.preventDefault()}
      >
        {status.state === 'ready' && viewer.current && (
          <Overlay3d viewer={viewer.current} measures={measures} pending={pending} hover={snapAt} readout={readout} exploded={exploded} cut={cut} measuring={measuring} />
        )}
        {tip && !measuring && (
          <div class="pcb-tip v3-tip" style={{ left: tip.x, top: tip.y }} data-testid="tip-3d">
            <b>{tip.title}</b>
            {tip.lines.map((l) => (
              <div key={l}>{l}</div>
            ))}
          </div>
        )}
      </div>
      {status.state === 'error' ? (
        <div class="v3-cover">
          <EmptyState
            icon="error"
            tone="bad"
            title={status.webgl ? 'This browser cannot draw the board in 3D' : 'The board could not be drawn in 3D'}
            help={status.webgl ? 'The 3D view needs WebGL 2. The 2D view still shows and edits the board.' : 'The 2D view still shows the board. The problems list may say why.'}
          >
            <div class="empty-error">
              <span class="text">{status.message}</span>
            </div>
          </EmptyState>
        </div>
      ) : (
        status.state === 'loading' && (
          <div class="v3-cover v3-loading" role="status" aria-live="polite" data-testid="pcb-3d-loading">
            <Spinner size={16} />
            Drawing the board in 3D…
          </div>
        )
      )}
      {status.state === 'ready' && (
        <>
          <div class="v3-bar no-pan" role="toolbar" aria-label="3D view" data-toast-clear>
            <Segmented<Preset | ''>
              label="View from"
              value={preset ?? ''}
              onChange={(p) => p && view3d.current?.preset(p)}
              options={PRESETS.map((p) => ({
                value: p.id,
                label: p.id === 'iso' ? 'Iso' : p.label,
                tip: `Look from the ${p.id === 'iso' ? 'front right, above' : p.id}`,
                keys: p.key,
                testid: `view-${p.id}`,
              }))}
            />
            <span class="tb-sep" aria-hidden="true" />
            <button class="icon-btn" data-tip="Flip the board over" data-tip-keys="Shift+F" aria-label="Flip the board over" data-testid="view-flip" onClick={() => view3d.current?.flip()}>
              <Icon name="flip" size={15} />
            </button>
            <button
              class={`tool v3-proj ${ortho ? 'on' : ''}`}
              data-tip={ortho ? 'Orthographic: no perspective, sizes compare across the view. Click for perspective.' : 'Perspective. Click for orthographic, where sizes compare across the view.'}
              data-tip-keys="P"
              aria-pressed={ortho}
              data-testid="view-projection"
              onClick={() => view3d.current?.toggleProjection()}
            >
              {ortho ? 'Orthographic' : 'Perspective'}
            </button>
          </div>
          <div class="v3-tools no-pan" role="toolbar" aria-label="3D tools" data-toast-clear>
            <button
              class={`tool ${measuring ? 'on' : ''}`}
              aria-pressed={measuring}
              data-tip="Measure: click two points. Snaps to pad centres, corners, the board's edge and faces"
              data-tip-keys="M"
              data-testid="tool-measure"
              onClick={() => toggleMeasure()}
            >
              <Icon name="ruler" size={14} />
              Measure
            </button>
            <button
              class={`tool ${cut ? 'on' : ''}`}
              aria-pressed={!!cut}
              data-tip="Section plane: cut the board across X, Y or Z to see the stackup"
              data-tip-keys="S"
              data-testid="tool-section"
              onClick={() => view3d.current?.toggleSection()}
            >
              <Icon name="columns" size={14} />
              Section
            </button>
            <button
              class={`tool ${exploded ? 'on' : ''}`}
              aria-pressed={exploded}
              disabled={!canExplode}
              data-tip={canExplode ? 'Pull the stackup apart: each layer with its thickness' : 'The scene file has no stackup to pull apart'}
              data-tip-keys="E"
              data-testid="tool-explode"
              onClick={() => view3d.current?.toggleExplode()}
            >
              <Icon name="layers" size={14} />
              Layers apart
            </button>
            <span class="tb-sep" aria-hidden="true" />
            <Popover icon="more" tip="More: draw this view with netsmith render" class="v3-more">
              {() => {
                const text = commandText();
                const nearest = viewer.current ? nearestView(viewer.current.towardEye()) : null;
                return (
                  <div class="v3-cmd" data-testid="render-command">
                    <div class="v3-cmd-head">Copy as command</div>
                    <p>
                      The picture of this view from <code>netsmith render</code>: no browser, no GPU.
                      {nearest && !nearest.exact && ` The camera is turned from the command's views; the nearest is ${nearest.view}.`}
                    </p>
                    <div class="cli">
                      <Icon name="terminal" size={13} class="cli-icon" />
                      <code data-testid="render-command-text">{text}</code>
                      <CopyButton text={text} label="Copy the command" />
                    </div>
                  </div>
                );
              }}
            </Popover>
          </div>
          {cut && <SectionPanel cut={cut} range={range} onChange={changeCut} onClose={() => changeCut(null)} />}
          {measuring && (
            <MeasurePanel
              measures={measures}
              readout={readout}
              exploded={exploded}
              onRemove={(id) => setMeasures((ms) => ms.filter((m) => m.id !== id))}
              onClear={() => {
                setMeasures([]);
                setPending(null);
              }}
              onClose={() => toggleMeasure()}
            />
          )}
          <div class="v3-info no-pan" data-toast-clear>
            {coverage && (
              <div class="v3-cov-wrap">
                <button
                  class={`chip v3-cov ${coverage.known === coverage.total ? 'ok' : ''}`}
                  aria-expanded={listOpen}
                  data-testid="height-coverage"
                  data-tip={unknown.length ? 'Show the parts with no known height' : 'Every part has a height'}
                  onClick={() => unknown.length && setListOpen(!listOpen)}
                >
                  <Icon name={coverage.known === coverage.total ? 'check-circle' : 'ruler'} size={12} />
                  {coverageLine(coverage)}
                  {unknown.length > 0 && <Icon name={listOpen ? 'chevron-up' : 'chevron-down'} size={12} />}
                </button>
                {listOpen && unknown.length > 0 && (
                  <div class="v3-unknown" role="list" aria-label="Parts with no known height" data-testid="unknown-heights">
                    <div class="v3-unknown-head">No known height: drawn flat, not checked against a limit</div>
                    {unknown.map((p) => (
                      <button
                        key={p.path}
                        role="listitem"
                        class={`v3-unknown-row ${picked.includes(p.path) ? 'on' : ''}`}
                        data-path={p.path}
                        data-hv={`part:${p.path}`}
                        onPointerEnter={() => hoverEnter(hoverOf(p.path))}
                        onPointerLeave={() => hoverLeave(hoverOf(p.path))}
                        onClick={() => choose(p.path)}
                      >
                        <b class="mono">{p.refdes}</b>
                        <span>{detail.value?.parts.find((x) => x.path === p.path)?.value ?? ''}</span>
                        <span class="v3-path mono">{p.path}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
            {cb !== 'provenance' ? (
              <InspectLegend by={cb} parts={parts} limits={limits} />
            ) : (
              <div class="v3-legend" aria-label="Where each height came from" data-testid="height-legend">
                <span>
                  <i class="v3-sw v3-sw-stated" aria-hidden="true" />
                  Stated {counts.stated}
                </span>
                <span>
                  <i class="v3-sw v3-sw-catalog" aria-hidden="true" />
                  Catalog {counts.catalog}
                </span>
                <span>
                  <i class="v3-sw v3-sw-unknown" aria-hidden="true" />
                  Unknown {counts.unknown}
                </span>
                {limits.some((l) => !l.violated) && (
                  <span>
                    <i class="v3-sw v3-sw-limit" aria-hidden="true" />
                    Height limit
                  </span>
                )}
                {over && (
                  <span>
                    <i class="v3-sw v3-sw-over" aria-hidden="true" />
                    Limit exceeded
                  </span>
                )}
              </div>
            )}
          </div>
          {pv.on && <PreviewLabel mm={pv.mm} count={previewed} />}
          <InspectBar />
          {!measuring && (
            <div class="v3-note" aria-hidden="true">
              Read-only: move parts in 2D · drag to orbit, right-drag to pan, scroll to zoom
            </div>
          )}
          <div class="zoombar no-pan" role="toolbar" aria-label="Zoom" data-toast-clear>
            <button title="Zoom out (-)" aria-label="Zoom out" onClick={() => viewer.current?.zoomBy(0.8)}>
              −
            </button>
            <button title="Zoom in (+)" aria-label="Zoom in" onClick={() => viewer.current?.zoomBy(1.25)}>
              +
            </button>
            <i class="zsep" aria-hidden="true" />
            <button title="Fit the whole board to the window (F or 0)" data-testid="view-fit" onClick={() => viewer.current?.fit()}>
              Fit
            </button>
          </div>
        </>
      )}
    </div>
  );
}
