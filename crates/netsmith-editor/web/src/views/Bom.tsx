import { useEffect, useState } from 'preact/hooks';

import type { Bom as BomData, BomLine, CatalogRefresh } from '../api';
import { api } from '../client';
import { JobProgress } from '../components/JobProgress';
import { analysis, board, catalogReport, job, pickPart, checkParts, selectedRefdes } from '../store';
import { revealRequest } from '../selection';
import { ageLabel, count, dollars } from '../util';

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

/** What the check does, never writes and leaves unchecked, in the words of the command (one line each). */
function Notice({ text }: { text: string }) {
  return (
    <div class="refresh-notice faint">
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
        {report.error}
        <Notice text={report.notice} />
      </div>
    );
  }
  const rows = report.files.flatMap((f) => f.rows.map((r) => ({ file: f.file, ...r })));
  return (
    <div class="refresh-result">
      <div>
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

function Rows({ lines, staleAfter }: { lines: BomLine[]; staleAfter: number }) {
  // The lines with a part of the shared selection (a part, a net's parts, a module's) are lit.
  const lit = selectedRefdes.value;
  const ask = revealRequest.value;
  // A go to anything asks for the selection to be shown: the first lit line scrolls into view.
  useEffect(() => {
    if (ask) document.querySelector('.bom tr.sel')?.scrollIntoView({ block: 'center' });
  }, [ask?.nonce]);
  return (
    <>
      {lines.map((l) => (
        <tr
          key={l.designators.join(',')}
          class={l.designators.some((d) => lit.has(d)) ? 'sel' : ''}
          aria-selected={l.designators.some((d) => lit.has(d))}
          onClick={() => pickPart(l.designators.find((d) => lit.has(d)) ?? l.designators[0], true)}
        >
          <td class="des">
            {(l.stale || l.unresolved) && (
              <span class="stale-dot" title={freshness(l, staleAfter)}>
                {'\u25CF'}
              </span>
            )}
            {l.designators.join(', ')}
          </td>
          <td class="num">{l.quantity}</td>
          <td title={l.kind}>{l.value}</td>
          <td title={l.footprint ?? ''}>{l.package ?? l.footprint ?? '-'}</td>
          <td class="mono">{l.lcsc ?? (l.unresolved ? 'unresolved' : l.handSolder ? 'by hand' : 'none')}</td>
          <td>{l.library ? <span class={`pill ${l.library.toLowerCase()}`}>{l.library}</span> : ''}</td>
          <td class="num" title={`${dollars(l.lineUsd)} for ${l.quantity}. ${freshness(l, staleAfter)}`}>
            {l.lcsc && l.unitUsd == null ? 'unknown' : dollars(l.unitUsd)}
            {(l.lcsc || l.unresolved) && <div class={`checked${l.stale || l.unresolved ? ' stale' : ''}`}>{readOn(l)}</div>}
          </td>
          <td class="num" title={freshness(l, staleAfter)}>
            {l.lcsc && l.stock == null ? 'unknown' : count(l.stock)}
          </td>
        </tr>
      ))}
    </>
  );
}

export function Bom() {
  const name = board.value;
  const version = analysis.value?.version;
  const [bom, setBom] = useState<BomData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState(false);
  const [offline, setOffline] = useState(false);
  const refreshing = job.value?.task === 'catalog';
  const report = catalogReport.value;

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

  if (!name) return <div class="empty">There is no board to show. See the problems.</div>;
  if (error) return <div class="empty bad">{error}</div>;
  if (!bom) return <div class="empty">Reading…</div>;
  const parts = bom.lines.reduce((s, l) => s + l.quantity, 0);
  const stale = [...bom.lines, ...bom.hand].filter((l) => l.stale).length;
  const refreshable = bom.catalogs.length > 0;
  return (
    <div class="bom">
      <div class="graph-title">
        Bill of materials of <b>{bom.board}</b>
        <span class="hint">
          {bom.lines.length} lines, {parts} parts for the board house
          {bom.totalUsd != null && <>, about <b>{dollars(bom.totalUsd)}</b> a board</>}
        </span>
      </div>
      <div class="refresh-bar">
        <button
          class="tool"
          disabled={refreshing}
          title={
            refreshable
              ? `Checks the manufacturer's number and package of the rows of ${bom.catalogs.join(', ')} against LCSC's record, as netsmith catalog refresh does, and writes only the day in a checked column. It never writes a price, stock or library: those are JLCPCB's numbers, entered by hand, and JLCPCB data is not fetched (its robots.txt disallows automated clients on /api). LCSC's terms of use are unread; official APIs are future work.`
              : 'netsmith.toml lists no catalogs of its own to refresh'
          }
          onClick={() => void checkParts(preview, offline)}
        >
          {refreshing ? 'Checking…' : 'Check parts against LCSC'}
        </button>
        <label>
          <input type="checkbox" checked={preview} onChange={(e) => setPreview((e.currentTarget as HTMLInputElement).checked)} />{' '}
          preview only
        </label>
        <label>
          <input type="checkbox" checked={offline} onChange={(e) => setOffline((e.currentTarget as HTMLInputElement).checked)} />{' '}
          offline (kept answers)
        </label>
        {stale > 0 && (
          <span class="chip stale-chip">
            {stale} line{stale === 1 ? '' : 's'} with a price and stock read more than {bom.staleAfterDays} days ago
          </span>
        )}
      </div>
      <JobProgress task="catalog" />
      {report && <RefreshResult report={report} />}
      <table class="grid">
        <thead>
          <tr>
            <th>Designators</th>
            <th>Qty</th>
            <th>Value</th>
            <th>Package</th>
            <th>LCSC</th>
            <th>Library</th>
            <th>Each</th>
            <th>Stock</th>
          </tr>
        </thead>
        <tbody>
          <Rows lines={bom.lines} staleAfter={bom.staleAfterDays} />
        </tbody>
        {bom.hand.length > 0 && (
          <>
            <thead>
              <tr>
                <th colSpan={8} class="sub">
                  Soldered by hand (not in bom.csv)
                </th>
              </tr>
            </thead>
            <tbody>
              <Rows lines={bom.hand} staleAfter={bom.staleAfterDays} />
            </tbody>
          </>
        )}
      </table>
      <p class="note">{bom.note}</p>
    </div>
  );
}
