import { fileTabMenu } from './FileTabMenu';
import { signal } from '@preact/signals';
import type { ComponentChildren } from 'preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';

import {
  type Drop,
  SECTIONS,
  type Section,
  canSplit,
  closePane,
  dropAt,
  dropTab,
  splitRight,
  splitWith,
} from '../layout';

import {
  activeFile,
  back,
  layout,
  requestBack,
  showSection,
  visible,
  windowWidth,
  board,
  chooseBoard,
  closeFile,
  dirty,
  moveTab,
  openFiles,
  project,
  job,
  reload,
  tabs,
  analysis,
  paneNotice,
  dismissPaneNotice,
} from '../store';
import { chordLabels, chordText, find, runCommand } from '../commands';
import { openPicker } from '../quickpick';
import { seconds } from '../util';
import { useElapsed } from './JobProgress';
import { Icon, SeverityIcon } from './Icons';
import { Popover, Spinner } from './ui';
import { firstError, goToFirstError, neverBuilt } from '../feedback/derive';
import { Logo } from './Logo';
import { dropBefore } from '../source/tabs';
import { fileIcon } from '../tree';
import { FileTypeIcon, LockIcon } from './TreeIcons';
import { SelectionChip } from './SelectionChip';

/** What `section` is for, in its tab's tooltip. */
const SECTION_HINT: Record<Section, string> = {
  schematic: 'the drawing of the board, one module at a time',
  power: 'the power tree: sources, converters and loads',
  system: 'the boards of a system and how they are joined',
  bom: 'the bill of materials',
  pcb: 'the board layout',
  build: 'what Write KiCad files and Make fab files did',
  compare: 'what changed in the circuit since a git ref',
  source: 'the code, with the project\'s files',
};

/** What the two-pane mark on a tab says: the section is open in the other pane. */
const OTHER_PANE = 'Open in the other pane: click to move it here';

export const SECTION_LABEL = Object.fromEntries(SECTIONS.map((s) => [s.id, s.label])) as Record<Section, string>;

/** Two panes, the one that holds the section filled: it marks a tab whose section is in the other pane. */
const OtherPaneMark = ({ side }: { side: 'left' | 'right' }) => (
  <svg class="icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" aria-hidden="true">
    <rect x="1.8" y="2.6" width="12.4" height="10.8" rx="1.2" />
    <path d="M8 2.6v10.8" />
    {side === 'right' ? <rect x="8" y="2.6" width="6.2" height="10.8" fill="currentColor" stroke="none" /> : <rect x="1.8" y="2.6" width="6.2" height="10.8" fill="currentColor" stroke="none" />}
  </svg>
);

/** The two actions: they write files, and say so while they run. They are not the sections. */
function Action({
  task,
  label,
  running,
  icon,
  tip,
  primary,
  start,
}: {
  task: 'build' | 'fab';
  label: string;
  running: string;
  icon: ComponentChildren;
  tip: string;
  primary?: boolean;
  start: () => void;
}) {
  const j = job.value;
  const mine = j?.task === task;
  const elapsed = useElapsed(mine ? (j?.started ?? null) : null);
  return (
    <button
      class={`tool action ${primary ? 'primary' : ''} ${mine ? 'running' : ''}`}
      data-tip={tip}
      aria-label={label}
      aria-busy={mine}
      disabled={j !== null}
      onClick={start}
    >
      {mine ? <span class="spinner" /> : icon}
      <span>{mine ? `${running} (${seconds(elapsed)})` : label}</span>
    </button>
  );
}

/** The board picker when there is no board: still loading, never built (with the first error), or none written. */
function EmptyBoardPick() {
  const loading = !analysis.value;
  const e = firstError.value;
  const never = neverBuilt.value && !!e;
  return (
    <Popover
      fixed
      class={`boardpick-empty ${never ? 'never' : ''}`}
      tip={loading ? 'Reading the project' : 'Why there is no board'}
      label={loading ? 'Reading the project' : 'No board built yet'}
      icon={loading ? undefined : never ? 'error' : 'board'}
      badge={loading ? <Spinner size={12} /> : undefined}
    >
      {(close) =>
        loading ? (
          <p>The first analysis is still running. The boards of the project are listed here when it is done.</p>
        ) : never ? (
          <>
            <h4>The project has an error, so nothing built</h4>
            <div class="empty-error">
              <SeverityIcon severity="error" size={14} />
              <span class="text">{e!.message}</span>
              {e!.at && <span class="where">{e!.at.file}:{e!.at.line}</span>}
            </div>
            <div class="empty-actions">
              <button
                class="tool primary"
                onClick={() => {
                  close();
                  goToFirstError();
                }}
              >
                Go to error
              </button>
            </div>
          </>
        ) : (
          <>
            <h4>The project has no board</h4>
            <p>Write a <code>board</code> in a top-level file; it is listed here as soon as it builds.</p>
          </>
        )
      }
    </Popover>
  );
}

