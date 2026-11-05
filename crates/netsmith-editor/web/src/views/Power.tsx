import type { ComponentChildren } from 'preact';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import type { PowerTree } from '../api';
import { api } from '../client';
import { NoBoard, ViewLoading } from '../components/ViewStatus';
import { MiniMap } from '../components/MiniMap';
import { PanZoom } from '../components/PanZoom';
import { analysis, board, detail, goTo, systemName } from '../store';
import { type Card, cardFor, partByPath, revealRequest, selection, selectionOfCard, setSelection } from '../selection';
import { DOTS, amps, loadsText, voltRangeText, voltsText } from '../power/format';
import { type Item, type Placed, type TreeLayout, layoutTree } from '../flow/layout';
import { RAIL_ANCHOR, RAIL_W, type Open, STAGE_W, closed, railHeight, stageHeight } from '../power/metrics';
import {
  type Issue,
  type Level,
  type Load,
  type PowerModel,
  type RailModel,
  type StageKind,
  type StageModel,
  levels,
  modelOf,
} from '../power/model';
import { type Move, linkKey, move, trace } from '../flow/select';

/** A minus sign that is as wide as a plus. */
const minus = (t: string) => t.replace('-', '−');

function Amount({ x }: { x: number }) {
  const a = amps(x);
  return (
    <span class="pt-amount">
      <span class="pt-n">{minus(a.num)}</span>
      <span class="pt-u">{a.unit}</span>
    </span>
  );
}

/** A range of currents as two columns that line up from row to row. */
function Span({ lo, hi }: { lo: number; hi: number }) {
  const same = amps(lo).num === amps(hi).num && amps(lo).unit === amps(hi).unit;
  return (
    <span class="pt-span">
      <Amount x={lo} />
      <span class="pt-dots">{same ? '' : DOTS.trim()}</span>
      {same ? <span class="pt-amount" /> : <Amount x={hi} />}
    </span>
  );
}

const GLYPH: Record<StageKind, ComponentChildren> = {
  buck: <path d="M2 4h4v8h8" />,
  boost: <path d="M2 12h4V4h8" />,
  ldo: <path d="M1.5 8h3l1.5-4 2 8 2-8 1.5 4h3" />,
  protection: <path d="M8 1.8l5 1.8v4c0 3-2.2 5-5 6.4C5.2 12.600 3 10.600 3 7.600v-4z" />,
  filter: <path d="M1.500 8c1.500-5 3-5 4.500 0s3 5 4.500 0 2-3 4-3" />,
  switch: <path d="M2 11h3M11 11h3M5 11l6-5" />,
  diode: <path d="M2 8h3M11 8h3M5 4v8l6-4zM11 4v8" />,
  link: <path d="M6.500 9.500l3-3M5 8L3.500 9.500a2.100 2.100 0 003 3L8 11M8 5l1.500-1.500a2.100 2.100 0 013 3L11 8" />,
  converter: <path d="M2.500 3.500h11v9h-11zM6 8h4" />,
};

function Glyph({ kind }: { kind: StageKind }) {
  return (
    <svg class="pt-glyph" width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      {GLYPH[kind]}
    </svg>
  );
}

const badgeText = (level: Level) => (level === 'bad' ? 'Over limit' : 'Check');

/** What a hover on a load shows: the part and the pin, the current, and the way to the source. */
interface Tip {
  load: Load;
  x: number;
  y: number;
}

function LoadRow({ l, supply, onTip }: { l: Load; supply?: boolean; onTip: (t: Tip | null, now?: boolean) => void }) {
  const show = (e: { currentTarget: Element }, now: boolean) => {
    const r = e.currentTarget.getBoundingClientRect();
    onTip({ load: l, x: r.left, y: r.bottom }, now);
  };
  return (
    <button
      class={`pt-row ${supply ? 'supply' : ''} ${l.unknown ? 'unknown' : ''}`}
      onClick={(e) => {
        e.stopPropagation();
        if (l.at) void goTo(l.at);
      }}
      onMouseEnter={(e) => show(e, false)}
      onMouseLeave={() => onTip(null)}
      onFocus={(e) => show(e, true)}
      onBlur={() => onTip(null)}
      aria-label={`${l.who}: ${l.unknown ? 'a current the solver has not settled' : supply ? 'supplies' : 'draws'}${l.unknown ? '' : ` ${amps(l.lo).num} ${amps(l.lo).unit} to ${amps(l.hi).num} ${amps(l.hi).unit}`}. Show in source.`}
    >
      <span class="pt-who">
        {supply && <span class="pt-arrow" aria-hidden="true">{'◂'}</span>}
        {l.who}
      </span>
      {l.unknown ? <span class="pt-unsettled">not settled</span> : <Span lo={l.lo} hi={l.hi} />}
    </button>
  );
}

