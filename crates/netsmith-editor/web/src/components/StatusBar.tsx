// The status bar, by VS Code's rule: the left says where the workspace stands (the board shown,
// the problems, whether the analysis is current, a job that runs, what is not saved) and the
// right says where the person is (cursor, language, the connection). Every item is a button.

import type { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';

import { ANALYSED_SHOWN_MS, analysedText, analysedTip, analysingText, clock, CONNECTION_LABEL, connectionTip } from '../feedback/status';
import { banner, counts, goToFirstError } from '../feedback/derive';
import {
  analysisSince,
  busy,
  connection,
  lastAnalysis,
  lostFor,
  now,
  pending,
  toggleDock,
  runClock,
  dock,
} from '../feedback/state';
import {
  activeFile,
  analysis,
  anyDirty,
  board,
  cursor,
  dirty,
  goTo,
  job,
  openFiles,
  project,
  save,
  visible,
} from '../store';
import { THEMES, type ThemePref, theme, themePref } from '../theme';
import { chooseTheme } from '../themeUi';
import { plural } from '../util';
import { useElapsed } from './JobProgress';
import { SeverityIcon } from './ProblemsDock';

/** What the open file is, as the bar names it. */
export function languageOf(path: string | null): string {
  if (!path) return '';
  if (path.endsWith('.nsm')) return 'netsmith';
  if (path.endsWith('.toml')) return 'TOML';
  return 'Plain text';
}

const Spinner = () => <span class="spinner" aria-hidden="true" />;

function Item({
  children,
  title,
  onClick,
  class: cls,
  label,
  pressed,
}: {
  children: ComponentChildren;
  title: string;
  onClick?: () => void;
  class?: string;
  label?: string;
  pressed?: boolean;
}) {
  return (
    <button class={`item ${cls ?? ''}`} title={title} aria-label={label} aria-pressed={pressed} onClick={onClick}>
      {children}
    </button>
  );
}

/** A small box above an item, closed by a click elsewhere or Escape. */
function Popover({ children, close, label }: { children: ComponentChildren; close: () => void; label: string }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node) && !(e.target as Element).closest?.('.status-anchor.open')) close();
    };
    const key = (e: KeyboardEvent) => e.key === 'Escape' && close();
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('keydown', key, true);
    };
  }, [close]);
  return (
    <div class="status-popover" ref={box} role="dialog" aria-label={label}>
      {children}
    </div>
  );
}

function GoToLine({ close }: { close: () => void }) {
  const c = cursor.value;
  const path = activeFile.value;
  const [text, setText] = useState('');
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const go = () => {
    const line = Math.max(1, parseInt(text, 10) || 0);
    if (!path || !line) return;
    close();
    void goTo({ file: path, line, column: 1, endLine: line, endColumn: 1 });
  };
  return (
    <Popover close={close} label="Go to line">
      <label>
        Go to line{c ? ` (now ${c.line})` : ''}
        <input
          ref={input}
          type="number"
          min={1}
          inputMode="numeric"
          value={text}
          onInput={(e) => setText((e.currentTarget as HTMLInputElement).value)}
          onKeyDown={(e) => e.key === 'Enter' && go()}
        />
      </label>
      <div class="status-popover-hint">Enter goes there; Escape closes.</div>
    </Popover>
  );
}

function ConnectionDetails({ close }: { close: () => void }) {
  const state = connection.value;
  const a = analysis.value;
  const last = lastAnalysis.value;
  return (
    <Popover close={close} label="Connection">
      <div class="status-popover-title">
        <span class={`dot ${state}`} /> {CONNECTION_LABEL[state]}
      </div>
      <p>{connectionTip(state, lostFor.value)}</p>
      <dl>
        <dt>Server</dt>
        <dd class="mono">{location.host}</dd>
        {a && (
          <>
            <dt>Analysis</dt>
            <dd>version {a.version}</dd>
          </>
        )}
        {last && (
          <>
            <dt>Last analysis</dt>
            <dd>
              took {clock(last.took)}
              {last.seen && <>, finished {clock(now.value - last.at)} ago</>}
            </dd>
          </>
        )}
      </dl>
    </Popover>
  );
}

