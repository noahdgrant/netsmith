// The problems dock: a panel under every view that lists what is wrong, wherever it was found,
// as a table. Opened with Ctrl+J (or Cmd+J) and by the counts in the status bar; F8 and Shift+F8
// step through it from anywhere. It remembers its height and whether it is open.

import type { ComponentChildren } from 'preact';
import { useEffect, useRef } from 'preact/hooks';

import type { Severity } from '../api';
import { allRows, counts, extras, listed, revealRow, viewScope } from '../feedback/derive';
import { type Group, ORIGIN_LABEL, type Row, filterRows, groupRows, whereText } from '../feedback/problems';
import {
  activeRow,
  dock,
  filter,
  group,
  resetDockHeight,
  setDockHeight,
  setDockOpen,
} from '../feedback/state';
import { SECTIONS } from '../layout';
import { plural } from '../util';

/** The most rows drawn at once; a project with more says so, and the filter narrows it. */
const MAX_ROWS = 1000;

const Svg = ({ children, size = 14 }: { children: ComponentChildren; size?: number }) => (
  <svg class="icon" width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {children}
  </svg>
);

export function SeverityIcon({ severity, size }: { severity: Severity; size?: number }) {
  if (severity === 'error') {
    return (
      <Svg size={size}>
        <circle cx="8" cy="8" r="6.2" />
        <path d="M5.6 5.6l4.8 4.8M10.4 5.6l-4.8 4.8" />
      </Svg>
    );
  }
  if (severity === 'warning') {
    return (
      <Svg size={size}>
        <path d="M8 2.2l6.2 11H1.8z" />
        <path d="M8 6.6v3.2M8 11.6v.1" />
      </Svg>
    );
  }
  return (
    <Svg size={size}>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M8 7.2v3.6M8 5v.1" />
    </Svg>
  );
}

export const CheckIcon = ({ size }: { size?: number }) => (
  <Svg size={size}>
    <circle cx="8" cy="8" r="6.2" />
    <path d="M5.2 8.2l2 2 3.6-4" />
  </Svg>
);

const SEVERITY_NAME: Record<Severity, string> = { error: 'Errors', warning: 'Warnings', note: 'Notes' };

/** A severity's toggle in the head: its icon and count; pressed, the rows of that severity show. */
function SeverityToggle({ severity, n, on, set }: { severity: Severity; n: number; on: boolean; set: (on: boolean) => void }) {
  return (
    <button
      class={`sev-toggle ${severity} ${on ? 'on' : ''}`}
      aria-pressed={on}
      title={`${on ? 'Hide' : 'Show'} ${SEVERITY_NAME[severity].toLowerCase()} (${plural(n, severity)})`}
      onClick={() => set(!on)}
    >
      <SeverityIcon severity={severity} size={13} />
      <span class="n">{n}</span>
      <span class="name">{SEVERITY_NAME[severity]}</span>
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
      <span class="origin" role="cell">
        {ORIGIN_LABEL[row.origin]}
      </span>
      <span class="where" role="cell">
        {where}
      </span>
      <span class="scope" role="cell" title={row.path ?? undefined}>
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
  const clear = () => (filter.value = { ...filter.value, text: '', thisView: false, errors: true, warnings: true });
  if (hidden.filtered === 'text') {
    return (
      <div class="dock-empty" data-testid="dock-empty">
        <div class="big">No problems match the filter</div>
        <div class="sub">
          <button class="link" onClick={clear}>
            Clear the filter
          </button>
        </div>
      </div>
    );
  }
  return (
    <div class="dock-empty ok" data-testid="dock-empty">
      <CheckIcon size={22} />
      <div class="big">{hidden.filtered === 'view' ? 'No problems in this view' : 'No problems'}</div>
      <div class="sub">
        {hidden.filtered === 'view' && (
          <>
            <button class="link" onClick={() => (filter.value = { ...filter.value, thisView: false })}>
              Show the whole project
            </button>{' '}
          </>
        )}
        {hidden.notes > 0 && (
          <>
            {plural(hidden.notes, 'note')} hidden.{' '}
            <button class="link" onClick={() => (filter.value = { ...filter.value, notes: true })}>
              Show notes
            </button>
          </>
        )}
      </div>
    </div>
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
      : f.text !== '' || !f.errors || !f.warnings
        ? 'text'
        : f.thisView && filterRows(allRows.value, { ...f, thisView: false }, view).length > 0
          ? 'view'
          : null;

  return (
    <section class="dock problems open" ref={host} style={{ height: `${d.height}px` }} aria-label="Problems" data-testid="dock">
      <Resizer host={host} />
      <div class="dock-head problems-head">
        <h2 class="dock-title">Problems</h2>
        <SeverityToggle severity="error" n={c.error} on={f.errors} set={(on) => set({ errors: on })} />
        <SeverityToggle severity="warning" n={c.warning} on={f.warnings} set={(on) => set({ warnings: on })} />
        <SeverityToggle severity="note" n={c.note} on={f.notes} set={(on) => set({ notes: on })} />
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
        <button class="icon-btn" aria-label="Close the problems panel (Ctrl+J)" title="Close (Ctrl+J)" onClick={() => setDockOpen(false)}>
          <Svg>
            <path d="M4 4l8 8M12 4l-8 8" />
          </Svg>
        </button>
      </div>
      {total === 0 ? (
        <Empty hidden={{ notes: f.notes ? 0 : noteCount, filtered }} />
      ) : (
        <div class="dock-body" role="table" aria-label="Problems" aria-rowcount={total}>
          <div class="dock-cols" role="row">
            <span role="columnheader" class="sev" aria-label="Severity" />
            <span role="columnheader">Message</span>
            <span role="columnheader">Source</span>
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
