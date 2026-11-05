// A big board drawn by module: one sheet at a time, large; an "All sheets" overview with a box
// for each module instance; the sheets by tab, button, key and the Modules list; the zoom of
// each sheet remembered; nets between modules are stubs with names.
import { activeSheet, openSheet, sheetNames, zoomPercent } from '../schematic-helpers.mjs';

export const fixture = 'big';

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic');
  step('a board with many sheets opens on one sheet, filling the view');
  await page.waitForSelector('.sheetpick .crumb-sheet');
  const listed = await sheetNames(page);
  expect(listed.length >= 26, `All sheets, each of the 24 module instances and the rest are in the picker (${listed.length})`);
  expect((await activeSheet(page)) === 'light1', 'the first sheet, in source order, is open');
  expect((await page.locator('.sheetnav .sl-row').count()) >= 26 || (await page.locator('.sheetnav').count()) === 0, 'the navigator lists them too');
  const host = await page.locator('.schematic-host').boundingBox();
  const sheet = await page.locator('.schematic .module-sheet[data-module="light1"] .module-frame').first().boundingBox();
  expect(sheet.width > host.width * 0.3 || sheet.height > host.height * 0.3, `the sheet is large in the view (${Math.round(sheet.width)} x ${Math.round(sheet.height)} of ${Math.round(host.width)} x ${Math.round(host.height)} px)`);
  await shot('one-sheet');

  step('the keys step through the sheets: ] and [ (the arrows pan)');
  await page.keyboard.press(']');
  expect((await activeSheet(page)) === 'light2', 'the second sheet');
  await page.keyboard.press(']');
  await page.keyboard.press('[');
  await page.keyboard.press('[');
  expect((await activeSheet(page)) === 'light1', 'back on the first');
  await page.getByRole('button', { name: 'Next sheet' }).click();
  expect((await activeSheet(page)) === 'light2', 'the Next button');

  step('the overview shows every sheet; a click on a box opens it');
  await openSheet(page, 'All sheets');
  await page.waitForSelector('.ov-card');
  const sheets = await page.locator('.ov-card').count();
  expect(sheets >= 24, `a card for each of the 24 module instances and one for the rest (${sheets})`);
  const titles = await page.locator('.ov-card .ov-head').allInnerTexts();
  expect(titles.some((t) => /stage1\s*LdoStage/.test(t.replace(/\n/g, ' '))), 'a card titled with the instance and its module');
  const order = await page.locator('.ov-card .ov-name').allInnerTexts();
  expect(order[0] === 'light1' && order.indexOf('light2') === 1, `the cards are in the order of the source, not alphabetical (${order.slice(0, 4).join(', ')})`);
  const shape = await page.evaluate(() => {
    const svg = document.querySelector('svg.schematic');
    const [, , w, h] = svg.getAttribute('data-bounds').split(' ').map(Number);
    return w / h;
  });
  expect(shape > 0.5 && shape < 3, `the drawing is not a strip (${shape.toFixed(2)} wide for 1 high)`);
  await shot('all-modules');
  await page.locator('.ov-card', { has: page.locator('.ov-name', { hasText: /^pulls2$/ }) }).click();
  expect((await activeSheet(page)) === 'pulls2', 'a click on the card of pulls2 opened its sheet');

  step('a rail between modules is a supply symbol with its name in each, not a wire across the page');
  await openSheet(page, 'All sheets');
  const symbols = await page.locator('.schematic .power.supply[data-net="VIN"]').count();
  expect(symbols >= 8, `the VIN supply symbol in each regulator stage (${symbols})`);
  const names = await page.locator('.schematic .power.supply[data-net="VIN"] text', { hasText: 'VIN' }).count();
  expect(names >= 8, 'with its name beside it');
  const wires = await page.locator('.schematic .wire[data-net="VIN"]').count();
  expect(wires >= 8, 'and a short wire from the pin to each');

  step('the sheet navigator opens a sheet, and filters');
  if (!(await page.locator('.sheetnav').count())) await page.getByRole('button', { name: 'Sheet navigator' }).click();
  const rows = page.locator('.sheetnav .sl-row');
  expect((await rows.count()) >= 24, 'an entry for each module');
  await page.locator('.sheetnav .sl-filter').fill('stage3');
  expect((await rows.count()) === 1, 'the filter leaves the one sheet');
  await rows.first().click();
  expect((await activeSheet(page)) === 'stage3', 'stage3 opened');
  await page.locator('.sheetnav .sl-filter').fill('');

  step('a sheet remembers its zoom; Fit, F and 0 fit it again; the zoom is true scale');
  const zoom = () => zoomPercent(page);
  const area = await page.locator('.schematic-host').boundingBox();
  const opened = await zoom();
  await page.mouse.move(area.x + area.width / 2, area.y + area.height / 2);
  await page.mouse.wheel(0, -500);
  await page.waitForTimeout(300);
  const zoomed = await zoom();
  expect(zoomed > opened, `zoomed in (${opened}% to ${zoomed}%)`);
  await page.getByRole('button', { name: 'Next sheet' }).click();
  await page.getByRole('button', { name: 'Previous sheet' }).click();
  expect(Math.abs((await zoom()) - zoomed) <= 1, `stage3 is as it was left (${await zoom()}%)`);
  await page.locator('.schematic-host').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('f');
  const fitted = await zoom();
  expect(fitted < zoomed, `the key F fits (${fitted}%)`);
  await page.keyboard.press('Shift+1');
  expect((await zoom()) === 100, 'Shift+1 is 100%, the drawing at its size on paper');
  await page.keyboard.press('0');
  expect((await zoom()) === fitted, 'the key 0 fits');
  await shot('sheet-fit');

  step('a drag pans');
  const before = await page.locator('.canvas').first().evaluate((e) => e.style.transform);
  await page.mouse.move(area.x + area.width / 2, area.y + area.height / 2);
  await page.mouse.down();
  await page.mouse.move(area.x + area.width / 2 + 80, area.y + area.height / 2 + 40, { steps: 4 });
  await page.mouse.up();
  const after = await page.locator('.canvas').first().evaluate((e) => e.style.transform);
  expect(before !== after, 'a drag moved the drawing');
}
