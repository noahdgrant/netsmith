// The panel on the right of the schematic: what is selected (a part, or a net with its pins as
// links), else a summary of the sheet; below it the parts, which filter and show the selection.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import type { NetInfo, PartInfo } from '../api';
import { ALL } from '../sheets';
import { hoverOfRefs } from '../hover';
import { detail, goTo, revealSection } from '../store';
import type { Drawing } from '../schematic/load';
import { busCount, busMembers, busName, busTargets } from '../schematic/bus';
import type { Bus } from '../schematic/drawing';
import { netNavOrder, netSheets, netSummary, parseTitle } from '../schematic/nav';
import { plural } from '../util';
import { Defs, InspectorAction, InspectorHead, InspectorSection, PinChip } from '../components/Inspector';
import { Icon } from '../components/Icons';
import type { SheetRow } from './SheetNav';
import { removedOn } from '../compare/model';
import { baseLocation, marks, openDiff, refText } from '../compare/store';
import { clearSelection, goToSource, selectedBus, selectedNet, selectedPart, showOnPcb } from './schematic-selection';

export interface PinRef {
  ref: string;
  pin: string;
}

export interface InspectorProps {
  drawing: Drawing;
  rows: SheetRow[];
  current: string;
  order: string[];
  openSheet: (pick: string) => void;
  goPart: (ref: string) => void;
  goPin: (ref: string, pin: string) => void;
  goNet: (net: string) => void;
  /** Pick a bus on the sheet that draws it and bring it into view. */
  goBus: (bus: Bus) => void;
  /** The pin the net navigator is on. */
  navPin: PinRef | null;
  stepPins: (dir: 1 | -1) => void;
  hover: { net?: string; part?: string; bus?: string } | null;
}