/** The job that runs (route, fab, build), with the time it has taken. */
function JobItem() {
  const j = job.value;
  const elapsed = useElapsed(j?.started ?? null);
  if (!j) return null;
  const names: Record<string, string> = { route: 'Routing', fab: 'Fab', order: 'Order', build: 'Build', drc: 'Design rules check', catalog: 'Catalog check' };
  return (
    <Item class="job-item" title="A long job is running; the editor stays usable. The Build section has its output." onClick={() => document.querySelector<HTMLElement>('.tab[data-section="build"]')?.click()}>
      <Spinner />
      {names[j.task] ?? j.task}: {j.lines[j.lines.length - 1] ?? 'starting'} ({clock(elapsed)})
    </Item>
  );
}

/** Whether the analysis is running, or the drawing is an old one, or all is current. */
function AnalysisItem() {
  const b = busy.value;
  const last = lastAnalysis.value;
  // The clock ticks while there is something to count, and while `Analysed in 46 s` is on show.
  const calm = !b && last?.seen === true && Date.now() - last.at <= ANALYSED_SHOWN_MS;
  useEffect(() => (b || calm ? runClock() : undefined), [b, calm]);
  const p = pending.value;
  const text = b && p.since !== null ? analysingText(now.value - p.since, p.server?.progress ?? null) : null;
  const ban = banner.value;
  if (text) {
    return (
      <Item
        class="analysing"
        title={`Checking the project${p.server ? ' (the server began at its own time, so a change made outside the editor shows here too)' : ''}. The views keep showing the last result until it is done.`}
        onClick={() => (dock.value = { ...dock.value, open: true })}
      >
        <Spinner />
        <span class="analysing-text">{text}</span>
      </Item>
    );
  }
  if (ban?.kind === 'stale') {
    return (
      <Item class="stale" title={`${ban.detail} Click to go to the first error.`} onClick={goToFirstError}>
        Last good version
      </Item>
    );
  }
  if (!analysis.value) {
    const since = analysisSince.value;
    return (
      <Item class="analysing" title="Reading the project for the first time. The server analyses it once, and a big project takes a while.">
        <Spinner />
        {since !== null ? (analysingText(Math.max(0, now.value - since), p.server?.progress ?? null) ?? 'Analysing…') : 'Reading the project…'}
      </Item>
    );
  }
  return (
    <Item class="idle" title={analysedTip(last)} onClick={toggleDock}>
      <svg class="icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M3.5 8.4l3 3 6-6.6" />
      </svg>
      {analysedText(last, now.value)}
    </Item>
  );
}

/** The theme menu: one radio for each theme, Arrow keys move, Enter or a click chooses. */
function ThemeMenu({ close }: { close: () => void }) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => list.current?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus(), []);
  const move = (e: KeyboardEvent) => {
    const radios = [...(list.current?.querySelectorAll<HTMLElement>('[role="radio"]') ?? [])];
    const at = radios.indexOf(document.activeElement as HTMLElement);
    const step = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    radios[(at + step + radios.length) % radios.length]?.focus();
  };
  const pick = (id: ThemePref) => {
    chooseTheme(id);
    close();
  };
  return (
    <Popover close={close} label="Theme">
      <div class="status-popover-title">Colour theme</div>
      <div class="theme-list" role="radiogroup" aria-label="Colour theme" ref={list} onKeyDown={move}>
        {THEMES.map((t) => (
          <button key={t.id} class="theme-option" role="radio" aria-checked={themePref.value === t.id} title={t.hint} onClick={() => pick(t.id)}>
            <span class="theme-check" aria-hidden="true">{themePref.value === t.id ? '●' : ''}</span>
            <span class={`theme-swatch sw-${t.id}`} aria-hidden="true" />
            {t.label}
          </button>
        ))}
      </div>
      <div class="status-popover-hint">Also in the command palette, as “Theme”.</div>
    </Popover>
  );
}

function focusBoardPicker() {
  const select = document.querySelector<HTMLSelectElement>('.boardpick select');
  if (!select) return;
  select.focus();
  try {
    select.showPicker?.();
  } catch {
    // A browser that will not open it from here still has the focus on it.
  }
}

