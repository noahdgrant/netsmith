// Checks shared by the recipes of the drawings that route edges in free channels (the power tree
// and the system view). A helper, not a recipe: only recipes/ holds recipes.
import { whenIdle } from './settle.mjs';

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

/**
 * The page's Size (90, 100, 110, 125 %) on a graph (the power tree, the system view): the cards open at that much more zoom, so the cards, their text and
 * their edges are 0.9, 1 and 1.25 times what they are at 100 % on screen, the layout (the cards' boxes in canvas pixels) is the same, no card is over
 * another and no route is bad, and the overview's rectangle for the window is inside the overview. Needs a graph that does not fit a 1280 x 800 window,
 * which opens at its least zoom. Puts the Size back to 100 %.
 */
export async function sizeSteps({ page, expect, step, section, id }) {
  const scope = `.sec-${id}`;
  step(`the page's Size reaches the ${id} cards: at 125 and 90 % cards, text and edges scale as one (the layout is the same), no card is over another, the overview is right`);
  await page.setViewportSize({ width: 1280, height: 800 });
  const measure = async (scale) => {
    await page.evaluate((s) => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: s, motion: 'system' })), scale);
    await page.reload();
    await page.waitForSelector('.app');
    await section(id);
    await page.waitForSelector(`${scope} .pt-node`);
    await whenIdle(page);
    const nodes = await nodesOf(page, scope);
    const edges = await edgesOf(page, scope);
    const m = await page.evaluate((s) => {
      const n = document.querySelector(`${s} .pt-node`);
      const r = n.getBoundingClientRect();
      const text = n.querySelector('[class*="name"], .pt-name, h3, b') ?? n;
      const tr = text.getBoundingClientRect();
      const k = r.width / n.offsetWidth;
      const mm = document.querySelector(`${s} .pt-minimap`);
      const view = mm?.querySelector('.mm-view');
      const vb = mm ? { width: Number(mm.getAttribute('width')), height: Number(mm.getAttribute('height')) } : null;
      return {
        k,
        width: r.width,
        font: parseFloat(getComputedStyle(text).fontSize) * k,
        textWidth: tr.width,
        overview: !!mm,
        view: view ? { x: +view.getAttribute('x'), y: +view.getAttribute('y'), w: +view.getAttribute('width'), h: +view.getAttribute('height') } : null,
        vb: vb ? { w: vb.width, h: vb.height } : null,
        overflow: document.documentElement.scrollWidth > innerWidth,
      };
    }, scope);
    return { nodes, edges, m };
  };
  const at100 = await measure(100);
  for (const scale of [125, 90]) {
    const at = await measure(scale);
    const f = scale / 100;
    expect(Math.abs(at.m.width / at100.m.width - f) < 0.03, `${scale} %: a card is ${at.m.width.toFixed(0)} px wide on screen, ${at100.m.width.toFixed(0)} at 100 % (x${(at.m.width / at100.m.width).toFixed(2)})`);
    expect(Math.abs(at.m.font / at100.m.font - f) < 0.03, `${scale} %: its text is ${at.m.font.toFixed(1)} px on screen, ${at100.m.font.toFixed(1)} at 100 %`);
    expect(JSON.stringify(at.nodes.map((n) => [n.id, n.x, n.y, n.w, n.h])) === JSON.stringify(at100.nodes.map((n) => [n.id, n.x, n.y, n.w, n.h])) && at.edges.length === at100.edges.length, 'the layout (cards and edges, in canvas pixels) is the same: nothing is laid out again');
    let over = 0;
    for (let i = 0; i < at.nodes.length; i++) for (let j = i + 1; j < at.nodes.length; j++) {
      const a = at.nodes[i];
      const b = at.nodes[j];
      if (a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h) over++;
    }
    expect(over === 0 && routeProblems(at.nodes, at.edges).length === 0, `${scale} %: no card over another, no bad route (${at.nodes.length} cards, ${at.edges.length} edges)`);
    expect(!at.m.overflow, `${scale} %: the page does not overflow`);
    expect(at.m.overview && at.m.view && at.m.vb && at.m.view.w > 0 && at.m.view.h > 0 && at.m.view.x + at.m.view.w / 2 >= 0 && at.m.view.x + at.m.view.w / 2 <= at.m.vb.w && at.m.view.y + at.m.view.h / 2 >= 0 && at.m.view.y + at.m.view.h / 2 <= at.m.vb.h, `${scale} %: the overview shows the window's rectangle inside the overview (${JSON.stringify(at.m.view)} in ${JSON.stringify(at.m.vb)})`);
  }
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 100, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');
}

/** The geometry of a table's frame (the BOM's, Compare's list) against its content: the rule is that a table frame is as tall as its content (header, rows,
 * the counting foot right under the last row) up to the height the pane has, and only then scrolls inside, so there is never an empty frame below the foot.
 * `frame` is the bordered box, `scroller` the element that scrolls, `foot` the counting foot, `last` a function of the foot that finds the last thing above it. */
export function frameFit(page, { frame, scroller, foot, section }) {
  return page.evaluate(
    ({ frame, scroller, foot, section }) => {
      const f = document.querySelector(frame).getBoundingClientRect();
      const sc = document.querySelector(scroller);
      const ft = document.querySelector(foot);
      const fr = ft.getBoundingClientRect();
      const sec = document.querySelector(section).getBoundingClientRect();
      // What is above the foot: the last row of a table, or the element before the foot.
      const table = sc.querySelector('table');
      const above = table ? table.getBoundingClientRect() : ft.previousElementSibling.getBoundingClientRect();
      const row = sc.querySelector('tbody tr:last-child, .cr:last-child, details.cmp-renum')?.getBoundingClientRect();
      return {
        scrolls: sc.scrollHeight > sc.clientHeight + 1,
        footToContent: fr.top - above.bottom,
        footToFrame: f.bottom - fr.bottom,
        frameToSection: sec.bottom - f.bottom,
        frameHeight: f.height,
        rowHeight: row ? row.height : 40,
        footHeight: fr.height,
        footText: ft.innerText.replace(/\s+/g, ' ').trim(),
        footInside: fr.top >= f.top && fr.bottom <= f.bottom + 1,
      };
    },
    { frame, scroller, foot, section },
  );
}

/** Assert the rule above for one table, whichever side of it the content falls: short, the frame ends where its rows do (the foot directly under the last
 * row, the frame's bottom the foot's bottom: no empty area below the foot); tall, the frame stays inside the page and the foot stays in the frame. */
export async function expectFrameFits(page, expect, what, sel) {
  const g = await frameFit(page, sel);
  const note = `${what}: ${g.scrolls ? 'scrolls inside' : 'sized to its content'}; foot ${g.footToContent.toFixed(0)} px under the content, ${g.footToFrame.toFixed(0)} px from the frame's end, ${g.frameToSection.toFixed(0)} px above the page's end`;
  if (!g.scrolls) {
    expect(g.footToContent >= -1 && g.footToContent <= 2 && Math.abs(g.footToFrame) <= 2, `${note}: the frame ends within a row of its content and never shows empty space below the foot`);
    expect(g.footToContent < g.rowHeight && g.footToFrame < g.rowHeight, `${note}: less than a row of slack`);
  } else {
    expect(g.footToFrame >= -1 && g.footToFrame <= 2 && g.footInside, `${note}: the foot is the end of the frame`);
    expect(g.frameToSection >= -1, `${note}: the frame stays inside the page`);
  }
  return g;
}
