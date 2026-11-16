// The side panel of the PCB view: what is selected, the parts, the keepouts and the nets.

import { useState } from 'preact/hooks';

import type { PcbBox, PcbKeepout, PcbPart, PcbRouteNet, PcbScene } from '../api';
import {
  type RoutePick,
  acceptProposal,
  changeRoutes,
  dropProposal,
  pickKeepout,
  pickRoute,
  pickedKeepout,
  pickedRoute,
  proposal,
  propose,
  proposeAnother,
  select,
  selection,
} from '../pcbstore';
import { boxOf, deleteChange, lockChange, nameOf, samePick, shapeOf } from './routes';
import { Defs, InspectorAction, InspectorHead, PinChip } from '../components/Inspector';
import { Icon } from '../components/Icons';
import { EmptyState } from '../components/ui';
import { hoverAttrs } from '../hover';
import { pickPart } from '../store';
import {
  changeKeepout,
  deleteKeepout,
  flipSelection,
  placeKeepout,
  releaseSelection,
  rotateSelection,
  toggleLock,
  toggleRequired,
  turnSelectionTo,
} from './actions';
import type { Drawing } from './drawing';
import { type ModuleRow, inModule, moduleOfAll, moduleRows, totalsLines } from './regenerate';
import { faceWord } from './side';
import { isRequired, rangeText } from './windows';

export function Panel(props: {
  scene: PcbScene;
  drawing: Drawing;
  picked: string | null;
  onPickNet: (net: string | null) => void;
  onHoverNet: (net: string | null) => void;
  onFocus: (box: PcbBox) => void;
  tab: string;
  onTab: (tab: string) => void;
  extra?: { id: string; label: string; body: preact.ComponentChildren }[];
}) {
  const { scene: sc } = props;
  const chosen = sc.parts.filter((p) => selection.value.includes(p.path));
  const keepout = sc.keepouts.find((k) => k.name === pickedKeepout.value) ?? null;
  const route = pickedRoute.value;
  const record = route ? sc.routes.find((n) => n.net === route.net) ?? null : null;
  const modules = moduleRows(sc.parts.map((p) => p.path));
  const tabs = [
    { id: 'parts', label: `Parts (${sc.parts.length})` },
    // The modules the board's parts are in, as a tree: one is picked and regenerated from here.
    ...(modules.length > 0 ? [{ id: 'modules', label: `Modules (${modules.length})` }] : []),
    { id: 'nets', label: `Nets (${props.drawing.nets.length})` },
    // The routes file's records, when it has some for this board.
    ...(sc.routes.length > 0 ? [{ id: 'routes', label: `Routes (${sc.routes.length})` }] : []),
    ...(props.extra ?? []).map((e) => ({ id: e.id, label: e.label })),
  ];
  return (
    <aside class="side no-pan">
      {proposal.value && proposal.value.board === sc.board && <ProposalCard />}
      {chosen.length > 0 && <PartInspector scene={sc} parts={chosen} onPickNet={props.onPickNet} />}
      {keepout && <KeepoutInspector scene={sc} keepout={keepout} />}
      {route && record && <RouteInspector scene={sc} pick={route} record={record} onFocus={props.onFocus} />}
      <div class="side-tabs">
        {tabs.map((t) => (
          <button key={t.id} class={props.tab === t.id ? 'on' : ''} onClick={() => props.onTab(t.id)}>
            {t.label}
          </button>
        ))}
      </div>
      {props.tab === 'parts' && <PartList scene={sc} onFocus={props.onFocus} />}
      {props.tab === 'modules' && <ModuleList scene={sc} rows={modules} onFocus={props.onFocus} />}
      {props.tab === 'routes' && <RouteList scene={sc} onFocus={props.onFocus} />}
      {props.tab === 'nets' && (
        <NetList drawing={props.drawing} picked={props.picked} onPick={props.onPickNet} onHover={props.onHoverNet} />
      )}
      {(props.extra ?? []).filter((e) => e.id === props.tab).map((e) => e.body)}
    </aside>
  );
}

