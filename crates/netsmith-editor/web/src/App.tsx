import { useEffect, useRef, useState } from 'preact/hooks';

import { Editor } from './components/Editor';
import {
  Conflict,
  PaneNotice,
  ReadOnlyBanner,
  SectionStrip,
  TabMenu,
  Tabs,
  TopBar,
  dragging,
  tabMenu,
} from './components/Chrome';
import { Breadcrumbs } from './components/Breadcrumbs';
import { ProblemsDock } from './components/ProblemsDock';
import { Sidebar } from './components/Sidebar';
import { About } from './components/About';
import { CheatSheet } from './components/CheatSheet';
import { FileTabMenu } from './components/FileTabMenu';
import { QuickPick } from './components/QuickPick';
import { Splash } from './components/Splash';
import { StatusBar } from './components/StatusBar';
import { Toasts } from './components/Toasts';
import { TooltipLayer } from './components/Tooltip';
import { ProgressLine, StaleStrip } from './components/ViewStatus';
import { commandForKey } from './commands';
import { registerAppCommands, registerCompareCommands } from './commandset';
import { registerSettingsCommands } from './settingsCommands';
import { banner } from './feedback/derive';
import { type Section, SECTIONS, clampSplit } from './layout';
import { cheatSheet, picker } from './quickpick';
import { back, goBack, layout, setFocusPane, visible, windowWidth } from './store';
import { Bom } from './views/Bom';
import { Build } from './views/Build';
import { Compare } from './views/Compare';
import { DiffPanel } from './views/compare/DiffPanel';
import { Pcb } from './views/Pcb';
import { Power } from './views/Power';
import { Schematic } from './views/Schematic';
import { System } from './views/System';

registerAppCommands();
registerSettingsCommands();
registerCompareCommands();

function SectionBody({ section }: { section: Section }) {
  switch (section) {
    case 'schematic':
      return <Schematic />;
    case 'power':
      return <Power />;
    case 'system':
      return <System />;
    case 'bom':
      return <Bom />;
    case 'pcb':
      return <Pcb />;
    case 'build':
      return <Build />;
    case 'compare':
      return <Compare />;
    case 'source':
      return (
        <div class="source-section">
          <Sidebar />
          <div class="code">
            <Tabs />
            <Breadcrumbs />
            <Conflict />
            <ReadOnlyBanner />
            <Editor />
            <DiffPanel where="source" />
          </div>
        </div>
      );
  }
}

/** Sections that keep what they are showing (the editor's text and cursor, a zoom, a scroll)
 * while another section is on screen, by staying in the page, hidden. The PCB view's keys
 * work on the whole window, so it leaves the page instead and finds its zoom again. */
const KEPT: Section[] = ['schematic', 'power', 'system', 'bom', 'build', 'compare', 'source'];

/** Commands that still work while go to anything or the cheat sheet is open: they switch the
 * window's mode. Every other key belongs to the window in front. */
const WHILE_OPEN = new Set(['goto.anything', 'palette.open', 'goto.symbol', 'goto.problem', 'help.shortcuts']);

/** The shortcuts: one table, the command registry (commands.ts, commandset.ts). A key runs its
 * command wherever the focus is, except that a plain key (1-7, ?) leaves fields and the source
 * editor alone. */
function useShortcuts() {
  useEffect(() => {
    const typing = (t: EventTarget | null) =>
      !!(t as HTMLElement | null)?.closest?.('input, textarea, select, [contenteditable="true"], .cm-editor');
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && tabMenu.peek()) {
        tabMenu.value = null;
        return;
      }
      const open = picker.peek() !== null || cheatSheet.peek();
      const cmd = commandForKey(e, typing(e.target));
      if (!cmd || (open && !WHILE_OPEN.has(cmd.id))) return;
      if (!cmd.passive) e.preventDefault();
      void Promise.resolve(cmd.run()).catch(() => undefined);
    };
    window.addEventListener('keydown', key, true);
    const resize = () => (windowWidth.value = window.innerWidth);
    window.addEventListener('resize', resize);
    // The browser's Back button, after a click-through, goes back.
    const pop = () => back.peek() && goBack();
    window.addEventListener('popstate', pop);
    return () => {
      window.removeEventListener('keydown', key, true);
      window.removeEventListener('resize', resize);
      window.removeEventListener('popstate', pop);
    };
  }, []);
}

/** The box the panes share inside the area: the area has the frame's gap as padding, and the
 * panes' widths are shares of what is left. */
export function innerBox(el: Element): { left: number; width: number } {
  const r = el.getBoundingClientRect();
  const c = getComputedStyle(el);
  const l = parseFloat(c.paddingLeft) || 0;
  return { left: r.left + l, width: r.width - l - (parseFloat(c.paddingRight) || 0) };
}

/** The divider between two panes: dragged, or moved by the arrow keys; a double click makes the
 * panes equal. */
