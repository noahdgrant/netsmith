// The Gerber view of the PCB area (issue 1107): the files of the fab package `fab` last wrote for the board, or of one a
// person opens, as the board house gets them. The server reads and draws them with the reader `netsmith view gerber`
// uses, so the picture is the command's SVG: each file a group in the colour of its function, the bottom mirrored, the
// holes drawn. The page turns files on and off (it hides their groups), measures with a ruler, and lists the holes and
// what the reader did not understand.

import { signal } from '@preact/signals';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';

import type { GerberView as View } from '../api';
import { ApiFailure, api } from '../client';
import { Icon } from '../components/Icons';
import { PanZoom } from '../components/PanZoom';
import { Disclosure, EmptyState, Segmented, Spinner, copyText } from '../components/ui';
import { notify } from '../feedback/state';
import { DrawingSwitch } from '../pcb/DrawingSwitch';
import './gerber.css';
import { fabReports, requestRun } from '../store';
import {
  SCALE,
  command,
  fileLabel,
  holesText,
  measure,
  measureText,
  pointText,
  svgPoint,
  toFrame,
  unreadWhere,
  viewLabel,
  viewTip,
} from './gerber';

type Source = { kind: 'fab' } | { kind: 'opened'; name: string };

/** Which package is shown, the view of it, the files turned off and whether the ruler is on: for the session. */
const source = signal<Source>({ kind: 'fab' });
const layer = signal<string | null>(null);
const hidden = signal<ReadonlySet<string>>(new Set());
const ruler = signal(false);

type Pt = { x: number; y: number };

