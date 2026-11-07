// One row of the Compare section: what a part, net, rail, line of the bill of materials, module or
// problem was and is. A click picks it in every view, the chevron opens the fields that differ, and
// the two links open where it was declared on each side.

import { useState } from 'preact/hooks';

import type { CompareField, Location } from '../../api';
import { Icon, type IconName, SeverityIcon } from '../../components/Icons';
import { Pill, type Tone } from '../../components/ui';
import { STATUS_LABEL, type ChangeRow, type RowKind, type RowStatus } from '../../compare/model';
import { baseLocation, compare, diffable, openDiff } from '../../compare/store';
import { hoverAttrs } from '../../hover';
import { selectAndReveal, selection } from '../../selection';
import { board, chooseBoard, goTo, revealSection } from '../../store';

const KIND_ICON: Record<RowKind, IconName> = { part: 'part', net: 'net', rail: 'power', bom: 'bom', module: 'module', port: 'bus', problem: 'note', file: 'file', system: 'system', mate: 'link', pcb: 'pcb' };
const TONE: Record<RowStatus, Tone> = { added: 'ok', removed: 'bad', changed: 'warn', renumbered: 'neutral', renamed: 'info', moved: 'info', new: 'bad', resolved: 'ok' };

/** Pick what a row stands for in every view, in the board it is on. False when it stands for nothing on the head side. */
export function activate(row: ChangeRow): boolean {
  // Two refs are not what the views draw: a row of that comparison picks nothing in them.
  if (!row.select || compare.peek()?.against) return false;
  if (row.board && row.board !== board.peek()) chooseBoard(row.board);
  selectAndReveal(row.select);
  return true;
}

/** `src/main.nsm:42`: where a thing is declared. */
export const where = (at: Location) => `${at.file}:${at.line}`;

/** The fields whose name the chips say for themselves: a value, the pins, a name, the designators. The others are named. */
const SELF_EVIDENT = new Set(['value', 'pins', 'name', 'type', 'designator']);

function Chips({ row }: { row: ChangeRow }) {
  const { before, after, status, trend } = row;
  if (before === undefined && after === undefined) return null;
  const named = row.field && !SELF_EVIDENT.has(row.field) && !(row.field === 'quantity' && status !== 'changed');
  return (
    <span class="cr-chips" data-testid="chips">
      {named && <span class="cr-field-name">{row.field}</span>}
      {before !== undefined && <span class="vchip vold" title={`Before: ${before}`}>{before}</span>}
      {before !== undefined && after !== undefined && <Icon name="arrow-right" size={12} class="cr-arrow" />}
      {after !== undefined && <span class={`vchip vnew ${trend ? `vt-${trend}` : ''} ${status === 'added' ? 'vadd' : ''}`} title={`After: ${after}`}>{after}</span>}
    </span>
  );
}

function Designators({ row }: { row: ChangeRow }) {
  const { desBase, desHead } = row;
  if (!desBase && !desHead) return <span class="cr-des" />;
  if (desBase && desHead && desBase !== desHead) {
    return (
      <span class="cr-des" title={`${desBase} at the ref, ${desHead} on disk`}>
        <span class="faint">{desBase}</span>
        <Icon name="arrow-right" size={11} class="cr-arrow" />
        {desHead}
      </span>
    );
  }
  return <span class="cr-des mono">{desHead ?? desBase}</span>;
}

function FieldsTable({ fields }: { fields: CompareField[] }) {
  return (
    <dl class="cr-fields">
      {fields.map((f) => (
        <div class="cr-field" key={f.field}>
          <dt>{f.field}</dt>
          <dd>
            {f.old !== undefined && <span class="vchip vold">{f.old === '' ? 'none' : f.old}</span>}
            {f.old !== undefined && f.new !== undefined && <Icon name="arrow-right" size={12} class="cr-arrow" />}
            {f.new !== undefined && <span class="vchip vnew">{f.new === '' ? 'none' : f.new}</span>}
          </dd>
        </div>
      ))}
    </dl>
  );
}