export function TopBar() {
  const a = analysis.value;
  const boards = a?.boards ?? [];
  const goto = find('goto.anything')?.keys ?? 'Mod+P';
  return (
    <header class="topbar">
      <div class="tb-left">
        <div class="brand"><Logo /></div>
        {boards.length === 0 ? (
          <EmptyBoardPick />
        ) : (
          <label class="boardpick" data-tip="The board the sections show">
            <Icon name="board" size={14} class="bp-icon" />
            <span class="sr-only">Board</span>
            <select
              value={board.value ?? ''}
              onChange={(e) => chooseBoard((e.currentTarget as HTMLSelectElement).value)}
            >
              {boards.map((b) => (
                <option key={b.name} value={b.name}>
                  {b.name}
                  {b.stale ? ' (stale)' : ''}
                </option>
              ))}
            </select>
            <Icon name="chevrons-up-down" size={12} class="bp-caret" />
          </label>
        )}
        <SelectionChip />
      </div>
      <button
        class="cmdk"
        data-tip={`Search parts, nets, modules and files (${chordText(goto)}); run a command (${chordText(find('palette.open')?.keys ?? 'Mod+Shift+P')})`}
        aria-label="Go to anything or run a command"
        aria-haspopup="dialog"
        onClick={() => openPicker()}
      >
        <Icon name="search" size={15} />
        <span class="cmdk-text">Go to anything</span>
        <span class="qp-keys">
          {chordLabels(goto).map((k) => (
            <kbd key={k}>{k}</kbd>
          ))}
        </span>
      </button>
      <div class="tb-right">
        <div class="tb-ghosts" role="group" aria-label="Appearance and help">
          <button
            class="icon-btn"
            data-tip={`Switch between the light and the dark theme (${chordText(find('theme.toggle')?.keys ?? '')})`.replace(' ()', '')}
            aria-label="Toggle the light and dark theme"
            onClick={() => runCommand('theme.toggle')}
          >
            <Icon name="theme" />
          </button>
          <button
            class="icon-btn"
            data-tip={`Keyboard shortcuts (${chordText(find('help.shortcuts')?.keys ?? '?')})`}
            aria-label="Keyboard shortcuts"
            aria-haspopup="dialog"
            onClick={() => runCommand('help.shortcuts')}
          >
            <Icon name="keyboard" />
          </button>
        </div>
        <div class="actions" role="group" aria-label="Actions: they write files">
          <Action
            task="build"
            label="Write KiCad files"
            running="Writing KiCad files…"
            icon={<Icon name="kicad" />}
            tip={
              "Writes a KiCad schematic (.kicad_sch) and project file (.kicad_pro) for every board of the project, " +
              "with the library tables, into the project's out folder (out/ unless netsmith.toml says another), " +
              "one folder for each board. The same as `netsmith build`. Takes a few seconds."
            }
            start={() => runCommand('build.kicad')}
          />
          <Action
            task="fab"
            label="Make fab files"
            running="Making fab files…"
            icon={<Icon name="fab" />}
            primary
            tip={
              "Places and routes the board chosen here, runs netsmith's design rules check and, when it passes, " +
              "writes the Gerbers, bom.csv, cpl.csv and fab-notes.txt for JLCPCB into the project's fab folder " +
              "(fab/ unless netsmith.toml says another). The same as `netsmith fab`. A long job: it can take minutes."
            }
            start={() => runCommand('build.fab')}
          />
        </div>
      </div>
    </header>
  );
}

/** A tab being dragged, and the drop it would make. */
export const dragging = signal<{ section: Section; x: number; y: number; drop: Drop | null } | null>(null);
/** The menu of a tab, opened by a right click or the menu key. */
export const tabMenu = signal<{ section: Section; x: number; y: number } | null>(null);

let clickSuppressed = false;

