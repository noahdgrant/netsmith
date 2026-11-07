// The status bar, by VS Code's rule: the left says where the workspace stands (the board shown,
// the problems, whether the analysis is current, a job that runs, what is not saved) and the
// right says where the person is (cursor, language, the connection). Every item is a button.

import type { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';

import {
  ANALYSED_SHOWN_MS,
  CANCELLED_TEXT,
  CANCELLING_TEXT,
  analysedText,
  analysedTip,
  analysingText,
  cancelledTip,
  clock,
  CONNECTION_LABEL,
  connectionTip,
} from '../feedback/status';
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
  cancelAnalysis,
  goTo,
  job,
  openFiles,
  project,
  runAnalysisAgain,
  save,
  visible,
} from '../store';
import { THEMES, type ThemePref, theme, themePref } from '../theme';
import { chooseTheme } from '../themeUi';
import { plural } from '../util';
import { useElapsed } from './JobProgress';
import { Icon, SeverityIcon } from './Icons';
import { settings } from '../settings';
import { COMMENT_FONTS, FONT, autosave, commentFont, fontSize, hoverCards, inlineHints, outlineOpen, resetEditorZoom, wordWrap, zoomEditor } from '../source/prefs';
import { chordText, find } from '../commands';
import { CompareChip } from './CompareChip';
import { SourceChangesItem } from './SourceChangesItem';
import { inModal } from './layers';

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
    <button class={`item ${cls ?? ''}`} data-tip={title} aria-label={label} aria-pressed={pressed} onClick={onClick}>
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
    const key = (e: KeyboardEvent) => e.key === 'Escape' && !inModal(e) && close();
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('keydown', key, true);
    };
  }, [close]);
  return (
    <div class="status-popover" ref={box} role="dialog" aria-label={label} data-esc-closes>
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
      <span class="sb-label sb-long">
        {names[j.task] ?? j.task}: {j.lines[j.lines.length - 1] ?? 'starting'} ({clock(elapsed)})
      </span>
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
      <>
        <Item
          class="analysing"
          title={`Checking the project${p.server ? ' (the server began at its own time, so a change made outside the editor shows here too)' : ''}. The views keep showing the last result until it is done.`}
          onClick={() => (dock.value = { ...dock.value, open: true })}
        >
          <Spinner />
          <span class="analysing-text">{p.cancelling ? CANCELLING_TEXT : text}</span>
        </Item>
        {p.server && !p.cancelling && (
          <Item
            class="cancel-analysis"
            label="Cancel the analysis"
            title="Stop the analysis. Nothing it has made is kept: the views go on showing the last result, and the next change to a file starts it again."
            onClick={() => void cancelAnalysis()}
          >
            <Icon name="close" size={12} />
            <span class="sb-label">Cancel</span>
          </Item>
        )}
      </>
    );
  }
  if (analysis.value?.cancelled) {
    return (
      <Item class="cancelled" label={`${CANCELLED_TEXT}. Run again`} title={cancelledTip(!!analysis.value.boards && analysis.value.boards.length === 0)} onClick={() => void runAnalysisAgain()}>
        <Icon name="refresh" size={13} />
        <span class="sb-label">{CANCELLED_TEXT} · Run again</span>
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
      <>
        <Item class="analysing" title="Reading the project for the first time. The server analyses it once, and a big project takes a while.">
          <Spinner />
          {p.cancelling
            ? CANCELLING_TEXT
            : since !== null
              ? (analysingText(Math.max(0, now.value - since), p.server?.progress ?? null) ?? 'Analysing…')
              : 'Reading the project…'}
        </Item>
        {p.server && !p.cancelling && (
          <Item class="cancel-analysis" label="Cancel the first analysis" title="Stop the first analysis. Nothing is shown until it is run again or a file changes." onClick={() => void cancelAnalysis()}>
            <Icon name="close" size={12} />
            <span class="sb-label">Cancel</span>
          </Item>
        )}
      </>
    );
  }
  return (
    <Item class="idle" title={analysedTip(last)} onClick={toggleDock}>
      <Icon name="check" size={13} />
      <span class="sb-label">{analysedText(last, now.value)}</span>
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
            <span class="theme-check" aria-hidden="true">{themePref.value === t.id ? <Icon name="check" size={12} /> : null}</span>
            <span class={`theme-swatch sw-${t.id}`} aria-hidden="true" />
            {t.label}
          </button>
        ))}
      </div>
      <div class="status-popover-hint">Also in the command palette, as “Theme”.</div>
    </Popover>
  );
}

