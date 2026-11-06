// The Compare section (ADR 0059): what changed in the circuit since a git ref, as parts, nets,
// rails, bill of materials and problems, not as lines. A header with the ref picker, the server's
// one-line summary, tiles, the board in scope, tabs, and a list of rows with before and after.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import type { Compare as CompareData, Problem } from '../api';
import { Icon } from '../components/Icons';
import { ViewHelp } from '../components/ViewHelp';
import { useElapsed } from '../components/JobProgress';
import { EmptyState, Pill, Segmented, Spinner } from '../components/ui';
import { ChangesSwitch } from '../components/ChangesSwitch';
import {
  CATEGORIES,
  type Category,
  type ChangeRow,
  FILE_STATUS,
  type GroupBy,
  type Tile,
  boardStatusText,
  circuitChanges,
  counts,
  filterRows,
  groupRows,
  onlySourceChanged,
  rowsOf,
  scopeSummary,
  stageIndex,
  stageText,
  STAGE_ORDER,
  tiles,
} from '../compare/model';
import { compareProgress } from '../compare/progress';
import { relativeDate } from '../compare/refs';
import { chooseRef, diffable, gitFailure, gitInfo, loadGit, openDiff, pickerOpen, refName, refText, runCompare, scope, scopeChoice, startingRef, state, tab } from '../compare/store';
import { analysis } from '../store';
import { load, save } from '../storage';
import { plural, seconds } from '../util';
import { DiffPanel } from './compare/DiffPanel';
import { RefPicker } from './compare/RefPicker';
import { RowView } from './compare/Row';

const GROUP_KEY = 'netsmith.compare.group';
/** The files of a project the server gives at a ref (ADR 0059). */
const READABLE = /\.(nsm|toml|csv)$/;

function useSeen() {
  // The repository is asked when the section is first shown, not at start: a project with no git costs nothing.
  // A ref this project used before is compared at once, so the section opens on an answer.
  useEffect(() => {
    void (async () => {
      const info = gitInfo.peek() ?? (await loadGit());
      const ref = refName.peek();
      if (info?.available && ref && state.peek().kind === 'idle') void runCompare(ref);
    })();
  }, []);
}

// ---- progress, errors, empty ----

function Progress({ since, against }: { since: number; against: string }) {
  const p = compareProgress.value;
  const elapsed = useElapsed(since);
  const waiting = !analysis.value;
  const at = stageIndex(p?.stage);
  return (
    <div class="cmp-progress" role="status" aria-live="polite" data-testid="cmp-progress">
      <div class="cmp-progress-head">
        <Spinner size={16} />
        <div>
          <div class="cmp-progress-title">Comparing with <span class="mono">{against}</span></div>
          <div class="cmp-progress-stage" data-testid="cmp-stage">
            {waiting ? 'Waiting for the first analysis of the files on disk' : stageText(p)}
            {p && p.total > 0 && !/\d of \d/.test(p.message) ? `, ${p.done} of ${p.total}` : ''}
          </div>
        </div>
        <span class="cmp-progress-time" data-testid="cmp-elapsed">{seconds(elapsed)}</span>
      </div>
      <ol class="cmp-stages" aria-label="Stages">
        {STAGE_ORDER.map((s, i) => (
          <li key={s} class={i < at ? 'done' : i === at ? 'now' : ''}>
            <i aria-hidden="true" />
            {s === 'diff' ? 'compare' : s}
          </li>
        ))}
      </ol>
      <div class="cmp-sk-tiles" aria-hidden="true">
        {[0, 1, 2, 3, 4].map((i) => (
          <span key={i} class="sk sk-tile" />
        ))}
      </div>
    </div>
  );
}

