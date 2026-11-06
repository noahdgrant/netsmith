// The card that shows in the source when the pointer rests on a part, a net, a module or a
// definition (or on the keyboard's request): a title row with its icon, what it is, its pins or members
// as chips that pick and light the same thing in the other views, and where to see it. Built from the
// shared inspector parts (`Inspector.tsx`); the data is assembled by `source/card.ts`.

import { useEffect } from 'preact/hooks';

import { hoverEnter, hoverLeave } from '../hover';
import type { BomLine } from '../api';
import { partSel, selectAndReveal, setSelection, netSel } from '../selection';
import { type Card, type CardContext, type DefinitionCard, type ModuleCard, type NetCard, type PartCard, type Target, cardFor } from '../source/card';
import { bomLineOf, bomLines, ensureBom, ensurePower, ensureText, power, textOfFile } from '../source/boarddata';
import { analysis, board, detail, goTo, project, revealSection, shown, viewBoardOfDefinition } from '../store';
import { dollars, plural } from '../util';
import { Defs, InspectorHead, PinChip } from './Inspector';
import { Icon, type IconName } from './Icons';
import { Pill, Units } from './ui';

const SECTION_ICON: Record<string, IconName> = { schematic: 'schematic', pcb: 'pcb', bom: 'bom', power: 'power' };

/** The card's context from the page: the board, the problems, the project, and what was fetched for cards. */
export function contextFor(path: string): CardContext {
  return {
    path,
    detail: detail.value,
    board: board.value,
    project: project.value,
    problems: shown.value?.problems ?? analysis.value?.problems ?? [],
    power: power.value,
    textOf: textOfFile,
  };
}

/** Pick `sel` in every view, bring it into view in `section`. */
function showOn(section: 'schematic' | 'pcb' | 'bom', sel: ReturnType<typeof partSel>, done: () => void) {
  selectAndReveal(sel);
  revealSection(section);
  done();
}

function ShowRow({ sel, sections, done }: { sel: ReturnType<typeof partSel>; sections: ('schematic' | 'pcb' | 'bom')[]; done: () => void }) {
  const label = { schematic: 'Schematic', pcb: 'PCB', bom: 'BOM' } as const;
  return (
    <div class="hc-foot" role="group" aria-label="Show on">
      <span class="hc-foot-label">Show on</span>
      {sections.map((s) => (
        <button key={s} type="button" class="tool small hc-show" data-section={s} onClick={() => showOn(s, sel, done)}>
          <Icon name={SECTION_ICON[s]} size={13} />
          {label[s]}
        </button>
      ))}
    </div>
  );
}

function catalogRows(card: PartCard, line: BomLine | undefined) {
  const rows: { label: string; value: preact.ComponentChildren; mono?: boolean }[] = [];
  if (card.package) rows.push({ label: 'Package', value: card.package, mono: true });
  const cat = [card.lcsc, line?.library].filter(Boolean).join(' · ');
  if (cat) rows.push({ label: 'Catalog', value: cat, mono: true });
  if (line?.mpn) rows.push({ label: 'MPN', value: line.mpn, mono: true });
  if (line?.unitUsd != null) rows.push({ label: 'Price', value: `${dollars(line.unitUsd)} each${line.stock != null ? `, ${line.stock.toLocaleString('en-US')} in stock` : ''}` });
  rows.push({ label: 'Assembly', value: card.assembled ? 'Placed by the fab' : 'Not placed by the fab' });
  return rows;
}

