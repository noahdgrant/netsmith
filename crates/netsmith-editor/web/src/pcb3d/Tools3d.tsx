// The 3D view's tools on screen (tools.ts has their numbers): the overlay drawn over the canvas in
// the window's pixels (the measurements and their labels, the snap target under the pointer, a
// picked part's height and its limit, the section plane's outline, the layers' labels of the
// exploded stackup), and the panels of the measure tool and the section plane. The overlay is
// drawn again after every frame of the view, so it follows the camera; it is crisp at any zoom
// because it is lines and text of the page, not of the scene.

import { useEffect, useState } from 'preact/hooks';

import { Icon } from '../components/Icons';
import { Segmented } from '../components/ui';
import type { PartInfo3d } from './model';
import { type Axis, AXES, type Measurement, type P3, type Snap, fixed, measureWords, mm3, parseCut, partReadout } from './tools';
import type { Viewer } from './viewer';

/** A picked part's height and the limit above it, drawn as two upright lines at its box's corner. */
export interface Readout {
  part: PartInfo3d;
  words: ReturnType<typeof partReadout>;
  /** The corner the lines stand at, on the face, and the face's z. */
  foot: P3;
}

function Marker({ kind, x, y }: { kind: Snap['kind']; x: number; y: number }) {
  if (kind === 'pad') return <circle class="v3m-snap" cx={x} cy={y} r={6} />;
  if (kind === 'corner') return <rect class="v3m-snap" x={x - 5} y={y - 5} width={10} height={10} />;
  if (kind === 'edge') return <path class="v3m-snap" d={`M${x} ${y - 6.5}L${x + 6.5} ${y}L${x} ${y + 6.5}L${x - 6.5} ${y}Z`} />;
  if (kind === 'top' || kind === 'face') return <path class="v3m-snap" d={`M${x - 6} ${y}H${x + 6}M${x} ${y - 6}V${y + 6}`} />;
  return <circle class="v3m-snap v3m-dot" cx={x} cy={y} r={2.5} />;
}

