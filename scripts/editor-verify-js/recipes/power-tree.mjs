// The power tree of an invented board (fixtures/power_tree.nsm): a left-to-right hierarchy of
// rails and the stages between them, no edge through a card, nothing printed over anything,
// the loads at 0 A in one line that opens on a click, a click on a card tracing its path from the
// source, a hover on a load with a link to the source, and the issues at the top.
export const fixture = 'power_tree';

export async function run({ page, shot, step, expect, section }) {
  const nodes = () =>
    page.evaluate(() =>
      [...document.querySelectorAll('.sec-power .pt-node')].map((n) => ({
        id: n.dataset.node,
        kind: n.dataset.kind,
        x: parseFloat(n.style.left),
        y: parseFloat(n.style.top),
        w: parseFloat(n.style.width),
        h: parseFloat(n.style.height),
        dim: n.classList.contains('dim'),
        selected: n.classList.contains('selected'),
      })),
    );

  for (const [w, h] of [[1366, 768], [2000, 1000]]) {
    await page.setViewportSize({ width: w, height: h });
    step(`the tree at ${w} x ${h}`);
    await section('power');
    await page.waitForSelector('.sec-power .pt-node');
    await page.waitForTimeout(400);
    const all = await nodes();
    const rails = all.filter((n) => n.kind === 'rail');
    const stages = all.filter((n) => n.kind === 'stage');
    expect(rails.length === 8, `a card for each of the 8 rails (${rails.map((r) => r.id).join(' ')})`);
    expect(stages.length === 7, `a stage between rails for each of the 7 converters (${stages.length})`);
    const kinds = (await page.locator('.pt-stage-title').allInnerTexts()).sort().join(' | ');
    expect(/Buck 5 V/.test(kinds) && /Boost 12 V/.test(kinds) && /LDO 3\.3 V/.test(kinds) && /Protection/.test(kinds), `the stages say their kind and what they make (${kinds})`);

    // Every edge goes left to right, and none runs through a card it does not join.
    const edges = await page.evaluate(() =>
      [...document.querySelectorAll('.sec-power .pt-edge')].map((p) => {
        const len = p.getTotalLength();
        const pts = [];
        for (let t = 6; t < len - 6; t += 3) {
          const q = p.getPointAtLength(t);
          pts.push([q.x, q.y]);
        }
        return { from: p.dataset.from, to: p.dataset.to, pts };
      }),
    );
    expect(edges.length === 14, `an edge on each side of each stage (${edges.length})`);
    const at = new Map(all.map((n) => [n.id, n]));
    for (const e of edges) {
      const a = at.get(e.from);
      const b = at.get(e.to);
      expect(b.x >= a.x + a.w, `${e.from} -> ${e.to} runs left to right`);
      for (const n of all) {
        if (n.id === e.from || n.id === e.to) continue;
        const hit = e.pts.some(([x, y]) => x > n.x - 1 && x < n.x + n.w + 1 && y > n.y - 1 && y < n.y + n.h + 1);
        if (hit) throw new Error(`the edge ${e.from} -> ${e.to} runs through ${n.id}`);
      }
    }
    expect(true, 'no edge runs through a card');
    // Nothing is printed over anything: no two cards overlap, so no label sits on another's text.
    for (const a of all) {
      for (const b of all) {
        if (a.id < b.id && !(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y)) throw new Error(`${a.id} overlaps ${b.id}`);
      }
    }
    expect(true, 'no two cards overlap, and the stage names are inside their boxes, not on the edges');
    const labels = await page.evaluate(() => {
      const boxes = [...document.querySelectorAll('.sec-power .pt-node')].map((n) => n.getBoundingClientRect());
      return [...document.querySelectorAll('.sec-power .pt-node .pt-stage-title, .sec-power .pt-node .pt-name')].every((t) => {
        const r = t.getBoundingClientRect();
        return boxes.some((b) => r.left >= b.left - 1 && r.right <= b.right + 1 && r.top >= b.top - 1 && r.bottom <= b.bottom + 1);
      });
    });
    expect(labels, 'every card name and stage title lies inside its card');
    const small = await page.evaluate(() =>
      Math.min(...[...document.querySelectorAll('.sec-power .pt-node *')].filter((e) => e.childNodes.length && [...e.childNodes].some((c) => c.nodeType === 3 && c.textContent.trim())).map((e) => parseFloat(getComputedStyle(e).fontSize))),
    );
    expect(small >= 12, `no text smaller than 12 px before zoom (${small} px)`);
    await shot(`fit-${w}`);
  }

  step('the card of the 3.3 V rail: nominal voltage, range, total, headroom against the limit');
  const v3 = page.locator('[data-node="V3V3"]');
  const text = (await v3.innerText()).replace(/[ \t]+/g, ' ');
  const flat = text.replace(/\s+/g, ' ');
  expect(/3\.3 V/.test(flat) && /3\.135\s*\.\.\s*3\.465 V/.test(flat), `nominal voltage and the range (${flat.slice(0, 50)})`);
  expect(/Total load/.test(flat) && /2\.5 mA\s*\.\.\s*89 mA/.test(flat), `the total as min .. max in mA (${flat})`);
  expect(/Headroom 11 mA of 100 mA/.test(flat), 'the headroom against the limit the regulator states');
  expect((await v3.locator('.pt-meter').count()) === 1, 'a bar for it');
  expect(/28 loads/.test(flat), `one summary line, "28 loads" (${text.split('\n').filter((l) => /loads/.test(l)).join(' / ')})`);
  expect((await v3.locator('.pt-row:not(.supply)').count()) === 0, 'nothing listed until it is opened');
  const supply = await v3.locator('.pt-row.supply').first().innerText();
  expect(/ldo\.vout/.test(supply) && /−/.test(supply), `a supply is marked by an arrow and a minus sign (${supply.replace(/\s+/g, ' ')})`);

  step('open the loads: the five heaviest at most, the 0 A loads in one line');
  await v3.locator('.pt-summary').click();
  const rows = await v3.locator('.pt-list .pt-row .pt-who').allInnerTexts();
  expect(rows.join() === 'mcu.vdd,radio.power,flash.power,gps.power,buzzer.power', `the five heaviest, heaviest first (${rows.join(', ')})`);
  expect(rows.length === 5, `five rows, not thirty (${rows.length})`);
  expect((await v3.locator('.pt-fold:not(.zero)').innerText()).trim() === '+1 more', 'the rest is "+1 more"');
  const zero = await v3.locator('.pt-fold.zero').innerText();
  expect(/22 loads at 0 A/.test(zero), `"${zero.trim()}"`);
  expect(!(await v3.innerText()).includes('pa0_ref'), 'no 0 A reference pin is listed one by one');
  await shot('opened-loads');
  await v3.locator('.pt-fold.zero').click();
  expect((await v3.locator('.pt-list .pt-row').count()) === 5 + 22, 'a click on the line lists the 22');
  await v3.locator('.pt-fold.zero').click();
  expect((await v3.locator('.pt-list .pt-row').count()) === 5, 'and a second click folds them again');

  step('click a card: its path from the source is lit, the rest dimmed');
  await v3.locator('.pt-head').click();
  await page.waitForSelector('.pt-node.selected');
  const lit = (await nodes()).filter((n) => !n.dim).map((n) => n.id).sort();
  expect(lit.join('|') === ['V3V3', 'V5', 'VIN', 'VPROT', 'stage:buck (BuckConverter)', 'stage:ldo (LdoStage)', 'stage:protect (ProtectionStage)'].sort().join('|'), `the chain VIN, protection, VPROT, buck, V5, LDO, V3V3 (${lit.join(', ')})`);
  expect((await page.locator('.pt-edge.hot').count()) === 6, 'six lit edges');
  expect((await page.locator('.pt-node.dim').count()) === 8, 'eight dimmed nodes');
  await shot('path-from-source');

  step('keys: the arrows move along the tree, Escape clears');
  await page.keyboard.press('ArrowLeft');
  expect((await page.locator('.pt-node.selected').getAttribute('data-node')) === 'stage:ldo (LdoStage)', 'Left goes to what feeds it (the LDO)');
  await page.keyboard.press('ArrowLeft');
  expect((await page.locator('.pt-node.selected').getAttribute('data-node')) === 'V5', 'Left again, to V5');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  expect((await page.locator('.pt-node.selected').getAttribute('data-node')) === 'V3V3', 'Right twice, back to V3V3');
  await page.keyboard.press('ArrowUp');
  expect((await page.locator('.pt-node.selected').getAttribute('data-node')) === 'V12', 'Up is the rail above in the same column');
  await page.keyboard.press('Escape');
  expect((await page.locator('.pt-node.selected').count()) === 0 && (await page.locator('.pt-node.dim').count()) === 0, 'Escape clears the selection and the dimming');

  step('hover a load: which part and pin it is, with "Show in source"');
  await page.setViewportSize({ width: 2000, height: 1000 });
  await page.getByRole('button', { name: '1:1' }).click();
  await page.waitForTimeout(300);
  await v3.scrollIntoViewIfNeeded();
  const row = v3.locator('.pt-list .pt-row', { hasText: 'radio.power' });
  await row.hover();
  await page.waitForSelector('.pt-tip');
  const tip = await page.locator('.pt-tip').innerText();
  expect(/radio\.power/.test(tip) && /Part\s+radio/.test(tip) && /Pin\s+power/.test(tip) && /0 A\s*\.\.\s*20 mA/.test(tip), `part, pin and current (${tip.replace(/\s+/g, ' ')})`);
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
  await page.locator('.pt-stage-name', { hasText: 'buck' }).click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.buck: BuckConverter'));
  expect(true, 'the editor is on the buck');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.section.sec-power:not(.away)');
}