export function RowView({ row, showBoard }: { row: ChangeRow; showBoard?: boolean }) {
  const [open, setOpen] = useState(false);
  const sel = selection.value;
  const picked = !!row.select && sel?.kind === row.select.kind && sel.key === row.select.key;
  const detail = row.fields.length > 1 || row.pins.length > 0 || (row.kind === 'problem' && row.title.length > 60) || (row.fields.length === 1 && row.fields[0].field !== row.field);
  const toggle = () => setOpen(!open);
  const baseAt = row.base;
  const headAt = row.head;
  // The ref's file is shown only where the server gives it: a declaration in the standard library has a Head and no Base.
  const baseTarget = baseLocation(baseAt);
  // Two refs: the head side's lines are in the second ref's file, so Head opens the comparison of the file and not the source.
  const refHead = !!compare.value?.against;
  const headTarget = refHead && headAt && diffable(headAt.file) ? ({ path: headAt.file, line: headAt.line, side: 'head', where: 'compare' } as const) : null;
  return (
    <div class={`cr cr-${row.status} ${picked ? 'sel' : ''} ${open ? 'open' : ''}`} data-testid="cmp-row" data-status={row.status} data-kind={row.kind} data-row={row.id} role="listitem">
      <div
        class="cr-main"
        tabIndex={0}
        {...hoverAttrs(row.select && !refHead ? { kind: row.select.kind, key: row.select.key } : null)}
        aria-selected={picked}
        onClick={() => {
          if (!activate(row)) toggle();
        }}
        onKeyDown={(e) => {
          if (e.target !== e.currentTarget) return;
          if (e.key === 'Enter') {
            e.preventDefault();
            if (!activate(row)) toggle();
          } else if (e.key === ' ') {
            e.preventDefault();
            toggle();
          } else if (e.key === 'ArrowRight' && detail && !open) setOpen(true);
          else if (e.key === 'ArrowLeft' && open) setOpen(false);
          else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            const rows = [...(e.currentTarget as HTMLElement).closest('.cmp-list')!.querySelectorAll<HTMLElement>('.cr-main')];
            rows[rows.indexOf(e.currentTarget as HTMLElement) + (e.key === 'ArrowDown' ? 1 : -1)]?.focus();
          }
        }}
      >
        <button
          type="button"
          class="cr-chev"
          tabIndex={-1}
          aria-expanded={open}
          aria-label={open ? 'Hide what differs' : 'Show what differs'}
          disabled={!detail}
          onClick={(e) => {
            e.stopPropagation();
            toggle();
          }}
        >
          <Icon name="chevron-right" size={12} class={`cr-chev-icon ${open ? 'open' : ''}`} />
        </button>
        <span class={`cr-kind k-${row.kind} s-${row.status}`} aria-hidden="true">
          {row.severity ? <SeverityIcon severity={row.severity} size={15} /> : <Icon name={KIND_ICON[row.kind]} size={15} />}
        </span>
        <span class="cr-name">
          <span class={`cr-title ${row.kind === 'problem' ? 'plain' : 'mono'} ${row.status === 'removed' ? 'gone' : ''}`} title={row.title}>
            {row.title}
          </span>
          {row.baseTitle && row.baseTitle !== row.title && <span class="cr-was" title={`At the ref: ${row.baseTitle}`}>was {row.baseTitle}</span>}
          {row.type && row.kind !== 'problem' && <span class="cr-type">{row.type}</span>}
          {row.kindChange && (
            <span class="cr-kindnote" data-testid="kind-changed">
              <Pill tone="info" title={`A different library part: ${row.kindChange.old} at the ref, ${row.kindChange.new} now`}>
                Kind changed
              </Pill>
            </span>
          )}
          {showBoard && row.board && <span class="cr-board">{row.board}</span>}
        </span>
        <Chips row={row} />
        <Designators row={row} />
        <Pill tone={TONE[row.status]} class="cr-badge">
          {STATUS_LABEL[row.status]}
        </Pill>
        <span class="cr-links">
          <button
            type="button"
            class="cr-link"
            data-testid="link-base"
            disabled={!baseTarget}
            data-tip={baseTarget ? `At the ref: ${where(baseAt!)}. Opens the file at the ref beside the file on disk.` : baseAt ? `At the ref: ${where(baseAt)}, in a file that is not the project's own` : 'Not at the ref'}
            aria-label={baseTarget ? `Base: ${where(baseAt!)}` : 'Base: not at the ref'}
            onClick={(e) => {
              e.stopPropagation();
              if (baseTarget) openDiff(baseTarget);
            }}
          >
            Base
          </button>
          <button
            type="button"
            class="cr-link"
            data-testid="link-head"
            disabled={refHead ? !headTarget : !headAt}
            data-tip={refHead ? (headTarget ? `At the second ref: ${where(headAt!)}. Opens the file at both refs.` : 'Not at the second ref') : headAt ? `On disk: ${where(headAt)}. Opens the source there.` : 'Not on disk'}
            aria-label={headAt ? `Head: ${where(headAt)}` : 'Head: not on disk'}
            onClick={(e) => {
              e.stopPropagation();
              if (refHead) {
                if (headTarget) openDiff(headTarget);
              } else if (headAt) void goTo(headAt);
            }}
          >
            Head
          </button>
          {row.select && !refHead && (
            <button
              type="button"
              class="cr-link icon"
              data-tip="Show in the Schematic"
              aria-label="Show in the Schematic"
              onClick={(e) => {
                e.stopPropagation();
                activate(row);
                revealSection('schematic');
              }}
            >
              <Icon name="schematic" size={13} />
            </button>
          )}
        </span>
      </div>
      {open && (
        <div class="cr-detail" data-testid="cmp-detail">
          {row.fields.length > 0 && <FieldsTable fields={row.fields} />}
          {row.pins.length > 0 && (
            <ul class="cr-pins" aria-label="Pins that joined or left">
              {row.pins.map((p) => (
                <li key={`${p.sign}${p.part}.${p.pin}`}>
                  <span class={`cr-sign ${p.sign === '+' ? 'add' : 'del'}`} aria-label={p.sign === '+' ? 'joined' : 'left'}>
                    {p.sign === '+' ? '+' : '−'}
                  </span>
                  <button type="button" class="link mono" disabled={!p.at} onClick={() => p.at && void goTo(p.at)} title={p.at ? `${p.part}: ${where(p.at)}` : p.part}>
                    {p.designator}.{p.pin}
                  </button>
                  <span class="faint mono">{p.part}</span>
                </li>
              ))}
            </ul>
          )}
          {row.kind === 'problem' && <div class="cr-msg">{row.title}</div>}
        </div>
      )}
    </div>
  );
}
