import { useEffect, useMemo, useState } from 'preact/hooks';

import type { Bom as BomData, BomLine, CatalogRefresh } from '../api';
import { api } from '../client';
import {
  type GroupBy,
  SOURCING_LABEL,
  type Sourcing,
  chipMembers,
  designatorRanges,
  firstOfRange,
  groupLines,
  isProblem,
  moduleOfPath,
  sourcing,
} from '../bomview';
import { ChangesSwitch } from '../components/ChangesSwitch';
import { Icon } from '../components/Icons';
import { useScrollEdges } from '../components/scrollEdges';
import { ViewHelp } from '../components/ViewHelp';
import { type BomMark, bomKey, bomMarks, costDelta, signed } from '../compare/model';
import { boardCompare, refText, showChanges } from '../compare/store';
import { JobProgress } from '../components/JobProgress';
import { EmptyState, Pill, Popover, Segmented, Spinner, type Tone } from '../components/ui';
import { NoBoard, ViewLoading } from '../components/ViewStatus';
import { analysis, board, catalogReport, checkParts, detail, job, pickPart, revealSection, selectedRefdes } from '../store';
import { type Hover, hoverAttrs, hoverOfRefs } from '../hover';
import { revealRequest } from '../selection';
import { load, save } from '../storage';
import { ageLabel, count, dollars, plural } from '../util';

/** Where the line's price and stock come from, when, and whether that is too long ago; or why nobody knows them. */
function freshness(l: BomLine, staleAfter: number): string {
  if (l.unresolved) return l.unresolved;
  if (!l.lcsc) return '';
  if (l.date == null) return `Price and stock unknown: ${l.unknown ?? 'no provider has them'}. Unknown is not 0.`;
  const when = `Price and stock from ${l.supply ?? 'the catalog row'}, read ${l.date}, ${ageLabel(l.ageDays)}`;
  return l.stale ? `${when}: older than ${staleAfter} days, ask the supplier again before relying on them` : when;
}

/** The small text under the price: the day it was read, or that it is unknown. */
function readOn(l: BomLine): string {
  if (l.unresolved) return 'unresolved';
  return l.date ?? 'unknown';
}

const TONE: Record<Sourcing, Tone> = { found: 'ok', stale: 'warn', unchecked: 'neutral', ambiguous: 'warn', missing: 'bad', hand: 'info' };
const MEANING: Record<Sourcing, string> = {
  found: 'An LCSC number with a price and stock that are not old',
  stale: 'An LCSC number whose price and stock were read long ago',
  unchecked: 'An LCSC number, but no provider has said what it costs or what is in stock',
  ambiguous: 'A generic part that no single catalog row stands for; the nearest rows are in the tooltip of the line',
  missing: 'No LCSC number: the board house cannot place this part',
  hand: 'Soldered by hand: the board house does not place it',
};

type ColumnId = 'qty' | 'value' | 'package' | 'lcsc' | 'library' | 'status' | 'each' | 'stock' | 'checked';
const COLUMNS: { id: ColumnId; label: string; num?: boolean }[] = [
  { id: 'qty', label: 'Qty', num: true },
  { id: 'value', label: 'Value' },
  { id: 'package', label: 'Package' },
  { id: 'lcsc', label: 'LCSC' },
  { id: 'library', label: 'Library' },
  { id: 'status', label: 'Sourcing' },
  { id: 'each', label: 'Each', num: true },
  { id: 'stock', label: 'Stock', num: true },
  { id: 'checked', label: 'Checked', num: true },
];
const COLUMN_KEY = 'netsmith.bom.columns';
const GROUP_KEY = 'netsmith.bom.group';

/** What the check does, never writes and leaves unchecked, in the words of the command (one line each). */
function Notice({ text }: { text: string }) {
  return (
    <div class="refresh-notice">
      {text
        .split('\n')
        .filter((l) => l)
        .map((l) => (
          <div key={l}>{l}</div>
        ))}
    </div>
  );
}