function PartList(props: { scene: PcbScene; onFocus: (box: PcbBox) => void }) {
  const [filter, setFilter] = useState('');
  const sc = props.scene;
  const q = filter.trim().toLowerCase();
  const parts = sc.parts.filter((p) => !q || `${p.refdes} ${p.value} ${p.kind} ${p.path}`.toLowerCase().includes(q));
  return (
    <>
      <label class="field-icon side-filter">
        <Icon name="search" size={13} />
        <input class="filter" type="search" placeholder="Filter parts" aria-label="Filter parts" value={filter} onInput={(e) => setFilter((e.target as HTMLInputElement).value)} />
      </label>
      <div class="rows">
        {parts.map((p) => (
          <div
            key={p.path}
            class={`row part-row ${selection.value.includes(p.path) ? 'sel' : ''}`}
            data-hv-ref={p.refdes}
            {...hoverAttrs({ kind: 'part', key: p.path })}
            onClick={(e) => {
              const on = selection.value.includes(p.path);
              select(e.shiftKey || e.ctrlKey ? (on ? selection.value.filter((x) => x !== p.path) : [...selection.value, p.path]) : [p.path]);
              props.onFocus(p.courtyard);
            }}
          >
            <b class="ref">{p.refdes}</b>
            <span class="val">{p.value}</span>
            <span class="faint kind">{p.kind}</span>
            {p.locked && <span class="tag lock" title="locked">lock</span>}
            {!p.locked && p.placement !== 'derived' && <span class="tag" title={`placed by ${p.placement}`}>{p.placement}</span>}
            {p.side === 'bottom' && <span class="tag back" data-testid="side-tag" title="on the back of the board (the bottom face)">back</span>}
          </div>
        ))}
        {parts.length === 0 && <EmptyState compact class="none" icon="search" title="No part matches" help="Try a designator, a value or a path." />}
        {sc.keepouts.length > 0 && <div class="sub">Keepouts</div>}
        {sc.keepouts.map((k) => (
          <div key={k.name} class={`row keepout-row ${pickedKeepout.value === k.name ? 'sel' : ''}`} onClick={() => {
            pickKeepout(k.name);
            if (k.area) props.onFocus(k.area);
          }}>
            <b class="ref">{k.name}</b>
            <span class="faint kind">{k.area ? summary(k) : 'no place yet'}</span>
            <span class="tag">{{ drawn: 'drawn', stated: 'physical', placed: 'layout', part: 'part', source: 'source' }[k.origin]}</span>
          </div>
        ))}
      </div>
    </>
  );
}

/** The tree of the board's modules: a click picks the parts of one (and shows them), and its button proposes to
 * place them again, as "Regenerate" does for the parts picked on the board. */