function RailCard({
  r,
  p,
  level,
  open,
  setOpen,
  selected,
  tint,
  dim,
  onSelect,
  onTip,
}: {
  r: RailModel;
  p: Placed;
  level: Level | undefined;
  open: Open;
  setOpen: (o: Open) => void;
  selected: boolean;
  tint: string;
  dim: boolean;
  onSelect: () => void;
  onTip: (t: Tip | null, now?: boolean) => void;
}) {
  const nominal = r.voltage ? (r.voltage.lo + r.voltage.hi) / 2 : null;
  const bar = r.fill === null ? 0 : Math.min(1, Math.max(0, r.fill));
  const tone = r.budget === 'over' ? 'bad' : r.fill !== null && r.fill > 0.85 ? 'warn' : r.capacity ? 'ok' : 'plain';
  const count = r.loads.length;
  const nonzero = r.top.length + r.more.length;
  const parts = [
    r.feeds.length > 0 ? `feeds ${r.feeds.length === 1 ? 'a converter' : `${r.feeds.length} converters`}` : '',
  ].filter(Boolean);
  const issues = !r.sourced ? 'No source' : level === 'bad' ? badgeText('bad') : level === 'warn' ? badgeText('warn') : '';
  return (
    <div
      class={`pt-node pt-rail ${level ?? ''} ${selected ? 'selected' : ''} ${tint} ${dim ? 'dim' : ''}`}
      data-node={r.id}
      data-kind="rail"
      style={{ left: p.x, top: p.y, width: p.w, height: p.h }}
      onClick={onSelect}
    >
      <div class="pt-head">
        <span class="pt-name" title={r.net}>
          {r.net}
        </span>
        {r.board && <span class="pt-chip">{r.board}</span>}
        {issues && <span class={`pt-badge ${level ?? 'warn'}`}>{issues}</span>}
      </div>
      <div class="pt-volt">
        {nominal !== null && <b>{voltsText(nominal)}</b>}
        {r.voltage && <span class="pt-range">{voltRangeText(r.voltage.lo, r.voltage.hi)}</span>}
      </div>
      {r.supplies.length > 0 && (
        <div class="pt-supplies">
          {r.supplies.slice(0, 3).map((l) => (
            <LoadRow key={l.who} l={l} supply onTip={onTip} />
          ))}
          {r.supplies.length > 3 && <div class="pt-restsup">+{r.supplies.length - 3} more supplies</div>}
        </div>
      )}
      <div class="pt-total">
        <span class="pt-label">Total load</span>
        {r.demand ? <Span lo={r.demand.lo} hi={r.demand.hi} /> : <span class="pt-unsettled">not settled</span>}
      </div>
      <div class={`pt-meter ${tone}`} role="img" aria-label={r.capacity ? `${Math.round(bar * 100)} % of the supply's limit` : 'no supply limit'}>
        <i style={{ width: `${Math.round(bar * 100)}%` }} />
      </div>
      <div class={`pt-note ${tone}`} title={r.capacityFrom ? `The limit is what ${r.capacityFrom} can give` : undefined}>
        {r.capacity && r.headroom !== null ? (
          r.headroom >= 0 ? (
            <>
              Headroom <b>{amps(r.headroom).num} {amps(r.headroom).unit}</b> of {amps(r.capacity.hi).num} {amps(r.capacity.hi).unit}
            </>
          ) : (
            <>
              Over by <b>{amps(-r.headroom).num} {amps(-r.headroom).unit}</b> of {amps(r.capacity.hi).num} {amps(r.capacity.hi).unit}
            </>
          )
        ) : (
          'No supply limit stated'
        )}

      </div>
      <button
        class="pt-summary"
        aria-expanded={open.list}
        onClick={(e) => {
          e.stopPropagation();
          setOpen({ ...open, list: !open.list });
        }}
        title={open.list ? 'Hide the loads' : 'List the loads, heaviest first'}
      >
        <span class="pt-chev" aria-hidden="true">{open.list ? '▾' : '▸'}</span>
        {count > 0 && <span>{loadsText(count)}</span>}
        {parts.length > 0 && <span class={count > 0 ? 'pt-aside' : ''}>{count > 0 ? parts.join(', ') : parts.join(', ').replace(/^./, (c) => c.toUpperCase())}</span>}
        {count === 0 && parts.length === 0 && <span>No loads</span>}
      </button>
      {open.list && (
        <div class="pt-list">
          {r.top.map((l) => (
            <LoadRow key={l.who} l={l} onTip={onTip} />
          ))}
          {open.more && r.more.map((l) => <LoadRow key={l.who} l={l} onTip={onTip} />)}
          {!open.more && r.more.length > 0 && (
            <button
              class="pt-fold"
              onClick={(e) => {
                e.stopPropagation();
                setOpen({ ...open, more: true });
              }}
            >
              +{r.more.length} more
            </button>
          )}
          {r.unknowns.map((l) => (
            <LoadRow key={l.who} l={l} onTip={onTip} />
          ))}
          {r.zeros.length > 0 && (
            <button
              class="pt-fold zero"
              aria-expanded={open.zeros}
              onClick={(e) => {
                e.stopPropagation();
                setOpen({ ...open, zeros: !open.zeros });
              }}
            >
              <span class="pt-chev" aria-hidden="true">{open.zeros ? '▾' : '▸'}</span>
              {r.zeros.length} {r.zeros.length === 1 ? 'load' : 'loads'} at 0 A
              {nonzero === 0 ? '' : ''}
            </button>
          )}
          {open.zeros && r.zeros.map((l) => <LoadRow key={l.who} l={l} onTip={onTip} />)}
        </div>
      )}
    </div>
  );
}

