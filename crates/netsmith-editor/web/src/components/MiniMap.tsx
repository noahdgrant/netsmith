import type { TreeLayout } from '../flow/layout';

const MAP_W = 188;

/** The whole drawing small, with the part the window shows and what is selected (`up` and `down`
 * tints); a click or a drag moves the view. */
export function MiniMap({
  layout,
  view,
  tintOf,
  levelOf,
  onGo,
}: {
  layout: TreeLayout;
  view: { k: number; x: number; y: number; w: number; h: number } | null;
  tintOf: (id: string) => string;
  levelOf?: (id: string) => string;
  onGo: (x: number, y: number) => void;
}) {
  const scale = MAP_W / Math.max(layout.width, 1);
  const h = Math.max(24, Math.min(140, layout.height * scale));
  const s = Math.min(scale, h / Math.max(layout.height, 1));
  const go = (e: PointerEvent) => {
    const r = (e.currentTarget as Element).getBoundingClientRect();
    onGo((e.clientX - r.left) / s, (e.clientY - r.top) / s);
  };
  return (
    <svg
      class="pt-minimap"
      width={Math.round(layout.width * s)}
      height={Math.round(layout.height * s)}
      role="img"
      aria-label="Overview of the whole drawing. Click to move the view."
      onPointerDown={(e) => {
        e.stopPropagation();
        (e.currentTarget as Element).setPointerCapture(e.pointerId);
        go(e);
      }}
      onPointerMove={(e) => e.buttons === 1 && go(e)}
    >
      {layout.nodes.map((n) => {
        const tint = tintOf(n.id);
        return (
          <rect
            key={n.id}
            x={n.x * s}
            y={n.y * s}
            width={Math.max(2, n.w * s)}
            height={Math.max(2, n.h * s)}
            rx="1.5"
            class={`mm-${n.kind} ${levelOf?.(n.id) ?? ''} ${tint ? `hot-${tint}` : ''}`}
            data-node={n.id}
          />
        );
      })}
      {view && <rect class="mm-view" x={(-view.x / view.k) * s} y={(-view.y / view.k) * s} width={(view.w / view.k) * s} height={(view.h / view.k) * s} />}
    </svg>
  );
}
