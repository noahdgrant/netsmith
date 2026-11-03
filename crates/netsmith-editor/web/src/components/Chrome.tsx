import type { ComponentChildren } from 'preact';

import type { Problem } from '../api';

import {
  activeFile,
  activeView,
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
  run,
  save,
  shown,
  showEditor,
  showView,
  tabs,
  toast,
  analysis,
  goTo,
  type ViewKind,
} from '../store';
import { firstLine, plural, seconds } from '../util';
import { useElapsed } from './JobProgress';

export const VIEWS: { id: ViewKind; label: string }[] = [
  { id: 'schematic', label: 'Schematic' },
  { id: 'power', label: 'Power tree' },
  { id: 'system', label: 'System' },
  { id: 'bom', label: 'BOM' },
  { id: 'pcb', label: 'PCB' },
  { id: 'build', label: 'Build' },
];

export function TopBar() {
  const a = analysis.value;
  const boards = a?.boards ?? [];
  return (
    <header class="topbar">
      <div class="brand">netsmith</div>
      <label class="boardpick" title="The board the views show">
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
      <span class="spacer" />
      <button
        class="tool"
        title="Write the KiCad schematic of every board"
        onClick={() => {
          activeView.value = 'build';
          showView.value = true;
          run.value = { what: 'build', nonce: Date.now() };
        }}
      >
        Build
      </button>
      <button
        class="tool primary"
        title="Make the files for a board house, after the design rules check"
        onClick={() => {
          activeView.value = 'build';
          showView.value = true;
          run.value = { what: 'fab', nonce: Date.now() };
        }}
      >
        Fab
      </button>
      <button
        class={`tool ${showEditor.value ? 'on' : ''}`}
        title="Show or hide the source"
        onClick={() => (showEditor.value = !showEditor.value)}
      >
        Source
      </button>
      <button
        class={`tool ${showView.value ? 'on' : ''}`}
        title="Show or hide the views"
        onClick={() => (showView.value = !showView.value)}
      >
        Views
      </button>
    </header>
  );
}

export function Tabs() {
  return (
    <div class="tabbar">
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

export function ViewTabs() {
  return (
    <div class="tabbar views">
      {VIEWS.map((v) => (
        <div
          key={v.id}
          class={`tab ${activeView.value === v.id ? 'active' : ''}`}
          onClick={() => (activeView.value = v.id)}
        >
          {v.label}
        </div>
      ))}
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
