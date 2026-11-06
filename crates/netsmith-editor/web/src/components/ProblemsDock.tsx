// The problems dock: a panel under every view that lists what is wrong, wherever it was found,
// as a table. Opened with Ctrl+J (or Cmd+J) and by the counts in the status bar; F8 and Shift+F8
// step through it from anywhere. It remembers its height and whether it is open.

import { useEffect, useRef } from 'preact/hooks';

import type { Severity } from '../api';
import { allRows, counts, extras, listed, revealRow, viewScope } from '../feedback/derive';
import { type Group, ORIGIN_LABEL, type Origin, type Row, filterRows, groupRows, stagesOf, whereText } from '../feedback/problems';
import {
  activeRow,
  dock,
  filter,
  group,
  resetDockHeight,
  setDockHeight,
  setDockOpen,
} from '../feedback/state';
import { type Hover, hoverAttrs } from '../hover';
import { SECTIONS } from '../layout';
import { partByPath } from '../selection';
import { detail } from '../store';
import { plural } from '../util';
import { CheckIcon, Icon, SeverityIcon } from './Icons';
import { EmptyState } from './ui';

/** The most rows drawn at once; a project with more says so, and the filter narrows it. */
const MAX_ROWS = 1000;

export { CheckIcon, SeverityIcon };

const SEVERITY_NAME: Record<Severity, string> = { error: 'Errors', warning: 'Warnings', note: 'Notes' };

/** A severity's toggle in the head: its icon and count; pressed, the rows of that severity show. */
function SeverityToggle({ severity, n, on, set }: { severity: Severity; n: number; on: boolean; set: (on: boolean) => void }) {
  return (
    <button
      class={`sev-toggle ${severity} ${on ? 'on' : ''}`}
      aria-pressed={on}
      data-tip={`${on ? 'Hide' : 'Show'} ${SEVERITY_NAME[severity].toLowerCase()} (${plural(n, severity)})`}
      onClick={() => set(!on)}
    >
      <SeverityIcon severity={severity} size={13} />
      <span class="n">{n}</span>
      <span class="name">{SEVERITY_NAME[severity]}</span>
    </button>
  );
}

/** A stage's chip: pressed, its rows show; the count is of all its rows. */
function StageChip({ origin, n, on, set }: { origin: Origin; n: number; on: boolean; set: (on: boolean) => void }) {
  return (
    <button
      class={`stage-chip stage-${origin} ${on ? 'on' : ''}`}
      aria-pressed={on}
      data-stage={origin}
      data-tip={`${on ? 'Hide' : 'Show'} what the ${ORIGIN_LABEL[origin].toLowerCase()} stage found (${plural(n, 'problem')})`}
      onClick={() => set(!on)}
    >
      {ORIGIN_LABEL[origin]} <span class="n">{n}</span>
    </button>
  );
}

function Resizer({ host }: { host: preact.RefObject<HTMLElement> }) {
  const drag = useRef(false);
  return (
    <div
      class="dock-resize"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize the problems panel. Double-click for the usual height."
      aria-valuenow={dock.value.height}
      tabIndex={0}
      title="Drag to resize; double-click for the usual height"
      onPointerDown={(e) => {
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        drag.current = true;
      }}
      onPointerMove={(e) => {
        if (!drag.current || !host.current) return;
        setDockHeight(host.current.getBoundingClientRect().bottom - e.clientY);
      }}
      onPointerUp={(e) => {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
        drag.current = false;
      }}
      onDblClick={resetDockHeight}
      onKeyDown={(e) => {
        if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
          e.preventDefault();
          setDockHeight(dock.value.height + (e.key === 'ArrowUp' ? 24 : -24));
        }
      }}
    />
  );
}