function StageCard({ s, p, level, selected, tint, dim, onSelect }: { s: StageModel; p: Placed; level: Level | undefined; selected: boolean; tint: string; dim: boolean; onSelect: () => void }) {
  return (
    <div
      class={`pt-node pt-stage kind-${s.kind} ${level ?? ''} ${selected ? 'selected' : ''} ${tint} ${dim ? 'dim' : ''}`}
      data-node={s.id}
      data-kind="stage"
      style={{ left: p.x, top: p.y, width: p.w, height: p.h }}
      onClick={onSelect}
    >
      <div class="pt-stage-head">
        <Glyph kind={s.kind} />
        <span class="pt-stage-title">{s.title}</span>
      </div>
      <div class="pt-stage-who">
        <button
          class="pt-stage-name"
          title={s.at ? `${s.name} (${s.type}): show in source` : s.name}
          onClick={(e) => {
            e.stopPropagation();
            if (s.at) void goTo(s.at);
          }}
        >
          {s.name}
        </button>
        {s.type && <span class="pt-stage-type">{s.type}</span>}
      </div>
      {s.efficiency && <div class="pt-eff">Efficiency {s.efficiency}</div>}
      {s.efficiencyUnknown && <div class="pt-eff unknown">Efficiency not settled</div>}
    </div>
  );
}

