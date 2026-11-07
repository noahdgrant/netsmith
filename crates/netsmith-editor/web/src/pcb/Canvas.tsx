// The PCB as a canvas: the server's drawing with an overlay that carries the selection, the
// grid, the lock marks and the tools. Every gesture is worked out here from the scene's boxes
// (a part is a courtyard, a keepout is a rectangle), so it is the same whatever is drawn.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import type { PcbBox, PcbKeepout, PcbPart, PcbScene } from '../api';
import { PanZoom } from '../components/PanZoom';
import { hoverEnter, hoverLeave } from '../hover';
import {
  commit,
  grid,
  pickKeepout,
  pickedKeepout,
  redo,
  select,
  selection,
  tool,
  undo,
} from '../pcbstore';
import { pickPart, say } from '../store';
import {
  addKeepout,
  changeKeepout,
  deleteKeepout,
  flipSelection,
  frameOf,
  nudge,
  releaseSelection,
  rotateSelection,
  selectAll,
  toggleLock,
} from './actions';
import { fitView } from '../sheets';
import { type Drawing, SCALE, light, previewMove } from './drawing';
import { moveParts } from './edits';
import { type HiddenPlane, fitPlaneLabels } from './planeLabels';
import { PlaneMore } from './PlaneMore';
import {
  type Handle,
  type Pt,
  boxFrom,
  contains,
  handlesOf,
  hitBox,
  hitHandle,
  inBox,
  moved,
  resizeBox,
  snap,
  snapMove,
} from './geometry';

export interface Tip {
  x: number;
  y: number;
  title: string;
  lines: string[];
}

type Gesture =
  | { kind: 'move'; start: Pt; screen: Pt; parts: PcbPart[]; hit: PcbPart; active: boolean; extra: boolean }
  | { kind: 'box'; start: Pt; screen: Pt; additive: boolean; active: boolean }
  | { kind: 'draw'; start: Pt }
  | { kind: 'kmove'; start: Pt; screen: Pt; name: string; box0: PcbBox; active: boolean }
  | { kind: 'kresize'; start: Pt; name: string; handle: Handle; box0: PcbBox };

/** What is drawn over the board while a gesture is on. */
interface Preview {
  /** How far the moving parts have gone. */
  delta?: Pt;
  moving?: string[];
  /** The box being selected, or the keepout being drawn. */
  box?: PcbBox;
  draw?: PcbBox;
  /** A keepout as it will be once the move or resize ends. */
  keepout?: { name: string; box: PcbBox };
}

const NONE: Pt = { x: 0, y: 0 };
/** The most a board is zoomed when it opens (3 times 12 px to the millimetre): 300 %. */
const MAX_OPEN = 3;
/** A drag shorter than this many screen pixels is a click. */
const SLOP = 4;
/** How close to a handle, or to the edge of a keepout, a press counts, in screen pixels. */
const REACH = 8;

function distanceToEdge(b: PcbBox, p: Pt): number {
  const dx = Math.max(b.x - p.x, 0, p.x - (b.x + b.width));
  const dy = Math.max(b.y - p.y, 0, p.y - (b.y + b.height));
  if (dx > 0 || dy > 0) return Math.hypot(dx, dy);
  return Math.min(p.x - b.x, b.x + b.width - p.x, p.y - b.y, b.y + b.height - p.y);
}

const forbids = (k: PcbKeepout) =>
  [k.tracks && 'tracks', k.vias && 'vias', k.copper && 'copper', k.parts && 'parts'].filter(Boolean).join(', ');