function ProblemRow({ row }: { row: Row }) {
  const on = activeRow.value === row.id;
  const where = whereText(row);
  return (
    <div
      class={`problem ${row.severity} ${on ? 'active' : ''}`}
      role="row"
      tabIndex={0}
      aria-current={on ? 'true' : undefined}
      data-id={row.id}
      data-testid="problem"
      {...hoverAttrs((): Hover | null =>
        row.net ? { kind: 'net', key: row.net } : row.path ? { kind: partByPath(detail.peek(), row.path) ? 'part' : 'module', key: row.path } : null,
      )}
      title={`${row.full}${row.at ? `\n${row.at.file}:${row.at.line}:${row.at.column}` : ''}`}
      onClick={() => revealRow(row)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          revealRow(row);
        }
      }}
    >
      <span class="sev" role="cell" aria-label={row.severity}>
        <SeverityIcon severity={row.severity} />
      </span>
      <span class="msg" role="cell">
        {row.message}
      </span>
      <span class={`origin stage-${row.origin}`} role="cell">
        {ORIGIN_LABEL[row.origin]}
      </span>
      <span class="where" role="cell">
        {where}
      </span>
      <span class="scope" role="cell" title={[row.board && `Board ${row.board}`, row.path && `Module ${row.path}`].filter(Boolean).join('\n') || undefined}>
        {row.path ?? row.board ?? ''}
      </span>
    </div>
  );
}

const GROUPS: { id: Group; label: string }[] = [
  { id: 'none', label: 'No grouping' },
  { id: 'file', label: 'Group by file' },
  { id: 'severity', label: 'Group by severity' },
];

function Empty({ hidden }: { hidden: { notes: number; filtered: 'text' | 'view' | null } }) {
  const clear = () => (filter.value = { ...filter.value, text: '', thisView: false, errors: true, warnings: true, off: [] });
  if (hidden.filtered === 'text') {
    return (
      <EmptyState
        compact
        class="dock-empty"
        testid="dock-empty"
        icon="filter"
        title="No problems match the filter"
        actions={
          <button class="tool small" onClick={clear}>
            Clear the filter
          </button>
        }
      />
    );
  }
  return (
    <EmptyState
      compact
      class="dock-empty ok"
      testid="dock-empty"
      icon="check-circle"
      tone="ok"
      title={hidden.filtered === 'view' ? 'No problems in this view' : 'No problems'}
      help={hidden.notes > 0 ? `${plural(hidden.notes, 'note')} hidden.` : undefined}
      actions={
        hidden.filtered === 'view' || hidden.notes > 0 ? (
          <>
            {hidden.filtered === 'view' && (
              <button class="tool small" onClick={() => (filter.value = { ...filter.value, thisView: false })}>
                Show the whole project
              </button>
            )}
            {hidden.notes > 0 && (
              <button class="tool small" onClick={() => (filter.value = { ...filter.value, notes: true })}>
                Show notes
              </button>
            )}
          </>
        ) : undefined
      }
    />
  );
}

