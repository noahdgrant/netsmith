// The panel on the right of the schematic: what is selected (a part, or a net with its pins as
// links), else a summary of the sheet; below it the parts, which filter and show the selection.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import type { NetInfo, PartInfo } from '../api';
import { ALL } from '../sheets';
import { goTo, detail } from '../store';
import type { Drawing } from '../schematic/load';
import { busCount, busMembers, busName, busTargets } from '../schematic/bus';
import type { Bus } from '../schematic/drawing';
import { netNavOrder, netSheets, netSummary, parseTitle } from '../schematic/nav';
import { plural } from '../util';
import type { SheetRow } from './SheetNav';
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
    <aside class="sinspector no-pan" aria-label="Details">
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
      {d && d.notes.length > 0 && (
        <>
          <h3>Notes</h3>
          <ul class="notes">
            {d.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </>
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
      <h3>
        Part {!sticky && <span class="hint">hover</span>}
      </h3>
      <div class="title">
        <span class="mono">{part.refdes}</span> <span class="sub">{part.kind}</span>
      </div>
      <dl>
        <dt>Value</dt>
        <dd>{part.value}</dd>
        <dt>Path</dt>
        <dd class="mono" title="Where the part is, from the board">
          {part.path}
        </dd>
        {row && sheet !== current && (
          <>
            <dt>Module</dt>
            <dd>
              <button class="link" onClick={() => openSheet(row.sheet.name)}>
                {row.name}
              </button>
            </dd>
          </>
        )}
        <dt>Package</dt>
        <dd>{part.package ?? '-'}</dd>
        <dt>LCSC</dt>
        <dd>{part.lcsc ?? (part.assembled ? '-' : 'hand-soldered')}</dd>
      </dl>
      <div class="actions">
        <button class="tool small" disabled={!part.decl} title="Open the source at this part's declaration (Enter, or double-click the part)" onClick={() => goToSource(part.refdes)}>
          Go to source
        </button>
        <button class="tool small" title="Show this part in the PCB view" onClick={() => showOnPcb(part.refdes)}>
          Show on PCB
        </button>
      </div>
      {part.decl && (
        <button class="link decl" onClick={() => void goTo(part.decl!)}>
          {part.decl.file}:{part.decl.line}
        </button>
      )}
      <table class="pins" aria-label="Pins">
        <tbody>
          {pins.map((p) => (
            <tr key={p.pin}>
              <td class="num">{p.pin}</td>
              <td>
                <button class="link net-link" title="Select this net" onClick={() => goNet(p.net)}>
                  {p.net}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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
      <h3>
        Net {!sticky && <span class="hint">hover</span>}
      </h3>
      <div class="title mono">{net.name}</div>
      <div class="sub">
        {summary.text}
        {net.named ? '' : ', unnamed'}
      </div>
      {sticky && (
        <div class="actions">
          <button class="tool small" title="Previous pin (Shift+Tab)" aria-label="Previous pin" onClick={() => stepPins(-1)} disabled={walk.length < 2}>
            ‹ Prev
          </button>
          <button class="tool small" title="Next pin (Tab)" aria-label="Next pin" onClick={() => stepPins(1)} disabled={walk.length < 2}>
            Next ›
          </button>
          <button class="tool small" title="Clear the selection (Esc)" onClick={clearSelection}>
            Clear
          </button>
        </div>
      )}
      <div class="net-groups">
        {order
          .filter((s) => bySheet.has(s))
          .map((s) => {
            const r = here(s);
            const pins = bySheet.get(s)!;
            return (
              <div class="net-group" key={s}>
                <button class={`grp ${s === current ? 'here' : ''}`} title={s === current ? 'This module' : `Open the module ${r?.name ?? s}`} onClick={() => openSheet(s)}>
                  <span>{r?.name ?? (s || 'board')}</span>
                  <span class="count">{pins.length}</span>
                </button>
                <div class="chips">
                  {pins.map((p) => (
                    <button
                      key={`${p.refdes}.${p.pin}`}
                      class={`chip ${navPin?.ref === p.refdes && navPin.pin === p.pin ? 'cur' : ''}`}
                      title={`Go to ${p.refdes} pin ${p.pin}`}
                      onClick={() => goPin(p.refdes, p.pin)}
                    >
                      {p.refdes}.{p.pin}
                    </button>
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
      <h3>
        Bus {!sticky && <span class="hint">hover</span>}
      </h3>
      <div class="title mono" title={`The interface instance ${bus.path}`}>
        {busName(bus.path)}
      </div>
      <div class="sub">
        {bus.type || 'Interface'} · {busCount(bus)}
        {bus.flag ? ' · leaves this module' : ''}
      </div>
      <dl>
        <dt>Instance</dt>
        <dd class="mono">{bus.path}</dd>
        <dt>Module</dt>
        <dd>
          {bus.sheet !== current && rows.length > 1 ? (
            <button class="link" onClick={() => openSheet(bus.sheet)}>
              {where(bus.sheet)}
            </button>
          ) : (
            where(bus.sheet)
          )}
        </dd>
      </dl>
      {sticky && (
        <div class="actions">
          <button class="tool small" title="Clear the selection (Esc)" onClick={clearSelection}>
            Clear
          </button>
        </div>
      )}
      <div class="sub-head">Members</div>
      <div class="bus-members" role="list" aria-label="Members">
        {members.map((m) => (
          <div class="bus-member" role="listitem" key={m.name + m.net} data-net={m.net}>
            <div class="bus-member-head">
              <span class="mono bus-member-name">{m.name}</span>
              <button class="link net-link mono" title="Select this net" onClick={() => goNet(m.net)}>
                {m.net}
              </button>
            </div>
            {m.modules.map((g) => (
              <div class="net-group" key={g.sheet}>
                <button class={`grp ${g.sheet === current ? 'here' : ''}`} title={g.sheet === current ? 'This module' : `Open the module ${where(g.sheet)}`} onClick={() => openSheet(g.sheet)}>
                  <span>{where(g.sheet)}</span>
                  <span class="count">{g.pins.length}</span>
                </button>
                <div class="chips">
                  {g.pins.map((p) => (
                    <button
                      key={`${p.refdes}.${p.pin}`}
                      class={`chip ${navPin?.ref === p.refdes && navPin.pin === p.pin ? 'cur' : ''}`}
                      title={`Go to ${p.refdes} pin ${p.pin}`}
                      onClick={() => goPin(p.refdes, p.pin)}
                    >
                      {p.refdes}.{p.pin}
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        ))}
      </div>
      {others.length > 0 && (
        <>
          <div class="sub-head">Also on</div>
          <div class="chips">
            {others.map((t) => (
              <button
                key={t.sheet}
                class="chip"
                title={t.bus ? `Go to the bus ${busName(t.bus.path)} in ${where(t.sheet)}` : `Open ${where(t.sheet)}`}
                onClick={() => (t.bus ? goBus(t.bus) : openSheet(t.sheet))}
              >
                {where(t.sheet)}
                {t.bus ? ` · ${busName(t.bus.path)}` : ''}
              </button>
            ))}
          </div>
        </>
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
        <h3>Board</h3>
        <div class="title">{d?.name ?? ''}</div>
        <dl>
          <dt>Modules</dt>
          <dd>{rows.length}</dd>
          <dt>Parts</dt>
          <dd>{parts}</dd>
          <dt>Nets</dt>
          <dd>{d?.nets.length ?? '-'}</dd>
        </dl>
        <p class="hint">Click a module to open it. Click a part or a wire there to select it.</p>
      </section>
    );
  }
  const row = rows.find((r) => r.sheet.name === current);
  if (!row) {
    return (
      <section aria-label="Board summary">
        <h3>Board</h3>
        <div class="title">{d?.name ?? ''}</div>
        <p class="hint">Click a part, a wire, a label or a flag to select it.</p>
      </section>
    );
  }
  const t = parseTitle(row.sheet.title);
  const s = row.stats;
  return (
    <section aria-label="Module summary">
      <h3>Module</h3>
      <div class="title">
        {t.name} {t.detail && <span class="sub">{t.detail}</span>}
      </div>
      {s && (
        <dl>
          <dt>Parts</dt>
          <dd>{s.parts}</dd>
          <dt>Nets</dt>
          <dd>
            {s.nets}
            {s.leaving > 0 ? `, ${s.leaving} leave` : ''}
          </dd>
          <dt>Problems</dt>
          <dd class={s.errors > 0 ? 'bad' : s.warnings > 0 ? 'warn' : ''}>
            {s.errors + s.warnings > 0 ? `${plural(s.errors, 'error')}, ${plural(s.warnings, 'warning')}` : 'none'}
            {s.notes > 0 && <span class="faint"> · {plural(s.notes, 'note')}</span>}
          </dd>
        </dl>
      )}
      {(leaving.length > 0 || leavingBuses.length > 0) && (
        <>
          <div class="sub-head">Leaves this module</div>
          <div class="chips">
            {leavingBuses.map((b) => (
              <button key={b.path} class="chip bus-chip" title={`Select the bus ${b.path}`} onClick={() => goBus(b)}>
                {busName(b.path)} · {busCount(b)}
              </button>
            ))}
            {leaving.map((n) => (
              <button key={n} class="chip" title="Select this net" onClick={() => goNet(n)}>
                {n}
              </button>
            ))}
          </div>
        </>
      )}
      <p class="hint">
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
    <>
      <div class="parts-head">
        <h3>
          Parts <span class="count">{d ? (q || scoped ? `${parts.length} of ${d.parts.length}` : d.parts.length) : ''}</span>
        </h3>
        {rows.length > 1 && current !== ALL && (
          <button class={`tool small ${scoped ? 'on' : ''}`} aria-pressed={scoped} title="Show the parts of this module, or of the whole board" onClick={() => setAll(scoped)}>
            This module
          </button>
        )}
      </div>
      <input
        class="parts-filter"
        type="search"
        placeholder="Filter by designator, value or kind…"
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
    </>
  );
}
