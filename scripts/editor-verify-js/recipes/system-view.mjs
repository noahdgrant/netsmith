// The system view of an invented board (fixtures/system_view.nsm): 13 modules from the input
// connector on the left to the processor on the right, each a card with a title and a two-column
// table of ports that never wraps and never overflows, long tables folded into "22 ports", the
// wires between two modules grouped ("10 nets", opening on a click), every wire in a free channel,
// every label on a clear spot, a click lighting what feeds a module and what it feeds.
import { edgesOf, nodesOf, routeProblems } from '../flow-helpers.mjs';

export const fixture = 'system_view';

export async function run({ page, shot, step, expect, section }) {
  const nodes = () => nodesOf(page, '.sec-system');
  const lit = async () => (await nodes()).filter((n) => !/\bdim\b/.test(n.cls)).map((n) => n.id).sort();

  for (const [w, h] of [[1366, 768], [2000, 1000]]) {
    await page.setViewportSize({ width: w, height: h });
    step(`the drawing at ${w} x ${h}`);
    await section('system');
    await page.waitForSelector('.sec-system .pt-node');
    await page.waitForTimeout(400);
    const all = await nodes();
    expect(all.length === 13, `a card for each of the 13 modules (${all.map((n) => n.id).join(' ')})`);

    const edges = await edgesOf(page, '.sec-system');
    expect(edges.length === 17, `one wire for each pair of connected modules, however many nets it carries (${edges.length})`);
    const at = new Map(all.map((n) => [n.id, n]));
    for (const e of edges) {
      const a = at.get(e.from);
      const b = at.get(e.to);
      if (!e.back) expect(b.x >= a.x + a.w, `${e.from} -> ${e.to} runs left to right`);
    }
    const flow = (id) => at.get(id).x;
    expect(flow('inlet') < flow('guard') && flow('guard') < flow('buck5') && flow('buck5') < flow('ldo') && flow('ldo') < flow('mcu'), 'power flows left to right: inlet, guard, buck5, ldo, mcu');
    expect(['flash', 'imu', 'adc', 'modem', 'gnss', 'debug'].every((id) => flow(id) < flow('mcu')), 'the processor, which they all connect to, is the rightmost');
    const bad = routeProblems(all, edges);
    expect(bad.length === 0, `no stretch of any wire meets a card it does not join, none is shorter than 6 px, every end is on the 8 px grid (${bad.slice(0, 3).join('; ')})`);

    // Labels: on a clear spot, none over another or over a card.
    const labels = await page.evaluate(() => [...document.querySelectorAll('.sec-system .sy-label')].map((t) => { const r = t.getBoundingClientRect(); return { text: t.textContent, x: r.left, y: r.top, w: r.width, h: r.height }; }));
    const cards = await page.evaluate(() => [...document.querySelectorAll('.sec-system .pt-node')].map((n) => { const r = n.getBoundingClientRect(); return { id: n.dataset.node, x: r.left, y: r.top, w: r.width, h: r.height }; }));
    expect(labels.length === 17, `a label on each wire (${labels.length})`);
    const apart = (a, b) => a.x + a.w <= b.x + 0.5 || b.x + b.w <= a.x + 0.5 || a.y + a.h <= b.y + 0.5 || b.y + b.h <= a.y + 0.5;
    for (let i = 0; i < labels.length; i++) {
      for (let j = i + 1; j < labels.length; j++) if (!apart(labels[i], labels[j])) throw new Error(`the labels "${labels[i].text}" and "${labels[j].text}" overprint`);
      for (const c of cards) if (!apart(labels[i], c)) throw new Error(`the label "${labels[i].text}" is over the card ${c.id}`);
    }
    expect(true, 'no label overprints another or lies on a card');

    // The ports tables: one line a row, nothing outside its card.
    const tables = await page.evaluate(() => {
      const problems = [];
      for (const card of document.querySelectorAll('.sec-system .pt-node')) {
        const c = card.getBoundingClientRect();
        for (const row of card.querySelectorAll('.sy-row')) {
          const r = row.getBoundingClientRect();
          const k = parseFloat(getComputedStyle(card.closest('.canvas')).transform.split(',')[0].replace('matrix(', ''));
          if (Math.abs(r.height / k - 20) > 0.6) problems.push(`${card.dataset.node}: a row ${(r.height / k).toFixed(1)} px tall`);
          for (const cell of row.children) {
            const q = cell.getBoundingClientRect();
            if (q.right > c.right + 1 || q.left < c.left - 1 || q.bottom > c.bottom + 1) problems.push(`${card.dataset.node}: ${cell.textContent} outside the card`);
            if (q.height / k > 24) problems.push(`${card.dataset.node}: ${cell.textContent} wraps`);
          }
        }
      }
      return problems;
    });
    expect(tables.length === 0, `every port row is one line and inside its card (${tables.slice(0, 3).join('; ')})`);
    const small = await page.evaluate(() => Math.min(...[...document.querySelectorAll('.sec-system .pt-node *, .sec-system .sy-label')].filter((e) => [...e.childNodes].some((c) => c.nodeType === 3 && c.textContent.trim())).map((e) => parseFloat(getComputedStyle(e).fontSize))));
    expect(small >= 12, `no text smaller than 12 px before zoom (${small} px)`);
    await shot(`fit-${w}`);
  }

  step('long tables are folded: "22 ports" on the processor, "10 ports" on the modem');
  const mcu = page.locator('[data-node="mcu"]');
  const modem = page.locator('[data-node="modem"]');
  expect(/22 ports/.test(await mcu.locator('.sy-fold').innerText()) && (await mcu.locator('.sy-row').count()) === 0, 'the processor shows one line, not 22 rows');
  expect(/10 ports/.test(await modem.locator('.sy-fold').innerText()) && (await modem.locator('.sy-row').count()) === 0, 'the modem shows one line, not 10');
  expect((await page.locator('[data-node="ldo"] .sy-row').count()) === 2, 'a short table (the linear regulator) is shown in full');
  await mcu.locator('.sy-fold').click();
  expect((await mcu.locator('.sy-row').count()) === 22, 'a click lists the 22 ports');
  const names = await mcu.locator('.sy-pname').allInnerTexts();
  expect(names.includes('vdd') && names.includes('modem_key') && names.includes('uart0'), `with their names (${names.slice(0, 4).join(', ')} ...)`);
  const vdd = (await mcu.locator('.sy-row', { hasText: 'vdd' }).innerText()).replace(/\s+/g, ' ');
  expect(/3\.3\d* V/.test(vdd), `and a value in the second column (${vdd})`);
  const sizes = await mcu.evaluate((n) => ({ h: parseFloat(n.style.height), rows: n.querySelectorAll('.sy-row').length }));
  expect(sizes.h >= 22 * 20, `the card is as tall as its table (${sizes.h} px)`);
  await shot('ports-opened');
  await mcu.locator('.sy-fold').click();
  expect((await mcu.locator('.sy-row').count()) === 0, 'and a second click folds them');

  step('the wires between two modules are one wire with a count, which opens on a click');
  const group = page.locator('.sy-label.group', { hasText: '10 nets' });
  expect((await group.count()) === 1, 'the processor and the modem share ten nets: one wire "10 nets"');
  expect((await page.locator('.sy-label.group', { hasText: '5 nets' }).count()) === 3, 'three more with five (the flash, the motion sensor and the ADC)');
  await group.click();
  await page.waitForSelector('.sy-tip');
  const items = await page.locator('.sy-tip li').allInnerTexts();
  expect(items.length === 10, `a click lists the ten (${items.slice(0, 3).join(', ')} ...)`);
  expect((await page.locator('.sy-tip').innerText()).includes('modem'), 'under the names of the two modules');
  await shot('wire-opened');
  await page.keyboard.press('Escape');
  await page.mouse.click(20, 200);

  step('click the first module (the connector): what it feeds, its whole downstream, in one tint');
  const before = new Map((await edgesOf(page, '.sec-system')).map((e) => [`${e.from}>${e.to}`, e.d]));
  await page.locator('[data-node="inlet"] .sy-head').click();
  await page.waitForSelector('.pt-node.selected');
  const total = (await nodes()).length;
  const down = await lit();
  expect(down.length >= 9 && ['guard', 'buck5', 'ldo', 'mcu', 'boost12', 'lamp'].every((id) => down.includes(id)), `the guard, the three regulators and what they feed are lit (${down.length} of ${total})`);
  expect((await page.locator('.pt-edge.hot.down').count()) >= 8 && (await page.locator('.pt-edge.hot.up').count()) === 0, 'only in the downstream tint');
  expect((await page.locator('.pt-minimap rect[class*="hot-down"]').count()) >= 9, 'and in the minimap');
  const after = new Map((await edgesOf(page, '.sec-system')).map((e) => [`${e.from}>${e.to}`, e.d]));
  expect([...before].every(([k, d]) => after.get(k) === d), 'a lit wire takes the route it had');
  await shot('source-selected');

  step('click a module in the middle: both directions in two tints');
  await page.locator('[data-node="buck5"] .sy-head').click();
  expect((await page.locator('.pt-edge.hot.up').count()) >= 2 && (await page.locator('.pt-edge.hot.down').count()) >= 3, 'what feeds it and what it feeds');
  expect((await page.locator('.pt-node.dim').count()) >= 3, 'the rest is dimmed');
  await shot('middle-selected');

  step('click the processor: everything that reaches it is lit, nothing after it');
  await page.locator('[data-node="mcu"] .sy-head').click();
  expect((await page.locator('.pt-edge.hot.down').count()) === 0 && (await page.locator('.pt-edge.hot.up').count()) >= 8, 'upstream tint only');
  await page.keyboard.press('Escape');
  expect((await page.locator('.pt-node.selected').count()) === 0 && (await page.locator('.pt-edge.hot').count()) === 0 && (await page.locator('.pt-node.dim').count()) === 0, 'Escape clears it all');

  step('the module name is a link to its source, and Back returns');
  await page.locator('[data-node="mcu"] .sy-name').click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.mcu: Processor'));
  expect(true, 'the editor is on `.mcu: Processor`');
  expect((await page.locator('.divider').count()) === 0, 'in the same pane');
  expect((await page.locator('.back-btn').innerText()).includes('System'), 'a Back button that names the System');
  await page.locator('.back-btn').click();
  await page.waitForSelector('.section.sec-system:not(.away)');
  expect(true, 'Back returns to the System');

  step('"List all ports" opens every long table');
  await page.getByRole('button', { name: 'List all ports' }).click();
  expect((await page.locator('[data-node="mcu"] .sy-row').count()) === 22 && (await page.locator('[data-node="modem"] .sy-row').count()) === 10, 'the processor and the modem');
  expect(routeProblems(await nodes(), await edgesOf(page, '.sec-system')).length === 0, 'and the wires still meet no card');
  await shot('all-ports');
}
