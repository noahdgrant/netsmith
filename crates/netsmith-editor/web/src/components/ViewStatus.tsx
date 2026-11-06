// What a view says when it is not showing the current source: the strip above it, the thin line
// under the tabs while the server works, and what stands in for a drawing that is not there yet.

import { useEffect } from 'preact/hooks';

import { banner, firstError, goToFirstError, neverBuilt } from '../feedback/derive';
import { analysisSince, busy, dock, now, pending, runClock, setDockOpen } from '../feedback/state';
import { ANALYSING_CLOCK_AFTER_MS, clock, progressText } from '../feedback/status';
import { emptyProject, entryFileOf } from '../entry';
import { analysis, job, openFile, project, revealSection } from '../store';
import { plural } from '../util';
import { Icon, SeverityIcon } from './Icons';
import { EmptyState } from './ui';
import { connection } from '../feedback/state';

/** The strip above a view that shows an earlier version of the source (it has an error) or the
 * saved file while the typed text has one. It names the first error and goes to it. */
export function StaleStrip() {
  const b = banner.value;
  if (!b) return null;
  return (
    <div class={`stale-strip ${b.kind}`} role="status" data-testid="stale-strip">
      <span class="chip">{b.chip}</span>
      <span class="stale-msg" title={b.detail}>
        <SeverityIcon severity="error" size={13} />
        <span class="text">{b.text}</span>
        {b.where && <span class="where">{b.where}</span>}
        {b.more > 0 && <span class="more">+{plural(b.more, 'more error')}</span>}
      </span>
      <button class="tool small" onClick={goToFirstError}>
        Go to error
      </button>
      <button class="tool small" onClick={() => setDockOpen(true)}>
        Problems
      </button>
    </div>
  );
}

/** A thin moving line under the tab strip while the server is working for longer than it takes to
 * blink. It does not show how far along the work is: the server does not say. */
export function ProgressLine() {
  const b = busy.value;
  const j = job.value;
  useEffect(() => (b ? runClock() : undefined), [b]);
  const since = pending.value.since;
  const analysing = b && since !== null && now.value - since >= ANALYSING_CLOCK_AFTER_MS;
  if (!analysing && !j) return null;
  return (
    <div class="progress-line" role="progressbar" aria-label={j ? 'A job is running' : 'Analysing the project'} aria-busy="true" />
  );
}

/** The first analysis, as a skeleton and a clock: `Analysing the project… 12 s`, and the stage the
 * server is in when it says. The page does not wait for it to paint, and takes the board when it ends. */
function Analysing() {
  const since = analysisSince.value;
  useEffect(() => runClock(), []);
  const progress = pending.value.server?.progress;
  const elapsed = since === null ? '' : clock(Math.max(0, now.value - since));
  return (
    <ViewLoading what="Analysing the project" detail={[elapsed, progress ? progressText(progress) : ''].filter(Boolean).join(' · ')} />
  );
}

/** Blocks that stand where a view will be, so a first read looks like a page that is on its way. */
export function ViewLoading({ what, detail }: { what: string; detail?: string }) {
  return (
    <div class="view-loading" role="status" aria-live="polite" data-testid="view-loading">
      <div class="skeleton-stage" aria-hidden="true">
        <div class="sk sk-sheet" />
        <div class="sk sk-side">
          <div class="sk sk-line w60" />
          <div class="sk sk-line w90" />
          <div class="sk sk-line w75" />
          <div class="sk sk-line w40" />
          <div class="sk sk-line w80" />
        </div>
      </div>
      <div class="view-loading-text">
        {what}…{detail ? ` ${detail}` : ''}
      </div>
    </div>
  );
}

/** What a view says when there is no board: why, with the error that stops it, and where to go. */
export function NoBoard({ what }: { what: string }) {
  // Never reached the server: say so, and that the page will take the project when it is there.
  if (!analysis.value && connection.value === 'offline') {
    return (
      <EmptyState
        icon="offline"
        tone="warn"
        testid="no-board"
        title="The editor server is not answering"
        help={
          <>
            Start <code>netsmith editor</code> in the project; this page connects by itself and shows the board when it is read.
          </>
        }
      />
    );
  }
  if (!analysis.value) return <Analysing />;
  const e = firstError.value;
  // A project that has no board yet is an error to the checker (a listed file must hold a board) and a place to start to the person.
  const begun = emptyProject(analysis.value.problems);
  if (!neverBuilt.value || !e || begun) {
    // Say where to write the first board, and open that file.
    const entry = begun?.file ?? entryFileOf(project.value?.files ?? []);
    const open = () => {
      revealSection('source');
      if (entry) void openFile(entry);
    };
    return (
      <EmptyState
        icon="board"
        testid="no-board"
        title="No boards yet"
        help={
          <>
            Write a <code>board</code> in <code>{entry ?? 'src/main.nsm'}</code>; it is drawn here as soon as it builds.
          </>
        }
        actions={
          <button class="tool primary" onClick={open} data-testid="open-entry">
            <Icon name="file" size={14} />
            {entry ? `Open ${entry}` : 'Open the source'}
          </button>
        }
      />
    );
  }
  return (
    <EmptyState
      icon="error"
      tone="bad"
      testid="no-board"
      class="no-board"
      title={`Nothing has built yet, so there is no board to ${what}`}
      actions={
        <>
          <button class="tool primary" onClick={goToFirstError}>
            Go to error
          </button>
          <button class="tool" onClick={() => (dock.value = { ...dock.value, open: true })}>
            Show problems
          </button>
        </>
      }
    >
      <div class="empty-error">
        <SeverityIcon severity="error" size={14} />
        <span class="text">{e.message}</span>
        {e.at && (
          <span class="where">
            {e.at.file}:{e.at.line}
          </span>
        )}
      </div>
    </EmptyState>
  );
}
