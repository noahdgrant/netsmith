// The build panel's orders: pick one, run it, read the summary, download its files.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  step('open the Build tab');
  await section('build');
  await page.waitForSelector('.order-pick select');
  expect((await page.locator('.order-pick .boardpick select').count()) === 1, 'the order is picked in the top bar\'s picker box, not a bare browser select');
  expect((await page.locator('.order-pick .boardpick select').evaluate((e) => getComputedStyle(e).appearance)) === 'none', 'drawn by the page, with its own chevrons');
  const listed = await page.locator('.order-pick select option').allInnerTexts();
  expect(listed.includes('demo'), `the order demo is listed (${listed.join(', ')})`);
  const rows = await page.locator('.order-boards tbody tr').evaluateAll((trs) => trs.map((tr) => [...tr.children].map((c) => c.textContent.trim())));
  expect(rows.some(([b, q]) => b === 'TwoBoards/led' && q === '5') && rows.some(([b, q]) => b === 'TwoBoards/power' && q === '2'), `its boards and quantities are shown (${JSON.stringify(rows)})`);
  await page.locator('.order-pick').scrollIntoViewIfNeeded();
  await shot('order-picked');

  step('make the order');
  await page.locator('button', { hasText: 'Make order' }).click();
  await page.waitForSelector('.build .chip:has-text("order made"), .build .chip:has-text("stopped")', { timeout: 240000 });
  const text = await page.locator('.build').innerText();
  expect(text.includes('order made'), `the order was made: ${text.slice(-400)}`);
  expect(text.includes('order demo: 2 board designs, 7 boards in all'), 'the summary counts the boards');
  expect(text.includes('TwoBoards/led x5') && text.includes('TwoBoards/power x2'), 'each board with its quantity');
  expect(text.includes('bill of materials') && text.includes('before you order'), 'the combined BOM and the notes');
  const links = await page.locator('.downloads a').allInnerTexts();
  expect(links.includes('order.txt') && links.includes('order-bom.csv'), `download links (${links.join(', ')})`);
  await page.locator('.order-summary').scrollIntoViewIfNeeded();
  await page.locator('.build .result .chip:has-text("order made")').scrollIntoViewIfNeeded();
  await shot('order-made');
}