function dropFor(x: number, y: number): Drop | null {
  const area = document.querySelector('.panes-area');
  if (!area) return null;
  const r = area.getBoundingClientRect();
  if (y < r.top || y > r.bottom || x < r.left || x > r.right) return null;
  // The area is set in by the frame's gap; the panes share what is inside it.
  const c = getComputedStyle(area);
  const pad = parseFloat(c.paddingLeft) || 0;
  return dropAt(layout.peek(), windowWidth.peek(), x, r.left + pad, r.width - pad - (parseFloat(c.paddingRight) || 0));
}

function startDrag(e: PointerEvent, section: Section) {
  if (e.button !== 0) return;
  const x0 = e.clientX;
  const y0 = e.clientY;
  let active = false;
  const end = () => {
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', up);
    window.removeEventListener('keydown', key, true);
    dragging.value = null;
  };
  const move = (ev: PointerEvent) => {
    if (!active && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 8) return;
    active = true;
    dragging.value = { section, x: ev.clientX, y: ev.clientY, drop: dropFor(ev.clientX, ev.clientY) };
  };
  const up = (ev: PointerEvent) => {
    const drop = active ? dropFor(ev.clientX, ev.clientY) : null;
    end();
    if (!active) return;
    // The click that ends a drag is not a click on the tab.
    clickSuppressed = true;
    setTimeout(() => (clickSuppressed = false), 0);
    if (drop) {
      back.value = null;
      layout.value = dropTab(layout.peek(), section, drop);
    }
  };
  const key = (ev: KeyboardEvent) => {
    if (ev.key === 'Escape') {
      ev.stopPropagation();
      clickSuppressed = true;
      setTimeout(() => (clickSuppressed = false), 0);
      end();
    }
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', up);
  window.addEventListener('keydown', key, true);
}

/** The tooltip of the split control: what it does, and, when the window is too narrow, why not. */
function splitTip() {
  const w = windowWidth.value;
  const wide = canSplit(w);
  const saved = layout.value.panes.length === 2;
  return (
    `Split right: show a second section beside this one (${chordText(find('pane.toggle')?.keys ?? 'Mod+\\')}). You can also drag a tab to the left or right half of the window.` +
    (wide
      ? ''
      : ` This window is ${w} px wide; a split needs about 1400 px, so one pane is shown${
          saved ? ' and your saved split comes back when the window is wide enough' : ''
        }.`)
  );
}

/** The sections of one pane: its tab strip. Tabs are dragged to split the window or to move a
 * section to the other pane, and a right click gives the same without dragging. */
export function SectionStrip({ pane }: { pane: { section: Section; index: number } }) {
  const vis = visible.value;
  const two = vis.length === 2;
  const other = vis.find((p) => p.index !== pane.index)?.section;
  const wide = canSplit(windowWidth.value);
  // The active tab's line glides: its place and width are measured and given to the strip as
  // variables, and the line's transition does the rest (it is off until the first measure).
  const strip = useRef<HTMLDivElement>(null);
  const place = () => {
    const el = strip.current;
    const t = el?.querySelector<HTMLElement>('.tab.active');
    if (!el || !t) return;
    el.style.setProperty('--ind-x', `${t.offsetLeft}px`);
    el.style.setProperty('--ind-w', `${t.offsetWidth}px`);
    if (!el.classList.contains('placed')) requestAnimationFrame(() => el.classList.add('placed'));
  };
  useLayoutEffect(place, [pane.section, two, other]);
  useEffect(() => {
    const el = strip.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(place);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return (
    <div
      class="tabbar sections"
      ref={strip}
      role="tablist"
      aria-label="Sections"
      onKeyDown={(e) => {
        // Arrow keys move between the tabs and show the section; Home and End go to the first and the last.
        const at = SECTIONS.findIndex((x) => x.id === pane.section);
        const to = e.key === 'ArrowRight' ? at + 1 : e.key === 'ArrowLeft' ? at - 1 : e.key === 'Home' ? 0 : e.key === 'End' ? SECTIONS.length - 1 : null;
        if (to === null || e.ctrlKey || e.altKey || e.metaKey) return;
        e.preventDefault();
        const next = SECTIONS[(to + SECTIONS.length) % SECTIONS.length];
        showSection(next.id, pane.index);
        queueMicrotask(() => e.currentTarget.querySelector<HTMLElement>(`[data-section="${next.id}"]`)?.focus());
      }}
    >
      {SECTIONS.map((v, i) => {
        const here = pane.section === v.id;
        const elsewhere = other === v.id;
        return (
          <button
            key={v.id}
            role="tab"
            aria-selected={here}
            tabIndex={here ? 0 : -1}
            data-section={v.id}
            class={`tab ${here ? 'active' : ''} ${elsewhere ? 'elsewhere' : ''}`}
            title={
              `${v.label}: ${SECTION_HINT[v.id]}. Key ${i + 1} (or Alt+${i + 1}). ` +
              (elsewhere ? `${OTHER_PANE} ` : '') +
              'Drag it to the side of the window to split, or right-click for the same.'
            }
            onPointerDown={(e) => startDrag(e, v.id)}
            onClick={() => {
              if (clickSuppressed) return;
              showSection(v.id, pane.index);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              tabMenu.value = { section: v.id, x: e.clientX, y: e.clientY };
            }}
          >
            <Icon name={v.id} size={15} class="tab-icon" />
            <span class="tab-label">{v.label}</span>
            {elsewhere && (
              <span class="elsewhere-mark" role="img" aria-label={OTHER_PANE} title={OTHER_PANE}>
                <OtherPaneMark side={pane.index === 0 ? 'right' : 'left'} />
              </span>
            )}
          </button>
        );
      })}
      <span class="tab-ind" aria-hidden="true" />
      <span class="spacer" />
      {two && pane.index === 1 ? (
        <button
          class="pane-btn"
          data-tip="Close this pane: the other one fills the window"
          onClick={() => {
            back.value = null;
            layout.value = closePane(layout.value, 1);
          }}
        >
          <Icon name="close" size={14} />
          <span>Close pane</span>
        </button>
      ) : !two ? (
        <button
          class="pane-btn"
          aria-disabled={!wide}
          title={splitTip()}
          onClick={() => {
            if (wide) layout.value = splitRight(layout.value);
          }}
        >
          <Icon name="split" size={14} />
          <span>Split right</span>
        </button>
      ) : null}
    </div>
  );
}

/** Said once, under the strip of the second pane, when the editor opened something in it. */
export function PaneNotice({ pane }: { pane: number }) {
  if (pane !== 1 || visible.value.length !== 2 || !paneNotice.value) return null;
  return (
    <div class="pane-notice" role="status">
      <span>Opened in a second pane. Close pane returns to one.</span>
      <button
        class="tool small"
        onClick={() => {
          dismissPaneNotice();
          back.value = null;
          layout.value = closePane(layout.value, 1);
        }}
      >
        Close pane
      </button>
      <button class="tool small" onClick={dismissPaneNotice}>
        Got it
      </button>
    </div>
  );
}

/** The right-click menu of a section tab. */
export function TabMenu() {
  const m = tabMenu.value;
  const menu = useRef<HTMLDivElement>(null);
  // The first choice takes the focus, so the keyboard can use the menu.
  useEffect(() => {
    menu.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, [m?.section, m?.x, m?.y]);
  if (!m) return null;
  const l = layout.value;
  const wide = canSplit(windowWidth.value);
  const close = () => (tabMenu.value = null);
  const act = (side: 'left' | 'right') => {
    back.value = null;
    layout.value = splitWith(l, m.section, side);
    close();
  };
  const items =
    l.panes.length === 2
      ? [
          { label: 'Show in the left pane', side: 'left' as const, off: l.panes[0] === m.section },
          { label: 'Show in the right pane', side: 'right' as const, off: l.panes[1] === m.section },
        ]
      : [
          { label: 'Split: open on the right', side: 'right' as const, off: false },
          { label: 'Split: open on the left', side: 'left' as const, off: false },
        ];
  const blocked = !wide;
  return (
    <>
      <div class="menu-shield" onPointerDown={close} onContextMenu={(e) => (e.preventDefault(), close())} />
      <div
        class="menu"
        ref={menu}
        role="menu"
        style={{ left: Math.min(m.x, window.innerWidth - 230), top: m.y }}
        onKeyDown={(e) => e.key === 'Escape' && close()}
      >
        <div class="menu-head">{SECTION_LABEL[m.section]}</div>
        {items.map((it) => (
          <button
            key={it.label}
            role="menuitem"
            disabled={it.off || blocked}
            title={blocked ? 'A split needs a window at least 1400 px wide' : undefined}
            onClick={() => act(it.side)}
          >
            {it.label}
          </button>
        ))}
      </div>
    </>
  );
}

/** The folder of a path, for a tab whose file name another open tab has too. */
const dirOf = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '');

/** Every open file as a list under the tabs' "more" button: for the tabs that do not fit. */
function TabOverflow({ close }: { close: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    box.current?.querySelector<HTMLElement>('[aria-current="true"]')?.focus();
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node) && !(e.target as Element).closest?.('.tab-more')) close();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        close();
      }
    };
    document.addEventListener('pointerdown', away, true);
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('pointerdown', away, true);
      document.removeEventListener('keydown', key, true);
    };
  }, []);
  const move = (e: KeyboardEvent) => {
    const items = [...(box.current?.querySelectorAll<HTMLElement>('.tab-menu-item') ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    items[(at + step + items.length) % items.length]?.focus();
  };
  return (
    <div class="tab-menu" ref={box} role="menu" aria-label="Open files" onKeyDown={move} data-testid="tab-menu">
      {tabs.value.map((path) => {
        const f = openFiles.value[path];
        return (
          <button
            key={path}
            type="button"
            role="menuitem"
            class={`tab-menu-item ${activeFile.value === path ? 'on' : ''}`}
            aria-current={activeFile.value === path}
            onClick={() => {
              activeFile.value = path;
              close();
            }}
          >
            <FileTypeIcon icon={fileIcon(path.split('/').pop() ?? path)} />
            <span class="tab-menu-name">{path.split('/').pop()}</span>
            <span class="tab-menu-dir">{dirOf(path)}</span>
            {f && dirty(f) && <span class="dot" aria-label="unsaved changes" />}
          </button>
        );
      })}
    </div>
  );
}

/** The source's file tabs: an icon, the name, a dot while there are changes that are not saved and a close
 * button that takes its place under the pointer; a middle click closes, a drag moves, and when there are more
 * tabs than room a button lists them all. After a click-through from another section, the way back is here. */
export function Tabs() {
  const b = back.value;
  const bar = useRef<HTMLDivElement>(null);
  // Where the strip has more to scroll to, its edge fades; the tab that is active is always in view.
  const [edge, setEdge] = useState({ l: false, r: false });
  const [list, setList] = useState(false);
  const [drag, setDrag] = useState<{ path: string; before: string | null } | null>(null);
  const measure = () => {
    const el = bar.current;
    if (el) setEdge({ l: el.scrollLeft > 2, r: el.scrollLeft + el.clientWidth < el.scrollWidth - 2 });
  };
  useEffect(() => {
    bar.current?.querySelector('.tab.active')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    measure();
  }, [activeFile.value, tabs.value.length]);
  useEffect(() => {
    const el = bar.current;
    if (!el || typeof ResizeObserver !== 'function') return;
    const o = new ResizeObserver(measure);
    o.observe(el);
    return () => o.disconnect();
  }, []);
  const names = tabs.value.map((p) => p.split('/').pop() ?? p);
  const overflowing = edge.l || edge.r;
  return (
    <div class="tabbar files">
      {b && (
        <button
          class="back-btn"
          data-tip={`Back to ${SECTION_LABEL[b.section]}, as you left it (Esc, or Alt+Left)`}
          onClick={requestBack}
        >
          <Icon name="arrow-left" size={14} />
          Back to {SECTION_LABEL[b.section]}
        </button>
      )}
      <div class={`tab-scroll ${edge.l ? 'fade-l' : ''} ${edge.r ? 'fade-r' : ''}`} ref={bar} onScroll={measure} role="tablist" aria-label="Open files">
        {tabs.value.map((path, i) => {
          const f = openFiles.value[path];
          const base = names[i];
          const twin = names.filter((n) => n === base).length > 1;
          const on = activeFile.value === path;
          return (
            <div
              key={path}
              role="tab"
              aria-selected={on}
              class={`tab ${on ? 'active' : ''} ${drag?.path === path ? 'dragging' : ''} ${drag && drag.before === path && drag.path !== path ? 'drop-before' : ''}`}
              title={path}
              data-path={path}
              tabIndex={0}
              draggable
              onClick={() => (activeFile.value = path)}
              onContextMenu={(e) => {
                e.preventDefault();
                fileTabMenu.value = { path, x: e.clientX, y: e.clientY };
              }}
              onKeyDown={(e) => {
                if (e.target !== e.currentTarget) return;
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  activeFile.value = path;
                } else if (e.key === 'Delete' || (e.key === 'w' && (e.ctrlKey || e.metaKey))) {
                  e.preventDefault();
                  closeFile(path);
                } else if ((e.altKey || e.ctrlKey) && e.shiftKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
                  // Move the tab: the keyboard's way of a drag.
                  e.preventDefault();
                  const at = tabs.value.indexOf(path);
                  const to = at + (e.key === 'ArrowRight' ? 2 : -1);
                  moveTab(path, tabs.value[to] ?? null);
                  requestAnimationFrame(() => bar.current?.querySelector<HTMLElement>(`.tab[data-path="${CSS.escape(path)}"]`)?.focus());
                }
              }}
              onMouseDown={(e) => {
                // A middle click is not a scroll.
                if (e.button === 1) e.preventDefault();
              }}
              onAuxClick={(e) => {
                if (e.button === 1) {
                  e.preventDefault();
                  closeFile(path);
                }
              }}
              onDragStart={(e) => {
                e.dataTransfer?.setData('text/plain', path);
                if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
                setDrag({ path, before: path });
              }}
              onDragOver={(e) => {
                if (!drag) return;
                e.preventDefault();
                const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                const before = dropBefore(tabs.value, path, e.clientX < r.left + r.width / 2);
                if (before !== drag.before) setDrag({ path: drag.path, before });
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (drag) moveTab(drag.path, drag.before);
                setDrag(null);
              }}
              onDragEnd={() => setDrag(null)}
            >
              <FileTypeIcon icon={fileIcon(base)} />
              <span class="tab-name">{base}</span>
              {twin && <span class="tab-dir">{dirOf(path)}</span>}
              {f?.readonly && (
                <span class="tab-lock" aria-label="read-only" data-tip="Read-only">
                  <LockIcon />
                </span>
              )}
              <span class="tab-end">
                {f && dirty(f) ? <span class="dot" aria-label="unsaved changes" /> : null}
                <span
                  class="close"
                  role="button"
                  aria-label={`Close ${base}`}
                  data-tip="Close"
                  onClick={(e) => {
                    e.stopPropagation();
                    closeFile(path);
                  }}
                >
                  <Icon name="close" size={12} />
                </span>
              </span>
            </div>
          );
        })}
        {drag && drag.before === null && <span class="tab-drop-end" aria-hidden="true" />}
        {/* The end of the strip takes a dragged tab to the last place. */}
        <span
          class="tab-rest"
          onDragOver={(e) => {
            if (drag) {
              e.preventDefault();
              if (drag.before !== null) setDrag({ path: drag.path, before: null });
            }
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (drag) moveTab(drag.path, null);
            setDrag(null);
          }}
        />
      </div>
      {overflowing && tabs.value.length > 1 && (
        <span class="tab-more-anchor">
          <button type="button" class={`icon-btn tab-more ${list ? 'on' : ''}`} aria-label="Open files" aria-haspopup="menu" aria-expanded={list} data-tip="All open files" data-testid="tab-more" onClick={() => setList(!list)}>
            <Icon name="chevron-down" size={14} />
          </button>
          {list && <TabOverflow close={() => setList(false)} />}
        </span>
      )}
    </div>
  );
}

/** A library file is shown to read and to follow, and cannot be changed here. */
export function ReadOnlyBanner() {
  const path = activeFile.value;
  const f = path ? openFiles.value[path] : undefined;
  const file = project.value?.files.find((x) => x.path === path);
  if (!path || !f?.readonly) return null;
  const std = file?.role === 'std' || path.startsWith('std/');
  return (
    <div class="ro-banner" role="note" data-testid="readonly-banner">
      <LockIcon />
      <span class="ro-title">{std ? 'Read-only: the standard library' : 'Read-only'}</span>
      <span class="ro-help">{std ? 'It ships with netsmith. Read it, follow a type to its definition, copy what you need.' : 'Edit this file outside the editor; the page shows it as it is on disk.'}</span>
    </div>
  );
}

export function Conflict() {
  const path = activeFile.value;
  const f = path ? openFiles.value[path] : undefined;
  if (!path || !f?.conflict) return null;
  return (
    <div class="banner">
      {path} changed on disk and this page has edits.
      <button onClick={() => void reload(path)}>Reload from disk</button>
    </div>
  );
}

export function Pane({ children, class: cls }: { children: ComponentChildren; class?: string }) {
  return <section class={`pane ${cls ?? ''}`}>{children}</section>;
}