function ProblemList({ list }: { list: Problem[] }) {
  return (
    <ul class="cmp-baseproblems" aria-label="Problems at the ref">
      {list.map((p, i) => (
        <li key={i}>
          <Icon name={p.severity} size={14} class={`bp-${p.severity}`} />
          <span class="bp-msg">{p.message.split('\n')[0]}</span>
          {p.at && (
            <button type="button" class="link mono" disabled={!diffable(p.at.file)} title="Open the file at the ref beside the file on disk" onClick={() => openDiff({ path: p.at!.file, line: p.at!.line, side: 'base', where: 'compare' })}>
              {p.at.file}:{p.at.line}
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}

function AnotherRef({ label = 'Compare with another ref' }: { label?: string }) {
  return (
    <button class="tool" data-testid="another-ref" onClick={() => (pickerOpen.value = true)}>
      <Icon name="compare" size={14} />
      {label}
    </button>
  );
}

/** A ref that does not analyse is an answer, not a failure: the sentence and the ref's own problems. */
function BrokenRef({ data }: { data: CompareData }) {
  return (
    <div class="cmp-card bad" role="alert" data-testid="cmp-broken">
      <div class="cmp-card-head">
        <span class="cmp-card-mark"><Icon name="error" size={18} /></span>
        <div>
          <div class="cmp-card-title">{data.ref} cannot be compared</div>
          <div class="cmp-card-sub" data-testid="cmp-error">{data.error}</div>
        </div>
      </div>
      {data.baseProblems.length > 0 && <ProblemList list={data.baseProblems} />}
      <div class="cmp-card-actions">
        <AnotherRef />
      </div>
    </div>
  );
}

// ---- the parts of a comparison ----

function Summary({ data }: { data: CompareData }) {
  const now = Date.now();
  const sc = scope.value;
  const total = circuitChanges(data, sc);
  const several = data.boards.length > 1;
  const took = `${data.cached ? 'The ref was read before; only the comparison ran' : 'The ref was read and analysed'} (${seconds(data.elapsedMs)}).`;
  return (
    <div class="cmp-summary">
      <p class="cmp-sentence" data-testid="cmp-summary" data-scope={sc}>{scopeSummary(data, sc)}</p>
      <div class="cmp-meta">
        <span class="mono" title={data.base.subject}>
          {data.base.short}
        </span>
        <span class="cmp-sub">{data.base.subject}</span>
        <span class="faint cmp-when" data-tip={`${new Date(data.base.date).toLocaleString()}. ${took}`}>{relativeDate(data.base.date, now)}</span>
        {data.head.dirty && (
          <span class="cmp-dirty" data-tip="The files on disk are compared as they are, uncommitted changes included">
            <Icon name="warning" size={12} />
            working tree has uncommitted changes (compared as is)
          </span>
        )}
        <span class="spacer" />
        <span class="cmp-total" data-testid="cmp-total" data-tip="Parts, nets, rails, modules and interfaces that changed. The bill of materials, the problems and the source's lines are counted in their own tabs.">
          <b>{total}</b> circuit {total === 1 ? 'change' : 'changes'}
          <span class="faint">{sc === 'all' ? (several ? `in ${plural(data.boards.length, 'board')}` : '') : `in ${sc}`}</span>
        </span>
      </div>
    </div>
  );
}

function TileView({ t }: { t: Tile }) {
  return (
    <button type="button" class={`cmp-tile ${t.tone}`} data-testid={`tile-${t.id}`} onClick={() => (tab.value = t.tab)} aria-label={`${t.label}: ${t.value}${t.sub ? `, ${t.sub}` : ''}. Show the list.`}>
      <span class="cmp-tile-label">
        {t.label}
        {t.tip && (
          <span class="stat-info" data-tip={t.tip}>
            <Icon name="note" size={12} />
          </span>
        )}
      </span>
      <span class={`cmp-tile-value ${t.muted ? 'muted' : ''}`}>{t.value}</span>
      <span class="cmp-tile-chips">
        {t.chips.length === 0 && <span class="cmp-tile-sub">{t.sub ?? 'no change'}</span>}
        {t.chips.map((c) => (
          <span key={c.sign} class={`tchip ${c.sign === '+' ? 'add' : c.sign === '-' ? 'del' : 'chg'}`} title={`${c.n} ${c.label}`}>
            {c.sign === '-' ? '−' : c.sign}
            {c.n}
          </span>
        ))}
        {t.chips.length > 0 && t.sub && <span class="cmp-tile-sub">{t.sub}</span>}
      </span>
    </button>
  );
}

function BoardScope({ data }: { data: CompareData }) {
  const current = scope.value;
  if (data.boards.length < 2) return null;
  return (
    <label class="cmp-scope" data-tip="The board the lists are about">
      <Icon name="board" size={14} class="bp-icon" />
      <span class="sr-only">Board</span>
      <select
        data-testid="cmp-board"
        value={current}
        onChange={(e) => (scopeChoice.value = (e.currentTarget as HTMLSelectElement).value)}
      >
        <option value="all">All boards</option>
        {data.boards.map((b) => (
          <option key={b.name} value={b.name}>
            {b.name} ({boardStatusText(b)})
          </option>
        ))}
      </select>
      <Icon name="chevrons-up-down" size={12} class="bp-caret" />
    </label>
  );
}

function Tabs({ n }: { n: Record<Category, number> }) {
  const active = tab.value;
  return (
    <div
      class="cmp-tabs"
      role="tablist"
      aria-label="What changed"
      onKeyDown={(e) => {
        const at = CATEGORIES.findIndex((c) => c.id === active);
        const to = e.key === 'ArrowRight' ? at + 1 : e.key === 'ArrowLeft' ? at - 1 : null;
        if (to === null) return;
        e.preventDefault();
        const next = CATEGORIES[(to + CATEGORIES.length) % CATEGORIES.length];
        tab.value = next.id;
        queueMicrotask(() => (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-tab="${next.id}"]`)?.focus());
      }}
    >
      {CATEGORIES.map((c) => (
        <button key={c.id} type="button" role="tab" id={`cmp-tab-${c.id}`} data-tab={c.id} aria-selected={active === c.id} aria-controls="cmp-panel" tabIndex={active === c.id ? 0 : -1} class={`cmp-tab ${active === c.id ? 'active' : ''} ${n[c.id] === 0 ? 'zero' : ''}`} onClick={() => (tab.value = c.id)}>
          {c.label}
          <span class="cmp-count" data-testid={`count-${c.id}`}>{n[c.id]}</span>
        </button>
      ))}
    </div>
  );
}

function EmptyTab({ category, against, only }: { category: Category; against: string; only: boolean }) {
  const word: Record<Category, string> = { parts: 'No part changed', nets: 'No net changed', rails: 'No rail changed', bom: 'No line of the bill of materials changed', modules: 'No module or interface changed', problems: 'No problem is new or resolved', source: 'No file differs' };
  return <EmptyState compact icon="check-circle" tone="ok" title={word[category]} help={only && category !== 'source' ? `Only the source differs from ${against}.` : `In this board, since ${against}.`} />;
}

function Source({ data }: { data: CompareData }) {
  const s = data.source;
  if (s.error) return <EmptyState compact icon="warning" tone="warn" title="Git could not count the lines" help={s.error} />;
  if (s.files.length === 0) return <EmptyTab category="source" against={refText.value} only={false} />;
  return (
    <div class="cmp-source" role="list">
      <div class="cmp-source-total" data-testid="source-total">
        {plural(s.filesChanged, 'file')}, <span class="plus">+{s.added}</span> <span class="minus">&minus;{s.removed}</span> lines
      </div>
      {s.files.map((f) => {
        // The server gives the project's own sources (.nsm, .toml, .csv) at a ref, and nothing else; git has the numbers of the rest.
        const readable = READABLE.test(f.path) && f.status !== 'deleted';
        return (
          <button
            key={f.path}
            type="button"
            role="listitem"
            class={`cr cr-file st-${f.status} ${readable ? '' : 'plain'}`}
            data-testid="cmp-file"
            aria-disabled={!readable}
            title={readable ? undefined : 'The comparison of a file reads the project\u2019s sources (.nsm, .toml, .csv); for this one git has the numbers.'}
            onClick={() => readable && openDiff({ path: f.path, where: 'compare' })}
          >
            <span class="cr-main">
              <span class="cr-kind k-file" aria-hidden="true">
                <Icon name="file" size={15} />
              </span>
              <span class="cr-name">
                <span class="cr-title mono">{f.path}</span>
              </span>
              <span class="cr-chips">
                <span class="tchip add">+{f.added}</span>
                <span class="tchip del">&minus;{f.removed}</span>
              </span>
              <Pill tone={f.status === 'added' || f.status === 'untracked' ? 'ok' : f.status === 'deleted' ? 'bad' : 'warn'} class="cr-badge">
                {FILE_STATUS[f.status]}
              </Pill>
              <span class="cr-links">
                {readable && (
                  <span class="cr-link show">
                    <Icon name="compare" size={13} />
                    Show changes
                  </span>
                )}
              </span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

function Body({ data }: { data: CompareData }) {
  const sc = scope.value;
  const active = tab.value;
  const [query, setQuery] = useState('');
  const [group, setGroup] = useState<GroupBy>(() => load<GroupBy>(GROUP_KEY, 'none'));
  const rows = useMemo(() => rowsOf(data, sc), [data, sc]);
  const n = useMemo(() => counts(data, sc), [data, sc]);
  const tl = useMemo(() => tiles(data, sc), [data, sc]);
  const list: ChangeRow[] = active === 'source' ? [] : (rows[active as Exclude<Category, 'source'>] as ChangeRow[]);
  const shown = filterRows(list, query);
  const renum = filterRows(rows.renumbered, query);
  // The bill of materials is always grouped by what happened to the line (Added, Removed, Changed): the server's order mixes the boards' lines.
  const groups = groupRows(shown, active === 'bom' ? 'change' : group);
  const several = sc === 'all' && data.boards.length > 1;
  const rowsTotal = n.parts + n.nets + n.rails + n.bom + n.modules + n.problems;
  // No change to the circuit: say so with a check, and what did differ in the source (comments, formatting, a layout file).
  const calm = !data.changed && rowsTotal === 0 && n.parts + n.nets + n.rails === 0;
  const onlySource = calm && data.source.filesChanged > 0;
  return (
    <>
      <div class="cmp-tiles" role="group" aria-label="Summary">
        {tl.map((t) => (
          <TileView key={t.id} t={t} />
        ))}
      </div>
      {calm ? (
        <>
          <EmptyState
            icon="check-circle"
            tone="ok"
            compact={onlySource}
            title="No changes in the circuit"
            help={
              onlySource
                ? `The parts, nets, rails and bill of materials are the same as at ${refText.value}. Only the source differs: ${plural(data.source.filesChanged, 'file')}, +${data.source.added} \u2212${data.source.removed} lines, which is comments, formatting or something the circuit does not read.`
                : `The circuit on disk is the same as at ${refText.value}${rows.renumbered.length ? ` (${plural(rows.renumbered.length, 'part')} only got another designator)` : ''}.`
            }
            testid="cmp-empty"
            actions={!onlySource ? <AnotherRef label="Compare with another ref" /> : undefined}
          />
          {onlySource && (
            <div class="cmp-scroll cmp-scroll-source" data-testid="cmp-source-only">
              <Source data={data} />
            </div>
          )}
        </>
      ) : (
        <>
          <Tabs n={n} />
          {active !== 'source' && (
          <div class="cmp-bar" role="toolbar" aria-label="Filter and group the rows">
            <label class="field-icon cmp-filter">
              <Icon name="search" size={13} />
              <input
                type="search"
                placeholder="Filter by name, value, designator"
                aria-label="Filter the changes"
                value={query}
                onInput={(e) => setQuery((e.currentTarget as HTMLInputElement).value)}
                onKeyDown={(e) => {
                  if (e.key === 'Escape' && query) {
                    e.stopPropagation();
                    setQuery('');
                  }
                }}
              />
            </label>
            <span class="spacer" />
            {(active === 'parts' || active === 'nets' || active === 'modules' || active === 'problems') && (
              <Segmented<GroupBy>
                label="Group the rows"
                value={group}
                onChange={(v) => {
                  setGroup(v);
                  save(GROUP_KEY, v);
                }}
                options={[
                  { value: 'none', label: 'Flat', tip: 'One list' },
                  { value: 'module', label: 'By module', tip: 'Group by the module the thing is in' },
                  { value: 'change', label: 'By change', tip: 'Group by the kind of change' },
                ]}
              />
            )}
          </div>
          )}
          <div class="cmp-scroll" id="cmp-panel" role="tabpanel" aria-labelledby={`cmp-tab-${active}`}>
            {active === 'source' ? (
              <Source data={data} />
            ) : list.length === 0 ? (
              <EmptyTab category={active} against={refText.value} only={onlySourceChanged(data)} />
            ) : shown.length === 0 && renum.length === 0 ? (
              <EmptyState compact icon="search" title="No row matches" help={`Nothing fits “${query.trim()}”.`} actions={<button class="tool" onClick={() => setQuery('')}>Clear the filter</button>} />
            ) : (
              <div class="cmp-list" role="list" data-testid="cmp-list">
                {groups.map((g) => (
                  <div class="cmp-group" key={g.key || 'all'}>
                    {g.label && (
                      <div class="cmp-group-head">
                        <b>{g.label}</b>
                        <span class="faint">{g.rows.length}</span>
                      </div>
                    )}
                    {g.rows.map((r) => (
                      <RowView key={r.id} row={r} showBoard={several} />
                    ))}
                  </div>
                ))}
                {active === 'parts' && renum.length > 0 && (
                  <details class="cmp-renum" data-testid="cmp-renumbered">
                    <summary>
                      <Icon name="chevron-right" size={12} class="cmp-renum-chev" />
                      Renumbered ({renum.length})
                      <span class="faint">Adding a part gives the ones after it other designators: the circuit is the same.</span>
                    </summary>
                    {renum.map((r) => (
                      <RowView key={r.id} row={r} showBoard={several} />
                    ))}
                  </details>
                )}
                {active === 'problems' && n.problems === 0 && null}
                <div class="cmp-foot" data-testid="cmp-foot">
                  <span>
                    {plural(shown.length, 'row')}
                    {shown.length !== list.length ? ` of ${list.length}` : ''}
                  </span>
                  {renum.length > 0 && <span class="faint">{renum.length} renumbered</span>}
                </div>
              </div>
            )}
          </div>
        </>
      )}
    </>
  );
}

export function Compare() {
  useSeen();
  const info = gitInfo.value;
  const s = state.value;
  const data = s.kind === 'ready' ? s.data : null;
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // The ref picker was asked for (the palette, the chip): bring the section's own button into view.
    if (pickerOpen.value) root.current?.querySelector('.cmp-head')?.scrollIntoView({ block: 'nearest' });
  }, [pickerOpen.value]);

  let body;
  if (gitFailure.value && !info) {
    body = <EmptyState icon="warning" tone="warn" title="The repository could not be read" help={gitFailure.value} actions={<button class="tool" onClick={() => void loadGit()}>Try again</button>} />;
  } else if (!info) {
    body = (
      <div class="cmp-progress" aria-busy="true" aria-label="Reading the repository" data-testid="cmp-skeleton">
        <div class="cmp-sk-tiles" aria-hidden="true">
          {[0, 1, 2, 3, 4].map((i) => (
            <span key={i} class="sk sk-tile" />
          ))}
        </div>
      </div>
    );
  } else if (!info.available) {
    body = (
      <EmptyState
        icon="branch"
        title={/not installed|git is not/i.test(info.reason ?? '') ? 'git is not installed' : 'This project is not in a git repository'}
        help="The Compare section reads a ref with git. Put the project under git (git init, then commit it), and it can say what changed in the circuit."
        testid="cmp-no-git"
      >
        {info.reason && <div class="cmp-reason mono" data-testid="cmp-reason">{info.reason}</div>}
      </EmptyState>
    );
  } else if (s.kind === 'idle') {
    const suggestion = startingRef();
    body = (
      <EmptyState
        icon="compare"
        title="Compare the circuit with a git ref"
        help="Pick a branch, a tag or a commit. The page says which parts, nets, rails and lines of the bill of materials differ from what is on disk, not which lines of text."
        testid="cmp-start"
        actions={
          <>
            {suggestion && (
              <button class="tool primary" data-testid="compare-suggested" onClick={() => chooseRef(suggestion)}>
                Compare with {suggestion}
              </button>
            )}
            <AnotherRef label="Choose a ref" />
          </>
        }
      />
    );
  } else if (s.kind === 'loading') {
    body = <Progress since={s.since} against={s.ref} />;
  } else if (s.kind === 'failed') {
    body = (
      <div class="cmp-card bad" role="alert" data-testid="cmp-failed">
        <div class="cmp-card-head">
          <span class="cmp-card-mark"><Icon name="error" size={18} /></span>
          <div>
            <div class="cmp-card-title">{s.notARef ? `There is no ref called “${s.ref}”` : 'The comparison failed'}</div>
            <div class="cmp-card-sub">{s.message}</div>
          </div>
        </div>
        <div class="cmp-card-actions">
          <AnotherRef />
          {!s.notARef && (
            <button class="tool" onClick={() => void runCompare(s.ref)}>
              <Icon name="refresh" size={14} />
              Try again
            </button>
          )}
        </div>
      </div>
    );
  } else if (data?.error) {
    body = <BrokenRef data={data} />;
  } else if (data) {
    body = (
      <>
        {data.head.errors > 0 && (
          <div class="cmp-strip warn" role="status" data-testid="cmp-head-errors">
            <Icon name="warning" size={14} />
            {plural(data.head.errors, 'error')} on disk: a board that does not build is shown as of its last good reading.
          </div>
        )}
        <Summary data={data} />
        <Body data={data} />
      </>
    );
  }

  return (
    <div class="cmp" ref={root} data-testid="compare-section">
      <header class="cmp-head">
        <div class="cmp-titles">
          <h1>Compare</h1>
          {info?.available && <RefPicker />}
          {s.kind === 'ready' && s.refreshing && <Spinner size={13} />}
        </div>
        <div class="cmp-actions">
          {data && !data.error && <BoardScope data={data} />}
          {info?.available && refName.value && (
            <>
              <ChangesSwitch label="Show changes in the other views" />
              <button class="icon-btn" data-tip="Compare again" aria-label="Compare again" disabled={s.kind === 'loading'} onClick={() => refName.value && void runCompare(refName.value)}>
                <Icon name="refresh" size={15} />
              </button>
            </>
          )}
        </div>
      </header>
      <div class="cmp-main">{body}</div>
      <DiffPanel where="compare" />
      <ViewHelp id="compare">
        <p>What changed in the circuit on disk against the same project at a git ref: its parts, nets, rails, modules and bill of materials, not the lines of text.</p>
        <p>
          <b>+</b> is added, <b>−</b> removed, <b>~</b> changed. A renumbered part only got another designator: the circuit is the same.
        </p>
        <p>
          <b>Base</b> opens the file at the ref beside the file on disk; <b>Head</b> opens the source where the thing is declared. <b>Show changes in the other views</b> marks the same things in the schematic, the board, the bill of materials and the source.
        </p>
      </ViewHelp>
    </div>
  );
}