/** The view of a tree: what the section shows once the editor has answered. */
export function PowerTreeView({ tree, fitKey }: { tree: PowerTree; fitKey: string }) {
  const model: PowerModel = useMemo(() => modelOf(tree), [tree]);
  const [opens, setOpens] = useState<Record<string, Open>>({});
  const [tip, setTip] = useState<Tip | null>(null);
  const [reveal, setReveal] = useState<{ x: number; y: number; w: number; h: number; seq: number } | undefined>();
  const [centre, setCentre] = useState<{ x: number; y: number; seq: number } | undefined>();
  const [view, setView] = useState<{ k: number; x: number; y: number; w: number; h: number } | null>(null);
  const timer = useRef<number | undefined>(undefined);
  /** Where an arrow key came from, so the opposite key goes back the same way. */
  const came = useRef(new Map<string, string>());
  const host = useRef<HTMLDivElement>(null);

  const openOf = (id: string): Open => opens[id] ?? closed;
  const rails = new Map(model.rails.map((r) => [r.id, r]));
  const stages = new Map(model.stages.map((s) => [s.id, s]));
  const items: Item[] = [
    ...model.rails.map((r): Item => ({ id: r.id, kind: 'rail', w: RAIL_W, h: railHeight(r, openOf(r.id)), anchor: RAIL_ANCHOR })),
    ...model.stages.map((s): Item => {
      const h = stageHeight(s);
      return { id: s.id, kind: 'stage', w: STAGE_W, h, anchor: h / 2 };
    }),
  ];
  const layout: TreeLayout = useMemo(
    () => layoutTree(items, model.links),
    // The items change with what is open.
    [model, JSON.stringify(items.map((i) => [i.id, i.h]))],
  );
  const placed = new Map(layout.nodes.map((n) => [n.id, n]));
  const lv = useMemo(() => levels(model.issues), [model]);
  // The selection is the page's: a rail stands for its net, a converter for the part or module it is.
  const cards: Card[] = useMemo(
    () => [
      ...model.rails.map((r): Card => ({ id: r.id, kind: 'net', key: r.net })),
      ...model.stages.map((s): Card => ({ id: s.id, kind: partByPath(detail.value, s.name) ? 'part' : 'module', key: s.name })),
    ],
    [model, detail.value],
  );
  const selected = cardFor(selection.value, cards);
  const path = useMemo(() => (selected ? trace(model.links, selected) : null), [model, selected]);
  const ask = revealRequest.value;
  useEffect(() => {
    const n = selected ? placed.get(selected) : undefined;
    if (ask && n) setReveal({ x: n.x, y: n.y, w: n.w, h: n.h, seq: (reveal?.seq ?? 0) + 1 });
  }, [ask?.nonce]);

  const showTip = (t: Tip | null, now?: boolean) => {
    window.clearTimeout(timer.current);
    if (t === null) {
      timer.current = window.setTimeout(() => setTip(null), 220);
    } else if (now) setTip(t);
    else timer.current = window.setTimeout(() => setTip(t), 260);
  };

  const select = (id: string | null, scroll = false) => {
    setSelection(id ? selectionOfCard(cards, id) : null);
    const n = id ? placed.get(id) : undefined;
    if (scroll && n) setReveal({ x: n.x, y: n.y, w: n.w, h: n.h, seq: (reveal?.seq ?? 0) + 1 });
  };

  const setOpen = (id: string, o: Open) => setOpens({ ...opens, [id]: o });
  const everyOpen = model.rails.every((r) => openOf(r.id).list);
  const setAll = (list: boolean) => setOpens(Object.fromEntries(model.rails.map((r) => [r.id, { ...openOf(r.id), list }])));

  const onKey = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const keys: Record<string, Move> = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down' };
    if (keys[e.key]) {
      e.preventDefault();
      e.stopPropagation();
      const next = move(layout, model.links, selected, keys[e.key], selected ? (came.current.get(selected) ?? null) : null);
      if (next) {
        if (selected && (keys[e.key] === 'left' || keys[e.key] === 'right')) came.current.set(next, selected);
        select(next, true);
      }
    } else if (e.key === 'Escape' && selected) {
      e.preventDefault();
      e.stopPropagation();
      select(null);
    } else if ((e.key === 'Enter' || e.key === ' ') && selected && rails.has(selected) && e.target === host.current) {
      e.preventDefault();
      setOpen(selected, { ...openOf(selected), list: !openOf(selected).list });
    }
  };

  const jump = (i: Issue) => select(i.id, true);

  return (
    <div class="graph-view power-view">
      <div class="graph-title pt-bar">
        <span>
          Power tree of <b>{tree.title}</b>
        </span>
        <div class="pt-issues" role="group" aria-label="Issues">
          {model.issues.length === 0 ? (
            <span class="pt-fine">No issues: every rail has a source and a known current.</span>
          ) : (
            <>
              <span class="pt-issues-label">Issues {model.issues.length}</span>
              {model.issues.map((i, n) => (
                <button key={n} class={`pt-issue ${i.level}`} onClick={() => jump(i)} title="Go to it in the tree">
                  <b>{i.where}</b> {i.text}
                </button>
              ))}
            </>
          )}
        </div>
        <span class="spacer" />
        <button class="tool small" onClick={() => setAll(!everyOpen)} title="List the loads of every rail, or hide them all">
          {everyOpen ? 'Hide all loads' : 'List all loads'}
        </button>
      </div>
      <div
        class="pt-host"
        ref={host}
        tabIndex={0}
        role="application"
        aria-label="Power tree. Arrow keys move along it, Escape clears the selection, Enter lists the loads of a rail."
        onKeyDown={onKey}
        onClick={(e) => {
          // A click on the empty canvas clears the selection.
          if (!(e.target as Element).closest('.pt-node, .pt-minimap, .zoombar')) select(null);
        }}
      >
        <PanZoom
          width={layout.width}
          height={layout.height}
          fitKey={fitKey}
          class="graph-host power-host"
          actualSize
          reveal={reveal}
          centre={centre}
          onViewport={setView}
        >
          <svg class="edges pt-edges" width={layout.width} height={layout.height}>
            <defs>
              <marker id="pt-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                <path d="M0,0 L10,5 L0,10 z" class="pt-arrowhead" />
              </marker>
              <marker id="pt-arrow-up" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                <path d="M0,0 L10,5 L0,10 z" class="pt-arrowhead up" />
              </marker>
              <marker id="pt-arrow-down" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                <path d="M0,0 L10,5 L0,10 z" class="pt-arrowhead down" />
              </marker>
            </defs>
            {layout.edges.map((e) => {
              const k = linkKey(e);
              const tint = path?.up.links.has(k) ? 'up' : path?.down.links.has(k) ? 'down' : '';
              return (
                <path
                  key={k}
                  data-from={e.from}
                  data-to={e.to}
                  data-points={JSON.stringify(e.points)}
                  data-back={e.back ? 'true' : undefined}
                  d={e.d}
                  class={`pt-edge ${tint ? `hot ${tint}` : ''} ${path && !tint ? 'dim' : ''}`}
                  marker-end={tint ? `url(#pt-arrow-${tint})` : 'url(#pt-arrow)'}
                />
              );
            })}
          </svg>
          {layout.nodes.map((n) => {
            const dim = path !== null && !path.nodes.has(n.id);
            const tint = path?.up.nodes.has(n.id) ? 'up' : path?.down.nodes.has(n.id) ? 'down' : '';
            const r = rails.get(n.id);
            if (r) {
              return (
                <RailCard
                  key={n.id}
                  r={r}
                  p={n}
                  level={lv.get(n.id)}
                  open={openOf(n.id)}
                  setOpen={(o) => setOpen(n.id, o)}
                  selected={selected === n.id}
                  tint={tint}
                  dim={dim}
                  onSelect={() => select(selected === n.id ? null : n.id)}
                  onTip={showTip}
                />
              );
            }
            const s = stages.get(n.id)!;
            return <StageCard key={n.id} s={s} p={n} level={lv.get(n.id)} selected={selected === n.id} tint={tint} dim={dim} onSelect={() => select(selected === n.id ? null : n.id)} />;
          })}
        </PanZoom>
        <MiniMap layout={layout} view={view} tintOf={(id) => (path?.up.nodes.has(id) ? 'up' : path?.down.nodes.has(id) ? 'down' : '')} levelOf={(id) => lv.get(id) ?? ''} onGo={(x, y) => setCentre({ x, y, seq: (centre?.seq ?? 0) + 1 })} />
      </div>
      {tip && <LoadTip tip={tip} onEnter={() => window.clearTimeout(timer.current)} onLeave={() => showTip(null)} />}
    </div>
  );
}

