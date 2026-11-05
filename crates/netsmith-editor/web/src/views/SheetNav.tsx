// Finding your way among the sheets: the outline on the left, the compact picker in the toolbar,
// and the toolbar itself.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { ALL, canStep, stepPick } from '../sheets';
import type { Sheet } from '../schematic/drawing';
import { filterSheets, parseTitle, type SheetStats } from '../schematic/nav';
import { plural } from '../util';

/** A sheet as the lists show it. */
export interface SheetRow {
  sheet: Sheet;
  /** Its place in the board's source order, from 1. */
  n: number;
  name: string;
  detail: string;
  stats: SheetStats | undefined;
  /** Whether the net that is selected is drawn on it. */
  carries: boolean;
  parts: string[];
}

export const rowOf = (sheet: Sheet, n: number, stats: SheetStats | undefined, carries: boolean, parts: string[]): SheetRow => ({
  sheet,
  n,
  ...parseTitle(sheet.title),
  stats,
  carries,
  parts,
});

/** `1 net leaves this module`, `3 nets leave this module`. */
function leaveText(n: number): string {
  return `${plural(n, 'net')} ${n === 1 ? 'leaves' : 'leave'} this module`;
}

function Badges({ stats }: { stats: SheetStats | undefined }) {
  if (!stats) return null;
  const bad = stats.errors + stats.warnings;
  return (
    <span class="sl-badges">
      {stats.leaving > 0 && (
        <span class="sl-badge leave" title={leaveText(stats.leaving)} aria-label={leaveText(stats.leaving)}>
          ⇄ {stats.leaving}
        </span>
      )}
      {bad > 0 && (
        <span
          class={`sl-badge ${stats.errors > 0 ? 'bad' : 'warn'}`}
          title={`${plural(stats.errors, 'error')}, ${plural(stats.warnings, 'warning')}`}
          aria-label={`${plural(stats.errors, 'error')}, ${plural(stats.warnings, 'warning')}`}
        >
          ! {bad}
        </span>
      )}
    </span>
  );
}

/** The sheets as a list with a filter box: the overview first, then each sheet in source order. */
export function SheetList({
  rows,
  current,
  onOpen,
  autoFocus,
  total,
  onDone,
}: {
  rows: SheetRow[];
  current: string;
  onOpen: (pick: string) => void;
  autoFocus?: boolean;
  /** Told when a choice was made (a picker closes). */
  onDone?: () => void;
  total: { parts: number };
}) {
  const [query, setQuery] = useState('');
  const box = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (autoFocus) box.current?.focus();
  }, [autoFocus]);
  const shown = useMemo(() => {
    const byName = new Map(rows.map((r) => [r.sheet.name, r]));
    return filterSheets(
      rows.map((r) => r.sheet),
      query,
      (s) => byName.get(s)?.parts ?? [],
    ).map((s) => byName.get(s.name)!);
  }, [rows, query]);
  // The open sheet is brought into view.
  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [current, shown.length]);
  const choose = (pick: string) => {
    onOpen(pick);
    onDone?.();
  };
  const move = (dir: 1 | -1 | 'first' | 'last') => {
    const items = [...(list.current?.querySelectorAll<HTMLButtonElement>('button[role="option"]') ?? [])];
    if (items.length === 0) return;
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = dir === 'first' ? 0 : dir === 'last' ? items.length - 1 : Math.min(Math.max(at + dir, 0), items.length - 1);
    items[next].focus();
  };
  return (
    <div class="sheetlist">
      {rows.length > 5 && (
        <input
          ref={box}
          class="sl-filter"
          type="search"
          placeholder="Filter modules or designators…"
          aria-label="Filter modules"
          value={query}
          onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape' && query) {
              e.stopPropagation();
              setQuery('');
            } else if (e.key === 'ArrowDown') {
              e.preventDefault();
              move('first');
            } else if (e.key === 'Enter' && shown.length > 0) {
              choose(shown[0].sheet.name);
            }
          }}
        />
      )}
      <div
        class="sl-items"
        role="listbox"
        aria-label="Modules"
        ref={list}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown') (e.preventDefault(), move(1));
          else if (e.key === 'ArrowUp') (e.preventDefault(), move(-1));
          else if (e.key === 'Home') (e.preventDefault(), move('first'));
          else if (e.key === 'End') (e.preventDefault(), move('last'));
        }}
      >
        {!query && (
          <button
            role="option"
            aria-selected={current === ALL}
            class={`sl-row all ${current === ALL ? 'active' : ''}`}
            title="Every module at once; click one to open it"
            onClick={() => choose(ALL)}
          >
            <span class="sl-n">▦</span>
            <span class="sl-main">
              <span class="sl-name">All modules</span>
              <span class="sl-mod">
                {plural(rows.length, 'module')}, {plural(total.parts, 'part')}
              </span>
            </span>
          </button>
        )}
        {shown.map((r) => (
          <button
            key={r.sheet.name}
            role="option"
            aria-selected={current === r.sheet.name}
            class={`sl-row ${current === r.sheet.name ? 'active' : ''} ${r.carries ? 'carries' : ''}`}
            title={`${r.sheet.title}${r.carries ? ' (carries the selected net)' : ''}`}
            onClick={() => choose(r.sheet.name)}
          >
            <span class="sl-n">{r.n}</span>
            <span class="sl-main">
              <span class="sl-name">{r.name}</span>
              <span class="sl-mod">
                {r.detail}
                {r.detail && r.stats ? ' · ' : ''}
                {r.stats ? plural(r.stats.parts, 'part') : ''}
              </span>
            </span>
            <Badges stats={r.stats} />
          </button>
        ))}
        {shown.length === 0 && <div class="sl-none">No module matches “{query}”.</div>}
      </div>
    </div>
  );
}

