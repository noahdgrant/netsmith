// Two panes side by side: the Split right button, a drag of a tab onto a half of the window, the
// right-click menu, the divider (drag, keys, double click), closing the pane, the layout kept across
// a reload, and the single pane in a narrow window.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  const strips = () => page.evaluate(() => [...document.querySelectorAll('.pane-strip')].map((s) => s.querySelector('.tab.active')?.dataset.section));
  const share = () => page.evaluate(() => {
    const a = document.querySelector('.sec-schematic').getBoundingClientRect();
    return a.width;
  });
  await page.setViewportSize({ width: 2000, height: 1000 });
  await page.selectOption('.boardpick select', 'TwoBoards/led');

  step('Split right: the schematic stays, the source opens beside it');
  await page.waitForSelector('.schematic');
  await page.getByRole('button', { name: /Split right/ }).click();
  await page.waitForSelector('.divider');
  expect((await strips()).join() === 'schematic,source', `two panes (${(await strips()).join()})`);
  expect((await page.getByRole('button', { name: /Close pane/ }).count()) === 1, 'a Close pane button on the second pane only');
  await shot('split');

  step('the tab whose section is in the other pane has a two-pane icon, not a dimmed tab with a dot');
  const mark = page.locator('.pane-strip').nth(0).locator('[data-section="source"] .elsewhere-mark');
  expect((await mark.count()) === 1, 'one mark on the Source tab of the first pane');
  expect((await mark.getAttribute('aria-label')) === 'Open in the other pane: click to move it here', 'its aria-label says what a click does');
  expect((await mark.getAttribute('title')) === 'Open in the other pane: click to move it here', 'and its tooltip too');
  expect((await mark.locator('svg').count()) === 1, 'it is an icon');
  const tab = page.locator('.pane-strip').nth(0).locator('[data-section="source"]');
  expect((await tab.evaluate((t) => getComputedStyle(t).opacity)) === '1', 'the tab is not dimmed');
  expect((await tab.evaluate((t) => getComputedStyle(t, '::after').content)) === 'none' || (await tab.evaluate((t) => getComputedStyle(t, '::after').content)) === 'normal', 'there is no little triangle after it');
  const markBox = await mark.boundingBox();
  expect(markBox.width >= 13 && markBox.height >= 13, `the icon is clearly visible (${markBox.width} x ${markBox.height} px)`);
  expect((await page.locator('.pane-strip').nth(1).locator('[data-section="schematic"] .elsewhere-mark').count()) === 1, 'the second pane marks the schematic the same way');
  await page.locator('.pane-strip').nth(0).locator('[data-section="power"]').hover();
  await shot('other-pane-marker');

  step('the divider: drag, a minimum width, keys, double click');
  const d = await page.locator('.divider').boundingBox();
  const before = await share();
  await page.mouse.move(d.x + 3, 500);
  await page.mouse.down();
  await page.mouse.move(d.x - 300, 500, { steps: 6 });
  await page.mouse.up();
  const dragged = await share();
  expect(Math.abs(dragged - (before - 300)) < 8, `the left pane follows the drag (${Math.round(dragged)} px, from ${Math.round(before)})`);
  await page.mouse.move(d.x - 300 + 3, 500);
  await page.mouse.down();
  await page.mouse.move(5, 500, { steps: 6 });
  await page.mouse.up();
  expect((await share()) >= 378, `the left pane keeps its minimum (${Math.round(await share())} px)`);
  await page.locator('.divider').focus();
  await page.keyboard.press('ArrowRight');
  expect((await share()) > 400, 'an arrow key moves the divider');
  await page.dblclick('.divider');
  expect(Math.abs((await share()) - before) < 4, 'a double click makes the panes equal (as they were when the split opened)');

  step('drag the Source tab onto the left half: the panes swap');
  const src = await page.locator('.pane-strip').nth(1).locator('.tab', { hasText: 'Source' }).boundingBox();
  await page.mouse.move(src.x + 10, src.y + 10);
  await page.mouse.down();
  await page.mouse.move(src.x - 300, 400, { steps: 8 });
  await page.waitForSelector('.drop-zone');
  await shot('dragging-a-tab');
  await page.mouse.move(300, 400, { steps: 4 });
  await page.mouse.up();
  expect((await strips()).join() === 'source,schematic', `swapped (${(await strips()).join()})`);

  step('the same section is never in both panes: choosing the other pane\'s section moves it');
  await page.locator('.pane-strip').nth(0).locator('[data-section="schematic"]').click();
  expect((await strips()).join() === 'schematic,source', `swapped by a click (${(await strips()).join()})`);

  step('the layout is kept across a reload');
  await page.reload();
  await page.waitForSelector('.divider');
  expect((await strips()).join() === 'schematic,source', 'two panes again');

  step('close the second pane');
  await page.getByRole('button', { name: /Close pane/ }).click();
  expect((await strips()).join() === 'schematic', 'one pane');

  step('the right-click menu: open the BOM on the left of the window');
  await page.locator('.pane-strip [data-section="bom"]').click({ button: 'right' });
  await page.getByRole('menuitem', { name: /open on the left/i }).click();
  expect((await strips()).join() === 'bom,schematic', `the BOM left of the schematic (${(await strips()).join()})`);
  await page.keyboard.press('Control+\\');
  expect((await strips()).length === 1, 'Ctrl+\\ closes the second pane');
  await page.keyboard.press('Control+\\');
  expect((await strips()).length === 2, 'and splits again');

  step('drag a tab onto the right half of a one-pane window');
  await page.getByRole('button', { name: /Close pane/ }).click();
  const pt = await page.locator('.pane-strip [data-section="power"]').boundingBox();
  await page.mouse.move(pt.x + 10, pt.y + 10);
  await page.mouse.down();
  await page.mouse.move(1500, 500, { steps: 8 });
  await page.waitForSelector('.drop-zone');
  await shot('drop-zone');
  await page.mouse.up();
  expect((await strips()).join().endsWith('power'), `the power tree on the right (${(await strips()).join()})`);

  step('a narrow window shows one pane and keeps the layout');
  await page.setViewportSize({ width: 1366, height: 768 });
  await page.waitForTimeout(300);
  expect((await page.locator('.divider').count()) === 0 && (await strips()).length === 1, 'one pane at 1366 px');
  const tip = await page.getByRole('button', { name: /Split right/ }).getAttribute('title');
  expect(tip.includes('1366') && tip.includes('saved split comes back'), `the Split right tooltip says why (${tip.slice(-120)})`);
  await shot('narrow');
  await page.setViewportSize({ width: 2000, height: 1000 });
  await page.waitForTimeout(300);
  expect((await strips()).length === 2, 'the split is back in a wide window');

  await page.setViewportSize({ width: 2000, height: 1000 });
  await page.waitForTimeout(300);

  step('a click-through changes the pane that was clicked, and makes no second pane');
  await page.getByRole('button', { name: /Close pane/ }).click();
  await section('power');
  await page.waitForSelector('.pt-rail');
  await page.locator('.pt-summary').evaluateAll((all) => all.forEach((b) => b.getAttribute('aria-expanded') !== 'true' && b.click()));
  await page.locator('.pt-list .pt-row').first().click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  expect((await strips()).join() === 'source' && (await page.locator('.divider').count()) === 0, `one pane, now the source (${(await strips()).join()})`);
  expect((await page.locator('.back-btn').count()) === 1, 'with Back');
  await page.locator('.back-btn').click();
  await page.waitForSelector('.section.sec-power:not(.away)');
  expect((await strips()).join() === 'power', 'Back puts the Power tree back');

  step('a split saved before it was recorded as the person\'s own is not opened into');
  await page.evaluate(() => localStorage.setItem('netsmith.layout.v1', JSON.stringify({ panes: ['power', 'bom'], split: 0.5, focus: 0 })));
  await page.reload();
  await page.waitForSelector('.divider');
  await page.waitForSelector('.pt-rail');
  await page.locator('.pt-summary').evaluateAll((all) => all.forEach((b) => b.getAttribute('aria-expanded') !== 'true' && b.click()));
  await page.locator('.pt-list .pt-row').first().click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  expect((await strips()).join() === 'source,bom', `the clicked pane changed, the other stayed (${(await strips()).join()})`);
  expect((await page.locator('.pane-notice').count()) === 0, 'and there is no notice');
  await page.locator('.back-btn').click();
  expect((await strips()).join() === 'power,bom', 'Back restores it');

  step('in a split the person made, the other pane is used, and the editor says so once');
  await page.evaluate(() => localStorage.setItem('netsmith.layout.v1', JSON.stringify({ panes: ['power', 'bom'], split: 0.5, focus: 0, chosen: true })));
  await page.reload();
  await page.waitForSelector('.divider');
  await page.waitForSelector('.pt-rail');
  await page.locator('.pt-summary').evaluateAll((all) => all.forEach((b) => b.getAttribute('aria-expanded') !== 'true' && b.click()));
  await page.locator('.pt-list .pt-row').first().click();
  await page.waitForSelector('.pane-notice');
  expect((await strips()).join() === 'power,source', `the source opened beside the power tree (${(await strips()).join()})`);
  const notice = await page.locator('.pane-notice').innerText();
  expect(/Opened in a second pane\. Close pane returns to one\./.test(notice), `the notice says it (${notice.replace(/\s+/g, ' ')})`);
  expect((await page.locator('.pane-notice').getByRole('button', { name: 'Close pane' }).count()) === 1, 'with a Close pane button right there');
  await shot('second-pane-notice');
  await page.locator('.pane-notice').getByRole('button', { name: 'Got it' }).click();
  expect((await page.locator('.pane-notice').count()) === 0, 'Got it dismisses it');
  await page.locator('.back-btn').click();
  await page.locator('.pt-list .pt-row').first().click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  expect((await page.locator('.pane-notice').count()) === 0, 'and it does not come back');
  await page.locator('.back-btn').click();
}
