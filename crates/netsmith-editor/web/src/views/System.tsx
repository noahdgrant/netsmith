import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import type { PowerTree, SystemView } from '../api';
import { api } from '../client';
import { Icon, type IconName } from '../components/Icons';
import { InspectorHead } from '../components/Inspector';
import { Legend, Pill, Popover, Units } from '../components/ui';
import { NoBoard, ViewLoading } from '../components/ViewStatus';
import { MiniMap } from '../components/MiniMap';
import { PanZoom } from '../components/PanZoom';
import { type Item, type Placed, type TreeLayout, layoutTree } from '../flow/layout';
import { type Move, linkKey, move, trace } from '../flow/select';
import { ANCHOR, CARD_W, STEP, cardHeight, labelWidth } from '../system/metrics';
import { type ModuleModel, type SystemModel, type Wire, modelOf, shorten } from '../system/model';
import { analysis, board, goTo, systemName } from '../store';
import { fitView } from '../sheets';
import { type Card, cardFor, revealRequest, selection, selectionOfCard, setSelection } from '../selection';

/** The longest label an edge carries: the rest is in its tooltip. */
const LABEL_CHARS = 32;

/** The roles of a mate's contacts, counted: `6 signal`, `2 ground`. */
function roleCounts(m: SystemView['mates'][number]): [string, number][] {
  const n = new Map<string, number>();
  for (const c of m.contacts) n.set(c.aRole, (n.get(c.aRole) ?? 0) + 1);
  return ['signal', 'power', 'ground', 'open'].filter((r) => n.has(r)).map((r) => [r, n.get(r)!]);
}

/** The mated connectors of a system, a row for each with the roles of its contacts counted, the contacts
 * under it, and the grounds they join (ADR 0040). */