/** What the source shows and how: hints, cards, wrapping, the outline and the size of the text. */
function SourceOptions({ close }: { close: () => void }) {
  const row = (label: string, on: boolean, flip: () => void, command: string, id: string) => (
    <button type="button" class="src-opt" role="menuitemcheckbox" aria-checked={on} data-testid={id} onClick={flip}>
      <span class="src-check" aria-hidden="true">{on ? <Icon name="check" size={12} /> : null}</span>
      {label}
      {find(command)?.keys && <kbd>{chordText(find(command)!.keys!)}</kbd>}
    </button>
  );
  return (
    <Popover close={close} label="Source options">
      <div class="status-popover-title">Source</div>
      <div class="src-opts" role="menu" aria-label="Source options">
        {row('Inline hints', inlineHints.value, () => (inlineHints.value = !inlineHints.value), 'source.hints.toggle', 'opt-hints')}
        {row('Hover cards', hoverCards.value, () => (hoverCards.value = !hoverCards.value), 'source.cards.toggle', 'opt-cards')}
        {row('Word wrap', wordWrap.value, () => (wordWrap.value = !wordWrap.value), 'source.wrap.toggle', 'opt-wrap')}
        {row('Autosave', autosave.value, () => (autosave.value = !autosave.value), 'source.autosave.toggle', 'opt-autosave')}
        {row('Outline', outlineOpen.value, () => (outlineOpen.value = !outlineOpen.value), 'source.outline.toggle', 'opt-outline')}
        <div class="src-comment" role="radiogroup" aria-label="Comment font">
          Comments
          {COMMENT_FONTS.map((f) => (
            <button key={f.id} type="button" class="tool small" role="radio" aria-checked={commentFont.value === f.id} title={f.hint} data-testid={`opt-comments-${f.id}`} onClick={() => (commentFont.value = f.id)}>
              {f.label}
            </button>
          ))}
        </div>
        <div class="src-size" role="group" aria-label="Text size">
          Text size
          <button type="button" class="icon-btn" aria-label="Smaller text" data-tip="Smaller text" disabled={fontSize.value <= FONT.min} onClick={() => zoomEditor(-1)}>
            <Icon name="minus" size={13} />
          </button>
          <output aria-live="polite" data-testid="font-size" title={`The editor's own size. The page's Size setting (${settings.value.scale} %) multiplies it: ${Math.round(fontSize.value * settings.value.scale) / 100} px on screen.`}>{fontSize.value} px</output>
          <button type="button" class="icon-btn" aria-label="Larger text" data-tip="Larger text" disabled={fontSize.value >= FONT.max} onClick={() => zoomEditor(1)}>
            <Icon name="plus" size={13} />
          </button>
          <button type="button" class="tool small src-reset" disabled={fontSize.value === FONT.initial} onClick={resetEditorZoom}>
            Reset
          </button>
        </div>
      </div>
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
  const [popover, setPopover] = useState<'line' | 'connection' | 'theme' | 'source' | null>(null);
  const unsaved = Object.values(openFiles.value).filter(dirty).length;
  const title = project.value?.title;
  // The right side is about the text being edited: it says nothing while no source is on screen.
  const inSource = visible.value.some((p) => p.section === 'source');
  return (
    <footer class="statusbar" aria-label="Status">
      <div class="left">
        {board.value && (
          <Item class="board" label={`Board: ${board.value}. Choose another`} title={`The board the views show${title ? ` (project ${title})` : ''}. Click to choose another.`} onClick={focusBoardPicker}>
            <Icon name="board" size={14} />
            <span class="sb-board">{board.value}</span>
          </Item>
        )}
        <Item
          class={`counts ${c.error > 0 ? 'bad' : ''}`}
          label={`${plural(c.error, 'error')}, ${plural(c.warning, 'warning')}${c.note > 0 ? `, ${plural(c.note, 'note')}` : ''}. Show the problems (Ctrl+J)`}
          title="Errors and warnings in the project, and its notes when there are any. Click to show the problems (Ctrl+J); F8 goes to the next one."
          onClick={toggleDock}
          pressed={dock.value.open}
        >
          <span class="e">
            <SeverityIcon severity="error" size={13} /> {c.error}
          </span>
          <span class="w">
            <SeverityIcon severity="warning" size={13} /> {c.warning}
          </span>
          {c.note > 0 && (
            <span class="nt">
              <SeverityIcon severity="note" size={13} /> {c.note}
            </span>
          )}
        </Item>
        <AnalysisItem />
        <JobItem />
        <CompareChip />
        {anyDirty.value && (
          <Item
            class="unsaved"
            title="Files with changes that are not saved. Click to save the open one (Ctrl+S)."
            onClick={() => path && f && dirty(f) && void save(path)}
          >
            <span class="unsaved-dot" aria-hidden="true" />
            {plural(unsaved, 'unsaved file')}
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
          <span class={`status-anchor ${popover === 'source' ? 'open' : ''}`}>
            <Item class="source-opts-item" label="Source options" title="Inline hints, hover cards, word wrap, the outline and the size of the text. Click to change." pressed={popover === 'source'} onClick={() => setPopover(popover === 'source' ? null : 'source')}>
              <Icon name="sliders" size={14} />
            </Item>
            {popover === 'source' && <SourceOptions close={() => setPopover(null)} />}
          </span>
        )}
        {inSource && path && <SourceChangesItem path={path} />}
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
            <Icon name="theme" size={14} />
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
            <span class="sb-label">{CONNECTION_LABEL[state]}</span>
          </Item>
          {popover === 'connection' && <ConnectionDetails close={() => setPopover(null)} />}
        </span>
      </div>
    </footer>
  );
}
