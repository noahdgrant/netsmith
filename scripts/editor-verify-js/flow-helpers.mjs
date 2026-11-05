// Checks shared by the recipes of the drawings that route edges in free channels (the power tree
// and the system view). A helper, not a recipe: only recipes/ holds recipes.

/** Every node (card) of the drawing in `scope` with its box in canvas pixels. */
export function nodesOf(page, scope) {
  return page.evaluate((s) =>
    [...document.querySelectorAll(`${s} .pt-node`)].map((n) => ({
      id: n.dataset.node,
      kind: n.dataset.kind,
      x: parseFloat(n.style.left),
      y: parseFloat(n.style.top),
      w: parseFloat(n.style.width),
      h: parseFloat(n.style.height),
      cls: n.className,
    })), scope);
}

/** Every edge with its corners (canvas pixels) and its path. */
export function edgesOf(page, scope) {
  return page.evaluate((s) =>
    [...document.querySelectorAll(`${s} .pt-edge`)].map((p) => ({
      from: p.dataset.from,
      to: p.dataset.to,
      back: p.dataset.back === 'true',
      points: JSON.parse(p.dataset.points),
      d: p.getAttribute('d'),
      cls: p.getAttribute('class'),
    })), scope);
}

/** What is wrong with the routes: a stretch whose box meets a card the edge does not join, a stretch
 * shorter than 6 px, a vertical step smaller than 16 px (a kink), an end that is off the 8 px grid. An empty list is a pass. */
export function routeProblems(nodes, edges) {
  const bad = [];
  for (const e of edges) {
    const pts = e.points;
    for (let i = 0; i + 1 < pts.length; i++) {
      const [x1, y1] = pts[i];
      const [x2, y2] = pts[i + 1];
      const len = Math.hypot(x2 - x1, y2 - y1);
      if (len < 6) bad.push(`${e.from} > ${e.to}: a stretch of ${len.toFixed(1)} px`);
      // A step in a wire smaller than two grid steps is a kink, not a turn.
      if (x1 === x2 && len < 16) bad.push(`${e.from} > ${e.to}: a jog of ${len.toFixed(1)} px`);
      const lo = [Math.min(x1, x2), Math.min(y1, y2)];
      const hi = [Math.max(x1, x2), Math.max(y1, y2)];
      for (const n of nodes) {
        if (n.id === e.from || n.id === e.to) continue;
        if (hi[0] > n.x - 1 && lo[0] < n.x + n.w + 1 && hi[1] > n.y - 1 && lo[1] < n.y + n.h + 1) bad.push(`${e.from} > ${e.to}: a stretch meets the card ${n.id}`);
      }
      // Even the stretches that end on its own cards stay outside every other card; those ends
      // sit on the edge of a card, never inside it.
      for (const n of nodes) {
        if (n.id !== e.from && n.id !== e.to) continue;
        const [px, py] = [x1, y1];
        if (i === 0 || i + 2 === pts.length) continue;
        if (px > n.x + 1 && px < n.x + n.w - 1 && py > n.y + 1 && py < n.y + n.h - 1) bad.push(`${e.from} > ${e.to}: a corner inside its own card ${n.id}`);
      }
    }
    const ends = [pts[0][1], pts[pts.length - 1][1]];
    for (const y of ends) if (Math.abs(y / 8 - Math.round(y / 8)) > 1e-6) bad.push(`${e.from} > ${e.to}: an end at y ${y} is off the 8 px grid`);
  }
  return bad;
}