function Mates({ view }: { view: SystemView }) {
  const [open, setOpen] = useState(true);
  if (view.mates.length === 0) return null;
  return (
    <div class={`mates ${open ? 'open' : ''}`}>
      <div class="mates-head">
        <button class="mates-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          <Icon name="chevron-right" size={12} class="disc-chev" />
          <Icon name="link" size={14} />
          <span class="mates-title">Mated connectors</span>
          <span class="disc-count">{view.mates.length}</span>
          {view.groundGroups.length > 0 && (
            <>
              <span class="mates-dot" aria-hidden="true" />
              <span class="mates-title">Ground groups</span>
              <span class="disc-count">{view.groundGroups.length}</span>
            </>
          )}
        </button>
        <Legend items={[{ kind: 'role-signal', label: 'Signal' }, { kind: 'role-power', label: 'Power' }, { kind: 'role-ground', label: 'Ground' }]} />
      </div>
      {open && (
        <div class="mates-body">
          {view.mates.map((m) => (
            <details class="mate" key={`${m.a}/${m.b}`}>
              <summary class="mate-row">
                <Icon name="chevron-right" size={12} class="disc-chev" />
                <span class="mate-pair">
                  <button class="chip pin-chip plain mate-end" title={`Show ${m.a} in the source`} onClick={(e) => { e.preventDefault(); if (m.at) void goTo(m.at); }}>
                    <b>{m.a}</b>
                  </button>
                  <Icon name="link" size={12} class="mate-link" />
                  <button class="chip pin-chip plain mate-end" title={`Show ${m.b} in the source`} onClick={(e) => { e.preventDefault(); if (m.at) void goTo(m.at); }}>
                    <b>{m.b}</b>
                  </button>
                </span>
                <span class="mate-count">{m.contacts.length} contacts</span>
                <span class="mate-roles">
                  {roleCounts(m).map(([r, c]) => (
                    <span key={r} class={`role-chip role-${r}`}>
                      <i aria-hidden="true" />
                      {c} {r}
                    </span>
                  ))}
                </span>
                {m.ports.length > 0 && (
                  <span class="mate-joins" title={m.ports.join(', ')}>
                    joins {m.ports.join(', ')}
                  </span>
                )}
                {m.reversible && (
                  <Pill tone="warn" icon="warning" title={m.reversible}>
                    reversible
                  </Pill>
                )}
              </summary>
              <div class="contacts">
                {m.contacts.map((c) => (
                  <span
                    key={c.number}
                    class={`contact role-${c.aRole}`}
                    title={`contact ${c.number}: ${m.a} ${c.aRole}${c.aNet ? ` (${c.aNet})` : ''}, ${m.b} ${c.bRole}${c.bNet ? ` (${c.bNet})` : ''}`}
                  >
                    <b>{c.number}</b> {c.aRole === c.bRole ? c.aRole : `${c.aRole}/${c.bRole}`}
                  </span>
                ))}
                {m.reversible && <div class="warn">{m.reversible}</div>}
              </div>
            </details>
          ))}
          {view.groundGroups.map((g) => (
            <div class="ground-group" key={g.nets.join('+')}>
              <span class="role-chip role-ground">
                <i aria-hidden="true" />
                Ground
              </span>
              <span class="gg-nets">{g.nets.join(' + ')}</span>
              <span class="hint">{g.contacts.join('; ')} ground contacts</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** A drawing opens at its true size, or smaller when it does not fit: never blown up to fill the window. */
export function placeAtMostActual(size: { width: number; height: number }) {
  return (w: number, h: number) => {
    const all = fitView(w, h, { x: 0, y: 0, w: size.width, h: size.height });
    const k = Math.min(all.k, 1);
    return { k, x: (w - size.width * k) / 2, y: Math.max(16, (h - size.height * k) / 2 - 12) };
  };
}

const KIND_ICON: Record<string, IconName> = { board: 'board', module: 'module', part: 'part', port: 'net' };

function ModuleCard({
  m,
  p,
  open,
  setOpen,
  selected,
  tint,
  dim,
  onSelect,
}: {
  m: ModuleModel;
  p: Placed;
  open: boolean;
  setOpen: (o: boolean) => void;
  selected: boolean;
  tint: string;
  dim: boolean;
  onSelect: () => void;
}) {
  const many = m.ports.length > 6;
  return (
    <div
      class={`pt-node sy-card kind-${m.kind} ${selected ? 'selected' : ''} ${tint} ${dim ? 'dim' : ''}`}
      data-node={m.id}
      data-kind="module"
      style={{ left: p.x, top: p.y, width: p.w, height: p.h }}
      onClick={onSelect}
    >
      <div class="sy-head">
        <span class={`sy-icon k-${m.kind}`} title={m.kind} aria-hidden="true">
          <Icon name={KIND_ICON[m.kind] ?? 'module'} size={13} />
        </span>
        <button
          class="sy-name"
          title={m.at ? `${m.name}: show in source` : m.name}
          onClick={(e) => {
            e.stopPropagation();
            if (m.at) void goTo(m.at);
          }}
        >
          {m.name}
        </button>
        <span class="sy-type" title={m.type}>
          {m.type}
        </span>
      </div>
      {m.ports.length > 0 && (
        <div class="sy-ports">
          {many && (
            <button
              class="sy-fold"
              aria-expanded={open}
              onClick={(e) => {
                e.stopPropagation();
                setOpen(!open);
              }}
              title={open ? 'Hide the ports' : 'List the ports'}
            >
              <Icon name="chevron-right" size={12} class={`sy-chev ${open ? 'open' : ''}`} />
              {m.ports.length} ports
            </button>
          )}
          {(!many || open) &&
            m.ports.map((r) => (
              <div class="sy-row" key={r.name} title={r.title}>
                <span class="sy-pname">{r.name}</span>
                <span class={`sy-pval ${r.supply ? 'supply' : ''}`}>
                  <span class="sy-volts">
                    <Units text={r.volts} />
                  </span>
                  <span class="sy-amps">
                    <Units text={r.current} />
                  </span>
                </span>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

function SystemFlow({ view, power, fitKey }: { view: SystemView; power: PowerTree | null; fitKey: string }) {
  const model: SystemModel = useMemo(() => modelOf(view, power), [view, power]);
  const [opens, setOpens] = useState<Record<string, boolean>>({});
  /** A card that stands for nothing the page can select (a board of a system, a port): its
   * own, dropped when the shared selection changes. */
  const [own, setOwn] = useState<string | null>(null);
  const [tip, setTip] = useState<{ wire: Wire; x: number; y: number } | null>(null);
  const [reveal, setReveal] = useState<{ x: number; y: number; w: number; h: number; seq: number } | undefined>();
  const [centre, setCentre] = useState<{ x: number; y: number; seq: number } | undefined>();
  const [vp, setVp] = useState<{ k: number; x: number; y: number; w: number; h: number } | null>(null);
  /** The card the pointer is over: the wires of it show a little more. */
  const [hover, setHover] = useState<string | null>(null);
  const host = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const came = useRef(new Map<string, string>());

  const modules = new Map(model.modules.map((m) => [m.id, m]));
  const sides = new Map(model.modules.map((m) => [m.id, { in: 0, out: 0 }]));
  for (const w of model.wires) {
    sides.get(w.from)!.out++;
    sides.get(w.to)!.in++;
  }
  const items: Item[] = model.modules.map((m) => ({
    id: m.id,
    kind: 'module',
    w: CARD_W,
    h: cardHeight(m, !!opens[m.id], sides.get(m.id)!),
    anchor: ANCHOR,
    portStep: STEP,
  }));
  const shown = (w: Wire) => shorten(w.text, LABEL_CHARS);
  const layout: TreeLayout = useMemo(
    () => layoutTree(items, model.wires.map((w) => ({ from: w.from, to: w.to, label: shown(w), labelW: labelWidth(shown(w)) }))),
    [model, JSON.stringify(items.map((i) => [i.id, i.h]))],
  );
  const placed = new Map(layout.nodes.map((n) => [n.id, n]));
  const wireOf = new Map(model.wires.map((w) => [linkKey(w), w]));
  // The selection is the page's: a module or part of the board is its card; a card that is
  // neither (a board of a system, a port) is selected here only.
  const cards: Card[] = useMemo(
    () =>
      model.modules.flatMap((m): Card[] =>
        m.kind === 'module' || m.kind === 'part' ? [{ id: m.id, kind: m.kind, key: m.name }] : [],
      ),
    [model],
  );
  const shared = selection.value;
  const selected = cardFor(shared, cards) ?? (shared === null && own && placed.has(own) ? own : null);
  useEffect(() => {
    if (shared) setOwn(null);
  }, [shared]);
  const tr = useMemo(() => (selected ? trace(model.links, selected) : null), [model, selected]);
  const ask = revealRequest.value;
  useEffect(() => {
    const n = selected ? placed.get(selected) : undefined;
    if (ask && n) setReveal({ x: n.x, y: n.y, w: n.w, h: n.h, seq: (reveal?.seq ?? 0) + 1 });
  }, [ask?.nonce]);

  const select = (id: string | null, scroll = false) => {
    const sel = id ? selectionOfCard(cards, id) : null;
    setOwn(id && !sel ? id : null);
    setSelection(sel);
    const n = id ? placed.get(id) : undefined;
    if (scroll && n) setReveal({ x: n.x, y: n.y, w: n.w, h: n.h, seq: (reveal?.seq ?? 0) + 1 });
  };

  const showTip = (t: { wire: Wire; x: number; y: number } | null, now = false) => {
    window.clearTimeout(timer.current);
    if (t === null) timer.current = window.setTimeout(() => setTip(null), 220);
    else if (now) setTip(t);
    else timer.current = window.setTimeout(() => setTip(t), 200);
  };

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
    } else if (e.key === 'Escape' && (selected || tip)) {
      e.preventDefault();
      e.stopPropagation();
      setTip(null);
      select(null);
    }
  };

  const everyOpen = model.modules.every((m) => m.ports.length <= 6 || opens[m.id]);

  return (
    <div class="graph-view power-view system-view">
      <div class="graph-title pt-bar">
        <span class="gt-name">
          <Icon name="system" size={16} />
          {view.kind === 'system' ? 'System' : 'Board'}
          <Pill tone="neutral">{view.name}</Pill>
          <Popover icon="note" tip="What this shows" class="bom-info" align="left">
            <h4>{view.kind === 'system' ? 'System' : 'Board'}</h4>
            <p>
              {view.kind === 'system'
                ? 'Boards and the joins between them, from what supplies to what is supplied. Below, the mated connectors and the grounds they join.'
                : 'Modules, parts and ports, from what supplies to what is supplied, joined by the nets they share.'}
            </p>
          </Popover>
        </span>
        <Legend
          items={[
            { kind: 'power', label: 'Supplies', tip: 'A wire from what supplies to what is supplied' },
            { kind: 'signal', label: 'Signal', tip: 'A wire with no direction known' },
            ...(tr ? [{ kind: 'up', label: 'Upstream', tip: 'What feeds the selection' }, { kind: 'down', label: 'Downstream', tip: 'What the selection feeds' }] : []),
          ]}
        />
        <span class="spacer" />
        {model.modules.some((m) => m.ports.length > 6) && (
          <button
            class="tool small"
            onClick={() => setOpens(Object.fromEntries(model.modules.map((m) => [m.id, !everyOpen])))}
            title="List the ports of every long table, or fold them all"
          >
            <Icon name={everyOpen ? 'collapse-all' : 'expand-all'} size={13} />
            {everyOpen ? 'Fold all ports' : 'List all ports'}
          </button>
        )}
      </div>
      <div
        class="pt-host"
        ref={host}
        tabIndex={0}
        role="application"
        aria-label="System. Arrow keys move along it, Escape clears the selection."
        onKeyDown={onKey}
        onMouseOver={(e) => setHover((e.target as Element).closest<HTMLElement>('[data-node]')?.dataset.node ?? null)}
        onMouseLeave={() => setHover(null)}
        onClick={(e) => {
          if (!(e.target as Element).closest('.pt-node, .pt-minimap, .zoombar, .sy-label')) {
            select(null);
            setTip(null);
          }
        }}
      >
        <PanZoom
          width={layout.width}
          height={layout.height}
          fitKey={fitKey}
          class="graph-host power-host"
          actualSize
          scale100={1}
          placeView={placeAtMostActual(layout)}
          reveal={reveal}
          centre={centre}
          onViewport={setVp}
        >
          <svg class="edges pt-edges" width={layout.width} height={layout.height}>
            <defs>
              {['', 'up', 'down'].map((t) => (
                <marker key={t} id={`sy-arrow${t ? `-${t}` : ''}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                  <path d="M0,0 L10,5 L0,10 z" class={`pt-arrowhead ${t}`} />
                </marker>
              ))}
            </defs>
            {layout.edges.map((e) => {
              const k = linkKey(e);
              const w = wireOf.get(k);
              const tint = tr?.up.links.has(k) ? 'up' : tr?.down.links.has(k) ? 'down' : '';
              return (
                <path
                  key={k}
                  data-from={e.from}
                  data-to={e.to}
                  data-points={JSON.stringify(e.points)}
                  data-back={e.back ? 'true' : undefined}
                  d={e.d}
                  class={`pt-edge sy-edge ${w?.kind ?? ''} ${tint ? `hot ${tint}` : ''} ${tr && !tint ? 'dim' : ''} ${!tr && hover && (e.from === hover || e.to === hover) ? 'peek' : ''}`}
                  marker-end={w?.kind === 'power' ? `url(#sy-arrow${tint ? `-${tint}` : ''})` : undefined}
                />
              );
            })}
            {layout.edges.map((e) => {
              const k = linkKey(e);
              const w = wireOf.get(k);
              if (!e.label || !w) return null;
              const tint = tr?.up.links.has(k) || tr?.down.links.has(k);
              const group = w.labels.length > 1;
              return (
                <text
                  key={`l${k}`}
                  x={e.label.x}
                  y={e.label.y}
                  data-label={k}
                  class={`sy-label ${group ? 'group' : ''} ${tr && !tint ? 'dim' : ''}`}
                  onClick={(ev) => {
                    if (!group) return;
                    ev.stopPropagation();
                    const r = (ev.currentTarget as SVGTextElement).getBoundingClientRect();
                    showTip({ wire: w, x: r.left, y: r.bottom }, true);
                  }}
                  onMouseEnter={(ev) => {
                    if (!group) return;
                    const r = (ev.currentTarget as SVGTextElement).getBoundingClientRect();
                    showTip({ wire: w, x: r.left, y: r.bottom });
                  }}
                  onMouseLeave={() => group && showTip(null)}
                >
                  <title>{w.labels.join('\n')}</title>
                  {e.label.text}
                  {group ? ' ▾' : ''}
                </text>
              );
            })}
          </svg>
          {layout.nodes.map((n) => {
            const m = modules.get(n.id)!;
            const tint = tr?.up.nodes.has(n.id) ? 'up' : tr?.down.nodes.has(n.id) ? 'down' : '';
            return (
              <ModuleCard
                key={n.id}
                m={m}
                p={n}
                open={!!opens[n.id]}
                setOpen={(o) => setOpens({ ...opens, [n.id]: o })}
                selected={selected === n.id}
                tint={tint}
                dim={tr !== null && !tr.nodes.has(n.id)}
                onSelect={() => select(selected === n.id ? null : n.id)}
              />
            );
          })}
        </PanZoom>
        <MiniMap
          layout={layout}
          view={vp}
          tintOf={(id) => (tr?.up.nodes.has(id) ? 'up' : tr?.down.nodes.has(id) ? 'down' : '')}
          onGo={(x, y) => setCentre({ x, y, seq: (centre?.seq ?? 0) + 1 })}
        />
      </div>
      {tip && (
        <div
          class="pt-tip sy-tip"
          role="tooltip"
          style={{ left: Math.min(tip.x, window.innerWidth - 320), top: Math.min(tip.y + 4, window.innerHeight - 40 - tip.wire.labels.length * 20) }}
          onMouseEnter={() => window.clearTimeout(timer.current)}
          onMouseLeave={() => showTip(null)}
        >
          <InspectorHead
            eyebrow="Wire"
            icon="link"
            tone="net"
            mono
            titleTag="div"
            title={
              <>
                {tip.wire.from} {'\u2192'} {tip.wire.to}
              </>
            }
            sub={`${tip.wire.labels.length} ${tip.wire.labels.length === 1 ? 'connection' : 'connections'}`}
          />
          <ul>
            {tip.wire.labels.map((l) => (
              <li key={l}>{l}</li>
            ))}
          </ul>
        </div>
      )}
      {view.kind === 'system' && <Mates view={view} />}
    </div>
  );
}

export function System() {
  const name = systemName.value ?? board.value;
  const version = analysis.value?.version;
  const [view, setView] = useState<SystemView | null>(null);
  const [power, setPower] = useState<PowerTree | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!name) return;
    let live = true;
    api
      .system(name)
      .then((v) => {
        if (!live) return;
        setView(v);
        setError(null);
        // Who supplies a net decides which way a wire runs: the power tree knows.
        if (v.kind === 'board') {
          api
            .power(name)
            .then((p) => live && setPower(p))
            .catch(() => live && setPower(null));
        } else setPower(null);
      })
      .catch((e: Error) => live && (setView(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  if (!name) return <NoBoard what="show" />;
  if (error) return <div class="empty bad">{error}</div>;
  if (!view) return <ViewLoading what="Reading the system" />;
  return <SystemFlow view={view} power={power} fitKey={`${name}:${version}:${view.nodes.length}`} />;
}
