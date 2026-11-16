// The Summary tab of the PCB view (issue 1003): the placed board as numbers, what `netsmith layout
// summary` prints. Wire length and crossings, how full each face is and where it is free, each
// module's box, the decoupling distances, what the datasheets' layout guidelines measure (issue
// 994), the longest nets, the parts near the selected parts, and
// the distance between two pads typed in. A click on a box or a part zooms the canvas to it.

import { useEffect, useState } from 'preact/hooks';

import type { PcbBox, PcbScene, PcbSummary } from '../api';
import { api, ApiFailure } from '../client';
import { EmptyState } from '../components/ui';
import { select, selection } from '../pcbstore';
import { fillWord, guidelineLine, mm, padName, padProblem, padsLine, sheetBox, summaryQuery } from './summary';

/** How many of the longest nets the tab lists. */
const LONGEST_NETS = 8;

export function SummaryTab(props: { scene: PcbScene; onFocus: (box: PcbBox) => void }) {
  const sc = props.scene;
  const [answer, setAnswer] = useState<PcbSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  // What the server said of the last pair asked for, which is then taken off again.
  const [refused, setRefused] = useState<string | null>(null);
  const [within, setWithin] = useState(2);
  const [pairs, setPairs] = useState<[string, string][]>([]);
  const [a, setA] = useState('');
  const [b, setB] = useState('');
  // The refdes of the selected parts: the neighbours listed are theirs.
  const near = sc.parts.filter((p) => selection.value.includes(p.path)).map((p) => p.refdes);
  const ask = summaryQuery(sc.board, near, within, pairs);
  // Asked again when the placement changes (its stamp), the selection, the distance or the pairs.
  useEffect(() => {
    let current = true;
    api
      .pcbSummary(ask)
      .then((r) => {
        if (!current) return;
        setAnswer(r);
        setError(null);
      })
      .catch((e) => {
        if (!current) return;
        const why = e instanceof ApiFailure || e instanceof Error ? e.message : String(e);
        // Only a pair can be wrong (the parts asked about are selected ones): it is taken off, with
        // what the server said, and the last answer stays.
        if (pairs.length > 0) {
          setRefused(why);
          setPairs((all) => all.slice(0, -1));
        } else {
          setError(why);
          setAnswer(null);
        }
      });
    return () => {
      current = false;
    };
  }, [ask, sc.layoutStamp]);
  const measure = () => {
    const [x, y] = [padName(a), padName(b)];
    if (!x || !y) {
      setRefused('A pad is a part, a dot and the pad number: R1.2, or stage.c1.1.');
      return;
    }
    const wrong = padProblem(sc, x) ?? padProblem(sc, y);
    if (wrong) {
      setRefused(wrong);
      return;
    }
    setRefused(null);
    setPairs([...pairs, [x, y]]);
    setA('');
    setB('');
  };
  const s = answer;
  return (
    <div class="summary" data-testid="summary">
      {error && (
        <div class="note-line bad" data-testid="summary-error">
          {error}
        </div>
      )}
      {!s && !error && <EmptyState compact class="none" icon="note" title="Summing up…" />}
      {s && (
        <div class="rows">
          <div class="sub">Totals</div>
          <div class="summary-line" data-testid="summary-totals">
            <b>{s.totals.hpwl} mm</b> wire (half perimeter), ratsnest {s.totals.ratsnest} mm, {s.totals.crossings} crossing{s.totals.crossings === 1 ? '' : 's'}; decoupling mean {s.totals.decouplingMean} mm, max {s.totals.decouplingMax} mm
          </div>
          <div class="sub">Near the selection</div>
          <label class="summary-line">
            within{' '}
            <input
              type="number"
              min="0"
              step="0.5"
              value={within}
              aria-label="Distance in mm, courtyard to courtyard"
              data-testid="summary-within"
              onChange={(e) => {
                const v = parseFloat((e.target as HTMLInputElement).value);
                if (Number.isFinite(v) && v >= 0) setWithin(v);
              }}
            />{' '}
            mm
          </label>
          {near.length === 0 && <div class="summary-line faint">Select a part to list the parts near it.</div>}
          {s.neighbours.map((n) => (
            <div key={n.path} class="summary-line" data-testid="summary-near">
              <b>{n.part}</b>
              {n.near.length === 0 ? ` has nothing within ${mm(n.within)} mm` : ': '}
              {n.near.map((q, i) => (
                <span key={q.path}>
                  {i > 0 && ', '}
                  {q.part} {mm(q.gap)} mm{q.face !== n.face ? ` (${q.face})` : ''}
                </span>
              ))}
            </div>
          ))}
          <div class="sub">Distance between two pads</div>
          <div class="summary-pads">
            <input class="filter" placeholder="R1.1" aria-label="First pad: part, dot, pad number" data-testid="summary-pad-a" value={a} onInput={(e) => setA((e.target as HTMLInputElement).value)} onKeyDown={(e) => e.key === 'Enter' && measure()} />
            <input class="filter" placeholder="U1.3" aria-label="Second pad: part, dot, pad number" data-testid="summary-pad-b" value={b} onInput={(e) => setB((e.target as HTMLInputElement).value)} onKeyDown={(e) => e.key === 'Enter' && measure()} />
            <button class="tool" data-testid="summary-measure" disabled={!a.trim() || !b.trim()} onClick={measure}>
              Measure
            </button>
          </div>
          {refused && (
            <div class="note-line bad" data-testid="summary-refused">
              {refused}
            </div>
          )}
          {s.pads.map((p, i) => (
            <div key={`${p.from}-${p.to}-${i}`} class="summary-line" data-testid="summary-pads">
              <b>
                {p.from} to {p.to}
              </b>
              : {padsLine(p)}
            </div>
          ))}
          {pairs.length > 0 && (
            <div class="summary-line">
              <button class="tool" data-testid="summary-clear" onClick={() => setPairs([])}>
                Clear the pads
              </button>
            </div>
          )}
          <div class="sub">Faces</div>
          {s.faces.map((f) => (
            <div key={f.face} class="summary-line" data-testid="summary-face" data-face={f.face}>
              <b>{f.face}</b>: {f.parts} part{f.parts === 1 ? '' : 's'}, courtyards {mm(f.courtyards)} of {mm(f.usable)} mm² ({fillWord(f.fill)}), {mm(f.free)} mm² free
              {f.largestFree.slice(0, 3).map((r, i) => (
                <button key={i} class="chip free-box" title="Zoom to this free rectangle" onClick={() => props.onFocus(sheetBox(sc, r))}>
                  {mm(r.max[0] - r.min[0])} × {mm(r.max[1] - r.min[1])} at {mm(r.min[0])}, {mm(r.min[1])}
                </button>
              ))}
            </div>
          ))}
          {s.modules.length > 0 && <div class="sub">Modules</div>}
          {s.modules.map((m) => (
            <div key={m.path} class="summary-line clickable" data-testid="summary-module" onClick={() => props.onFocus(sheetBox(sc, m.box))}>
              <b>{m.path}</b>: {m.parts.length} parts in {mm(m.box.max[0] - m.box.min[0])} × {mm(m.box.max[1] - m.box.min[1])} mm, spread {mm(m.spread)} mm
              {m.intruders.length > 0 && <span class="tag" title="Parts of other modules inside this module's box">intruders {m.intruders.join(', ')}</span>}
            </div>
          ))}
          {s.decoupling.length > 0 && <div class="sub">Decoupling</div>}
          {s.decoupling.map((d) => (
            <div
              key={d.capPath}
              class="summary-line clickable"
              data-testid="summary-decoupling"
              onClick={() => {
                const hit = sc.parts.find((p) => p.path === d.capPath);
                if (hit) {
                  select([hit.path]);
                  props.onFocus(hit.courtyard);
                }
              }}
            >
              <b>{d.cap}</b> to {d.part}: {mm(d.distance)} mm
            </div>
          ))}
          {s.guidelines.length > 0 && <div class="sub">Datasheet guidelines</div>}
          {s.guidelines.map((g) => (
            <div
              key={`${g.part}.${g.name}.${g.other ?? ''}`}
              class={`summary-line clickable${g.met === false ? ' bad' : ''}`}
              data-testid="summary-guideline"
              title={g.source}
              onClick={() => {
                const hit = sc.parts.find((p) => p.path === g.part);
                if (hit) {
                  select([hit.path]);
                  props.onFocus(hit.courtyard);
                }
              }}
            >
              <b>
                {g.part}.{g.name}
              </b>{' '}
              {guidelineLine(g)}
            </div>
          ))}
          <div class="sub">Longest nets</div>
          {[...s.nets]
            .sort((x, y) => y.hpwl - x.hpwl)
            .slice(0, LONGEST_NETS)
            .map((n) => (
              <div key={n.name} class="summary-line" data-testid="summary-net">
                <b>{n.name}</b>: {n.pads} pads, {mm(n.hpwl)} mm
                {n.crossings > 0 && `, ${n.crossings} crossing${n.crossings === 1 ? '' : 's'}`}
                {n.longest && <span class="faint"> longest {n.longest.from} to {n.longest.to}, {mm(n.longest.length)} mm</span>}
              </div>
            ))}
          {s.findings.length > 0 && <div class="sub">Findings</div>}
          {s.findings.map((f, i) => (
            <div key={i} class="summary-line faint">
              {f}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