export function Overlay3d({
  viewer,
  measures,
  pending,
  hover,
  readout,
  exploded,
  cut,
  measuring,
}: {
  viewer: Viewer;
  measures: Measurement[];
  pending: Snap | null;
  hover: Snap | null;
  readout: Readout | null;
  exploded: boolean;
  cut: { axis: Axis; at: number } | null;
  measuring: boolean;
}) {
  const [, setTick] = useState(0);
  useEffect(() => viewer.onDraw(() => setTick((t) => (t + 1) % 1e6)), [viewer]);
  const at = (p: P3) => viewer.toScreen(p);
  const lines: preact.JSX.Element[] = [];
  const labels: preact.JSX.Element[] = [];
  const showMeasures = !exploded;

  if (showMeasures)
    measures.forEach((m, i) => {
      const a = at(m.a.at);
      const b = at(m.b.at);
      if (!a.front || !b.front) return;
      const w = measureWords(m.a.at, m.b.at);
      lines.push(
        <g key={`m${m.id}`} class="v3m-measure">
          <line class="v3m-halo" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
          <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
          <circle cx={a.x} cy={a.y} r={3} />
          <circle cx={b.x} cy={b.y} r={3} />
        </g>,
      );
      labels.push(
        <div key={`l${m.id}`} class="v3m-label" style={{ left: (a.x + b.x) / 2, top: (a.y + b.y) / 2 }} data-testid="measure-label" title={`${m.a.label} to ${m.b.label}`}>
          <b>
            <span class="v3m-n">{i + 1}</span>
            {w.distance}
          </b>
          <span>{w.parts}</span>
        </div>,
      );
    });

  if (measuring && showMeasures && pending) {
    const a = at(pending.at);
    lines.push(<circle key="pa" class="v3m-pending" cx={a.x} cy={a.y} r={3.5} />);
    if (hover) {
      const b = at(hover.at);
      const w = measureWords(pending.at, hover.at);
      lines.push(<line key="pl" class="v3m-rubber" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />);
      labels.push(
        <div key="pw" class="v3m-label v3m-live" style={{ left: (a.x + b.x) / 2, top: (a.y + b.y) / 2 }}>
          <b>{w.distance}</b>
          <span>{w.parts}</span>
        </div>,
      );
    }
  }

  if (measuring && hover && showMeasures) {
    const h = at(hover.at);
    lines.push(<Marker key="snap" kind={hover.kind} x={h.x} y={h.y} />);
    labels.push(
      <div key="snapw" class="v3m-snapword" style={{ left: h.x + 12, top: h.y + 10 }} data-testid="measure-snap" data-kind={hover.kind}>
        {hover.label}
        <span>
          {fixed(hover.at[0])}, {fixed(hover.at[1])}, {fixed(hover.at[2])}
        </span>
      </div>,
    );
  }

  if (readout && measuring && showMeasures) {
    const { foot, words, part } = readout;
    const base = at(foot);
    const sign = part.side === 'top' ? 1 : -1;
    if (part.heightMm !== null && base.front) {
      const top = at([foot[0], foot[1], foot[2] + sign * part.heightMm]);
      lines.push(
        <g key="rh" class="v3m-measure">
          <line class="v3m-halo" x1={base.x} y1={base.y} x2={top.x} y2={top.y} />
          <line x1={base.x} y1={base.y} x2={top.x} y2={top.y} />
          <path d={`M${base.x - 5} ${base.y}H${base.x + 5}M${top.x - 5} ${top.y}H${top.x + 5}`} />
        </g>,
      );
      labels.push(
        <div key="rhw" class="v3m-label v3m-side" style={{ left: (base.x + top.x) / 2 + 10, top: (base.y + top.y) / 2 }} data-testid="measure-height">
          <b>{mm3(part.heightMm)}</b>
          <span>above the {part.side} face</span>
        </div>,
      );
    }
    if (words.limitMm !== null && base.front) {
      const lim = at([foot[0], foot[1], foot[2] + sign * words.limitMm]);
      const over = words.marginMm !== null && words.marginMm < 0;
      lines.push(<line key="rl" class={`v3m-limit ${over ? 'over' : ''}`} x1={base.x - 12} y1={lim.y} x2={base.x + 12} y2={lim.y} />);
      labels.push(
        <div key="rlw" class={`v3m-label v3m-side v3m-lim ${over ? 'over' : ''}`} style={{ left: base.x - 14, top: lim.y }} data-testid="measure-limit">
          <b>{words.marginMm === null ? `${words.limitName} ${mm3(words.limitMm)}` : words.marginMm < 0 ? `${mm3(-words.marginMm)} over` : `${mm3(words.marginMm)} to spare`}</b>
          <span>limit {words.limitName}</span>
        </div>,
      );
    }
  }

  if (cut) {
    const box = viewer.sceneBox();
    const i = AXES.indexOf(cut.axis);
    const [j, k] = [0, 1, 2].filter((x) => x !== i);
    const corner = (u: number, v: number): P3 => {
      const p: P3 = [0, 0, 0];
      p[i] = cut.at;
      p[j] = u ? box.max[j] : box.min[j];
      p[k] = v ? box.max[k] : box.min[k];
      return p;
    };
    const pts = [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)].map(at);
    if (pts.every((p) => p.front)) lines.push(<polygon key="cut" class="v3m-cut" points={pts.map((p) => `${p.x},${p.y}`).join(' ')} />);
  }

  if (exploded) {
    const b = viewer.board;
    for (const s of viewer.slots()) {
      const p = at([b?.widthMm ?? 0, 0, s.now]);
      if (!p.front) continue;
      lines.push(<line key={`sl${s.key}`} class="v3m-leader" x1={p.x + 4} y1={p.y} x2={p.x + 26} y2={p.y} />);
      labels.push(
        <div key={`s${s.key}`} class="v3m-layer" style={{ left: p.x + 30, top: p.y }} data-testid="stack-label" data-layer={s.key}>
          <b>{s.label}</b>
          <span>{s.detail}</span>
        </div>,
      );
    }
  }

  return (
    <div class="v3m" aria-hidden="true">
      <svg class="v3m-svg">{lines}</svg>
      {labels}
    </div>
  );
}