function RefreshResult({ report }: { report: CatalogRefresh }) {
  if (report.error) {
    return (
      <div class="refresh-result bad">
        <div class="rr-head">
          <Pill tone="bad" icon="error">
            Could not check
          </Pill>
        </div>
        <div class="rr-error">{report.error}</div>
        <Notice text={report.notice} />
      </div>
    );
  }
  const rows = report.files.flatMap((f) => f.rows.map((r) => ({ file: f.file, ...r })));
  return (
    <div class="refresh-result">
      <div class="rr-head">
        <span class={`chip ${report.dryRun ? '' : 'ok'}`}>{report.dryRun ? 'preview, nothing written' : 'checked, no price or stock written'}</span>
        {report.files.map((f) => (
          <span key={f.file} class="chip">
            {f.file}: {f.lookedUp} rows, {f.verified} verified{f.written ? ', checked dates written' : ''}
          </span>
        ))}
        <span class="took">{report.requests} requests to LCSC</span>
      </div>
      {rows.length > 0 && (
        <table class="grid refresh-rows">
          <tbody>
            {rows.map((r) => (
              <tr key={`${r.file}:${r.line}`}>
                <td class="mono">{r.lcsc}</td>
                <td>
                  <span class={`pill status-${r.status.toLowerCase()}`}>{r.status}</span>
                </td>
                <td>
                  {r.message ||
                    r.changes.map((c) => `${c.column} ${c.from || '(none)'} → ${c.to}`).join(', ') ||
                    'as it was'}
                  {r.notes.map((n) => (
                    <div key={n} class="faint">
                      {n}
                    </div>
                  ))}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {report.files.flatMap((f) => f.notes).map((n) => (
        <div key={n} class="faint">
          {n}
        </div>
      ))}
      <Notice text={report.notice} />
    </div>
  );
}

/** Designators as chips and ranges; a click on one selects that part. */
function Designators({ line, lit, staleAfter }: { line: BomLine; lit: Set<string>; staleAfter: number }) {
  const chips = designatorRanges(line.designators);
  const flag = line.stale || line.unresolved;
  return (
    <div class="des-cell">
      {flag && (
        <span class="stale-dot" title={freshness(line, staleAfter)}>
          {'●'}
        </span>
      )}
      {chips.map((c) => {
        const first = firstOfRange(c);
        const on = chipMembers(c, line.designators).some((d) => lit.has(d));
        return (
          <button
            key={c}
            type="button"
            class={`dchip ${on ? 'lit' : ''}`}
            title={c.includes('\u2013') ? `Select ${first}, the first of ${c}` : `Select ${c}`}
            onClick={(e) => {
              e.stopPropagation();
              pickPart(first, true);
            }}
          >
            {c}
          </button>
        );
      })}
    </div>
  );
}

/** The change of a line since the ref: `+1` where it grew, `-1` where it shrank, and the price. */
function DeltaCell({ mark }: { mark: BomMark | undefined }) {
  if (!mark) return <td class="delta" />;
  const dq = mark.status === 'added' ? mark.quantityHead : mark.quantityHead - (mark.quantityBase ?? mark.quantityHead);
  return (
    <td class="delta" data-testid="bom-delta" title={mark.status === 'added' ? 'New since the ref' : `Quantity ${mark.quantityBase} at the ref, ${mark.quantityHead} now`}>
      {dq !== 0 && <span class={`dq ${dq > 0 ? 'up' : 'down'}`}>{signed(dq)}</span>}
      {dq === 0 && <span class="dq">same qty</span>}
      {mark.deltaUsd !== undefined && Math.abs(mark.deltaUsd) >= 5e-7 && <span class="dp">{costDelta(mark.deltaUsd)}</span>}
    </td>
  );
}

function Row({ l, cols, lit, staleAfter, mark, marking }: { l: BomLine; cols: Set<ColumnId>; lit: Set<string>; staleAfter: number; mark?: BomMark; marking?: boolean }) {
  const on = l.designators.some((d) => lit.has(d));
  const st = sourcing(l);
  return (
    <tr
      class={`dr ${on ? 'sel' : ''} ${mark ? `cmp-row-${mark.status}` : ''}`}
      data-cmp={mark?.status}
      aria-selected={on}
      data-sourcing={st}
      data-hv-ref={l.designators.join(' ')}
      tabIndex={0}
      {...hoverAttrs((): Hover | null => hoverOfRefs(detail.peek(), l.designators))}
      onClick={() => pickPart(l.designators.find((d) => lit.has(d)) ?? l.designators[0], true)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && e.target === e.currentTarget) pickPart(l.designators.find((d) => lit.has(d)) ?? l.designators[0], true);
      }}
    >
      <td class="des">
        <Designators line={l} lit={lit} staleAfter={staleAfter} />
      </td>
      {marking && <DeltaCell mark={mark} />}
      {cols.has('qty') && <td class="num">{l.quantity}</td>}
      {cols.has('value') && <td title={l.kind}>{l.value}</td>}
      {cols.has('package') && <td title={l.footprint ?? ''}>{l.package ?? l.footprint ?? '-'}</td>}
      {cols.has('lcsc') && <td class="mono lcsc">{l.lcsc ?? (l.unresolved ? 'unresolved' : l.handSolder ? 'by hand' : 'none')}</td>}
      {cols.has('library') && <td>{l.library ? <span class={`pill ${l.library.toLowerCase()}`}>{l.library}</span> : ''}</td>}
      {cols.has('status') && (
        <td>
          <Pill tone={TONE[st]} dot title={st === 'ambiguous' ? freshness(l, staleAfter) : MEANING[st]}>
            {SOURCING_LABEL[st]}
          </Pill>
        </td>
      )}
      {cols.has('each') && (
        <td class={`num ${l.lcsc && l.unitUsd == null ? 'unknown' : ''}`} title={`${dollars(l.lineUsd)} for ${l.quantity}. ${freshness(l, staleAfter)}`}>
          {l.lcsc && l.unitUsd == null ? 'unknown' : dollars(l.unitUsd)}
        </td>
      )}
      {cols.has('stock') && (
        <td class={`num ${l.lcsc && l.stock == null ? 'unknown' : ''}`} title={freshness(l, staleAfter)}>
          {l.lcsc && l.stock == null ? 'unknown' : count(l.stock)}
        </td>
      )}
      {cols.has('checked') && (
        <td class="num">{(l.lcsc || l.unresolved) && <span class={`checked${l.stale || l.unresolved ? ' stale' : ''}`}>{readOn(l)}</span>}</td>
      )}
    </tr>
  );
}

function Stat({ label, value, sub, tone, icon, muted, info }: { label: string; value: string; sub?: string; tone?: Tone; icon?: 'check-circle' | 'warning'; /** Not a number yet: said small and quiet, not as the figure of the tile. */ muted?: boolean; /** What a person finds out by pointing at the (i) beside the label. */ info?: string }) {
  return (
    <div class={`bstat ${tone ?? ''}`}>
      <div class="bstat-label">
        {icon && <Icon name={icon} size={13} />}
        {label}
        {info && (
          <span class="stat-info" data-tip={info} tabIndex={0} role="img" aria-label={info}>
            <Icon name="note" size={13} />
          </span>
        )}
      </div>
      <div class={`bstat-value ${muted ? 'muted' : ''}`}>{value}</div>
      {sub && <div class="bstat-sub">{sub}</div>}
    </div>
  );
}

export function Bom() {
  const name = board.value;
  const version = analysis.value?.version;
  const [bom, setBom] = useState<BomData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState(false);
  const [offline, setOffline] = useState(false);
  const [query, setQuery] = useState('');
  const [problemsOnly, setProblemsOnly] = useState(false);
  const [group, setGroup] = useState<GroupBy>(() => load<GroupBy>(GROUP_KEY, 'none'));
  const [hidden, setHidden] = useState<ColumnId[]>(() => load<ColumnId[]>(COLUMN_KEY, []));
  const [folded, setFolded] = useState<Record<string, boolean>>({});
  // The table scrolls sideways in a narrow window: the edge that has more fades, and the first column stays.
  const { ref: scroller, edges, onScroll } = useScrollEdges<HTMLDivElement>(bom);
  const refreshing = job.value?.task === 'catalog';
  const report = catalogReport.value;
  const lit = selectedRefdes.value;
  const ask = revealRequest.value;
  const d = detail.value;
  const moduleByRef = useMemo(() => new Map((d?.parts ?? []).map((p) => [p.refdes, moduleOfPath(p.path)])), [d]);

  useEffect(() => {
    if (!name) return;
    let live = true;
    api
      .bom(name)
      .then((b) => live && (setBom(b), setError(null)))
      .catch((e: Error) => live && (setBom(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  // A go to anything asks for the selection to be shown: the first lit line scrolls into view.
  useEffect(() => {
    if (ask) document.querySelector('.bom tr.sel')?.scrollIntoView({ block: 'center' });
  }, [ask?.nonce]);

  // The comparison with a git ref, when its marks are on: lines new or changed, and the lines that are gone.
  const cmp = boardCompare.value;
  const marking = showChanges.value && !!cmp;
  const bomDelta = useMemo(() => (marking ? bomMarks(cmp?.bom) : null), [marking, cmp]);
  const markOf = (l: BomLine): BomMark | undefined => bomDelta?.byKey.get(bomKey(l));
  const cols = useMemo(() => new Set(COLUMNS.map((c) => c.id).filter((id) => !hidden.includes(id))), [hidden]);
  const q = query.trim().toLowerCase();
  const matches = (l: BomLine) =>
    (!problemsOnly || isProblem(l)) &&
    (!q ||
      [l.designators.join(' '), l.value, l.kind, l.package ?? '', l.footprint ?? '', l.lcsc ?? '', l.mpn ?? '', l.library ?? '', SOURCING_LABEL[sourcing(l)]]
        .join(' ')
        .toLowerCase()
        .includes(q));
  const lines = useMemo(() => (bom ? bom.lines.filter(matches) : []), [bom, q, problemsOnly]);
  const hand = useMemo(() => (bom ? bom.hand.filter(matches) : []), [bom, q, problemsOnly]);
  const gone = useMemo(
    () =>
      (bomDelta?.removed ?? []).filter(
        (l) => !q || [l.designators.join(' '), l.value, l.kind, l.package ?? '', l.footprint ?? '', l.lcsc ?? '', l.mpn ?? ''].join(' ').toLowerCase().includes(q),
      ),
    [bomDelta, q],
  );
  const groups = useMemo(() => groupLines(lines, group, (des) => moduleByRef.get(des) ?? ''), [lines, group, moduleByRef]);

  if (!name) return <NoBoard what="show" />;
  if (error) return <div class="empty bad">{error}</div>;
  if (!bom) return <ViewLoading what="Reading the bill of materials" />;

  const everything = [...bom.lines, ...bom.hand];
  const parts = bom.lines.reduce((s, l) => s + l.quantity, 0);
  const handParts = bom.hand.reduce((s, l) => s + l.quantity, 0);
  const uniqueValues = new Set(everything.map((l) => `${l.value}|${l.package ?? ''}`)).size;
  const stale = everything.filter((l) => l.stale).length;
  const problems = everything.filter(isProblem).length;
  const unpriced = bom.lines.filter((l) => l.lcsc && l.unitUsd == null).length;
  const refreshable = bom.catalogs.length > 0;
  const shownCols = COLUMNS.filter((c) => cols.has(c.id));
  const span = 1 + shownCols.length + (marking ? 1 : 0);
  const toggleColumn = (id: ColumnId) => {
    const next = hidden.includes(id) ? hidden.filter((h) => h !== id) : [...hidden, id];
    setHidden(next);
    save(COLUMN_KEY, next);
  };
  const empty = bom.lines.length === 0 && bom.hand.length === 0;
  const nothingShown = !empty && lines.length === 0 && hand.length === 0;

  return (
    <div class="bom">
      <header class="bom-head">
        <div class="bom-titles">
          <h1>Bill of materials</h1>
          <Pill icon="board" tone="neutral">
            {bom.board}
          </Pill>
          <Popover icon="note" tip="Where these prices come from" class="bom-info" align="left">
            <h4>About the numbers</h4>
            <p>{bom.note}</p>
          </Popover>
        </div>
        <div class="refresh-bar" role="group" aria-label="Check the parts">
          {stale > 0 && (
            <span class="chip stale-chip" title={`${plural(stale, 'line')} with a price and stock that were read more than ${bom.staleAfterDays} days ago`}>
              <Icon name="clock" size={12} />
              {plural(stale, 'line')} read more than {bom.staleAfterDays} days ago
            </span>
          )}
          <label class="toggle" title="Show what the check would do, and write nothing">
            <input type="checkbox" checked={preview} onChange={(e) => setPreview((e.currentTarget as HTMLInputElement).checked)} />
            <span class="toggle-track" aria-hidden="true" />
            Preview only
          </label>
          <label class="toggle" title="Use the answers kept from an earlier check; ask nobody">
            <input type="checkbox" checked={offline} onChange={(e) => setOffline((e.currentTarget as HTMLInputElement).checked)} />
            <span class="toggle-track" aria-hidden="true" />
            Offline
          </label>
          <button
            class="tool"
            disabled={refreshing}
            aria-label="Check parts against LCSC"
            title={
              refreshable
                ? `Checks the manufacturer's number and package of the rows of ${bom.catalogs.join(', ')} against LCSC's record, as netsmith catalog refresh does, and writes only the day in a checked column. It never writes a price, stock or library: those are JLCPCB's numbers, entered by hand, and JLCPCB data is not fetched (its robots.txt disallows automated clients on /api). LCSC's terms of use are unread; official APIs are future work.`
                : 'netsmith.toml lists no catalogs of its own to refresh'
            }
            onClick={() => void checkParts(preview, offline)}
          >
            {refreshing ? <Spinner size={13} /> : <Icon name="refresh" size={14} />}
            {refreshing ? 'Checking…' : 'Check parts'}
          </button>
        </div>
      </header>
      <div class="bom-stats" role="group" aria-label="Summary">
        <Stat label="Parts" value={String(parts + handParts)} sub={`${plural(bom.lines.length + bom.hand.length, 'line')}${handParts ? `, ${handParts} by hand` : ''}`} />
        <Stat label="Unique values" value={String(uniqueValues)} sub="by value and package" />
        <Stat
          label="Cost per board"
          value={bom.totalUsd != null ? dollars(bom.totalUsd) : 'unknown'}
          muted={bom.totalUsd == null}
          info={bom.totalUsd == null ? 'No prices in the catalogs yet' : undefined}
          sub={marking && cmp?.bom?.deltaUsd != null ? `${costDelta(cmp.bom.deltaUsd)} since ${refText.value}` : unpriced > 0 ? `leaves out ${plural(unpriced, 'line')} with no known price` : bom.totalUsd != null ? 'without the feeder fee and shipping' : 'no provider has a price'}
        />
        <Stat
          label="Problems"
          value={problems === 0 ? 'None' : String(problems)}
          tone={problems === 0 ? 'ok' : 'warn'}
          icon={problems === 0 ? 'check-circle' : 'warning'}
          sub={problems === 0 ? 'every line has a number' : 'ambiguous, missing or stale lines'}
        />
      </div>
      <div class="bom-bar" role="toolbar" aria-label="Filter and group the lines">
        <label class="field-icon bom-filter">
          <Icon name="search" size={13} />
          <input
            type="search"
            placeholder="Filter by designator, value, package, number"
            aria-label="Filter the bill of materials"
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
        <button class={`tool small ${problemsOnly ? 'on' : ''}`} aria-pressed={problemsOnly} onClick={() => setProblemsOnly(!problemsOnly)} data-tip="Show only lines that need a look">
          <Icon name="warning" size={13} />
          Problems
          {problems > 0 && <span class="pl pl-neutral">{problems}</span>}
        </button>
        <ChangesSwitch />
        <span class="spacer" />
        <Segmented<GroupBy>
          label="Group the lines"
          value={group}
          onChange={(v) => {
            setGroup(v);
            save(GROUP_KEY, v);
          }}
          options={[
            { value: 'none', label: 'Flat', tip: 'One list' },
            { value: 'value', label: 'By value', tip: 'Group the lines by value' },
            { value: 'module', label: 'By module', tip: 'Group the lines by the module of the part' },
          ]}
        />
        <Popover icon="columns" tip="Choose the columns" align="right" label="Columns">
          <h4>Columns</h4>
          {COLUMNS.map((c) => (
            <label key={c.id} class="pop-row">
              <input type="checkbox" checked={cols.has(c.id)} onChange={() => toggleColumn(c.id)} />
              {c.label}
            </label>
          ))}
        </Popover>
      </div>
      <JobProgress task="catalog" />
      {report && <RefreshResult report={report} />}
      <div class={`bom-frame ${edges.l ? 'scrolled' : ''} ${edges.r ? 'more' : ''}`}>
      <div class="bom-scroll" ref={scroller} onScroll={onScroll}>
        {empty ? (
          <EmptyState icon="bom" title="No parts on this board yet" help="Parts you add to the board's source show up here, with their LCSC numbers once the catalogs know them." actions={<button class="tool" onClick={() => revealSection('source')}>Open the source</button>} />
        ) : nothingShown ? (
          <EmptyState
            icon="search"
            title="No line matches"
            help={`Nothing in the bill of materials fits ${q ? `“${query.trim()}”` : 'the filter'}.`}
            actions={
              <button
                class="tool"
                onClick={() => {
                  setQuery('');
                  setProblemsOnly(false);
                }}
              >
                Clear the filter
              </button>
            }
          />
        ) : (
          <table class="grid bom-table">
            <thead>
              <tr>
                <th class="th-des">Designators</th>
                {marking && <th class="delta" title={`Quantity and price since ${refText.value}`}>Change</th>}
                {shownCols.map((c) => (
                  <th key={c.id} class={c.num ? 'num' : ''}>
                    {c.label}
                  </th>
                ))}
              </tr>
            </thead>
            {groups.map((g) => {
              const closed = g.key !== '' && folded[g.key];
              return (
                <tbody key={g.key || 'all'} class={g.key !== '' ? 'grouped' : ''}>
                  {g.key !== '' && (
                    <tr class="grp-row" onClick={() => setFolded({ ...folded, [g.key]: !closed })}>
                      <td colSpan={span}>
                        <span class="grp-label">
                          <Icon name="chevron-right" size={12} class={`grp-chev ${closed ? '' : 'open'}`} />
                          <b>{g.label}</b>
                          <span class="faint">
                            {plural(g.lines.reduce((s, l) => s + l.quantity, 0), 'part')} · {plural(g.lines.length, 'line')}
                          </span>
                        </span>
                      </td>
                    </tr>
                  )}
                  {!closed && g.lines.map((l) => <Row key={`${g.key}/${l.designators.join(',')}`} l={l} cols={cols} lit={lit} staleAfter={bom.staleAfterDays} mark={markOf(l)} marking={marking} />)}
                </tbody>
              );
            })}
            {hand.length > 0 && (
              <tbody class="grouped hand">
                <tr class="grp-row static">
                  <td colSpan={span}>
                    <span class="grp-label">
                      <Icon name="user" size={13} />
                      <b>Soldered by hand</b>
                      <span class="faint">not in bom.csv</span>
                    </span>
                  </td>
                </tr>
                {hand.map((l) => (
                  <Row key={`hand/${l.designators.join(',')}`} l={l} cols={cols} lit={lit} staleAfter={bom.staleAfterDays} mark={markOf(l)} marking={marking} />
                ))}
              </tbody>
            )}
            {gone.length > 0 && (
              <tbody class="grouped removed" data-testid="bom-removed">
                <tr class="grp-row static removed">
                  <td colSpan={span}>
                    <span class="grp-label">
                      <Icon name="minus" size={13} />
                      <b>Removed since {refText.value}</b>
                      <span class="faint">not on the board any more</span>
                    </span>
                  </td>
                </tr>
                {gone.map((l) => (
                  <tr key={`gone/${bomKey(l)}`} class="dr cmp-row-removed" data-cmp="removed" data-testid="bom-removed-row">
                    <td class="des">
                      <div class="des-cell">
                        {l.designators.map((d) => (
                          <span key={d} class="dchip" title={`${d} at the ref`}>
                            {d}
                          </span>
                        ))}
                      </div>
                    </td>
                    <td class="delta">
                      <span class="dq down">{signed(-l.quantity)}</span>
                      {l.unitUsd != null && <span class="dp">{costDelta(-l.unitUsd * l.quantity)}</span>}
                    </td>
                    {cols.has('qty') && <td class="num">{l.quantity}</td>}
                    {cols.has('value') && <td title={l.kind}>{l.value}</td>}
                    {cols.has('package') && <td title={l.footprint ?? ''}>{l.package ?? l.footprint ?? '-'}</td>}
                    {cols.has('lcsc') && <td class="mono lcsc">{l.lcsc ?? (l.handSolder ? 'by hand' : 'none')}</td>}
                    {cols.has('library') && <td />}
                    {cols.has('status') && <td />}
                    {cols.has('each') && <td class="num">{l.unitUsd != null ? dollars(l.unitUsd) : ''}</td>}
                    {cols.has('stock') && <td />}
                    {cols.has('checked') && <td />}
                  </tr>
                ))}
              </tbody>
            )}
          </table>
        )}
      </div>
      {!empty && !nothingShown && (
        <div class="bom-foot" data-testid="bom-foot">
          <span>
            {plural(lines.length + hand.length, 'line')}
            {lines.length + hand.length !== bom.lines.length + bom.hand.length ? ` of ${bom.lines.length + bom.hand.length}` : ''}
          </span>
          <span class="faint">{plural(parts + handParts, 'part')}</span>
        </div>
      )}
      </div>
      <ViewHelp id="bom">
        <p>One row for each value and package the board needs, with the designators it covers and the catalog line chosen for it.</p>
        <p>
          <b>Library</b> is the part's JLCPCB library: the solver takes Basic before Preferred before Extended. <b>Soldered by hand</b> lists the parts the board house does not place.
        </p>
        <p>
          <b>Each</b> and <b>Stock</b> are the catalog's own figures, entered by hand. <b>Checked</b> is the day Check parts last found the row's manufacturer number and package equal to LCSC's record.
        </p>
        <p>Click a row to pick its parts in every other view.</p>
      </ViewHelp>
    </div>
  );
}
