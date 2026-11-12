// The PCB view in 3D (ADR 0105): the board's scene as the server writes it (`GET /api/pcb/3d`, the
// file `netsmith board3d --glb` writes), drawn by three.js. It is read-only: a part is picked and
// hovered here through the page's shared selection and hover, and moved in the 2D view. A part
// with no known height is a flat hatched outline, never a box of a guessed height, and the chip
// says how many parts have a known height. This component and three.js are a chunk of their own,
// loaded the first time 3D opens.

import { useEffect, useRef, useState } from 'preact/hooks';

import { focusedSection } from '../commandset';
import { EmptyState, Segmented, Spinner } from '../components/ui';
import { Icon } from '../components/Icons';
import { type Hover, hover as sharedHover, hoverEnter, hoverLeave, resolveHover, sameHover } from '../hover';
import { scene as scene2d, select, selection as pcbSelection } from '../pcbstore';
import { partPathsOf, revealRequest, selection as sharedSelection } from '../selection';
import { reducedMotion, settings } from '../settings';
import { analysis, detail, pcbStamp, pickNet, pickPart } from '../store';
import { InspectBar, InspectLegend, PreviewLabel } from './InspectBar';
import { KINDS, MARGIN_TOKENS, findingTargets, litNets } from './inspect';
import { colourBy, focus3d, heightPreview, xray } from './inspectMode';
import { type LimitInfo, type PartInfo3d, coverageLine, limitTip, partTip, provenanceCounts, unknownParts } from './model';
import { view3d } from './mode';
import { LOOKS, type Role, TOKENS } from './palette';
import { PRESETS, type Preset } from './presets';
import { type Colours, type Hit, Viewer } from './viewer';
import './view3d.css';

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
      setStatus({ state: 'error', webgl: true, message: e instanceof Error ? e.message : String(e) });
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
      setStatus({ state: 'error', webgl: true, message: 'The graphics context was lost' });
    };
    v.canvas.addEventListener('webglcontextlost', lost);
    // For the recipes: where a part is on screen, and what the view drew, asked of the view itself.
    (el as unknown as { view3d: unknown }).view3d = {
      project: (path: string) => v.project(path),
      sample: () => v.sample(),
      colourAt: (x: number, y: number) => v.colourAt(x, y),
      parts: () => v.partList(),
      preset: () => v.currentPreset(),
      ortho: () => v.isOrtho(),
      nets: () => v.netList(),
      previewCount: () => v.previewCount(),
      focused: () => v.focusedList(),
      litReport: () => v.litReport(),
    };
    view3d.current = {
      preset: (p) => v.preset(p),
      flip: () => v.flip(),
      toggleProjection: () => {
        v.toggleProjection();
        setOrtho(v.isOrtho());
      },
      fit: () => v.fit(),
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
        v.highlight(pcbSelection.peek(), []);
        if (host.current) host.current.dataset.stamp = r.headers.get('X-Netsmith-Layout-Stamp') ?? '';
      } catch (e) {
        if (!ask.signal.aborted) setStatus({ state: 'error', message: e instanceof Error ? e.message : String(e) });
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
      } else if (k === 'Escape') {
        if (pcbSelection.peek().length) {
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
    if (pointed.current) hoverLeave(pointed.current);
    pointed.current = null;
    setTip(null);
  };

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
          const hit = viewer.current?.pick(e.clientX, e.clientY);
          viewer.current?.clearFocus();
          if (hit?.kind === 'part') select(e.shiftKey ? [...new Set([...pcbSelection.peek(), hit.info.path])] : [hit.info.path]);
          else if (hit?.kind === 'copper' && hit.info.net) pickNet(pickedNet === hit.info.net ? null : hit.info.net);
          else if (!e.shiftKey) select([]);
        }}
        onDblClick={(e) => {
          const hit = viewer.current?.pick(e.clientX, e.clientY);
          if (hit?.kind === 'part') pickPart(hit.info.refdes, true);
          else viewer.current?.fit();
        }}
        onPointerMove={onMove}
        onPointerLeave={onLeave}
        onContextMenu={(e) => e.preventDefault()}
      >
        {tip && (
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
          <div class="v3-note" aria-hidden="true">
            Read-only: move parts in 2D · drag to orbit, right-drag to pan, scroll to zoom
          </div>
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
