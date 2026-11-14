// The card that shows in the source when the pointer rests on a part, a net, a module or a
// definition (or on the keyboard's request): a title row with its icon, what it is, its pins or members
// as chips that pick and light the same thing in the other views, and where to see it. Built from the
// shared inspector parts (`Inspector.tsx`); the data is assembled by `source/card.ts`.

import { useEffect, useState } from 'preact/hooks';

import { hoverEnter, hoverLeave } from '../hover';
import type { BomLine, Solved } from '../api';
import { partSel, selectAndReveal, setSelection, netSel } from '../selection';
import { type Card, type CardContext, type DefinitionCard, type ModuleCard, type NetCard, type PartCard, type Target, boardsUsing, cardFor, offersBoards, solvedRows, usesOf, valueFromText } from '../source/card';
import { bomLineOf, bomLines, detailOfBoard, ensureBoardDetail, ensureBom, ensureDefinition, ensurePower, ensureText, power, servedDefinition, textOfFile } from '../source/boarddata';
import { analysis, board, chooseBoard, detail, goTo, openFiles, project, revealSection, shown, viewBoardOfDefinition } from '../store';
import { dollars, plural } from '../util';
import { Defs, InspectorHead, PinChip } from './Inspector';
import { Icon, type IconName } from './Icons';
import { Pill, Units } from './ui';

const SECTION_ICON: Record<string, IconName> = { schematic: 'schematic', pcb: 'pcb', bom: 'bom', power: 'power' };

