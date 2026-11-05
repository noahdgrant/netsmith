import { signal } from '@preact/signals';
import type { ComponentChildren } from 'preact';
import { useEffect, useRef } from 'preact/hooks';

import type { Problem } from '../api';
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
  anyDirty,
  board,
  chooseBoard,
  closeFile,
  connected,
  cursor,
  dirty,
  openFiles,
  problems,
  job,
  notesOpen,
  problemsOpen,
  reload,
  save,
  shown,
  tabs,
  toast,
  analysis,
  goTo,
  paneNotice,
  dismissPaneNotice,
} from '../store';
import { chordLabels, chordText, find, runCommand } from '../commands';
import { openPicker } from '../quickpick';
import { firstLine, plural, seconds } from '../util';
import { useElapsed } from './JobProgress';
import { Logo } from './Logo';
import { SelectionChip } from './SelectionChip';

/** What `section` is for, in its tab's tooltip. */
const SECTION_HINT: Record<Section, string> = {
  schematic: 'the drawing of the board, one sheet at a time',
  power: 'the power tree: sources, converters and loads',
  system: 'the boards of a system and how they are joined',
  bom: 'the bill of materials',
  pcb: 'the board layout',
  build: 'what Write KiCad files and Make fab files did',
  source: 'the code, with the project\'s files',
};

/** What the two-pane mark on a tab says: the section is open in the other pane. */
const OTHER_PANE = 'Open in the other pane: click to move it here';

export const SECTION_LABEL = Object.fromEntries(SECTIONS.map((s) => [s.id, s.label])) as Record<Section, string>;

function Icon({ children }: { children: ComponentChildren }) {
  return (
    <svg class="icon" width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      {children}
    </svg>
  );
}

const KiCadIcon = () => (
  <Icon>
    <path d="M3 1.8h6.2L13 5.6v8.6H3z" />
    <path d="M9 1.8v4h4" />
    <path d="M5.5 10.2h5M8 8v4.4M6.3 11l1.7 1.6 1.7-1.6" />
  </Icon>
);

const FabIcon = () => (
  <Icon>
    <rect x="1.8" y="3.2" width="12.4" height="9.6" rx="1.2" />
    <circle cx="5" cy="6.4" r="0.9" />
    <circle cx="11" cy="9.6" r="0.9" />
    <path d="M5.9 6.4h2.6v3.2h1.6" />
  </Icon>
);

const SplitIcon = () => (
  <Icon>
    <rect x="1.8" y="2.6" width="12.4" height="10.8" rx="1.2" />
    <path d="M8 2.6v10.8" />
  </Icon>
);

/** Two panes, the one that holds the section filled: it marks a tab whose section is in the other pane. */
const OtherPaneMark = ({ side }: { side: 'left' | 'right' }) => (
  <svg class="icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" aria-hidden="true">
    <rect x="1.8" y="2.6" width="12.4" height="10.8" rx="1.2" />
    <path d="M8 2.6v10.8" />
    {side === 'right' ? <rect x="8" y="2.6" width="6.2" height="10.8" fill="currentColor" stroke="none" /> : <rect x="1.8" y="2.6" width="6.2" height="10.8" fill="currentColor" stroke="none" />}
  </svg>
);

