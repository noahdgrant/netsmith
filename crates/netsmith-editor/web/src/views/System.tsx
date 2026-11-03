import { useEffect, useState } from 'preact/hooks';

import type { SystemView } from '../api';
import { api } from '../client';
import { analysis, board, goTo, systemName } from '../store';
import { Graph, type GraphNode } from './Graph';

/** The mated connectors of a system, contact by contact, and the grounds they join (ADR 0040). */
function Mates({ view }: { view: SystemView }) {
  if (view.mates.length === 0) return null;
  return (
    <div class="mates">
      {view.mates.map((m) => (
        <div class="mate" key={`${m.a}/${m.b}`}>
          <div class="mate-head link" onClick={() => m.at && void goTo(m.at)}>
            <b>{m.a}</b> mates <b>{m.b}</b>
            <span class="hint">
              {m.contacts.length} contacts{m.ports.length > 0 ? `; joins ${m.ports.join(', ')}` : ''}
            </span>
          </div>
          <div class="contacts">
            {m.contacts.map((c) => (
              <span
                key={c.number}
                class={`contact role-${c.aRole}`}
                title={`contact ${c.number}: ${m.a} ${c.aRole}${c.aNet ? ` (${c.aNet})` : ''}, ${m.b} ${c.bRole}${c.bNet ? ` (${c.bNet})` : ''}`}
              >
                {c.number} {c.aRole === c.bRole ? c.aRole : `${c.aRole}/${c.bRole}`}
              </span>
            ))}
          </div>
          {m.reversible && <div class="warn">{m.reversible}</div>}
        </div>
      ))}
      {view.groundGroups.map((g) => (
        <div class="ground-group" key={g.nets.join('+')}>
          <b>Ground</b> {g.nets.join(' + ')}
          <span class="hint">{g.contacts.join('; ')} ground contacts</span>
        </div>
      ))}
    </div>
  );
}

export function System() {
  const name = systemName.value ?? board.value;
  const version = analysis.value?.version;
  const [view, setView] = useState<SystemView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!name) return;
    let live = true;
    api
      .system(name)
      .then((v) => live && (setView(v), setError(null)))
      .catch((e: Error) => live && (setView(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  if (!name) return <div class="empty">There is no board to show. See the problems.</div>;
  if (error) return <div class="empty bad">{error}</div>;
  if (!view) return <div class="empty">Reading…</div>;

  const nodes: GraphNode[] = view.nodes.map((n) => ({
    id: n.id,
    w: view.kind === 'system' ? 290 : 230,
    h: 62 + n.ports.length * 20,
    class: `sys ${n.kind}`,
    body: (
      <>
        <div class="card-head link" onClick={() => n.at && void goTo(n.at)}>
          <span class={`badge k-${n.kind}`}>{n.kind[0].toUpperCase()}</span>
          <span class="net">{n.name}</span>
          <span class="type">{n.typeName}</span>
        </div>
        {n.ports.map((p) => (
          <div class="line" key={p.name}>
            <span class="who">{p.name}</span>
            <span class="amount">{[p.voltage, p.current].filter(Boolean).join('  ')}</span>
          </div>
        ))}
      </>
    ),
  }));
  return (
    <div class="graph-view">
      <div class="graph-title">
        {view.kind === 'system' ? 'System' : 'Board'} <b>{view.name}</b>
        <span class="hint">
          {view.kind === 'system'
            ? 'Boards, their ports and the joins between them; below, the mated connectors and the grounds they join.'
            : 'Modules, parts and ports, joined by the nets they share.'}
        </span>
      </div>
      <Graph gap={view.kind === 'system' ? 170 : 110} nodes={nodes} edges={view.edges} fitKey={`${name}:${version}:${view.nodes.length}`} />
      {view.kind === 'system' && <Mates view={view} />}
    </div>
  );
}