function ModuleList(props: { scene: PcbScene; rows: ModuleRow[]; onFocus: (box: PcbBox) => void }) {
  const sc = props.scene;
  const shown = proposal.value?.board === sc.board ? proposal.value.answer.module : null;
  const partsOf = (m: string) => sc.parts.filter((p) => inModule(p.path, m));
  const picked = (m: string) => {
    const mine = partsOf(m).map((p) => p.path);
    return mine.length > 0 && mine.length === selection.value.length && mine.every((p) => selection.value.includes(p));
  };
  return (
    <div class="rows" data-testid="module-tree" role="tree" aria-label="Modules">
      {props.rows.map((r) => (
        <div
          key={r.path}
          role="treeitem"
          aria-level={r.depth + 1}
          aria-selected={picked(r.path)}
          class={`row module-row ${picked(r.path) ? 'sel' : ''} ${shown === r.path ? 'proposed' : ''}`}
          data-module={r.path}
          style={{ paddingLeft: `${0.75 + r.depth * 0.875}rem` }}
          title={r.path}
          onClick={() => {
            const mine = partsOf(r.path);
            select(mine.map((p) => p.path));
            const box = around(mine.map((p) => p.courtyard));
            if (box) props.onFocus(box);
          }}
        >
          <Icon name="module" size={13} class="module-icon" />
          <b class="ref">{r.label}</b>
          <span class="faint kind">{`${r.parts} part${r.parts === 1 ? '' : 's'}`}</span>
          <button
            class="icon-btn regen-module"
            data-testid="regen-module"
            aria-label={`Regenerate ${r.path}`}
            title={`Regenerate ${r.path}: place its parts again with every other part held, and show the result before anything is written`}
            onClick={(e) => {
              e.stopPropagation();
              void propose(r.path, 0);
            }}
          >
            <Icon name="refresh" size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}

/** The box round several boxes; null for none. */
function around(boxes: PcbBox[]): PcbBox | null {
  if (boxes.length === 0) return null;
  const x0 = Math.min(...boxes.map((b) => b.x));
  const y0 = Math.min(...boxes.map((b) => b.y));
  const x1 = Math.max(...boxes.map((b) => b.x + b.width));
  const y1 = Math.max(...boxes.map((b) => b.y + b.height));
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** What a keepout in the list says: what it forbids, and the height it allows where it states one. */
function summary(k: PcbKeepout): string {
  const what = forbidden(k);
  const limit = k.maxHeight != null ? `max ${fmt(k.maxHeight)} mm tall` : '';
  return [what && `no ${what}`, limit].filter(Boolean).join(', ');
}

function forbidden(k: PcbKeepout): string {
  return [k.tracks && 'tracks', k.vias && 'vias', k.copper && 'copper', k.parts && 'parts'].filter(Boolean).join(', ');
}

function NetList(props: {
  drawing: Drawing;
  picked: string | null;
  onPick: (net: string | null) => void;
  onHover: (net: string | null) => void;
}) {
  const [filter, setFilter] = useState('');
  const nets = props.drawing.nets.filter((n) => n.name.toLowerCase().includes(filter.trim().toLowerCase()));
  return (
    <>
      <label class="field-icon side-filter">
        <Icon name="search" size={13} />
        <input class="filter" type="search" placeholder="Filter nets" aria-label="Filter nets" value={filter} onInput={(e) => setFilter((e.target as HTMLInputElement).value)} />
      </label>
      <div class="rows net-rows">
        {nets.map((n) => (
          <div
            key={n.name}
            class={`net-row ${props.picked === n.name ? 'sel' : ''}`}
            {...hoverAttrs({ kind: 'net', key: n.name })}
            onPointerEnter={(e) => {
              props.onHover(n.name);
              hoverAttrs({ kind: 'net', key: n.name }).onPointerEnter(e);
            }}
            onPointerLeave={(e) => {
              props.onHover(null);
              hoverAttrs({ kind: 'net', key: n.name }).onPointerLeave(e);
            }}
            onClick={() => props.onPick(props.picked === n.name ? null : n.name)}
          >
            <span class={`swatch ${n.plane ? 'plane' : ''}`} aria-hidden="true" />
            <span class="net-name">{n.name}</span>
            <span class="faint">{n.plane ? (n.layer ? `plane ${n.layer}` : 'plane') : n.pads}</span>
          </div>
        ))}
        {nets.length === 0 && <EmptyState compact class="none" icon="search" title="No net matches" help="Try the name of a net." />}
      </div>
    </>
  );
}

/** The part's turn, which a person may type: any angle from 0 up to 360, not only the right
 * angles the Rotate button steps by (issue #996). Enter or leaving the field writes it to the
 * layout file; a locked part shows its turn only. */
function TurnField(props: { part: PcbPart }) {
  const { part } = props;
  const shown = fmt(part.rotation);
  if (part.locked) return <>{shown}°</>;
  return (
    <label class="turn-field">
      <input
        type="number"
        min="0"
        max="360"
        step="any"
        key={`${part.path}:${shown}`}
        value={shown}
        aria-label={`Turn of ${part.refdes} in degrees, counter-clockwise`}
        title="Any angle, counter-clockwise; Enter writes it to the layout file"
        data-testid="part-turn"
        onChange={(e) => {
          const v = parseFloat((e.target as HTMLInputElement).value);
          if (Number.isFinite(v)) void turnSelectionTo(v);
        }}
      />
      °
    </label>
  );
}

function PartInspector(props: { scene: PcbScene; parts: PcbPart[]; onPickNet: (net: string | null) => void }) {
  const { parts, scene: sc } = props;
  const one = parts.length === 1 ? parts[0] : null;
  const allLocked = parts.every((p) => p.locked);
  const allRequired = parts.every(isRequired);
  const elsewhere = parts.find((p) => p.requiredElsewhere);
  const module = moduleOfAll(parts.map((p) => p.path));
  return (
    <section class="inspector insp" data-testid="part-inspector">
      <InspectorHead
        eyebrow={one ? 'Part on the board' : 'Selection'}
        icon="part"
        tone="part"
        mono
        title={one ? one.refdes : `${parts.length} parts`}
        kind={one ? one.value : undefined}
        sub={one ? `${one.kind}${one.package ? ` · ${one.package}` : ''}` : undefined}
        actions={one ? <InspectorAction icon="source" label="Show in the source" onClick={() => pickPart(one.refdes, true)} /> : undefined}
      />
      {one && (
        <Defs
          rows={[
            { label: 'Position', value: `${fmt(one.x - sc.originX)}, ${fmt(one.y - sc.originY)} mm` },
            { label: 'Turn', value: <TurnField part={one} /> },
            { label: 'Face', value: faceWord(one.side) },
            { label: 'Placed by', value: `${one.locked ? 'locked, ' : ''}${one.placement}` },
            { label: 'Height', value: one.height == null ? 'not stated' : `${fmt(one.height)} mm` },
          ]}
        />
      )}
      <div class="actions">
        <button class="tool small" title="Rotate 90° (R, Shift+R the other way)" onClick={() => void rotateSelection(90)}>
          <Icon name="rotate" size={13} />
          Rotate
        </button>
        <button class="tool small" title="Put on the other face (F)" onClick={() => void flipSelection()}>
          <Icon name="flip" size={13} />
          Flip
        </button>
        <button class="tool small" title="Lock or unlock (L)" onClick={() => void toggleLock()}>
          <Icon name={allLocked ? 'unlock' : 'lock'} size={13} />
          {allLocked ? 'Unlock' : 'Lock'}
        </button>
        <button class="tool small" title="Remove its entry from the layout file; the placer places it (Delete)" onClick={() => void releaseSelection()}>
          <Icon name="close" size={13} />
          Release
        </button>
        {module && (
          <button
            class="tool small"
            data-testid="regenerate"
            title={`Place the parts of ${module} again with every other part held, and show the result before anything is written`}
            onClick={() => void propose(module, 0)}
          >
            <Icon name="refresh" size={13} />
            Regenerate {module}
          </button>
        )}
        <button
          class="tool small"
          data-testid="require-here"
          disabled={!!elsewhere}
          title={
            elsewhere
              ? `The physical file states where ${elsewhere.refdes} goes, ${elsewhere.requiredElsewhere}: change it there`
              : allRequired
                ? 'Take out the requirements on its place from the physical file'
                : 'Write where it is, its turn and its face as requirements in the physical file: the placer keeps it there and fab checks it'
          }
          onClick={() => void toggleRequired()}
        >
          <Icon name={allRequired ? 'close' : 'ruler'} size={13} />
          {allRequired ? 'Drop requirement' : 'Require here'}
        </button>
      </div>
      {one && one.requirements.length > 0 && (
        <>
          <div class="ih-sec">
            Requirements <span class="disc-count">{one.requirements.length}</span>
          </div>
          <ul class="req-list" data-testid="requirement-list">
            {one.requirements.map((r) => (
              <li key={`${r.id}:${r.field}`} title={r.source ? `${r.source}${r.module ? ', for every part of the module' : ''}` : undefined}>
                <code>{r.id}</code> <span class="faint">{rangeText(r)}</span>
              </li>
            ))}
          </ul>
        </>
      )}
      {one && (
        <>
          <div class="ih-sec">
            Pads <span class="disc-count">{one.pads.length}</span>
          </div>
          <table class="pads">
            <tbody>
              {one.pads.map((pad, i) => (
                <tr key={`${pad.number}-${i}`}>
                  <td class="pad-no">{pad.number}</td>
                  <td>
                    {pad.net ? (
                      <PinChip title="Light this net on the board" hv={{ kind: 'net', key: pad.net }} onClick={() => props.onPickNet(pad.net)}>
                        {pad.net}
                      </PinChip>
                    ) : (
                      <span class="faint">no net</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}

/** A regeneration the server proposed (issue 825): what it moves, what copper it routes again, and the findings and totals
 * before and after; Accept writes it as one edit, Another asks for the next seed, Reject drops it. */
function ProposalCard() {
  const p = proposal.value?.answer;
  if (!p) return null;
  const moved = p.parts.length;
  return (
    <section class="inspector insp regen-card" data-testid="regen-card">
      <InspectorHead
        eyebrow="Proposal"
        icon="module"
        tone="module"
        mono
        title={`Regenerate ${p.module}`}
        sub={`Seed ${p.seed}. Nothing is written until you accept; every other part stays where it is.`}
      />
      <Defs
        rows={[
          { label: 'Placed again', value: p.parts.map((q) => q.refdes).join(', ') || 'none' },
          ...(p.held.length > 0 ? [{ label: 'Held', value: p.held.join(', ') }] : []),
          { label: 'Routed again', value: p.nets.length > 0 ? p.nets.join(', ') : 'nothing' },
          ...(p.routesDropped.length > 0 ? [{ label: 'Routes file', value: `no longer matches: ${p.routesDropped.join(', ')}` }] : []),
        ]}
      />
      <ul class="regen-totals" data-testid="regen-totals">
        {totalsLines(p.before, p.after).map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      {(p.appeared.length > 0 || p.gone.length > 0) && (
        <ul class="regen-findings" data-testid="regen-findings">
          {p.appeared.map((f) => (
            <li key={`+${f}`} class="appeared">new: {f}</li>
          ))}
          {p.gone.map((f) => (
            <li key={`-${f}`} class="gone">gone: {f}</li>
          ))}
        </ul>
      )}
      <div class="actions">
        <button class="tool small primary" data-testid="regen-accept" disabled={moved === 0} title="Write the new places to the layout file: one step of undo" onClick={() => void acceptProposal()}>
          <Icon name="check" size={13} />
          Accept
        </button>
        <button class="tool small" data-testid="regen-another" title="Another arrangement, with the next seed" onClick={() => void proposeAnother()}>
          <Icon name="refresh" size={13} />
          Another
        </button>
        <button class="tool small" data-testid="regen-reject" title="Drop the proposal; nothing was written" onClick={() => dropProposal()}>
          <Icon name="close" size={13} />
          Reject
        </button>
      </div>
    </section>
  );
}

function KeepoutInspector(props: { scene: PcbScene; keepout: PcbKeepout }) {
  const { keepout: k, scene: sc } = props;
  const drawn = k.origin === 'drawn';
  const set = (to: Partial<PcbKeepout>, what: string) => void changeKeepout(k, to, `${what} keepout ${k.name}`);
  const toggleLayer = (layer: string) => {
    const layers = k.layers.includes(layer) ? k.layers.filter((l) => l !== layer) : [...k.layers, layer];
    if (layers.length > 0) set({ layers: sc.layers.filter((l) => layers.includes(l)) }, 'change the layers of');
  };
  const toggle = (field: 'tracks' | 'vias' | 'copper' | 'parts') => {
    const next = { ...k, [field]: !k[field] };
    if (!(next.tracks || next.vias || next.copper || next.parts)) return;
    set({ [field]: !k[field] }, `change what is forbidden by`);
  };
  return (
    <section class="inspector insp" data-testid="keepout-inspector">
      <InspectorHead
        eyebrow="Keepout"
        icon="keepout"
        tone="module"
        mono
        title={k.name}
        sub={
          drawn
            ? 'Drawn here, kept in the physical file.'
            : k.origin === 'stated'
              ? 'Stated in the physical file: moving or resizing it changes its numbers there; the rest is changed in the file.'
              : k.origin === 'placed'
              ? 'Sized by the source; the layout file places it.'
              : k.origin === 'part'
                ? 'Carried by a part: it moves with it.'
                : 'Placed and sized by the source: change it there.'
        }
      />
      <Defs
        rows={[
          { label: 'Size', value: `${fmt(k.width)} x ${fmt(k.height)} mm` },
          ...(k.maxHeight != null ? [{ label: 'Max height', value: `${fmt(k.maxHeight)} mm on ${k.layers.join(', ')}` }] : []),
          ...(k.area ? [{ label: 'Position', value: `${fmt(k.area.x - sc.originX)}, ${fmt(k.area.y - sc.originY)} mm` }] : []),
        ]}
      />
      <div class="checks">
        <span class="faint">forbids</span>
        {(['tracks', 'vias', 'copper', 'parts'] as const).map((f) => (
          <label key={f}>
            <input type="checkbox" checked={k[f]} disabled={!drawn} onChange={() => toggle(f)} />
            {f}
          </label>
        ))}
      </div>
      <div class="checks">
        <span class="faint">layers</span>
        {sc.layers.map((l) => (
          <label key={l}>
            <input type="checkbox" checked={k.layers.includes(l)} disabled={!drawn} onChange={() => toggleLayer(l)} />
            {l}
          </label>
        ))}
      </div>
      <div class="actions">
        {k.origin === 'placed' && !k.area && (
          <button class="tool" onClick={() => void placeKeepout(k.name)}>Place it</button>
        )}
        {drawn && <button class="tool" onClick={() => void deleteKeepout(k.name)}>Delete</button>}
        <button class="tool" onClick={() => pickKeepout(null)}>Done</button>
      </div>
    </section>
  );
}

/** The routes file's records: each net with its tracks and vias, numbered as `netsmith routes show` numbers
 * them, with what is locked and what is not drawn. A click picks one and zooms to it. */
function RouteList(props: { scene: PcbScene; onFocus: (box: PcbBox) => void }) {
  const sc = props.scene;
  const picked = pickedRoute.value;
  const choose = (net: PcbRouteNet, pick: RoutePick) => {
    pickRoute(pick);
    const box = boxOf(net, pick);
    if (box) props.onFocus(box);
  };
  if (sc.routes.length === 0) {
    return (
      <EmptyState
        compact
        class="none"
        icon="route"
        title="No fixed copper"
        help={`${sc.routesFile} has no record for this board. \`netsmith routes import\`, \`add-track\` and \`add-via\` write one; its copper is drawn before the router's, which keeps it.`}
      />
    );
  }
  return (
    <div class="rows route-rows" data-testid="route-list">
      {sc.routes.map((net) => (
        <div key={net.net} class="route-net">
          <div
            class={`row part-row route-row ${samePick(picked, { net: net.net }) ? 'sel' : ''}`}
            data-route-row={net.net}
            {...hoverAttrs({ kind: 'net', key: net.net })}
            onClick={() => choose(net, { net: net.net })}
          >
            <b class="ref">{net.net}</b>
            <span class="faint kind">
              {net.tracks.length} track{net.tracks.length === 1 ? '' : 's'}, {net.vias.length} via{net.vias.length === 1 ? '' : 's'}
            </span>
            {net.lock && <span class="tag lock" title="locked: the router adds no copper to the net">lock</span>}
            {!net.current && (
              <span class="tag warn" title="the net's pads are not where the record was written: only its locked shapes are drawn (see the findings)">
                stale
              </span>
            )}
          </div>
          {net.tracks.map((t, i) => (
            <div
              key={`t${i}`}
              class={`row part-row route-shape ${samePick(picked, { net: net.net, track: i + 1 }) ? 'sel' : ''} ${t.drawn ? '' : 'undrawn'}`}
              data-route-row={`${net.net}/track/${i + 1}`}
              onClick={() => choose(net, { net: net.net, track: i + 1 })}
            >
              <span class="ref">track {i + 1}</span>
              <span class="faint kind">
                {t.layer}, {fmt(t.width)} mm{t.mid ? ', arc' : ''}
              </span>
              {t.lock && <span class="tag lock" title="locked: kept when the net's pads move">lock</span>}
              {!t.drawn && <span class="tag" title="not on the board: its record is stale">not drawn</span>}
            </div>
          ))}
          {net.vias.map((v, i) => (
            <div
              key={`v${i}`}
              class={`row part-row route-shape ${samePick(picked, { net: net.net, via: i + 1 }) ? 'sel' : ''} ${v.drawn ? '' : 'undrawn'}`}
              data-route-row={`${net.net}/via/${i + 1}`}
              onClick={() => choose(net, { net: net.net, via: i + 1 })}
            >
              <span class="ref">via {i + 1}</span>
              <span class="faint kind">
                {fmt(v.diameter)} mm, drill {fmt(v.drill)} mm
              </span>
              {v.lock && <span class="tag lock" title="locked: kept when the net's pads move">lock</span>}
              {!v.drawn && <span class="tag" title="not on the board: its record is stale">not drawn</span>}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

/** A record of the routes file, or one shape of it: where it is, and lock, unlock and delete, which the
 * server makes as `netsmith routes lock`, `unlock` and `delete` do and refuses (a locked shape) with the reason. */
function RouteInspector(props: { scene: PcbScene; pick: RoutePick; record: PcbRouteNet; onFocus: (box: PcbBox) => void }) {
  const { pick, record, scene: sc } = props;
  const shape = shapeOf(record, pick);
  const at = (p: { x: number; y: number }) => `${fmt(p.x - sc.originX)}, ${fmt(p.y - sc.originY)} mm`;
  const rows = !shape
    ? [
        { label: 'Copper', value: `${record.tracks.length} tracks, ${record.vias.length} vias` },
        { label: 'Router', value: record.lock ? 'adds nothing (locked)' : 'may add copper' },
        { label: 'Pads', value: record.current ? 'as written' : 'moved: only locked shapes drawn' },
      ]
    : 'layer' in shape
      ? [
          { label: 'Layer', value: shape.layer },
          { label: 'Width', value: `${fmt(shape.width)} mm` },
          { label: 'From', value: at(shape.from) },
          ...(shape.mid ? [{ label: 'Through', value: at(shape.mid) }] : []),
          { label: 'To', value: at(shape.to) },
          { label: 'On the board', value: shape.drawn ? 'yes' : 'no (stale record)' },
        ]
      : [
          { label: 'At', value: at(shape.at) },
          { label: 'Size', value: `${fmt(shape.diameter)} mm, drill ${fmt(shape.drill)} mm` },
          { label: 'On the board', value: shape.drawn ? 'yes' : 'no (stale record)' },
        ];
  const locked = shape ? shape.lock : record.lock;
  return (
    <section class="inspector insp" data-testid="route-inspector">
      <InspectorHead
        eyebrow={shape ? 'Fixed copper' : 'Routes file record'}
        icon="route"
        tone="net"
        mono
        title={shape ? nameOf(pick) : `net ${record.net}`}
        sub={`${sc.routesFile}${record.lock ? ' · net locked' : ''}${shape?.lock ? ' · shape locked' : ''}`}
      />
      <Defs rows={rows} />
      <div class="actions">
        <button
          class="tool small"
          title={shape ? `${locked ? 'Unlock' : 'Lock'} it: a locked shape is kept when the net's pads move, and not deleted (L)` : `${locked ? 'Unlock' : 'Lock'} the net: the router adds no copper to a locked net (L)`}
          data-testid="route-lock"
          onClick={() => void changeRoutes([lockChange(pick, !locked)])}
        >
          <Icon name={locked ? 'unlock' : 'lock'} size={13} />
          {locked ? 'Unlock' : 'Lock'}
        </button>
        {shape && (
          <button
            class="tool small"
            title={`${record.lock ? 'Unlock' : 'Lock'} the whole net ${record.net}`}
            data-testid="route-lock-net"
            onClick={() => void changeRoutes([lockChange({ net: record.net }, !record.lock)])}
          >
            <Icon name={record.lock ? 'unlock' : 'lock'} size={13} />
            {record.lock ? 'Unlock net' : 'Lock net'}
          </button>
        )}
        <button
          class="tool small"
          title={shape ? 'Delete it from the routes file (Delete); refused while it is locked' : "Delete the net's record (Delete); refused while it or a shape of it is locked"}
          data-testid="route-delete"
          onClick={() => void changeRoutes([deleteChange(pick)])}
        >
          <Icon name="close" size={13} />
          Delete
        </button>
        <button class="tool small" onClick={() => pickRoute(null)}>
          Done
        </button>
      </div>
    </section>
  );
}

function fmt(v: number): string {
  return String(Math.round(v * 100) / 100);
}
