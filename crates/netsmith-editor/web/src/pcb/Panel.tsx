// The side panel of the PCB view: what is selected, the parts, the keepouts and the nets.

import { useState } from 'preact/hooks';

import type { PcbBox, PcbKeepout, PcbPart, PcbRouteNet, PcbScene } from '../api';
import { type RoutePick, changeRoutes, pickKeepout, pickRoute, pickedKeepout, pickedRoute, select, selection } from '../pcbstore';
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
} from './actions';
import type { Drawing } from './drawing';

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
  const tabs = [
    { id: 'parts', label: `Parts (${sc.parts.length})` },
    { id: 'nets', label: `Nets (${props.drawing.nets.length})` },
    // The routes file's records, when it has some for this board.
    ...(sc.routes.length > 0 ? [{ id: 'routes', label: `Routes (${sc.routes.length})` }] : []),
    ...(props.extra ?? []).map((e) => ({ id: e.id, label: e.label })),
  ];
  return (
    <aside class="side no-pan">
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
            {p.side === 'bottom' && <span class="tag" title="on the bottom face">bottom</span>}
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
            <span class="tag">{{ drawn: 'drawn', placed: 'layout', part: 'part', source: 'source' }[k.origin]}</span>
          </div>
        ))}
      </div>
    </>
  );
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

function PartInspector(props: { scene: PcbScene; parts: PcbPart[]; onPickNet: (net: string | null) => void }) {
  const { parts, scene: sc } = props;
  const one = parts.length === 1 ? parts[0] : null;
  const allLocked = parts.every((p) => p.locked);
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
            { label: 'Turn', value: `${fmt(one.rotation)}°` },
            { label: 'Face', value: one.side },
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
      </div>
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
            ? 'Drawn here, kept in the layout file.'
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