function Divider({ area }: { area: preact.RefObject<HTMLDivElement> }) {
  const [active, setActive] = useState(false);
  const set = (share: number) => {
    const w = area.current ? innerBox(area.current).width : 0;
    layout.value = { ...layout.peek(), split: clampSplit(share, w), chosen: true };
  };
  return (
    <div
      class={`divider ${active ? 'active' : ''}`}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the panes. Double-click for equal panes."
      aria-valuenow={Math.round(layout.value.split * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
      tabIndex={0}
      title="Drag to resize the panes; double-click for equal panes"
      onPointerDown={(e) => {
        const el = e.currentTarget as HTMLElement;
        el.setPointerCapture(e.pointerId);
        setActive(true);
      }}
      onPointerMove={(e) => {
        if (!active || !area.current) return;
        const r = innerBox(area.current);
        set((e.clientX - r.left) / r.width);
      }}
      onPointerUp={(e) => {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
        setActive(false);
      }}
      onDblClick={() => set(0.5)}
      onKeyDown={(e) => {
        if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
          e.preventDefault();
          e.stopPropagation();
          set(layout.value.split + (e.key === 'ArrowLeft' ? -0.02 : 0.02));
        }
      }}
    />
  );
}

/** Where the dragged tab would land, drawn over the window. */
function DropZone() {
  const d = dragging.value;
  if (!d?.drop) return null;
  const l = layout.value;
  const left = 'side' in d.drop ? d.drop.side === 'left' : d.drop.pane === 0;
  const share = l.panes.length === 2 ? (left ? l.split : 1 - l.split) : 0.5;
  return (
    <div
      class="drop-zone"
      style={{ left: left ? 0 : `${(1 - share) * 100}%`, width: `${share * 100}%` }}
    >
      <span>
        {SECTIONS.find((s) => s.id === d.section)?.label}
        {' → '}
        {l.panes.length === 2 ? (left ? 'left pane' : 'right pane') : left ? 'split left' : 'split right'}
      </span>
    </div>
  );
}

export function App() {
  useShortcuts();
  const area = useRef<HTMLDivElement>(null);
  const vis = visible.value;
  const l = layout.value;
  const two = vis.length === 2;
  // Every section that is on screen or kept: it sits in the grid cell of its pane, or, hidden,
  // under the first pane's.
  // The source is always in the page, so the editor and the tree are ready when it is shown.
  const seen = useRef(new Set<Section>(['source']));
  for (const p of vis) seen.current.add(p.section);
  const mounted = SECTIONS.map((s) => s.id).filter(
    (id) => seen.current.has(id) && (KEPT.includes(id) || vis.some((p) => p.section === id)),
  );
  const dragged = dragging.value;
  const stale = banner.value?.kind === 'stale';
  return (
    <div class={`app ${dragged ? 'dragging' : ''}`}>
      <button
        class="skip-link"
        onClick={() => document.querySelector<HTMLElement>('.section:not(.away)')?.focus()}
      >
        Skip to the view
      </button>
      <TopBar />
      <div
        ref={area}
        class="panes-area"
        role="main"
        aria-label="Workspace"
        style={{
          gridTemplateColumns: two ? `minmax(0, ${l.split}fr) 6px minmax(0, ${1 - l.split}fr)` : 'minmax(0, 1fr)',
        }}
        onPointerDownCapture={(e) => {
          const cell = (e.target as Element).closest('[data-pane]');
          if (cell) setFocusPane(Number(cell.getAttribute('data-pane')));
        }}
      >
        {vis.map((p, i) => (
          <div key={`strip${i}`} class="pane-strip" data-pane={p.index} style={{ gridColumn: two ? (i === 0 ? 1 : 3) : 1, gridRow: 1 }}>
            <SectionStrip pane={p} />
            <PaneNotice pane={p.index} />
          </div>
        ))}
        {mounted.map((id) => {
          const at = vis.findIndex((p) => p.section === id);
          const shown = at >= 0;
          return (
            <section
              key={id}
              class={`section sec-${id} ${shown ? '' : 'away'} ${stale && id !== 'source' && id !== 'build' && id !== 'compare' ? 'is-stale' : ''}`}
              data-section={id}
              data-pane={shown ? vis[at].index : undefined}
              aria-hidden={!shown}
              aria-label={SECTIONS.find((x) => x.id === id)?.label}
              tabIndex={shown ? -1 : undefined}
              style={{ gridColumn: shown && two ? (at === 0 ? 1 : 3) : 1, gridRow: 2 }}
            >
              {id !== 'source' && id !== 'build' && id !== 'compare' && <StaleStrip />}
              <SectionBody section={id} />
            </section>
          );
        })}
        {two && <Divider area={area} />}
        <DropZone />
        <ProgressLine />
      </div>
      <ProblemsDock />
      <StatusBar />
      <TabMenu />
      <FileTabMenu />
      <QuickPick />
      <CheatSheet />
      <About />
      <Splash />
      <Toasts />
      <TooltipLayer />
      {dragged && <div class="drag-ghost" style={{ left: dragged.x + 12, top: dragged.y + 12 }}>{SECTIONS.find((s) => s.id === dragged.section)?.label}</div>}
    </div>
  );
}