/** The card's context from the page: the board, the problems, the project, and what was fetched for cards. */
export function contextFor(path: string, over: Partial<CardContext> = {}): CardContext {
  return {
    path,
    detail: detail.value,
    board: board.value,
    project: project.value,
    problems: shown.value?.problems ?? analysis.value?.problems ?? [],
    power: power.value,
    textOf: textOfFile,
    ...over,
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
  if (card.package) {
    const from = card.packageFrom;
    const value = from ? (
      <>
        {card.package}{' '}
        <button type="button" class="link hc-from" data-testid="hc-package-from" title="Go to the statement that gives the package" onClick={() => void goTo(from.at)}>
          {valueFromText(from)}
        </button>
      </>
    ) : (
      card.package
    );
    rows.push({ label: 'Package', value, mono: true });
  }
  const cat = [card.lcsc, line?.library].filter(Boolean).join(' · ');
  if (cat) rows.push({ label: 'Catalog', value: cat, mono: true });
  if (line?.mpn) rows.push({ label: 'MPN', value: line.mpn, mono: true });
  if (line?.unitUsd != null) rows.push({ label: 'Price', value: `${dollars(line.unitUsd)} each${line.stock != null ? `, ${line.stock.toLocaleString('en-US')} in stock` : ''}` });
  rows.push({ label: 'Assembly', value: card.assembled ? 'Placed by the fab' : 'Not placed by the fab' });
  return rows;
}

/** What the solver worked out for a part: what a capacitor is worth in use, its tolerance, what it is rated for and the catalog row. */
function SolvedBlock({ solved }: { solved: Solved }) {
  const rows = solvedRows(solved);
  if (!rows.length) return null;
  return (
    <div class="hc-solved" role="group" aria-label="What the solver worked out" data-testid="hc-solved">
      <div class="hc-label">Solved</div>
      <Defs rows={rows} />
    </div>
  );
}

/** The boards that use the module a declaration is in, one chip each: the card is for the one that is pressed. */
function BoardSwitch({ boards, active, shown: shownBoard, onPick }: { boards: string[]; active: string | null; shown: string | null; onPick: (board: string) => void }) {
  return (
    <div class="hc-boards" role="group" aria-label="Boards that use this module" data-testid="hc-boards">
      <div class="hc-label">{boards.length > 1 ? `Used by ${boards.length} boards` : 'Used by'}</div>
      <div class="hc-chips">
        {boards.map((b) => (
          <PinChip key={b} current={b === active} pressed={b === active} title={b === shownBoard ? `${b}, the board the views show` : `Show this card for ${b}`} onClick={() => onPick(b)} class="hc-board">
            {b}
            {b === shownBoard && <span class="faint hc-shown"> shown</span>}
          </PinChip>
        ))}
      </div>
    </div>
  );
}

/** Where to see a card that is about another board than the one the views show: switch the views to it. */
function OtherBoardRow({ name, done }: { name: string; done: () => void }) {
  return (
    <div class="hc-foot" role="group" aria-label="Another board">
      <span class="hc-foot-label">This is {name}</span>
      <button type="button" class="tool small hc-show" data-testid="hc-open-board" onClick={() => (chooseBoard(name), done())}>
        <Icon name="board" size={13} />
        Show it in the views
      </button>
    </div>
  );
}

function PartBody({ card, done, other }: { card: PartCard; done: () => void; other: string | null }) {
  const line = other ? undefined : bomLineOf(bomLines.value, card.refdes);
  const here = detail.value;
  const pick = (net: string) => setSelection(netSel(net));
  return (
    <>
      <InspectorHead
        titleId="hc-title"
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
      {card.solved && <SolvedBlock solved={card.solved} />}
      {card.pins.length > 0 && (
        <div class="hc-pins" role="group" aria-label="Pins and their nets">
          <div class="hc-label">Pins</div>
          <div class="hc-chips">
            {card.pins.map((p) => (
              <PinChip key={p.pin} title={`Pin ${p.pin}${p.name ? ` (${p.name})` : ''}${p.function ? `, as ${p.function}` : ''} is on ${p.net}. Click to pick the net.`} hv={{ kind: 'net', key: p.net }} onClick={() => pick(p.net)} class="hc-pin">
                <b>{p.pin}</b>
                {p.name && <span class="hc-pname">{p.name}</span>}
                <span class={p.named ? '' : 'faint'}>{p.net}</span>
                {p.function && <span class="hc-pfn">{p.function}</span>}
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
      {here && !other && <ShowRow sel={card.sel} sections={['schematic', 'pcb', 'bom']} done={done} />}
      {other && <OtherBoardRow name={other} done={done} />}
    </>
  );
}

function NetBody({ card, done }: { card: NetCard; done: () => void }) {
  const d = detail.value;
  const span = [...card.modules, ...(card.onBoard ? ['the board'] : [])];
  return (
    <>
      <InspectorHead
        titleId="hc-title"
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
              <PinChip
                key={`${p.refdes}.${p.pin}`}
                title={part ? `${p.refdes} ${part.value}, pin ${p.pin}${p.name ? ` (${p.name})` : ''}${p.function ? `, as ${p.function}` : ''}. Click to pick the part.` : p.refdes}
                hv={part ? { kind: 'part', key: part.path } : undefined}
                onClick={() => part && setSelection(partSel(part.path))}
              >
                <b>{p.refdes}</b>
                <span>.{p.pin}</span>
                {p.name && <span class="hc-pname">{p.name}</span>}
                {p.function && <span class="hc-pfn">{p.function}</span>}
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

function ModuleBody({ card, done, other }: { card: ModuleCard; done: () => void; other: string | null }) {
  return (
    <>
      <InspectorHead
        titleId="hc-title"
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
      {other ? <OtherBoardRow name={other} done={done} /> : <ShowRow sel={card.sel} sections={['schematic', 'pcb', 'bom']} done={done} />}
    </>
  );
}

const DEF_ICON: Record<string, IconName> = { board: 'board', system: 'system', panel: 'panel', module: 'module', part: 'part', generic: 'part', interface: 'bus' };

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
        titleId="hc-title"
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

export function HoverCard({ target, path, close, typeAt }: { target: Target; path: string; close: () => void; typeAt?: number }) {
  // A declaration of a module that several boards use has a card for each board: the board shown, or the one picked here.
  const [pick, setPick] = useState<string | null>(null);
  const shownBoard = board.value;
  const owner = target.type === 'decl' ? target.owner : null;
  const boards = boardsUsing(usesOf(analysis.value), path, owner ? owner.line : null);
  const offer = offersBoards(boards, shownBoard);
  const elsewhere = pick && pick !== shownBoard ? detailOfBoard(pick) : undefined;
  const active = elsewhere ? pick : shownBoard;
  const other = elsewhere ? pick : null;
  // The signature and the doc comment of a definition are the server's (`/api/definition` for the name on the page's text).
  const text = openFiles.value[path]?.text;
  const served = typeAt !== undefined && text !== undefined ? servedDefinition(path, typeAt, text) : undefined;
  const ctx = contextFor(path, { served: served ?? null, ...(elsewhere ? { detail: elsewhere, board: pick, power: null } : {}) });
  const card = cardFor(ctx, target);
  useEffect(() => {
    // The data a card may want, asked for once: the rail of a net, the catalog line of a part, the answer for a definition (the text of a
    // file only when the server had nothing to say).
    if (card?.type === 'net') ensurePower();
    if (card?.type === 'part') ensureBom();
    if (card?.type === 'definition' && typeAt !== undefined && text !== undefined) ensureDefinition(path, typeAt, text);
    if (card?.type === 'definition' && card.source === 'none' && (card.partial || served === null)) ensureText(card.file);
  }, [card?.type, card?.type === 'definition' ? card.source : '', card?.type === 'definition' ? card.partial : false, served === null]);
  useEffect(() => {
    if (pick && pick !== shownBoard) ensureBoardDetail(pick);
  }, [pick]);
  const lit = card && !other ? litKey(card) : null;
  useEffect(() => {
    if (!lit) return;
    hoverEnter(lit);
    return () => hoverLeave(lit);
  }, [lit?.kind, lit?.key]);
  if (!card) return null;
  const kind = card.type === 'definition' ? 'definition' : card.type;
  return (
    <div class={`hc hc-${kind}`} role="dialog" aria-labelledby="hc-title" data-testid="hover-card" data-kind={kind} data-board={active ?? ''}>
      {offer && <BoardSwitch boards={boards} active={active} shown={shownBoard} onPick={(b) => setPick(b === shownBoard ? null : b)} />}
      {card.type === 'part' && <PartBody card={card} done={close} other={other} />}
      {card.type === 'net' && <NetBody card={card} done={close} />}
      {card.type === 'module' && <ModuleBody card={card} done={close} other={other} />}
      {card.type === 'definition' && <DefinitionBody card={card} done={close} />}
    </div>
  );
}