export function Inspector(p: InspectorProps) {
  const d = detail.value;
  const net = selectedNet.value;
  const sel = selectedPart.value;
  const pickedBus = selectedBus.value;
  // The bus drawn (its sheet and box), the picked one or the one the pointer is over.
  const busOf = (path: string | undefined) => (path ? p.drawing.index.buses.find((b) => b.path === path) : undefined);
  const bus = pickedBus ? busOf(pickedBus.path) : !net && !sel ? busOf(p.hover?.bus) : undefined;
  const hoverNet = !net && !sel ? p.hover?.net : undefined;
  const hoverPart = !net && !sel ? p.hover?.part : undefined;
  const netInfo = (net ?? hoverNet) ? d?.nets.find((n) => n.name === (net ?? hoverNet)) : undefined;
  const part = d?.parts.find((x) => x.refdes === (sel ?? hoverPart));
  return (
    <aside class="sinspector insp no-pan" aria-label="Details">
      <div class="insp-top">
        {bus ? (
          <BusPanel {...p} bus={bus} sticky={!!pickedBus} />
        ) : netInfo ? (
          <NetPanel {...p} net={netInfo} sticky={!!net} />
        ) : part ? (
          <PartPanel {...p} part={part} sticky={!!sel} />
        ) : (
          <SheetPanel {...p} />
        )}
      </div>
      <PartsList {...p} />
      <RemovedParts {...p} />
      {d && d.notes.length > 0 && (
        <InspectorSection title="Notes" count={d.notes.length} open={false}>
          <ul class="notes">
            {d.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </InspectorSection>
      )}
    </aside>
  );
}

function PartPanel({ part, sticky, current, rows, openSheet, goNet, drawing }: InspectorProps & { part: PartInfo; sticky: boolean }) {
  const d = detail.value;
  const sheet = drawing.index.partSheet.get(part.refdes);
  const row = rows.find((r) => r.sheet.name === sheet);
  const pins = useMemo(
    () =>
      (d?.nets ?? [])
        .flatMap((n) => n.pins.filter((p) => p.refdes === part.refdes).map((p) => ({ pin: p.pin, net: n.name, named: n.named })))
        .sort((a, b) => a.pin.localeCompare(b.pin, undefined, { numeric: true })),
    [d, part.refdes],
  );
  return (
    <section aria-label={`Part ${part.refdes}`}>
      <InspectorHead
        eyebrow="Part"
        hover={!sticky}
        icon="part"
        tone="part"
        mono
        title={part.refdes}
        kind={part.kind}
        actions={
          <>
            <InspectorAction icon="source" label="Go to source" keys="Enter" disabled={!part.decl} onClick={() => goToSource(part.refdes)} />
            <InspectorAction icon="pcb" label="Show on PCB" onClick={() => showOnPcb(part.refdes)} />
          </>
        }
      />
      <Defs
        rows={[
          { label: 'Value', value: part.value },
          { label: 'Path', value: part.path, mono: true, title: 'Where the part is, from the board' },
          ...(row && sheet !== current
            ? [
                {
                  label: 'Module',
                  value: (
                    <button class="link" onClick={() => openSheet(row.sheet.name)}>
                      {row.name}
                    </button>
                  ),
                },
              ]
            : []),
          { label: 'Package', value: part.package ?? '-' },
          { label: 'LCSC', value: part.lcsc ?? (part.assembled ? '-' : 'hand-soldered'), mono: !!part.lcsc },
        ]}
      />
      {part.decl && (
        <button class="decl-link" onClick={() => void goTo(part.decl!)} title="Open the declaration in the source">
          <Icon name="file" size={13} />
          <span class="mono">
            {part.decl.file}:{part.decl.line}
          </span>
        </button>
      )}
      <InspectorSection title="Pins" count={pins.length}>
        <div class="pinlist" role="list" aria-label="Pins">
          {pins.map((p) => (
            <div class="pinrow" role="listitem" key={p.pin}>
              <span class="pin-no">{p.pin}</span>
              <PinChip class="net-link" title="Select this net" hv={{ kind: 'net', key: p.net }} onClick={() => goNet(p.net)}>
                {p.net}
              </PinChip>
            </div>
          ))}
        </div>
      </InspectorSection>
    </section>
  );
}

function NetPanel({ net, sticky, drawing, rows, order, current, openSheet, goPin, navPin, stepPins }: InspectorProps & { net: NetInfo; sticky: boolean }) {
  const partSheet = drawing.index.partSheet;
  const summary = netSummary(net, partSheet);
  const bySheet = netSheets(net.pins, partSheet);
  const walk = netNavOrder(net.pins, partSheet, order);
  const here = (name: string) => rows.find((r) => r.sheet.name === name);
  return (
    <section aria-label={`Net ${net.name}`}>
      <InspectorHead
        eyebrow="Net"
        hover={!sticky}
        icon="net"
        tone="net"
        mono
        title={net.name}
        sub={
          <>
            {summary.text}
            {net.named ? '' : ', unnamed'}
          </>
        }
        actions={
          sticky ? (
            <>
              <InspectorAction icon="chevron-left" label="Previous pin" keys="Shift+Tab" disabled={walk.length < 2} onClick={() => stepPins(-1)} />
              <InspectorAction icon="chevron-right" label="Next pin" keys="Tab" disabled={walk.length < 2} onClick={() => stepPins(1)} />
              <InspectorAction icon="close" label="Clear the selection" keys="Esc" onClick={clearSelection} />
            </>
          ) : undefined
        }
      />
      <div class="net-groups">
        {order
          .filter((s) => bySheet.has(s))
          .map((s) => {
            const r = here(s);
            const pins = bySheet.get(s)!;
            return (
              <div class="net-group" key={s}>
                <button class={`grp ${s === current ? 'here' : ''}`} title={s === current ? 'This module' : `Open the module ${r?.name ?? s}`} onClick={() => openSheet(s)}>
                  <Icon name="module" size={13} />
                  <span>{r?.name ?? (s || 'board')}</span>
                  <span class="count">{pins.length}</span>
                </button>
                <div class="chips">
                  {pins.map((p) => (
                    <PinChip
                      key={`${p.refdes}.${p.pin}`}
                      current={navPin?.ref === p.refdes && navPin.pin === p.pin}
                      title={`Go to ${p.refdes} pin ${p.pin}`}
                      hv={() => hoverOfRefs(detail.peek(), [p.refdes])}
                      onClick={() => goPin(p.refdes, p.pin)}
                    >
                      {p.refdes}.{p.pin}
                    </PinChip>
                  ))}
                </div>
              </div>
            );
          })}
      </div>
    </section>
  );
}

/** A bus: its instance, its interface, its members with their nets as links and the pins of each
 * net by module, and where else its nets go. */
function BusPanel({ bus, sticky, drawing, rows, order, current, openSheet, goNet, goPin, goBus, navPin }: InspectorProps & { bus: Bus; sticky: boolean }) {
  const d = detail.value;
  const nets = d?.nets ?? [];
  const partSheet = drawing.index.partSheet;
  const members = useMemo(() => busMembers(bus, nets, partSheet, order), [bus, d, partSheet, order]);
  const others = useMemo(() => busTargets(bus, nets, partSheet, order, drawing.index.buses), [bus, d, partSheet, order, drawing]);
  const where = (name: string) => rows.find((r) => r.sheet.name === name)?.name ?? (name || 'board');
  return (
    <section aria-label={`Bus ${busName(bus.path)}`} data-testid="bus-panel">
      <InspectorHead
        eyebrow="Bus"
        hover={!sticky}
        icon="bus"
        tone="bus"
        mono
        title={busName(bus.path)}
        sub={
          <>
            {bus.type || 'Interface'} · {busCount(bus)}
            {bus.flag ? ' · leaves this module' : ''}
          </>
        }
        actions={sticky ? <InspectorAction icon="close" label="Clear the selection" keys="Esc" onClick={clearSelection} /> : undefined}
      />
      <Defs
        rows={[
          { label: 'Instance', value: bus.path, mono: true },
          {
            label: 'Module',
            value:
              bus.sheet !== current && rows.length > 1 ? (
                <button class="link" onClick={() => openSheet(bus.sheet)}>
                  {where(bus.sheet)}
                </button>
              ) : (
                where(bus.sheet)
              ),
          },
        ]}
      />
      <InspectorSection title="Members" count={members.length}>
        <div class="bus-members" role="list" aria-label="Members">
          {members.map((m) => (
            <div class="bus-member" role="listitem" key={m.name + m.net} data-net={m.net}>
              <div class="bus-member-head">
                <span class="mono bus-member-name">{m.name}</span>
                <PinChip class="net-link" title="Select this net" hv={{ kind: 'net', key: m.net }} onClick={() => goNet(m.net)}>
                  {m.net}
                </PinChip>
              </div>
              {m.modules.map((g) => (
                <div class="net-group" key={g.sheet}>
                  <button class={`grp ${g.sheet === current ? 'here' : ''}`} title={g.sheet === current ? 'This module' : `Open the module ${where(g.sheet)}`} onClick={() => openSheet(g.sheet)}>
                    <Icon name="module" size={13} />
                    <span>{where(g.sheet)}</span>
                    <span class="count">{g.pins.length}</span>
                  </button>
                  <div class="chips">
                    {g.pins.map((p) => (
                      <PinChip
                        key={`${p.refdes}.${p.pin}`}
                        current={navPin?.ref === p.refdes && navPin.pin === p.pin}
                        title={`Go to ${p.refdes} pin ${p.pin}`}
                        hv={() => hoverOfRefs(detail.peek(), [p.refdes])}
                        onClick={() => goPin(p.refdes, p.pin)}
                      >
                        {p.refdes}.{p.pin}
                      </PinChip>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>
      </InspectorSection>
      {others.length > 0 && (
        <InspectorSection title="Also on" count={others.length}>
          <div class="chips">
            {others.map((t) => (
              <PinChip
                key={t.sheet}
                mono={false}
                title={t.bus ? `Go to the bus ${busName(t.bus.path)} in ${where(t.sheet)}` : `Open ${where(t.sheet)}`}
                onClick={() => (t.bus ? goBus(t.bus) : openSheet(t.sheet))}
              >
                {where(t.sheet)}
                {t.bus ? ` · ${busName(t.bus.path)}` : ''}
              </PinChip>
            ))}
          </div>
        </InspectorSection>
      )}
    </section>
  );
}

function SheetPanel({ rows, current, drawing, goNet, goBus, openSheet }: InspectorProps) {
  const d = detail.value;
  const leaving = useMemo(() => {
    const out: string[] = [];
    for (const [net, places] of drawing.index.netPlaces) {
      const at = places.get(current);
      if (at && at.flags.length > 0) out.push(net);
    }
    return out.sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  }, [drawing, current]);
  const leavingBuses = drawing.index.buses.filter((b) => b.flag && b.sheet === current);
  if (current === ALL) {
    const parts = d?.parts.length ?? rows.reduce((n, r) => n + (r.stats?.parts ?? 0), 0);
    return (
      <section aria-label="Board summary">
        <InspectorHead eyebrow="Board" icon="board" tone="board" title={d?.name ?? ''} titleTag="div" />
        <Defs
          rows={[
            ...(rows.length > 0 ? [{ label: 'Modules', value: rows.length }] : []),
            { label: 'Parts', value: parts },
            { label: 'Nets', value: d?.nets.length ?? '-' },
          ]}
        />
        <p class="ih-help">Click a module to open it. Click a part or a wire there to select it.</p>
      </section>
    );
  }
  const row = rows.find((r) => r.sheet.name === current);
  if (!row) {
    return (
      <section aria-label="Board summary">
        <InspectorHead eyebrow="Board" icon="board" tone="board" title={d?.name ?? ''} titleTag="div" />
        <p class="ih-help">Click a part, a wire, a label or a flag to select it.</p>
      </section>
    );
  }
  const t = parseTitle(row.sheet.title);
  const s = row.stats;
  return (
    <section aria-label="Module summary">
      <InspectorHead eyebrow="Module" icon="module" tone="module" title={t.name} kind={t.detail || undefined} titleTag="div" />
      {s && (
        <Defs
          rows={[
            { label: 'Parts', value: s.parts },
            { label: 'Nets', value: `${s.nets}${s.leaving > 0 ? `, ${s.leaving} leave` : ''}` },
            {
              label: 'Problems',
              tone: s.errors > 0 ? 'bad' : s.warnings > 0 ? 'warn' : undefined,
              value: (
                <>
                  {s.errors + s.warnings > 0 ? `${plural(s.errors, 'error')}, ${plural(s.warnings, 'warning')}` : 'none'}
                  {s.notes > 0 && <span class="faint"> · {plural(s.notes, 'note')}</span>}
                </>
              ),
            },
          ]}
        />
      )}
      {(leaving.length > 0 || leavingBuses.length > 0) && (
        <InspectorSection title="Leaves this module" count={leaving.length + leavingBuses.length}>
          <div class="chips">
            {leavingBuses.map((b) => (
              <PinChip key={b.path} tone="bus-chip" title={`Select the bus ${b.path}`} hv={{ kind: 'bus', key: b.path, nets: b.nets }} onClick={() => goBus(b)}>
                {busName(b.path)} · {busCount(b)}
              </PinChip>
            ))}
            {leaving.map((n) => (
              <PinChip key={n} title="Select this net" hv={{ kind: 'net', key: n }} onClick={() => goNet(n)}>
                {n}
              </PinChip>
            ))}
          </div>
        </InspectorSection>
      )}
      <p class="ih-help">
        Click a part, a wire, a label or a flag to select it.
        {rows.length > 1 && (
          <>
            {' '}
            <button class="link" onClick={() => openSheet(ALL)}>
              All modules
            </button>
          </>
        )}
      </p>
    </section>
  );
}

function PartsList({ drawing, rows, current, goPart }: InspectorProps) {
  const d = detail.value;
  const [query, setQuery] = useState('');
  const [all, setAll] = useState(false);
  const sel = selectedPart.value;
  const table = useRef<HTMLTableElement>(null);
  const partSheet = drawing.index.partSheet;
  const scoped = !all && current !== ALL && rows.length > 1;
  const q = query.trim().toLowerCase();
  const parts = useMemo(
    () =>
      (d?.parts ?? []).filter(
        (p) =>
          (!scoped || partSheet.get(p.refdes) === current) &&
          (!q || p.refdes.toLowerCase().includes(q) || p.value.toLowerCase().includes(q) || p.kind.toLowerCase().includes(q) || p.path.toLowerCase().includes(q)),
      ),
    [d, scoped, current, q, partSheet],
  );
  useEffect(() => {
    table.current?.querySelector('tr.sel')?.scrollIntoView({ block: 'nearest' });
  }, [sel, parts.length]);
  return (
    <InspectorSection
      title="Parts"
      count={d ? (q || scoped ? `${parts.length} of ${d.parts.length}` : d.parts.length) : ''}
      right={
        rows.length > 1 && current !== ALL ? (
          <button class={`tool small ${scoped ? 'on' : ''}`} aria-pressed={scoped} title="Show the parts of this module, or of the whole board" onClick={() => setAll(scoped)}>
            This module
          </button>
        ) : undefined
      }
    >
      <label class="field-icon">
        <Icon name="search" size={13} />
        <input
          class="parts-filter"
          type="search"
          placeholder="Filter by designator, value or kind"
          aria-label="Filter the parts"
          value={query}
          onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape' && query) {
              e.stopPropagation();
              setQuery('');
            }
          }}
        />
      </label>
      <table class="parts" ref={table}>
        <tbody>
          {parts.map((p) => (
            <tr
              key={p.refdes}
              class={sel === p.refdes ? 'sel' : ''}
              aria-selected={sel === p.refdes}
              tabIndex={0}
              onClick={() => goPart(p.refdes)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  goPart(p.refdes);
                }
              }}
            >
              <td class="ref">{p.refdes}</td>
              <td>{p.value}</td>
              <td class="faint">{p.kind}</td>
            </tr>
          ))}
          {parts.length === 0 && (
            <tr class="none">
              <td colSpan={3}>{d ? 'No part matches.' : 'Loading…'}</td>
            </tr>
          )}
        </tbody>
      </table>
    </InspectorSection>
  );
}

/** Parts the comparison with a git ref says are gone from this module: the head has nothing to draw, so they are listed, with the
 * value they had and where they were declared at the ref. */
function RemovedParts({ current, drawing }: InspectorProps) {
  const m = marks.value;
  const d = detail.value;
  if (!m || m.removedParts.length === 0) return null;
  const list = removedOn(m.removedParts, d?.parts ?? [], drawing.index.partSheet, current, ALL);
  if (list.length === 0) return null;
  return (
    <InspectorSection title={current === ALL ? 'Removed from this board' : 'Removed from this module'} count={list.length}>
      <ul class="removed-list" data-testid="removed-parts" aria-label={`Parts at ${refText.value} that are gone`}>
        {list.map((r) => (
          <li class="removed-row" key={r.path}>
            <span class="rm-des">{r.designator}</span>
            <span class="rm-val" title={`${r.kind} ${r.value} at ${r.path}`}>
              {r.value} <span class="faint">{r.kind}</span>
            </span>
            {r.at && baseLocation(r.at, 'source') && (
              <button
                class="link"
                title={`Where it was declared at ${refText.value}: ${r.at.file}:${r.at.line}`}
                onClick={() => {
                  const t = baseLocation(r.at, 'source');
                  if (t) {
                    openDiff(t);
                    revealSection('source');
                  }
                }}
              >
                Base
              </button>
            )}
          </li>
        ))}
      </ul>
    </InspectorSection>
  );
}
