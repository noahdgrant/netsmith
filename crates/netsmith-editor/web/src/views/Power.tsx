import { useEffect, useState } from 'preact/hooks';

import type { Budget, Draw, PowerNode, PowerTree } from '../api';
import { api } from '../client';
import { analysis, board, goTo, systemName } from '../store';
import { Graph, type GraphNode } from './Graph';

const WIDTH = 320;
/** Pixels across an average letter of the cards' text, and of its monospaced numbers. */
const LETTER = 6.6;
const DIGIT = 7.3;
const LINE_HEIGHT = 16;
/** The room inside a card, side to side. */
const INNER = WIDTH - 26;

/** How many lines a name takes beside an amount, when it wraps. */
function lines(text: string, taken: number): number {
  const per = Math.max(8, Math.floor((INNER - taken) / LETTER));
  return Math.max(1, Math.ceil(text.length / per));
}

function lineHeight(d: Draw): number {
  return lines(d.who, (d.range?.text.length ?? 1) * DIGIT + 10) * LINE_HEIGHT + 4;
}

/** The biggest current a draw can be, to put the heaviest first. */
function weight(d: Draw): number {
  return Math.max(Math.abs(d.range?.hi ?? 0), Math.abs(d.range?.lo ?? 0));
}

function byCurrent(list: Draw[]): Draw[] {
  return [...list].sort((a, b) => weight(b) - weight(a) || a.who.localeCompare(b.who));
}

const BUDGET: Record<Budget, { text: string; class: string }> = {
  within: { text: 'within what the supply can give', class: 'ok' },
  over: { text: 'MORE THAN THE SUPPLY CAN GIVE', class: 'bad' },
  unlimited: { text: 'no supply limit is stated', class: 'plain' },
  unknown: { text: 'a current is not settled', class: 'plain' },
};

function Line({ d }: { d: Draw }) {
  return (
    <div class={`line ${d.at ? 'link' : ''}`} onClick={() => d.at && void goTo(d.at)} title={d.at ? 'Go to the source' : ''}>
      <span class="who">{d.who}</span>
      <span class="amount">{d.range?.text ?? '?'}</span>
    </div>
  );
}

function bar(n: PowerNode): number {
  if (!n.demand || !n.capacity || n.capacity.hi <= 0) return 0;
  return Math.min(1, n.demand.hi / n.capacity.hi);
}

function card(n: PowerNode): GraphNode {
  const supplies = byCurrent(n.supplies);
  const loads = byCurrent(n.loads);
  const sections = (supplies.length ? 1 : 0) + (loads.length ? 1 : 0);
  const budget = BUDGET[n.budget];
  const head = lines(n.net, (n.voltage?.text.length ?? 0) * DIGIT + (n.board ? n.board.length * 6 + 24 : 0) + 20) * 20;
  const body = [...supplies, ...loads].reduce((sum, d) => sum + lineHeight(d), 0);
  const note = lines(budget.text + (n.capacityFrom ? ` (${n.capacityFrom})` : ''), 0) * 14;
  return {
    id: n.id,
    w: WIDTH,
    // The padding, the head, a label for each section, the lines, and the budget under them.
    h: 20 + head + sections * 22 + body + 48 + note,
    class: `power ${budget.class}`,
    body: (
      <>
        <div class="card-head">
          <span class="net">{n.net}</span>
          {n.budget === 'over' && <span class="over-badge">OVER</span>}
          {n.board && <span class="chip">{n.board}</span>}
          <span class="volt">{n.voltage?.text ?? ''}</span>
        </div>
        {supplies.length > 0 && <div class="section-label">Supplied by</div>}
        {supplies.map((d) => (
          <Line key={`s${d.who}`} d={d} />
        ))}
        {loads.length > 0 && <div class="section-label">Loads, heaviest first</div>}
        {loads.map((d) => (
          <Line key={`l${d.who}`} d={d} />
        ))}
        <div class="budget">
          <div class="meter">
            <div class={`fill ${budget.class}`} style={{ width: `${Math.round(bar(n) * 100)}%` }} />
          </div>
          <div class="budget-text">
            {n.demand ? <b>{n.demand.text}</b> : <b>?</b>}
            {n.capacity ? ` of ${n.capacity.text.replace(/^0A\.\./, '')}` : ''}
          </div>
          <div class={`budget-note ${budget.class}`}>
            {budget.text}
            {n.capacityFrom ? ` (${n.capacityFrom})` : ''}
          </div>
        </div>
      </>
    ),
  };
}

export function Power() {
  const name = systemName.value ?? board.value;
  const version = analysis.value?.version;
  const [tree, setTree] = useState<PowerTree | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!name) return;
    let live = true;
    api
      .power(name)
      .then((t) => live && (setTree(t), setError(null)))
      .catch((e: Error) => live && (setTree(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  if (!name) return <div class="empty">There is no board to show. See the problems.</div>;
  if (error) return <div class="empty bad">{error}</div>;
  if (!tree) return <div class="empty">Reading the solved model…</div>;
  if (tree.nodes.length === 0) return <div class="empty">No rail draws from a net on this board.</div>;
  return (
    <div class="graph-view">
      <div class="graph-title">
        Power tree of <b>{tree.title}</b>
        <span class="hint">Every number is from the solved model, in the worst case.</span>
      </div>
      <Graph
        gap={110}
        nodes={tree.nodes.map(card)}
        edges={tree.edges}
        fitKey={`${name}:${version}:${tree.nodes.length}`}
      />
    </div>
  );
}
