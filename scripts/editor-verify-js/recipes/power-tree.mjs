// The power tree of an invented board (fixtures/power_tree.nsm): a left-to-right hierarchy of
// rails and the stages between them, every edge in a free channel (none behind or through a card,
// the one that closes a cycle round under the drawing), the loads at 0 A in one line that opens on
// a click, a click on any card lighting what feeds it and what it feeds, a hover on a load with a
// link to the source, and the issues at the top.
import { whenIdle } from '../settle.mjs';
import { edgesOf, nodesOf, routeProblems, sizeSteps } from '../flow-helpers.mjs';

export const fixture = 'power_tree';

export async function run({ page, shot, step, expect, section }) {
  // The overview is drawn only while part of the drawing is out of the window (it would otherwise
  // sit on the cards): zoom in to see it, then fit again.
  const zoomIn = async (scope) => {
    for (let i = 0; i < 2; i++) await page.locator(`${scope} .zoombar button[title^="Zoom in"]`).click();
    await whenIdle(page);
  };
  const fitAgain = async (scope) => {
    await page.locator(`${scope} .zoombar button`, { hasText: 'Fit' }).click();
    await whenIdle(page);
  };
  const nodes = () => nodesOf(page, '.sec-power');
  const lit = async () => (await nodes()).filter((n) => !/\bdim\b/.test(n.cls)).map((n) => n.id).sort();

  for (const [w, h] of [[1366, 768], [2000, 1000]]) {
    await page.setViewportSize({ width: w, height: h });
    step(`the tree at ${w} x ${h}`);
    await section('power');
    await page.waitForSelector('.sec-power .pt-node');
    await whenIdle(page);
    const all = await nodes();
    const rails = all.filter((n) => n.kind === 'rail');
    const stages = all.filter((n) => n.kind === 'stage');
    expect(rails.length === 9, `a card for each of the 9 rails (${rails.map((r) => r.id).join(' ')})`);
    expect(stages.length === 9, `a stage between rails for each of the 9 converters (${stages.length})`);
    const kinds = (await page.locator('.pt-stage-title').allInnerTexts()).sort().join(' | ');
    expect(/Buck 5 V/.test(kinds) && /Boost 12 V/.test(kinds) && /LDO 3\.3 V/.test(kinds) && /Protection/.test(kinds), `the stages say their kind and what they make (${kinds})`);

    // Every route is clear of every card it does not join, and straight unless it must turn.
    const edges = await edgesOf(page, '.sec-power');
    expect(edges.length === 18, `an edge on each side of each stage (${edges.length})`);
    const at = new Map(all.map((n) => [n.id, n]));
    for (const e of edges) {
      const a = at.get(e.from);
      const b = at.get(e.to);
      if (!e.back) expect(b.x >= a.x + a.w, `${e.from} -> ${e.to} runs left to right`);
    }
    const bad = routeProblems(all, edges);
    expect(bad.length === 0, `no stretch of any edge meets a card it does not join, none is shorter than 6 px, no step is a kink under 16 px, every end is on the 8 px grid (${bad.slice(0, 3).join('; ')})`);
    const back = edges.filter((e) => e.back);
    expect(back.length >= 1, `the edge that closes the cycle goes round under the drawing (${back.map((e) => `${e.from} > ${e.to}`).join(', ')})`);
    const lowest = Math.max(...all.map((n) => n.y + n.h));
    expect(back.every((e) => Math.max(...e.points.map((p) => p[1])) > lowest), 'under every card');
    // Nothing is printed over anything: no two cards overlap.
    for (const a of all) {
      for (const b of all) {
        if (a.id < b.id && !(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y)) throw new Error(`${a.id} overlaps ${b.id}`);
      }
    }
    expect(true, 'no two cards overlap');
    const labels = await page.evaluate(() => {
      const boxes = [...document.querySelectorAll('.sec-power .pt-node')].map((n) => n.getBoundingClientRect());
      return [...document.querySelectorAll('.sec-power .pt-node .pt-stage-title, .sec-power .pt-node .pt-name')].every((t) => {
        const r = t.getBoundingClientRect();
        return boxes.some((b) => r.left >= b.left - 1 && r.right <= b.right + 1 && r.top >= b.top - 1 && r.bottom <= b.bottom + 1);
      });
    });
    expect(labels, 'every card name and stage title lies inside its card');
    const small = await page.evaluate(() =>
      Math.min(...[...document.querySelectorAll('.sec-power .pt-node *')].filter((e) => [...e.childNodes].some((c) => c.nodeType === 3 && c.textContent.trim())).map((e) => parseFloat(getComputedStyle(e).fontSize))),
    );
    expect(small >= 12, `no text smaller than 12 px before zoom (${small} px)`);
    await shot(`fit-${w}`);
  }

  step('the card of the 3.3 V rail: nominal voltage, range, total, headroom against the limit');
  const v3 = page.locator('[data-node="V3V3"]');
  const flat = (await v3.innerText()).replace(/\s+/g, ' ');
  expect(/3\.3 V/.test(flat) && /3\.135\s*\.\.\s*3\.465 V/.test(flat), `nominal voltage and the range (${flat.slice(0, 50)})`);
  expect(/Total load 2\.5 mA\s*\.\.\s*82 mA/.test(flat), `the total as min .. max in mA (${flat})`);
  expect(/Headroom 1[78]\.?\d* mA of 100 mA/.test(flat), `the headroom against the limit the regulator states (${flat})`);
  expect((await v3.locator('.pt-meter').count()) === 1, 'a bar for it');
  expect(/28 loads/.test(flat), '"28 loads" in one line');
  expect((await v3.locator('.pt-row:not(.supply)').count()) === 0, 'nothing listed until it is opened');
  const supply = (await v3.locator('.pt-row.supply').first().innerText()).replace(/\s+/g, ' ');
  expect(/−/.test(supply), `a supply is marked by an arrow and a minus sign (${supply})`);

  step('open the loads: the five heaviest at most, the 0 A loads in one line');
  await v3.locator('.pt-summary').click();
  const rows = await v3.locator('.pt-list .pt-row .pt-who').allInnerTexts();
  expect(rows.join() === 'mcu.vdd,flash.power,radio.power,buzzer.power,eeprom.power', `the five heaviest, heaviest first (${rows.join(', ')})`);
  expect((await v3.locator('.pt-fold:not(.zero)').innerText()).trim() === '+1 more', 'the rest is "+1 more"');
  const zero = await v3.locator('.pt-fold.zero').innerText();
  expect(/22 loads at 0 A/.test(zero), `"${zero.trim()}"`);
  expect(!(await v3.innerText()).includes('pa0_ref'), 'no 0 A reference pin is listed one by one');
  await shot('opened-loads');
  await v3.locator('.pt-fold.zero').click();
  expect((await v3.locator('.pt-list .pt-row').count()) === 5 + 22, 'a click on the line lists the 22');
  await v3.locator('.pt-fold.zero').click();
  expect((await v3.locator('.pt-list .pt-row').count()) === 5, 'and a second click folds them again');

  step('click the first rail (a source): it lights what it feeds, its whole downstream tree');
  const before = new Map((await edgesOf(page, '.sec-power')).map((e) => [`${e.from}>${e.to}`, e.d]));
  const total = (await nodes()).length;
  await page.locator('[data-node="VIN"] .pt-head').click();
  await page.waitForSelector('.pt-node.selected');
  expect((await lit()).length === total, `every card is downstream of VIN, so all ${total} are lit`);
  expect((await page.locator('.pt-edge.hot.down').count()) === (await page.locator('.pt-edge').count()) && (await page.locator('.pt-edge.hot.up').count()) === 0, 'every edge is lit in the downstream tint, none in the upstream one');
  expect((await page.locator('.pt-node.dim').count()) === 0, 'nothing is dimmed');
  // A tree that would need less than 75 % to fit opens at 75 % (round 5: below it the text is under 8 px), so the whole of it is in the window after Fit.
  const opened = await page.evaluate(() => Number(document.querySelector('.sec-power .zoombar span')?.textContent?.replace('%', '')));
  expect(opened >= 75, `the tree opens at 75 % or more (${opened} %)`);
  await page.locator('.sec-power .zoombar button', { hasText: 'Fit' }).click();
  await whenIdle(page);
  expect((await page.locator('.sec-power .pt-minimap').count()) === 0, 'no overview while the whole tree is in the window: it would only cover cards');
  await zoomIn('.sec-power');
  expect((await page.locator('.sec-power .pt-minimap rect[class*="hot-down"]').count()) >= total - 1, 'zoomed in, the minimap shows them');
  await fitAgain('.sec-power');
  const after = new Map((await edgesOf(page, '.sec-power')).map((e) => [`${e.from}>${e.to}`, e.d]));
  expect([...before].every(([k, d]) => after.get(k) === d), 'a highlighted edge takes the same route as before');
  const lightBad = routeProblems(await nodes(), await edgesOf(page, '.sec-power'));
  expect(lightBad.length === 0, 'and still meets no card');
  await shot('source-selected');

  step('click a leaf: it lights what feeds it, and nothing else');
  await page.locator('[data-node="VANA"] .pt-head').click();
  const leaf = await lit();
  expect(['VANA', 'V3V3', 'V5', 'VPROT', 'VIN'].every((id) => leaf.includes(id)), `the chain VIN, VPROT, V5, V3V3, VANA is lit (${leaf.length} cards)`);
  expect((await page.locator('.pt-edge.hot.down').count()) === 0 && (await page.locator('.pt-edge.hot.up').count()) >= 8, 'in the upstream tint only');
  expect((await page.locator('.pt-node.dim').count()) >= 3 && !leaf.includes('V12'), 'the rest is dimmed (V12 is not on the way)');
  await shot('leaf-selected');

  step('click a card in the middle: both directions, in two tints');
  await page.locator('[data-node="V5"] .pt-head').click();
  expect((await page.locator('.pt-edge.hot.up').count()) >= 2 && (await page.locator('.pt-edge.hot.down').count()) >= 2, 'what feeds it and what it feeds are lit in two tints');
  expect((await page.locator('.pt-node.up').count()) >= 3 && (await page.locator('.pt-node.down').count()) >= 3, 'with a tint on the cards');
  await zoomIn('.sec-power');
  expect((await page.locator('.sec-power .pt-minimap rect[class*="hot-up"]').count()) >= 3 && (await page.locator('.sec-power .pt-minimap rect[class*="hot-down"]').count()) >= 3, 'and in the minimap');
  await fitAgain('.sec-power');
  await shot('middle-selected');

  step('keys: the arrows move along the tree, Escape clears');
  await page.keyboard.press('ArrowLeft');
  expect((await page.locator('.pt-node.selected').getAttribute('data-kind')) === 'stage', 'Left goes to what feeds it (a stage)');
  await page.keyboard.press('ArrowRight');
  expect((await page.locator('.pt-node.selected').getAttribute('data-node')) === 'V5', 'Right returns to V5');
  await page.keyboard.press('ArrowRight');
  expect((await page.locator('.pt-node.selected').getAttribute('data-kind')) === 'stage', 'Right goes to a stage it feeds');
  const x = await page.locator('.pt-node.selected').evaluate((n) => n.style.left);
  await page.keyboard.press('ArrowDown');
  const same = await page.locator('.pt-node.selected').evaluate((n) => n.style.left);
  expect(x === same, 'Down is the neighbour in the same column');
  await page.keyboard.press('Escape');
  expect((await page.locator('.pt-node.selected').count()) === 0 && (await page.locator('.pt-node.dim').count()) === 0 && (await page.locator('.pt-edge.hot').count()) === 0, 'Escape clears the selection, the dimming and the lit edges');

  step('hover a load: which part and pin it is, with "Show in source"');
  await page.setViewportSize({ width: 2000, height: 1000 });
  await page.getByRole('button', { name: '1:1' }).click();
  await whenIdle(page);
  await v3.scrollIntoViewIfNeeded();
  const row = v3.locator('.pt-list .pt-row', { hasText: 'radio.power' });
  await row.hover();
  await page.waitForSelector('.pt-tip');
  const tip = await page.locator('.pt-tip').innerText();
  expect(/radio\.power/.test(tip) && /Part\s+radio/.test(tip) && /Pin\s+power/.test(tip) && /0 A\s*\.\.\s*10 mA/.test(tip), `part, pin and current (${tip.replace(/\s+/g, ' ')})`);
  await shot('hover-load');
  await page.locator('.pt-tip').getByRole('button', { name: 'Show in source' }).click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.power: Rail'));
  expect(true, 'the editor is on the line that declares the rail the radio draws from (`.power: Rail` of `Load`)');
  expect((await page.locator('.divider').count()) === 0 && (await page.locator('.pane-strip').count()) === 1, 'a click-through does not open a second pane');
  const back = page.locator('.back-btn');
  expect((await back.innerText()).includes('Power tree'), 'a Back button that names the Power tree');
  await shot('source-after-click');
  await back.click();
  await page.waitForSelector('.section.sec-power:not(.away)');
  expect((await page.locator('.pane-strip .tab.active').innerText()).includes('Power tree'), 'Back returns to the Power tree');

  step('a stage name is a link to its source');
  await page.locator('.pt-stage-name', { hasText: /^buck$/ }).click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.buck: BuckConverter'));
  expect(true, 'the editor is on the buck');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.section.sec-power:not(.away)');
  await sizeSteps({ page, expect, step, section, id: 'power' });
}