function PartBody({ card, done }: { card: PartCard; done: () => void }) {
  const line = bomLineOf(bomLines.value, card.refdes);
  const here = detail.value;
  const pick = (net: string) => setSelection(netSel(net));
  return (
    <>
      <InspectorHead
        eyebrow={card.module ? `Part in ${card.module}` : 'Part'}
        icon="part"
        tone="part"
        kind={card.partKind}
        title={
          <>
            {card.refdes} <span class="hc-value">{card.value}</span>
          </>
        }
        mono
        sub={<span class="mono">{card.path}</span>}
        actions={card.problems ? <Pill tone="warn" icon="warning">{plural(card.problems, 'problem')}</Pill> : undefined}
        titleTag="div"
      />
      <Defs rows={catalogRows(card, line)} />
      {card.pins.length > 0 && (
        <div class="hc-pins" role="group" aria-label="Pins and their nets">
          <div class="hc-label">Pins</div>
          <div class="hc-chips">
            {card.pins.map((p) => (
              <PinChip key={p.pin} title={`Pin ${p.pin} is on ${p.net}. Click to pick the net.`} hv={{ kind: 'net', key: p.net }} onClick={() => pick(p.net)} class="hc-pin">
                <b>{p.pin}</b>
                <span class={p.named ? '' : 'faint'}>{p.net}</span>
              </PinChip>
            ))}
          </div>
        </div>
      )}
      {card.also.length > 0 && (
        <div class="hc-pins">
          <div class="hc-label">The same declaration is also</div>
          <div class="hc-chips">
            {card.also.slice(0, 6).map((p) => (
              <PinChip key={p.path} title={p.path} hv={{ kind: 'part', key: p.path }} onClick={() => setSelection(partSel(p.path))}>
                {p.refdes}
              </PinChip>
            ))}
            {card.also.length > 6 && <span class="hc-more">+{card.also.length - 6}</span>}
          </div>
        </div>
      )}
      {here && <ShowRow sel={card.sel} sections={['schematic', 'pcb', 'bom']} done={done} />}
    </>
  );
}

function NetBody({ card, done }: { card: NetCard; done: () => void }) {
  const d = detail.value;
  const span = [...card.modules, ...(card.onBoard ? ['the board'] : [])];
  return (
    <>
      <InspectorHead
        eyebrow="Net"
        icon="net"
        tone="net"
        kind={plural(card.pins.length, 'pin')}
        title={card.name}
        mono
        sub={span.length ? `On ${span.join(', ')}` : undefined}
        titleTag="div"
      />
      {card.rail ? (
        <Defs
          rows={[
            ...(card.rail.voltage ? [{ label: 'Voltage', value: <Units text={card.rail.voltage} /> }] : []),
            ...(card.rail.demand ? [{ label: 'Demand', value: <Units text={card.rail.demand} /> }] : []),
            ...(card.rail.capacity ? [{ label: 'Supply', value: <Units text={card.rail.capacity} /> }] : []),
            { label: 'Rail', value: `${card.rail.supplies} ${card.rail.supplies === 1 ? 'supply' : 'supplies'}, ${plural(card.rail.loads, 'load')}`, tone: card.rail.budget === 'over' ? 'bad' : undefined },
          ]}
        />
      ) : (
        <p class="hc-note">{card.named ? 'Not a power rail the solver tracks.' : 'Unnamed: the compiler made this name.'}</p>
      )}
      <div class="hc-pins" role="group" aria-label="Pins on the net">
        <div class="hc-label">Pins</div>
        <div class="hc-chips">
          {card.pins.slice(0, 14).map((p) => {
            const part = d?.parts.find((x) => x.refdes === p.refdes);
            return (
              <PinChip key={`${p.refdes}.${p.pin}`} title={part ? `${p.refdes} ${part.value}, pin ${p.pin}. Click to pick the part.` : p.refdes} hv={part ? { kind: 'part', key: part.path } : undefined} onClick={() => part && setSelection(partSel(part.path))}>
                <b>{p.refdes}</b>
                <span>.{p.pin}</span>
              </PinChip>
            );
          })}
          {card.pins.length > 14 && <span class="hc-more">+{card.pins.length - 14}</span>}
        </div>
      </div>
      <ShowRow sel={card.sel} sections={['schematic', 'pcb', 'bom']} done={done} />
    </>
  );
}

function ModuleBody({ card, done }: { card: ModuleCard; done: () => void }) {
  return (
    <>
      <InspectorHead
        eyebrow="Module instance"
        icon="module"
        tone="module"
        kind={card.typeName ?? undefined}
        title={card.path}
        mono
        sub={`${plural(card.parts.length, 'part')}, ${plural(card.nets.length, 'net')}${card.also.length ? `, ${card.also.length} more like it` : ''}`}
        titleTag="div"
      />
      <div class="hc-pins" role="group" aria-label="Parts in the module">
        <div class="hc-label">Parts</div>
        <div class="hc-chips">
          {card.parts.slice(0, 12).map((p) => (
            <PinChip key={p.path} title={`${p.refdes} ${p.value}. Click to pick it.`} hv={{ kind: 'part', key: p.path }} onClick={() => setSelection(partSel(p.path))}>
              <b>{p.refdes}</b>
              <span>{p.value}</span>
            </PinChip>
          ))}
          {card.parts.length > 12 && <span class="hc-more">+{card.parts.length - 12}</span>}
        </div>
      </div>
      <ShowRow sel={card.sel} sections={['schematic', 'pcb', 'bom']} done={done} />
    </>
  );
}