export function StatusBar() {
  const c = counts.value;
  const path = activeFile.value;
  const f = path ? openFiles.value[path] : undefined;
  const cur = cursor.value;
  const state = connection.value;
  const [popover, setPopover] = useState<'line' | 'connection' | 'theme' | null>(null);
  const unsaved = Object.values(openFiles.value).filter(dirty).length;
  const title = project.value?.title;
  // The right side is about the text being edited: it says nothing while no source is on screen.
  const inSource = visible.value.some((p) => p.section === 'source');
  return (
    <footer class="statusbar" aria-label="Status">
      <div class="left">
        {board.value && (
          <Item class="board" title={`The board the views show${title ? ` (project ${title})` : ''}. Click to choose another.`} onClick={focusBoardPicker}>
            <svg class="icon" width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" aria-hidden="true">
              <rect x="1.8" y="3.2" width="12.4" height="9.6" rx="1.2" />
              <path d="M5 6.4h3M5 9.4h6" />
            </svg>
            {board.value}
          </Item>
        )}
        <Item
          class={`counts ${c.error > 0 ? 'bad' : ''}`}
          label={`${plural(c.error, 'error')}, ${plural(c.warning, 'warning')}. Show the problems (Ctrl+J)`}
          title="Errors and warnings in the project. Click to show the problems (Ctrl+J); F8 goes to the next one."
          onClick={toggleDock}
          pressed={dock.value.open}
        >
          <span class="e">
            <SeverityIcon severity="error" size={13} /> {c.error}
          </span>
          <span class="w">
            <SeverityIcon severity="warning" size={13} /> {c.warning}
          </span>
        </Item>
        <AnalysisItem />
        <JobItem />
        {anyDirty.value && (
          <Item
            class="unsaved"
            title="Files with changes that are not saved. Click to save the open one (Ctrl+S)."
            onClick={() => path && f && dirty(f) && void save(path)}
          >
            ● {plural(unsaved, 'unsaved file')}
          </Item>
        )}
      </div>
      <div class="right">
        {inSource && cur && f && (
          <span class={`status-anchor ${popover === 'line' ? 'open' : ''}`}>
            <Item class="cursor" title="Line and column of the cursor. Click to go to a line." onClick={() => setPopover(popover === 'line' ? null : 'line')}>
              Ln {cur.line}, Col {cur.column}
            </Item>
            {popover === 'line' && <GoToLine close={() => setPopover(null)} />}
          </span>
        )}
        {inSource && path && (
          <Item class="language" title={`${path} is read as ${languageOf(path)}`}>
            {languageOf(path)}
          </Item>
        )}
        <span class={`status-anchor ${popover === 'theme' ? 'open' : ''}`}>
          <Item
            class="theme-item"
            label={`Colour theme: ${THEMES.find((t) => t.id === themePref.value)?.label}`}
            title={`Colour theme: ${THEMES.find((t) => t.id === themePref.value)?.label}${themePref.value === 'system' ? ` (now ${theme.value})` : ''}. Click to change.`}
            pressed={popover === 'theme'}
            onClick={() => setPopover(popover === 'theme' ? null : 'theme')}
          >
            <svg class="icon" width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
              <circle cx="8" cy="8" r="5.6" />
              <path d="M8 2.4a5.6 5.6 0 0 1 0 11.2z" fill="currentColor" stroke="none" />
            </svg>
          </Item>
          {popover === 'theme' && <ThemeMenu close={() => setPopover(null)} />}
        </span>
        <span class={`status-anchor ${popover === 'connection' ? 'open' : ''}`}>
          <Item
            class={`conn ${state}`}
            label={`Connection: ${CONNECTION_LABEL[state]}`}
            title={connectionTip(state, lostFor.value)}
            onClick={() => setPopover(popover === 'connection' ? null : 'connection')}
          >
            <span class={`dot ${state}`} aria-hidden="true" />
            {CONNECTION_LABEL[state]}
          </Item>
          {popover === 'connection' && <ConnectionDetails close={() => setPopover(null)} />}
        </span>
      </div>
    </footer>
  );
}