export function ProblemsDock() {
  const host = useRef<HTMLElement>(null);
  const d = dock.value;
  const f = filter.value;
  const c = counts.value;
  const rows = listed.value;
  const active = activeRow.value;
  const view = viewScope.value;

  // The row F8 lands on is brought into view.
  useEffect(() => {
    if (!active) return;
    host.current?.querySelector(`[data-id="${CSS.escape(active)}"]`)?.scrollIntoView({ block: 'nearest' });
  }, [active, d.open]);

  if (!d.open) return null;
  const shownGroups = groupRows(rows, group.value);
  const total = rows.length;
  let budget = MAX_ROWS;
  const set = (patch: Partial<typeof f>) => (filter.value = { ...f, ...patch });
  const noteCount = allRows.value.filter((r) => r.severity === 'note').length;
  // Why the list is empty: what the person asked to leave out, or nothing is wrong.
  const filtered: 'text' | 'view' | null =
    allRows.value.length === 0 || total > 0
      ? null
      : f.text !== '' || !f.errors || !f.warnings || f.off.length > 0
        ? 'text'
        : f.thisView && filterRows(allRows.value, { ...f, thisView: false }, view).length > 0
          ? 'view'
          : null;

  return (
    <section class="dock problems open" ref={host} style={{ height: `${d.height}px` }} aria-label="Problems" data-testid="dock" data-toast-clear>
      <Resizer host={host} />
      <div class="dock-head problems-head">
        <h2 class="dock-title"><Icon name="panel" size={14} />Problems</h2>
        <SeverityToggle severity="error" n={c.error} on={f.errors} set={(on) => set({ errors: on })} />
        <SeverityToggle severity="warning" n={c.warning} on={f.warnings} set={(on) => set({ warnings: on })} />
        <SeverityToggle severity="note" n={c.note} on={f.notes} set={(on) => set({ notes: on })} />
        <div class="stage-chips" role="group" aria-label="Stages">
          {stagesOf(allRows.value).map((s) => (
            <StageChip
              key={s.origin}
              origin={s.origin}
              n={s.n}
              on={!f.off.includes(s.origin)}
              set={(on) => set({ off: on ? f.off.filter((o) => o !== s.origin) : [...f.off, s.origin] })}
            />
          ))}
        </div>
        <label class="dock-search">
          <input
            type="search"
            placeholder="Filter: text, file, stage"
            aria-label="Filter the problems by text"
            value={f.text}
            onInput={(e) => set({ text: (e.currentTarget as HTMLInputElement).value })}
            onKeyDown={(e) => e.key === 'Escape' && f.text && (e.stopPropagation(), set({ text: '' }))}
          />
        </label>
        <label class="dock-check" title={`Only what the ${SECTIONS.find((x) => x.id === view.section)?.label ?? ''} section shows`}>
          <input type="checkbox" checked={f.thisView} onChange={(e) => set({ thisView: (e.currentTarget as HTMLInputElement).checked })} />
          This view only
        </label>
        <select
          class="dock-group"
          aria-label="Grouping"
          value={group.value}
          onChange={(e) => (group.value = (e.currentTarget as HTMLSelectElement).value as Group)}
        >
          {GROUPS.map((g) => (
            <option key={g.id} value={g.id}>
              {g.label}
            </option>
          ))}
        </select>
        <span class="spacer" />
        {extras.value === 'loading' && (
          <span class="dock-note" title="The PCB's and the power tree's findings are being read from the server">
            <span class="spinner" /> Checking PCB and power…
          </span>
        )}
        <button class="icon-btn" aria-label="Close the problems panel (Ctrl+J)" data-tip="Close" data-tip-keys="Ctrl+J" onClick={() => setDockOpen(false)}>
          <Icon name="close" />
        </button>
      </div>
      {total === 0 ? (
        <Empty hidden={{ notes: f.notes ? 0 : noteCount, filtered }} />
      ) : (
        <div class="dock-body" role="table" aria-label="Problems" aria-rowcount={total}>
          <div class="dock-cols" role="row">
            <span role="columnheader" class="sev" aria-label="Severity" />
            <span role="columnheader">Message</span>
            <span role="columnheader">Stage</span>
            <span role="columnheader">Location</span>
            <span role="columnheader">Module</span>
          </div>
          <div
            class="dock-rows"
            role="presentation"
            onKeyDown={(e) => {
              if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
              const all = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('.problem')];
              const at = all.indexOf(document.activeElement as HTMLElement);
              const next = all[Math.min(all.length - 1, Math.max(0, at + (e.key === 'ArrowDown' ? 1 : -1)))];
              if (next) {
                e.preventDefault();
                next.focus();
              }
            }}
          >
            {shownGroups.map((g) => {
              const slice = g.rows.slice(0, Math.max(0, budget));
              budget -= slice.length;
              return (
                <div key={g.key} class="dock-group-block" role="rowgroup">
                  {g.title && (
                    <div class="dock-group-head" role="row">
                      <span role="columnheader">{g.title}</span>
                      <span class="count">{g.rows.length}</span>
                    </div>
                  )}
                  {slice.map((r) => (
                    <ProblemRow key={r.id} row={r} />
                  ))}
                </div>
              );
            })}
            {total > MAX_ROWS && (
              <div class="dock-more">
                {total - MAX_ROWS} more not drawn. Filter to narrow the list.
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