function LoadTip({ tip, onEnter, onLeave }: { tip: Tip; onEnter: () => void; onLeave: () => void }) {
  const l = tip.load;
  const flip = tip.y + 150 > window.innerHeight;
  return (
    <div
      class="pt-tip"
      role="tooltip"
      style={{ left: Math.min(tip.x, window.innerWidth - 300), ...(flip ? { bottom: window.innerHeight - tip.y + 24 } : { top: tip.y + 4 }) }}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
    >
      <div class="pt-tip-who">{l.who}</div>
      <dl>
        <dt>Part</dt>
        <dd>{l.part}</dd>
        {l.pin && (
          <>
            <dt>Pin</dt>
            <dd>{l.pin}</dd>
          </>
        )}
        <dt>{l.hi <= 0 && l.lo < 0 ? 'Supplies' : 'Draws'}</dt>
        <dd>{l.unknown ? 'a current the solver has not settled' : `${amps(l.lo).num} ${amps(l.lo).unit}${DOTS}${amps(l.hi).num} ${amps(l.hi).unit}`}</dd>
      </dl>
      {l.at && (
        <button class="link" onClick={() => void goTo(l.at!)}>
          Show in source
        </button>
      )}
    </div>
  );
}

export function Power() {
  const name = systemName.value ?? board.value;
  const version = analysis.value?.version;
  const [tree, setTree] = useState<PowerTree | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!name) return;
    let live = true;
    api
      .power(name)
      .then((t) => live && (setTree(t), setError(null)))
      .catch((e: Error) => live && (setTree(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  if (!name) return <NoBoard what="show" />;
  if (error) return <div class="empty bad">{error}</div>;
  if (!tree) return <ViewLoading what="Reading the power tree" />;
  if (tree.nodes.length === 0) return <div class="empty">No rail draws from a net on this board.</div>;
  return <PowerTreeView tree={tree} fitKey={`${name}:${version}:${tree.nodes.length}`} />;
}