const DEF_ICON: Record<string, IconName> = { board: 'board', system: 'system', panel: 'panel', module: 'module', part: 'part', interface: 'bus' };

function DefinitionBody({ card, done }: { card: DefinitionCard; done: () => void }) {
  const shownMembers = card.members.slice(0, 8);
  const go = () => {
    void goTo({ file: card.file, line: card.line, column: 1, endLine: card.line, endColumn: 1 }, card.name);
    viewBoardOfDefinition(card.defKind, card.name);
    done();
  };
  return (
    <>
      <InspectorHead
        eyebrow={card.library ? 'Library definition' : 'Definition'}
        icon={DEF_ICON[card.defKind] ?? 'definition'}
        tone={card.defKind === 'interface' ? 'bus' : card.defKind}
        kind={card.defKind}
        title={card.name}
        mono
        sub={<span class="mono">{card.file}:{card.line}</span>}
        actions={
          card.errors + card.warnings > 0 ? (
            <Pill tone={card.errors ? 'bad' : 'warn'} icon={card.errors ? 'error' : 'warning'}>
              {card.errors + card.warnings}
            </Pill>
          ) : undefined
        }
        titleTag="div"
      />
      <pre class="hc-sig">{card.signature}</pre>
      {card.note && <p class="hc-note">{card.note}</p>}
      {card.doc.length > 0 && <p class="hc-doc">{card.doc.join(' ')}</p>}
      {card.partial && <p class="hc-note">Reading {card.file}…</p>}
      {shownMembers.length > 0 && (
        <div class="hc-members" role="list" aria-label="What it declares">
          {shownMembers.map((m) => (
            <div class="hc-member" role="listitem" key={m.line + m.name}>
              <span class="mono">.{m.name}</span>
              {m.typeName && <span class="hc-type mono">{m.typeName}</span>}
            </div>
          ))}
          {card.members.length > shownMembers.length && <div class="hc-more">and {card.members.length - shownMembers.length} more</div>}
        </div>
      )}
      <div class="hc-foot">
        {(card.parts > 0 || card.ports > 0) && (
          <span class="hc-count">
            {plural(card.parts, 'part')}, {plural(card.ports, 'port')}
          </span>
        )}
        <button type="button" class="tool small hc-show" onClick={go}>
          <Icon name="source" size={13} />
          Go to definition
        </button>
      </div>
    </>
  );
}

/** The kind of selection a card stands for, so the hover store lights it in the other views while the card is open. */
function litKey(card: Card): { kind: 'part' | 'net' | 'module'; key: string } | null {
  switch (card.type) {
    case 'part':
      return { kind: 'part', key: card.path };
    case 'net':
      return { kind: 'net', key: card.name };
    case 'module':
      return { kind: 'module', key: card.path };
    default:
      return null;
  }
}

export function HoverCard({ target, path, close }: { target: Target; path: string; close: () => void }) {
  const ctx = contextFor(path);
  const card = cardFor(ctx, target);
  useEffect(() => {
    // The data a card may want, asked for once: the rail of a net, the catalog line of a part, the text of a library file.
    if (card?.type === 'net') ensurePower();
    if (card?.type === 'part') ensureBom();
    if (card?.type === 'definition' && card.partial) ensureText(card.file);
  }, [card?.type, card?.type === 'definition' ? card.partial : false]);
  const lit = card ? litKey(card) : null;
  useEffect(() => {
    if (!lit) return;
    hoverEnter(lit);
    return () => hoverLeave(lit);
  }, [lit?.kind, lit?.key]);
  if (!card) return null;
  const kind = card.type === 'definition' ? 'definition' : card.type;
  return (
    <div class={`hc hc-${kind}`} role="dialog" aria-label={`${card.type === 'part' ? card.refdes : card.type === 'net' ? `Net ${card.name}` : card.type === 'module' ? card.path : card.name}`} data-testid="hover-card" data-kind={kind}>
      {card.type === 'part' && <PartBody card={card} done={close} />}
      {card.type === 'net' && <NetBody card={card} done={close} />}
      {card.type === 'module' && <ModuleBody card={card} done={close} />}
      {card.type === 'definition' && <DefinitionBody card={card} done={close} />}
    </div>
  );
}
