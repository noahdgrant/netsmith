// The side panel of the PCB view: what is selected, the parts, the keepouts and the nets.

import { useState } from 'preact/hooks';

import type { PcbBox, PcbKeepout, PcbPart, PcbScene } from '../api';
import { pickKeepout, pickedKeepout, select, selection } from '../pcbstore';
import { Defs, InspectorAction, InspectorHead, PinChip } from '../components/Inspector';
import { Icon } from '../components/Icons';
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
  const tabs = [
    { id: 'parts', label: `Parts (${sc.parts.length})` },
    { id: 'nets', label: `Nets (${props.drawing.nets.length})` },
    ...(props.extra ?? []).map((e) => ({ id: e.id, label: e.label })),
  ];
  return (
    <aside class="side no-pan">
      {chosen.length > 0 && <PartInspector scene={sc} parts={chosen} onPickNet={props.onPickNet} />}
      {keepout && <KeepoutInspector scene={sc} keepout={keepout} />}
      <div class="side-tabs">
        {tabs.map((t) => (
          <button key={t.id} class={props.tab === t.id ? 'on' : ''} onClick={() => props.onTab(t.id)}>
            {t.label}
          </button>
        ))}
      </div>
      {props.tab === 'parts' && <PartList scene={sc} onFocus={props.onFocus} />}
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
        {parts.length === 0 && <div class="none">No part matches.</div>}
        {sc.keepouts.length > 0 && <div class="sub">Keepouts</div>}
        {sc.keepouts.map((k) => (
          <div key={k.name} class={`row keepout-row ${pickedKeepout.value === k.name ? 'sel' : ''}`} onClick={() => {
            pickKeepout(k.name);
            if (k.area) props.onFocus(k.area);
          }}>
            <b class="ref">{k.name}</b>
            <span class="faint kind">{k.area ? `no ${forbidden(k)}` : 'no place yet'}</span>
            <span class="tag">{{ drawn: 'drawn', placed: 'layout', part: 'part', source: 'source' }[k.origin]}</span>
          </div>
        ))}
      </div>
    </>
  );
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
            onPointerEnter={() => props.onHover(n.name)}
            onPointerLeave={() => props.onHover(null)}
            onClick={() => props.onPick(props.picked === n.name ? null : n.name)}
          >
            <span class={`swatch ${n.plane ? 'plane' : ''}`} aria-hidden="true" />
            <span class="net-name">{n.name}</span>
            <span class="faint">{n.plane ? 'plane' : n.pads}</span>
          </div>
        ))}
        {nets.length === 0 && <div class="none">No net matches.</div>}
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
                      <PinChip title="Light this net on the board" onClick={() => props.onPickNet(pad.net)}>
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

function fmt(v: number): string {
  return String(Math.round(v * 100) / 100);
}
