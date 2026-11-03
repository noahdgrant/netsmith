import type { ComponentChildren } from 'preact';

import type { Edge } from '../api';
import { PanZoom } from '../components/PanZoom';
import { type Box, curve, layoutGraph } from '../graph';
import { goTo } from '../store';

export interface GraphNode extends Box {
  body: ComponentChildren;
  class?: string;
}

/** Cards joined by curves, laid out in columns. */
export function Graph({ nodes, edges, fitKey, gap }: { nodes: GraphNode[]; edges: Edge[]; fitKey: string; gap?: number }) {
  const layout = layoutGraph(
    nodes,
    edges.map((e) => ({ from: e.from, to: e.to })),
    gap,
  );
  const at = new Map(layout.nodes.map((n) => [n.id, n]));
  const byId = new Map(nodes.map((n) => [n.id, n]));
  return (
    <PanZoom width={layout.width} height={layout.height} fitKey={fitKey} class="graph-host">
      <svg class="edges" width={layout.width} height={layout.height}>
        <defs>
          <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" class="arrowhead" />
          </marker>
        </defs>
        {edges.map((e, i) => {
          const a = at.get(e.from);
          const b = at.get(e.to);
          if (!a || !b) return null;
          const c = curve(a, b);
          return (
            <g key={i} class={`edge ${e.kind}`}>
              <path d={c.d} marker-end="url(#arrow)" />
              <text
                x={c.mx}
                y={c.my - 8}
                text-anchor="middle"
                class={`edge-label ${e.at ? 'link' : ''}`}
                onClick={() => e.at && void goTo(e.at)}
              >
                {e.label}
              </text>
            </g>
          );
        })}
      </svg>
      {layout.nodes.map((n) => (
        <div
          key={n.id}
          class={`card ${byId.get(n.id)?.class ?? ''}`}
          style={{ left: n.x, top: n.y, width: n.w, height: n.h }}
        >
          {byId.get(n.id)?.body}
        </div>
      ))}
    </PanZoom>
  );
}