export function GerberView({ board }: { board: string }) {
  const [view, setView] = useState<View | null>(null);
  const [failure, setFailure] = useState<{ status: number; message: string } | null>(null);
  const [loading, setLoading] = useState(false);
  const [opening, setOpening] = useState(false);
  const [colours, setColours] = useState<Record<string, string>>({});
  const [cursor, setCursor] = useState<Pt | null>(null);
  const [line, setLine] = useState<{ a: Pt; b: Pt; done: boolean } | null>(null);
  const stage = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const src = source.value;
  const want = layer.value;
  // A fab run of this board writes a new package: it is read again.
  const written = fabReports.value[board];

  useEffect(() => {
    let live = true;
    setLoading(true);
    const ask = src.kind === 'fab' ? api.gerber(board, want ?? undefined) : api.gerberOpened(src.name, want ?? undefined);
    ask
      .then((v) => {
        if (!live) return;
        setView(v);
        setFailure(null);
      })
      .catch((e: unknown) => {
        if (!live) return;
        // A view the package has no file for: its first view.
        if (e instanceof ApiFailure && e.status === 400 && want !== null) {
          layer.value = null;
          return;
        }
        if (e instanceof ApiFailure && e.status === 404 && src.kind === 'opened') {
          source.value = { kind: 'fab' };
          return;
        }
        setView(null);
        setFailure({ status: e instanceof ApiFailure ? e.status : 0, message: e instanceof Error ? e.message : String(e) });
      })
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [board, src.kind === 'fab' ? '' : src.name, want, written]);

  // The ruler's line belongs to the drawing it was made on.
  useEffect(() => setLine(null), [view?.svg]);

  // The colour of each file, as the drawing paints it; and the files turned off, hidden.
  useLayoutEffect(() => {
    const host = stage.current;
    if (!host) return;
    const found: Record<string, string> = {};
    for (const g of host.querySelectorAll<SVGGElement>('g.layer')) {
      const file = g.dataset.file ?? '';
      const painted = g.querySelector('path, circle, line');
      const colour = painted?.getAttribute('fill') ?? painted?.getAttribute('stroke');
      if (colour) found[file] = colour;
      g.style.display = hidden.value.has(file) ? 'none' : '';
    }
    setColours((was) => ({ ...was, ...found }));
  }, [view?.svg, hidden.value]);

  const openFile = async (file: File) => {
    setOpening(true);
    try {
      // Another package may not have the view shown now: it opens at its first.
      const v = await api.gerberOpen(file.name, await file.arrayBuffer());
      layer.value = null;
      source.value = { kind: 'opened', name: v.package };
      setView(v);
      setFailure(null);
    } catch (e) {
      notify({ kind: 'bad', title: `${file.name} could not be read`, detail: e instanceof Error ? e.message : String(e) });
    } finally {
      setOpening(false);
    }
  };

  const toggle = (file: string) => {
    const next = new Set(hidden.value);
    if (next.has(file)) next.delete(file);
    else next.add(file);
    hidden.value = next;
  };

  /** The point of the frame under the pointer, and the point of the SVG. */
  const at = (e: PointerEvent): { svg: Pt; frame: Pt } | null => {
    const host = stage.current;
    if (!host || !view || view.region.length !== 4) return null;
    const r = host.getBoundingClientRect();
    const svg = svgPoint(e.clientX - r.left, e.clientY - r.top, r.width, r.height, view.region);
    return { svg, frame: toFrame(svg, view.region, view.mirrored) };
  };

  const shown = view && view.svg !== '';
  const cmd = view?.path ? command(view.path, view.layer) : null;
  const fabbed = src.kind === 'fab';
  const reading = line ? measureText(measure(toFrame(line.a, view!.region, view!.mirrored), toFrame(line.b, view!.region, view!.mirrored))) : null;
  const [rx, ry, rw, rh] = view?.region.length === 4 ? view.region : [0, 0, 1, 1];

  const bar = (
    <div class="toolbar pcb-bar gerber-bar" role="toolbar" aria-label="Gerber view">
      <DrawingSwitch />
      {view && (
        <div class="tb-group">
          <Segmented<string>
            label="Gerber view"
            value={view.layer}
            onChange={(v) => (layer.value = v)}
            options={view.views.map((v) => ({ value: v, label: viewLabel(v), tip: viewTip(v), testid: `gerber-view-${v}` }))}
          />
        </div>
      )}
      <div class="tb-group">
        {view && (
          <button
            class={`tool ${ruler.value ? 'on' : ''}`}
            aria-pressed={ruler.value}
            data-testid="gerber-ruler-toggle"
            data-tip="Measure: drag from one point to another (the middle or right button pans while it is on)"
            onClick={() => {
              ruler.value = !ruler.value;
              if (!ruler.value) setLine(null);
            }}
          >
            <Icon name="ruler" size={14} />
            <span class="tb-label">Ruler</span>
          </button>
        )}
        <button
          class="tool"
          data-testid="gerber-open"
          disabled={opening}
          data-tip="Open a fab package: a .zip of Gerber and drill files, or one file, made by netsmith or another tool"
          onClick={() => input.current?.click()}
        >
          {opening ? <Spinner size={13} /> : <Icon name="folder" size={14} />}
          <span class="tb-label">Open package…</span>
        </button>
        {!fabbed && (
          <button class="tool" data-testid="gerber-fab" data-tip="Show the package fab wrote for this board" onClick={() => (source.value = { kind: 'fab' })}>
            <span class="tb-label">This board's package</span>
          </button>
        )}
        {cmd && (
          <button
            class="tool"
            data-testid="gerber-copy-command"
            data-tip={`Copy the command that draws this view: ${cmd}`}
            onClick={async () => {
              if (await copyText(cmd)) notify({ kind: 'ok', title: 'Copied the netsmith view gerber command', detail: cmd });
            }}
          >
            <Icon name="terminal" size={14} />
            <span class="tb-label">Copy command</span>
          </button>
        )}
        <input
          ref={input}
          type="file"
          accept=".zip,.gbr,.ger,.drl,.xln,.gtl,.gbl,.gts,.gbs,.gto,.gbo,.gtp,.gbp,.gko,.gm1,application/zip"
          hidden
          data-testid="gerber-file"
          onChange={(e) => {
            const f = (e.currentTarget as HTMLInputElement).files?.[0];
            (e.currentTarget as HTMLInputElement).value = '';
            if (f) void openFile(f);
          }}
        />
      </div>
      <div class="tb-group status">
        {loading && <Spinner size={13} />}
        {view && view.unread.length > 0 && (
          <span class="chip bad" data-testid="gerber-unread-chip">
            {view.unread.length} not understood
          </span>
        )}
      </div>
      <span class="spacer" />
      <span class="cursor-at" data-testid="gerber-ruler" data-mm={line ? measure(toFrame(line.a, view!.region, view!.mirrored), toFrame(line.b, view!.region, view!.mirrored)).d.toFixed(3) : undefined}>
        {reading ?? (cursor ? pointText(cursor) : '')}
      </span>
    </div>
  );

  if (!view) {
    return (
      <>
        {bar}
        {failure ? (
          failure.status === 404 && fabbed ? (
            <EmptyState
              icon="fab"
              title="No fab package for this board yet"
              help="Make fab files writes the Gerbers for the board house; they are drawn here as it will get them. Or open a package another tool made."
              testid="gerber-none"
              actions={
                <>
                  <button class="tool primary" onClick={() => requestRun('fab')}>
                    Make fab files
                  </button>
                  <button class="tool" onClick={() => input.current?.click()}>
                    Open package…
                  </button>
                </>
              }
            />
          ) : (
            <EmptyState icon="error" tone="bad" title="The package could not be read" testid="gerber-failed">
              <div class="empty-error">
                <span class="text">{failure.message}</span>
              </div>
            </EmptyState>
          )
        ) : (
          <div class="pcb-main">
            <Spinner size={16} />
          </div>
        )}
      </>
    );
  }

  return (
    <>
      {bar}
      <div class="pcb-main gerber-main">
        {shown ? (
          <PanZoom
            width={rw * SCALE}
            height={rh * SCALE}
            fitKey={`gerber:${view.package}:${view.layer}:${rw}x${rh}`}
            class="pcb-host gerber-host"
            primaryPans={!ruler.value}
            scale100={1}
            scaleTitle="Zoom. 100% is 12 px to the millimetre"
            label="The Gerber files of the package"
          >
            <div
              ref={stage}
              class={`gerber-stage ${ruler.value ? 'measuring' : ''}`}
              data-testid="gerber-drawing"
              data-layer={view.layer}
              data-mirrored={view.mirrored ? '1' : undefined}
              data-package={view.package}
              style={{ width: `${rw * SCALE}px`, height: `${rh * SCALE}px` }}
              onPointerDown={(e) => {
                if (!ruler.value || e.button !== 0) return;
                const p = at(e);
                if (!p) return;
                e.stopPropagation();
                (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
                setLine({ a: p.svg, b: p.svg, done: false });
              }}
              onPointerMove={(e) => {
                const p = at(e);
                setCursor(p?.frame ?? null);
                if (p && line && !line.done) setLine({ ...line, b: p.svg });
              }}
              onPointerUp={(e) => {
                const p = at(e);
                if (line && !line.done) setLine({ a: line.a, b: p?.svg ?? line.b, done: true });
              }}
              onPointerLeave={() => setCursor(null)}
            >
              <div class="gerber-svg" dangerouslySetInnerHTML={{ __html: view.svg }} />
              {line && (
                <svg class="gerber-measure" viewBox={`${rx} ${ry} ${rw} ${rh}`} aria-hidden="true">
                  <line x1={line.a.x} y1={line.a.y} x2={line.b.x} y2={line.b.y} />
                  <circle cx={line.a.x} cy={line.a.y} r={0.15} />
                  <circle cx={line.b.x} cy={line.b.y} r={0.15} />
                </svg>
              )}
            </div>
          </PanZoom>
        ) : (
          <EmptyState icon="error" tone="bad" title="Nothing of this view could be read" help="The files of this view use what the reader does not understand: they are listed at the side." />
        )}
        <aside class="side no-pan gerber-side" data-testid="gerber-side">
          <div class="gerber-package">
            <b>{view.package}</b>
            <span class="gerber-muted">
              {fabbed ? 'what fab wrote for this board' : 'opened'}
              {view.board ? ` · board ${view.board.width.toFixed(2)} × ${view.board.height.toFixed(2)} mm` : ' · no outline file'}
            </span>
          </div>
          <div class="rows">
            <Disclosure title="Layers" count={view.files.length}>
              {view.files.map((f) => {
                const drawn = view.drawn.includes(f.file);
                const off = hidden.value.has(f.file);
                return (
                  <label
                    key={f.file}
                    class={`gerber-layer ${drawn ? '' : 'absent'} ${f.read ? '' : 'unread'}`}
                    data-testid="gerber-layer"
                    data-file={f.file}
                    data-function={f.function}
                    data-layer={f.layer ?? undefined}
                    title={`${f.file}${f.fileFunction ? ` (${f.fileFunction})` : ''}${f.identifiedBy ? `; its function from its ${f.identifiedBy === 'x2' ? 'X2 attribute' : f.identifiedBy}` : ''}${drawn ? '' : '; not in this view'}`}
                  >
                    <input type="checkbox" checked={!off} disabled={!drawn} onChange={() => toggle(f.file)} />
                    <span class="swatch" style={{ background: colours[f.file] ?? 'transparent' }} />
                    <span class="gerber-layer-name">{fileLabel(f)}</span>
                    <span class="gerber-layer-file">{f.file.split('/').pop()}</span>
                  </label>
                );
              })}
            </Disclosure>
            {view.holes.length > 0 && (
              <Disclosure title="Holes" count={view.holes.reduce((n, h) => n + h.holes + h.slots, 0)}>
                {view.holes.map((h) => (
                  <div key={`${h.diameter}-${h.plated}`} class="gerber-note">
                    {holesText(h)}
                  </div>
                ))}
              </Disclosure>
            )}
            {view.unread.length > 0 && (
              <Disclosure title="Not understood" count={view.unread.length} class="gerber-unread">
                {view.unread.map((u) => (
                  <div key={u.file} class="gerber-note bad" data-testid="gerber-unread">
                    <b>{unreadWhere(u)}</b>
                    <div>{u.message}</div>
                    <div class="gerber-muted">not drawn</div>
                  </div>
                ))}
              </Disclosure>
            )}
            {view.skipped.length > 0 && (
              <Disclosure title="Not Gerber or drill" count={view.skipped.length} open={false}>
                {view.skipped.map((s) => (
                  <div key={s.file} class="gerber-note gerber-muted">
                    {s.file}: {s.why}
                  </div>
                ))}
              </Disclosure>
            )}
          </div>
        </aside>
      </div>
    </>
  );
}