export function Canvas(props: {
  drawing: Drawing;
  scene: PcbScene;
  /** The stylesheet that hides the layers that are switched off. */
  css: string;
  /** The net the pointer is on in the list or the drawing, or the one picked. */
  litNet: string | null;
  onHoverNet: (net: string | null) => void;
  onPickNet: (net: string) => void;
  onTip: (tip: Tip | null) => void;
  onCursor: (at: Pt | null) => void;
  /** A box on the sheet to zoom to when `seq` changes. */
  focus: { box: PcbBox; seq: number } | null;
  /** Findings to mark on the board. */
  marks: { id: string; box: PcbBox; level: string; on: boolean }[];
  /** What the comparison with a ref says of each part (by path), when its marks are on: a crisp outline outside the pick's (`.cmp-rect`). */
  cmp?: ReadonlyMap<string, 'added' | 'changed' | 'renumbered'> | null;
  /** The chip `+N` zooms the board to a plane whose label has no room. */
  onZoomPlane?: (box: PcbBox) => void;
}) {
  const { drawing, scene: sc } = props;
  const holder = useRef<HTMLDivElement>(null);
  const gesture = useRef<Gesture | null>(null);
  const [zoom, setZoom] = useState(1);
  const [preview, setPreview] = useState<Preview>({});
  const [hovered, setHovered] = useState<string | null>(null);
  // The planes whose label has no room at this zoom (the chip `+N` lists them), and the one the list points at.
  const [noRoom, setNoRoom] = useState<HiddenPlane[]>([]);
  const [peek, setPeek] = useState<HiddenPlane | null>(null);
  const frame = frameOf(sc);
  const picked = selection.value;
  const selected = useMemo(() => sc.parts.filter((p) => picked.includes(p.path)), [sc, picked]);
  const keepoutPicked = pickedKeepout.value;
  const activeTool = tool.value;
  const step = grid.value;
  const svgEl = () => holder.current?.querySelector('svg') as SVGSVGElement | null;

  // Nets of the selected parts are lit, unless the pointer is on a net or one is picked.
  useEffect(() => {
    const svg = svgEl();
    if (!svg) return;
    const nets = new Set<string>();
    if (props.litNet) nets.add(props.litNet);
    else for (const p of selected) for (const pad of p.pads) if (pad.net) nets.add(pad.net);
    light(svg, nets, new Set(selected.map((p) => p.refdes)));
  }, [drawing, props.litNet, selected]);

  // A plane's label goes to the corner of its plane where no part lies, at the zoom the board is shown at (planeLabels.ts); the planes
  // that have no room are the chip's. A layer switched off (`props.css`) changes which planes are drawn.
  useEffect(() => {
    const svg = svgEl();
    if (!svg) return;
    const { planes } = fitPlaneLabels(svg);
    setNoRoom((was) => (was.length === planes.length && was.every((w, i) => w.name === planes[i].name) ? was : planes));
  }, [drawing, zoom, props.css]);

  // The plane the chip's list points at is outlined on the board.
  useEffect(() => {
    const svg = svgEl();
    if (!svg) return;
    svg.querySelectorAll('.l-planes > rect.plane-peek').forEach((e) => e.classList.remove('plane-peek'));
    if (!peek) return;
    [...svg.querySelectorAll('.l-planes > rect[data-net]')]
      .find((e) => e.getAttribute('data-net') === peek.net && e.getAttribute('data-layer') === peek.layer)
      ?.classList.add('plane-peek');
  }, [peek, drawing]);

  // The canvas in pixels to the sheet: one pointer position in mm, and how many pixels an mm is.
  function toBoard(e: { clientX: number; clientY: number }): { p: Pt; scale: number } | null {
    const svg = svgEl();
    const m = svg?.getScreenCTM();
    if (!svg || !m) return null;
    const at = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    return { p: { x: at.x, y: at.y }, scale: m.a };
  }

  const keepoutAt = (name: string | null) => sc.keepouts.find((k) => k.name === name) ?? null;

  function describe(e: PointerEvent, p: Pt): Tip | null {
    const target = e.target as Element;
    const stage = holder.current?.closest('.panzoom')?.getBoundingClientRect();
    if (!stage) return null;
    const at = { x: e.clientX - stage.left + 14, y: e.clientY - stage.top + 14 };
    const info = (part: PcbPart) => [`${part.value}  ${part.kind}${part.package ? ` ${part.package}` : ''}`];
    const pad = target.closest('[data-pad]');
    const owner = target.closest('[data-path]');
    const part = (owner && sc.parts.find((x) => x.path === owner.getAttribute('data-path'))) || hitBox(sc.parts, p);
    if (pad && part) {
      const number = pad.getAttribute('data-pad')!;
      const net = pad.getAttribute('data-net');
      return { ...at, title: `${part.refdes}.${number}`, lines: [...info(part), net ? `net ${net}` : 'no net'] };
    }
    const copper = target.closest('.l-front-copper [data-net], .l-back-copper [data-net], .l-vias [data-net]');
    if (copper) return { ...at, title: `net ${copper.getAttribute('data-net')}`, lines: [] };
    if (part) {
      const how = part.locked ? 'locked' : part.placement === 'derived' ? 'placed by netsmith' : `placed by ${part.placement}`;
      return { ...at, title: part.refdes, lines: [...info(part), `${part.side} side, ${how}`] };
    }
    const keepout = sc.keepouts.find((k) => k.area && distanceToEdge(k.area, p) < 0.3 && contains(expand(k.area, 0.3), p));
    if (keepout) return { ...at, title: `keepout ${keepout.name}`, lines: [`no ${forbids(keepout)}`, `on ${keepout.layers.join(', ')}`] };
    return null;
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    const at = toBoard(e);
    if (!at) return;
    const { p, scale } = at;
    const screen = { x: e.clientX, y: e.clientY };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    props.onTip(null);
    if (tool.value === 'keepout') {
      gesture.current = { kind: 'draw', start: p };
      setPreview({ draw: boxFrom(p, p) });
      return;
    }
    const additive = e.shiftKey || e.ctrlKey || e.metaKey;
    const k = keepoutAt(pickedKeepout.value);
    if (k?.area && k.origin === 'drawn') {
      const handle = hitHandle(k.area, p, REACH / scale);
      if (handle) {
        gesture.current = { kind: 'kresize', start: p, name: k.name, handle, box0: k.area };
        return;
      }
    }
    const target = e.target as Element;
    const net = target.closest('.l-front-copper [data-net], .l-back-copper [data-net], .l-vias [data-net]');
    if (net && !additive) {
      props.onPickNet(net.getAttribute('data-net')!);
      return;
    }
    const part = hitBox(sc.parts, p);
    if (part) {
      if (additive) {
        select(picked.includes(part.path) ? picked.filter((x) => x !== part.path) : [...picked, part.path]);
        return;
      }
      const already = picked.includes(part.path);
      if (!already) select([part.path]);
      const parts = already ? selected : [part];
      gesture.current = { kind: 'move', start: p, screen, parts, hit: part, active: false, extra: already && parts.length > 1 };
      return;
    }
    const near = sc.keepouts
      .filter((x) => x.area && (distanceToEdge(x.area, p) <= REACH / scale || (x.name === keepoutPicked && contains(x.area, p))))
      .sort((a, b) => a.area!.width * a.area!.height - b.area!.width * b.area!.height)[0];
    if (near?.area) {
      pickKeepout(near.name);
      if (near.origin === 'drawn' || near.origin === 'placed') {
        gesture.current = { kind: 'kmove', start: p, screen, name: near.name, box0: near.area, active: false };
      }
      return;
    }
    gesture.current = { kind: 'box', start: p, screen, additive, active: false };
  }

  function move(e: PointerEvent) {
    const at = toBoard(e);
    if (!at) return;
    const { p } = at;
    props.onCursor({ x: p.x - sc.originX, y: p.y - sc.originY });
    const g = gesture.current;
    if (!g) {
      props.onTip(describe(e, p));
      const part = hitBox(sc.parts, p);
      setHovered(part?.path ?? null);
      // What is lit in the other views: the net of a track, a via or a pad under the pointer, else the part whose box it is in.
      const net = (e.target as Element).closest?.('[data-net]')?.getAttribute('data-net');
      if (e.pointerType !== 'touch') {
        if (net) hoverEnter({ kind: 'net', key: net });
        else if (part) hoverEnter({ kind: 'part', key: part.path });
        else hoverLeave();
      }
      return;
    }
    const far = 'screen' in g ? Math.hypot(e.clientX - g.screen.x, e.clientY - g.screen.y) > SLOP : true;
    const by = { x: p.x - g.start.x, y: p.y - g.start.y };
    if (g.kind === 'move') {
      if (!g.active && far) g.active = true;
      if (!g.active) return;
      const free = g.parts.filter((x) => !x.locked);
      if (free.length === 0) return;
      // The part that was taken ends on the grid and the others keep their distance to it.
      const first = free.includes(g.hit) ? g.hit : free[0];
      const anchor = { x: first.x - sc.originX, y: first.y - sc.originY };
      const delta = snapMove(anchor, by, step);
      const svg = svgEl();
      if (svg) previewMove(svg, free, delta);
      setPreview({ delta, moving: free.map((x) => x.path) });
    } else if (g.kind === 'box') {
      if (!g.active && far) g.active = true;
      if (g.active) setPreview({ box: boxFrom(g.start, p) });
    } else if (g.kind === 'draw') {
      const a = { x: snap(g.start.x - sc.originX, step) + sc.originX, y: snap(g.start.y - sc.originY, step) + sc.originY };
      const b = { x: snap(p.x - sc.originX, step) + sc.originX, y: snap(p.y - sc.originY, step) + sc.originY };
      setPreview({ draw: boxFrom(a, b) });
    } else if (g.kind === 'kmove') {
      if (!g.active && far) g.active = true;
      if (!g.active) return;
      const d = snapMove({ x: g.box0.x - sc.originX, y: g.box0.y - sc.originY }, by, step);
      setPreview({ keepout: { name: g.name, box: moved(g.box0, d) } });
    } else if (g.kind === 'kresize') {
      const d = snapMove({ x: 0, y: 0 }, by, step);
      setPreview({ keepout: { name: g.name, box: resizeBox(g.box0, g.handle, d) } });
    }
  }

  async function up(e: PointerEvent) {
    const g = gesture.current;
    gesture.current = null;
    (e.currentTarget as Element).releasePointerCapture?.(e.pointerId);
    if (!g) return;
    const pv = preview;
    setPreview({});
    if (g.kind === 'move') {
      if (!g.active) {
        // A press on one of several selected parts, with no drag, picks that one.
        if (g.extra) select([g.hit.path]);
        return;
      }
      const free = g.parts.filter((x) => !x.locked);
      if (free.length === 0) {
        say(`${g.hit.refdes} is locked: select it and press L to unlock`, true);
        return;
      }
      const delta = pv.delta ?? NONE;
      setPreview({ delta, moving: free.map((x) => x.path) });
      const label = free.length === 1 ? `move ${free[0].refdes}` : `move ${free.length} parts`;
      await commit(moveParts(free, frame, delta), label);
      const svg = svgEl();
      if (svg) previewMove(svg, free, NONE);
      setPreview({});
    } else if (g.kind === 'box') {
      if (!g.active) {
        if (!g.additive) select([]);
        return;
      }
      const hit = inBox(sc.parts, pv.box ?? boxFrom(g.start, g.start)).map((x) => x.path);
      select(g.additive ? [...new Set([...picked, ...hit])] : hit);
    } else if (g.kind === 'draw') {
      const box = pv.draw;
      tool.value = 'select';
      if (!box || box.width < 0.5 || box.height < 0.5) {
        say('A keepout is drawn by dragging out a rectangle');
        return;
      }
      const name = await addKeepout(box);
      if (name) pickKeepout(name);
    } else if (g.kind === 'kmove' || g.kind === 'kresize') {
      const k = keepoutAt(g.name);
      const box = pv.keepout?.box;
      if (!k || !box || (g.kind === 'kmove' && !g.active)) return;
      setPreview({ keepout: { name: g.name, box } });
      await changeKeepout(k, { area: box }, `${g.kind === 'kmove' ? 'move' : 'resize'} keepout ${k.name}`);
      setPreview({});
    }
  }

  function leave() {
    props.onTip(null);
    props.onCursor(null);
    setHovered(null);
    hoverLeave();
  }

  // The keys. They work wherever the focus is except in a field or the source editor.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest('input, textarea, select, [contenteditable="true"], .cm-editor')) return;
      const ctrl = e.ctrlKey || e.metaKey;
      const k = e.key.toLowerCase();
      const used = () => {
        e.preventDefault();
        e.stopPropagation();
      };
      if (ctrl && k === 'z') {
        used();
        void (e.shiftKey ? redo() : undo());
      } else if (ctrl && k === 'y') {
        used();
        void redo();
      } else if (ctrl && k === 'a') {
        used();
        selectAll();
      } else if (ctrl || e.altKey) {
        return;
      } else if (k === 'r') {
        used();
        void rotateSelection(e.shiftKey ? -90 : 90);
      } else if (k === 'f') {
        used();
        void flipSelection();
      } else if (k === 'l') {
        used();
        void toggleLock();
      } else if (k === 'k') {
        used();
        tool.value = tool.value === 'keepout' ? 'select' : 'keepout';
      } else if (k === 'escape') {
        used();
        if (gesture.current) {
          gesture.current = null;
          const svg = svgEl();
          const moving = (preview.moving ?? []).map((path) => sc.parts.find((x) => x.path === path)!).filter(Boolean);
          if (svg) previewMove(svg, moving, NONE);
          setPreview({});
        } else if (tool.value !== 'select') tool.value = 'select';
        else {
          select([]);
          pickKeepout(null);
        }
      } else if (k === 'delete' || k === 'backspace') {
        if (pickedKeepout.value) {
          used();
          void deleteKeepout(pickedKeepout.value);
        } else if (selection.value.length) {
          used();
          void releaseSelection();
        }
      } else if (k.startsWith('arrow')) {
        used();
        const dx = k === 'arrowleft' ? -1 : k === 'arrowright' ? 1 : 0;
        const dy = k === 'arrowup' ? -1 : k === 'arrowdown' ? 1 : 0;
        void nudge(dx, dy, e.shiftKey);
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });

  // The sheet's box to canvas pixels, for PanZoom.
  const focus = props.focus
    ? {
        x: (props.focus.box.x - drawing.left) * SCALE,
        y: (props.focus.box.y - drawing.top) * SCALE,
        w: Math.max(props.focus.box.width, 4) * SCALE,
        h: Math.max(props.focus.box.height, 4) * SCALE,
        seq: props.focus.seq,
      }
    : undefined;

  const dots = step > 0 && step * SCALE * zoom >= 7;
  const o = sc.outline;
  const delta = preview.delta ?? NONE;
  const lockSize = 1.3;
  return (
    <PanZoom
      width={drawing.width}
      height={drawing.height}
      fitKey={`${sc.board}:${drawing.width}x${drawing.height}`}
      class={`pcb-host tool-${activeTool}`}
      primaryPans={false}
      keys={false}
      // 100 % is 12 px to the millimetre. A board opens (and Fit puts it) as large as the window allows, but not past 300 %: a small board
      // is not blown up until its pads are the size of the screen.
      scale100={1}
      scaleTitle="Zoom. 100% is 12 px to the millimetre"
      placeView={(w, h) => {
        const all = fitView(w, h, { x: 0, y: 0, w: drawing.width, h: drawing.height });
        const k = Math.min(all.k, MAX_OPEN);
        return { k, x: (w - drawing.width * k) / 2, y: (h - drawing.height * k) / 2 };
      }}
      fitPlaces
      memoKey={`pcb|${sc.board}`}
      onView={setZoom}
      focus={focus}
      overlay={<PlaneMore planes={noRoom} onPeek={setPeek} onZoom={(p) => props.onZoomPlane?.(p.box)} />}
    >
      <style>{props.css}</style>
      <div
        class="hit-target pcb-stage"
        // --px is a millimetre of the drawing per screen pixel (a number, for a width); --pxl is the same with its unit, for a size of text
        style={{ '--px': 1 / (SCALE * zoom), '--pxl': `${1 / (SCALE * zoom)}px` }}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerLeave={leave}
        onPointerOver={(e) => props.onHoverNet((e.target as Element).closest('[data-net]')?.getAttribute('data-net') ?? null)}
        onDblClick={(e) => {
          const at = toBoard(e);
          const part = at && hitBox(sc.parts, at.p);
          if (part) pickPart(part.refdes, true);
        }}
      >
        <div ref={holder} dangerouslySetInnerHTML={{ __html: drawing.html }} />
        <svg
          class="pcb-overlay"
          width={drawing.width}
          height={drawing.height}
          viewBox={`${drawing.left} ${drawing.top} ${drawing.mmWidth} ${drawing.mmHeight}`}
        >
          {dots && (
            <>
              <defs>
                <pattern
                  id="pcb-grid"
                  patternUnits="userSpaceOnUse"
                  x={sc.originX - step / 2}
                  y={sc.originY - step / 2}
                  width={step}
                  height={step}
                >
                  <circle cx={step / 2} cy={step / 2} r={Math.min(0.06, step / 8)} fill="var(--pcb-grid)" />
                </pattern>
              </defs>
              <rect x={o.x} y={o.y} width={o.width} height={o.height} fill="url(#pcb-grid)" class="pcb-grid-dots" />
            </>
          )}
          {preview.keepout &&
            (() => {
              const k = keepoutAt(preview.keepout.name);
              const b = preview.keepout.box;
              return (
                <rect class="k-preview" x={b.x} y={b.y} width={b.width} height={b.height} data-name={k?.name} />
              );
            })()}
          {sc.keepouts.map((k) =>
            k.area && k.name === keepoutPicked ? (
              <g key={k.name} class="k-picked">
                {(() => {
                  const b = preview.keepout?.name === k.name ? preview.keepout.box : k.area;
                  return (
                    <>
                      <rect x={b.x} y={b.y} width={b.width} height={b.height} />
                      {k.origin === 'drawn' &&
                        handlesOf(b).map((h) => <rect key={h.handle} class="handle" x={h.x - 0.35} y={h.y - 0.35} width={0.7} height={0.7} />)}
                    </>
                  );
                })()}
              </g>
            ) : null,
          )}
          {hovered && !picked.includes(hovered) && !gesture.current && (
            <Outline box={sc.parts.find((p) => p.path === hovered)?.courtyard} class="hover" />
          )}
          {props.cmp &&
            sc.parts.map((p) => {
              const m = props.cmp!.get(p.path);
              if (!m) return null;
              const b = expand(preview.moving?.includes(p.path) ? moved(p.courtyard, delta) : p.courtyard, 0.35);
              return <rect key={`cmp:${p.path}`} class={`cmp-rect cmp-${m}`} x={b.x} y={b.y} width={b.width} height={b.height} rx="0.3" data-cmp-ref={p.refdes} />;
            })}
          {selected.map((p) => (
            <Outline
              key={p.path}
              class="picked"
              box={preview.moving?.includes(p.path) ? moved(p.courtyard, delta) : p.courtyard}
            />
          ))}
          {sc.parts
            .filter((p) => p.locked)
            .map((p) => {
              const b = preview.moving?.includes(p.path) ? moved(p.courtyard, delta) : p.courtyard;
              return <Lock key={p.path} x={b.x + b.width} y={b.y} size={lockSize} />;
            })}
          {preview.box && <rect class="box-select" x={preview.box.x} y={preview.box.y} width={preview.box.width} height={preview.box.height} />}
          {preview.draw && (
            <rect class="k-draw" x={preview.draw.x} y={preview.draw.y} width={preview.draw.width} height={preview.draw.height} />
          )}
          {props.marks.map((m) => (
            <rect
              key={m.id}
              class={`mark ${m.level} ${m.on ? 'on' : ''}`}
              x={m.box.x - 0.4}
              y={m.box.y - 0.4}
              width={m.box.width + 0.8}
              height={m.box.height + 0.8}
            />
          ))}
        </svg>
      </div>
    </PanZoom>
  );
}

function expand(b: PcbBox, by: number): PcbBox {
  return { x: b.x - by, y: b.y - by, width: b.width + 2 * by, height: b.height + 2 * by };
}

function Outline(props: { box?: PcbBox; class: string }) {
  const b = props.box;
  if (!b) return null;
  return <rect class={`outline-${props.class}`} x={b.x} y={b.y} width={b.width} height={b.height} rx="0.2" />;
}

/** A padlock at the corner of a locked part. */
function Lock(props: { x: number; y: number; size: number }) {
  const s = props.size;
  return (
    <g class="lock-mark" transform={`translate(${props.x - s * 0.5} ${props.y - s * 0.5}) scale(${s / 10})`}>
      <circle cx="5" cy="5" r="6.4" />
      <path d="M3.3 4.6V3.5a1.7 1.7 0 013.4 0v1.1" fill="none" />
      <rect x="2.6" y="4.6" width="4.8" height="3.6" rx="0.5" />
    </g>
  );
}