const CloseIcon = () => (
  <Icon>
    <path d="M4 4l8 8M12 4l-8 8" />
  </Icon>
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
      title={tip}
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

export function TopBar() {
  const a = analysis.value;
  const boards = a?.boards ?? [];
  return (
    <header class="topbar">
      <div class="brand"><Logo /></div>
      <label class="boardpick" title="The board the sections show">
        Board
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
      </label>
      <SelectionChip />
      <span class="spacer" />
      <button
        class="cmdk"
        title={`Search parts, nets, modules and files (${chordText(find('goto.anything')?.keys ?? 'Mod+P')}); run a command (${chordText(find('palette.open')?.keys ?? 'Mod+Shift+P')})`}
        aria-label="Go to anything or run a command"
        aria-haspopup="dialog"
        onClick={() => openPicker()}
      >
        <svg class="icon" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
          <circle cx="7" cy="7" r="4.6" />
          <path d="M10.6 10.6L14 14" />
        </svg>
        <span class="cmdk-text">Go to anything</span>
        <span class="qp-keys">
          {chordLabels(find('goto.anything')?.keys ?? 'Mod+P').map((k) => (
            <kbd key={k}>{k}</kbd>
          ))}
        </span>
      </button>
      <span class="spacer" />
      <div class="actions" role="group" aria-label="Actions: they write files">
        <Action
          task="build"
          label="Write KiCad files"
          running="Writing KiCad files…"
          icon={<KiCadIcon />}
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
          icon={<FabIcon />}
          primary
          tip={
            "Places and routes the board chosen here, runs netsmith's design rules check and, when it passes, " +
            "writes the Gerbers, bom.csv, cpl.csv and fab-notes.txt for JLCPCB into the project's fab folder " +
            "(fab/ unless netsmith.toml says another). The same as `netsmith fab`. A long job: it can take minutes."
          }
          start={() => runCommand('build.fab')}
        />
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
  return dropAt(layout.peek(), windowWidth.peek(), x, r.left, r.width);
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
  return (
    <div class="tabbar sections" role="tablist" aria-label="Sections">
      {SECTIONS.map((v, i) => {
        const here = pane.section === v.id;
        const elsewhere = other === v.id;
        return (
          <button
            key={v.id}
            role="tab"
            aria-selected={here}
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
            {v.label}
            {elsewhere && (
              <span class="elsewhere-mark" role="img" aria-label={OTHER_PANE} title={OTHER_PANE}>
                <OtherPaneMark side={pane.index === 0 ? 'right' : 'left'} />
              </span>
            )}
          </button>
        );
      })}
      <span class="spacer" />
      {two && pane.index === 1 ? (
        <button
          class="pane-btn"
          title="Close this pane: the other one fills the window"
          onClick={() => {
            back.value = null;
            layout.value = closePane(layout.value, 1);
          }}
        >
          <CloseIcon />
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
          <SplitIcon />
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

/** The source's file tabs. After a click-through from another section, the way back is here. */
export function Tabs() {
  const b = back.value;
  return (
    <div class="tabbar files">
      {b && (
        <button
          class="back-btn"
          title={`Back to ${SECTION_LABEL[b.section]}, as you left it (Esc, or Alt+Left outside the text)`}
          onClick={requestBack}
        >
          ← Back to {SECTION_LABEL[b.section]}
        </button>
      )}
      {tabs.value.map((path) => {
        const f = openFiles.value[path];
        const base = path.split('/').pop() ?? path;
        return (
          <div
            key={path}
            class={`tab ${activeFile.value === path ? 'active' : ''}`}
            title={path}
            onClick={() => (activeFile.value = path)}
            onAuxClick={() => closeFile(path)}
          >
            <span>{base}</span>
            {f && dirty(f) ? <span class="dot" /> : null}
            <span
              class="close"
              onClick={(e) => {
                e.stopPropagation();
                closeFile(path);
              }}
            >
              ×
            </span>
          </div>
        );
      })}
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

function ProblemRow({ p }: { p: Problem }) {
  return (
    <div class={`problem ${p.severity}`} title={p.rendered} onClick={() => p.at && void goTo(p.at)}>
      <span class="sev">{p.severity === 'error' ? '●' : p.severity === 'warning' ? '▲' : '·'}</span>
      <span class="msg">{firstLine(p.message)}</span>
      {p.at && (
        <span class="where">
          {p.at.file}:{p.at.line}:{p.at.column}
        </span>
      )}
    </div>
  );
}

/** Errors and warnings are listed; notes (what the solver left alone) are in a section that
 * is closed until asked for. */
export function ProblemsPanel() {
  const list = problems.value;
  const open = problemsOpen.value;
  const errors = list.filter((p) => p.severity === 'error');
  const warnings = list.filter((p) => p.severity === 'warning');
  const notes = list.filter((p) => p.severity === 'note');
  const real = errors.length + warnings.length;
  return (
    <div class={`problems ${open ? 'open' : ''}`}>
      <div class="problems-head" onClick={() => (problemsOpen.value = !open)}>
        <span class="twisty">{open ? '▾' : '▸'}</span>
        Problems
        <span class="count">{real}</span>
        {notes.length > 0 && <span class="faint">{plural(notes.length, 'note')}</span>}
        {anyDirty.value && <span class="hint">checking unsaved text</span>}
      </div>
      {open && (
        <div class="problem-list">
          {real === 0 && <div class="none">No errors or warnings.</div>}
          {errors.length > 0 && <div class="section-head">Errors <span class="count">{errors.length}</span></div>}
          {errors.map((p, i) => (
            <ProblemRow key={`e${i}`} p={p} />
          ))}
          {warnings.length > 0 && <div class="section-head">Warnings <span class="count">{warnings.length}</span></div>}
          {warnings.map((p, i) => (
            <ProblemRow key={`w${i}`} p={p} />
          ))}
          {notes.length > 0 && (
            <>
              <div class="section-head clickable" onClick={() => (notesOpen.value = !notesOpen.value)}>
                <span class="twisty">{notesOpen.value ? '▾' : '▸'}</span>
                Notes <span class="count">{notes.length}</span>
                <span class="faint">what the solver left alone</span>
              </div>
              {notesOpen.value &&
                notes.map((p, i) => (
                  <ProblemRow key={`n${i}`} p={p} />
                ))}
            </>
          )}
        </div>
      )}
    </div>
  );
}

/** What the running job is doing, in the status bar, so it shows whatever view is open. */
function JobItem() {
  const j = job.value;
  const elapsed = useElapsed(j?.started ?? null);
  if (!j) return null;
  const names: Record<string, string> = { route: 'Routing', fab: 'Fab', order: 'Order', build: 'Build' };
  return (
    <span class="item job-item" title="A long job is running; the editor stays usable">
      <span class="spinner" />
      {names[j.task] ?? j.task}: {j.lines[j.lines.length - 1] ?? 'starting'} ({seconds(elapsed)})
    </span>
  );
}

export function StatusBar() {
  const s = shown.value;
  const c = cursor.value;
  const path = activeFile.value;
  const f = path ? openFiles.value[path] : undefined;
  return (
    <footer class="statusbar">
      <span
        class={`item counts ${s && s.errors > 0 ? 'bad' : ''}`}
        title="Errors and warnings: click to list them"
        onClick={() => (problemsOpen.value = !problemsOpen.value)}
      >
        <span class="e">⊗ {s?.errors ?? 0}</span>
        <span class="w">▲ {s?.warnings ?? 0}</span>
      </span>
      <JobItem />
      {anyDirty.value && (
        <span class="item" title="Ctrl+S saves">
          ● {plural(Object.values(openFiles.value).filter(dirty).length, 'unsaved file')}
          {path && f && dirty(f) && (
            <button class="link" onClick={() => void save(path)}>
              Save
            </button>
          )}
        </span>
      )}
      <span class="spacer" />
      {c && f && (
        <span class="item">
          Ln {c.line}, Col {c.column}
        </span>
      )}
      {board.value && <span class="item">{board.value}</span>}
      <span class={`item ${connected.value ? 'ok' : 'bad'}`}>{connected.value ? '● live' : '○ offline'}</span>
    </footer>
  );
}

export function Toast() {
  const t = toast.value;
  if (!t) return null;
  return <div class={`toast ${t.bad ? 'bad' : ''}`}>{t.text}</div>;
}

export function Pane({ children, class: cls }: { children: ComponentChildren; class?: string }) {
  return <section class={`pane ${cls ?? ''}`}>{children}</section>;
}