/** The measure tool's panel: what to do, the measurements, the picked part's height and margin. */
export function MeasurePanel({
  measures,
  readout,
  exploded,
  onRemove,
  onClear,
  onClose,
}: {
  measures: Measurement[];
  readout: Readout | null;
  exploded: boolean;
  onRemove(id: number): void;
  onClear(): void;
  onClose(): void;
}) {
  return (
    <div class="v3-panel v3-measure no-pan" role="region" aria-label="Measure" data-testid="measure-panel" data-toast-clear>
      <div class="v3-panel-head">
        <Icon name="ruler" size={14} />
        <b>Measure</b>
        <span class="v3-panel-hint">{exploded ? 'Put the layers back to measure' : 'Click two points · Shift+click a part for its height · Esc leaves'}</span>
        <button class="icon-btn" aria-label="Leave the measure tool" data-tip="Leave the measure tool" data-tip-keys="Esc" onClick={onClose}>
          <Icon name="close" size={13} />
        </button>
      </div>
      {measures.length > 0 && (
        <ol class="v3-measures" data-testid="measure-list">
          {measures.map((m, i) => {
            const w = measureWords(m.a.at, m.b.at);
            return (
              <li key={m.id}>
                <span class="v3m-n">{i + 1}</span>
                <b class="mono">{w.distance}</b>
                <span class="mono v3-parts">{w.parts}</span>
                <span class="v3-ends">
                  {m.a.label} → {m.b.label}
                </span>
                <button class="icon-btn" aria-label={`Remove measurement ${i + 1}`} onClick={() => onRemove(m.id)}>
                  <Icon name="close" size={12} />
                </button>
              </li>
            );
          })}
        </ol>
      )}
      {readout && (
        <div class="v3-readout" data-testid="measure-readout">
          <b class="mono">{readout.part.refdes}</b>
          <span>{readout.words.height}</span>
          <span>{readout.words.limit}</span>
        </div>
      )}
      {measures.length > 0 && (
        <button class="tool small v3-clear" data-testid="measure-clear" onClick={onClear}>
          Clear all
        </button>
      )}
    </div>
  );
}

/** The section plane's panel: the axis, a handle to drag the cut, and its place typed in mm. */
export function SectionPanel({
  cut,
  range,
  onChange,
  onClose,
}: {
  cut: { axis: Axis; at: number };
  range: [number, number];
  onChange(cut: { axis: Axis; at: number }): void;
  onClose(): void;
}) {
  const [text, setText] = useState(fixed(cut.at));
  useEffect(() => setText(fixed(cut.at)), [cut.at, cut.axis]);
  const commit = () => {
    const v = parseCut(text, range);
    if (v === null) setText(fixed(cut.at));
    else onChange({ ...cut, at: v });
  };
  return (
    <div class="v3-panel v3-section no-pan" role="region" aria-label="Section plane" data-testid="section-panel" data-toast-clear>
      <span class="v3-panel-title">Section</span>
      <Segmented<Axis>
        label="Cut across"
        value={cut.axis}
        onChange={(axis) => onChange({ axis, at: cut.at })}
        options={AXES.map((a) => ({ value: a, label: a.toUpperCase(), tip: `Cut across ${a.toUpperCase()}`, keys: a.toUpperCase(), testid: `section-${a}` }))}
      />
      <input
        type="range"
        class="v3-handle"
        aria-label={`Where the cut is along ${cut.axis.toUpperCase()}, in mm`}
        data-testid="section-handle"
        min={range[0]}
        max={range[1]}
        step={cut.axis === 'z' ? 0.005 : 0.05}
        value={cut.at}
        onInput={(e) => onChange({ ...cut, at: Number((e.target as HTMLInputElement).value) })}
      />
      <label class="v3-at">
        <span>{cut.axis} =</span>
        <input
          class="mono"
          inputMode="decimal"
          aria-label={`The cut's place along ${cut.axis.toUpperCase()}, in mm`}
          data-testid="section-at"
          value={text}
          onInput={(e) => setText((e.target as HTMLInputElement).value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            if (e.key === 'Escape') {
              setText(fixed(cut.at));
              (e.target as HTMLInputElement).blur();
            }
            e.stopPropagation();
          }}
          onBlur={commit}
        />
        <span>mm</span>
      </label>
      <span class="v3-panel-hint" data-tip="As netsmith render --section: the side nearer the camera is not drawn">
        keeps the far side
      </span>
      <button class="icon-btn" aria-label="Turn the section off" data-tip="Turn the section off" data-tip-keys="S" onClick={onClose}>
        <Icon name="close" size={13} />
      </button>
    </div>
  );
}