/** The collapsible outline on the left. */
export function SheetNavPanel(props: Parameters<typeof SheetList>[0] & { onCollapse: () => void }) {
  return (
    <nav class="sheetnav no-pan" aria-label="Module navigator">
      <div class="sn-head">
        <h3>
          Modules <span class="count">{props.rows.length}</span>
        </h3>
        <button class="icon-btn" title="Hide the module navigator" aria-label="Hide the module navigator" onClick={props.onCollapse}>
          ‹
        </button>
      </div>
      <SheetList {...props} />
    </nav>
  );
}

/** The toolbar: the navigator's toggle, Previous and Next, the open sheet as a picker, and the details toggle. */
export function SheetBar({
  rows,
  names,
  current,
  boardName,
  onOpen,
  navOpen,
  toggleNav,
  side,
  toggleSide,
  total,
  help,
}: {
  help: () => void;
  rows: SheetRow[];
  names: string[];
  current: string;
  boardName: string;
  onOpen: (pick: string) => void;
  navOpen: boolean;
  toggleNav: () => void;
  side: boolean;
  toggleSide: () => void;
  total: { parts: number };
}) {
  const can = canStep(names, current);
  const multi = rows.length > 1;
  const row = rows.find((r) => r.sheet.name === current);
  const place = current === ALL ? plural(rows.length, 'module') : row ? `${row.n} of ${rows.length}` : '';
  return (
    <div class="sheetbar" role="toolbar" aria-label="Modules">
      {multi && (
        <button
          class={`icon-btn ${navOpen ? 'on' : ''}`}
          aria-pressed={navOpen}
          title="Show or hide the module navigator"
          aria-label="Module navigator"
          onClick={toggleNav}
        >
          ☰
        </button>
      )}
      {multi && (
        <>
          <button class="step" disabled={!can.prev} title="Previous module ([ or Page Up)" aria-label="Previous module" onClick={() => onOpen(stepPick(names, current, -1))}>
            ‹
          </button>
          <button class="step" disabled={!can.next} title="Next module (] or Page Down)" aria-label="Next module" onClick={() => onOpen(stepPick(names, current, 1))}>
            ›
          </button>
          <SheetPicker rows={rows} current={current} onOpen={onOpen} boardName={boardName} total={total} />
          <span class="sheet-place" aria-live="polite">
            {place}
          </span>
        </>
      )}
      {!multi && <span class="crumb">{boardName}</span>}
      <span class="spacer" />
      <button class="icon-btn" title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts" onClick={help}>
        ?
      </button>
      <button class={`tool small ${side ? 'on' : ''}`} aria-pressed={side} title="Show or hide the panel with the selection, the module and the parts" onClick={toggleSide}>
        Details
      </button>
    </div>
  );
}

/** The open sheet as a button that drops a list with a filter: the breadcrumb `Board › sheet`. */
function SheetPicker({
  rows,
  current,
  onOpen,
  boardName,
  total,
}: {
  rows: SheetRow[];
  current: string;
  onOpen: (pick: string) => void;
  boardName: string;
  total: { parts: number };
}) {
  const [open, setOpen] = useState(false);
  const holder = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!holder.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, [open]);
  const row = rows.find((r) => r.sheet.name === current);
  return (
    <div
      class="sheetpick"
      ref={holder}
      onKeyDown={(e) => {
        if (e.key === 'Escape' && open) {
          e.stopPropagation();
          setOpen(false);
          holder.current?.querySelector('button')?.focus();
        }
      }}
    >
      <button class="pick-btn" aria-haspopup="listbox" aria-expanded={open} title="Go to a module" onClick={() => setOpen(!open)}>
        <span class="crumb-board">{boardName}</span>
        <span class="crumb-sep">›</span>
        <span class="crumb-sheet">{current === ALL ? 'All modules' : (row?.name ?? current)}</span>
        {row?.detail && <span class="crumb-mod">{row.detail}</span>}
        <span class="caret">▾</span>
      </button>
      {open && (
        <div class="pick-pop no-pan">
          <SheetList rows={rows} current={current} onOpen={onOpen} onDone={() => setOpen(false)} autoFocus total={total} />
        </div>
      )}
    </div>
  );
}
