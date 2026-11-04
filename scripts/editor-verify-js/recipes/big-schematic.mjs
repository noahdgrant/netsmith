// A big board drawn by module: one sheet at a time, large; an "All sheets" overview with a box
// for each module instance; the sheets by tab, button, key and the Modules list; the zoom of
// each sheet remembered; nets between modules are stubs with names.
export const fixture = 'big';

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic');
  step('a board with many sheets opens on one sheet, filling the view');
  await page.waitForSelector('.sheet-tab.active');
  const tabs = await page.locator('.sheet-tab').count();
  expect(tabs >= 26, `a tab for All sheets, each of the 24 module instances and the rest (${tabs})`);
  expect((await page.locator('.sheet-tab.active').innerText()).startsWith('light1: Indicator'), 'the first sheet is open');
  const host = await page.locator('.schematic-host').boundingBox();
  const sheet = await page.locator('.schematic .module-sheet[data-module="light1"] .module-frame').boundingBox();
  expect(sheet.width > host.width * 0.6 || sheet.height > host.height * 0.6, `the sheet fills the view (${Math.round(sheet.width)} x ${Math.round(sheet.height)} of ${Math.round(host.width)} x ${Math.round(host.height)} px)`);
  await shot('one-sheet');

  step('the keys step through the sheets: right arrow, ] and [');
  await page.keyboard.press('ArrowRight');
  expect((await page.locator('.sheet-tab.active').innerText()).startsWith('light2'), 'the second sheet');
  await page.keyboard.press(']');
  await page.keyboard.press('[');
  await page.keyboard.press('ArrowLeft');
  expect((await page.locator('.sheet-tab.active').innerText()).startsWith('light1'), 'back on the first');
  await page.getByRole('button', { name: 'Next sheet' }).click();
  expect((await page.locator('.sheet-tab.active').innerText()).startsWith('light2'), 'the Next button');

  step('the overview shows every sheet; a click on a box opens it');
  await page.locator('.sheet-tab', { hasText: 'All sheets' }).click();
  await page.waitForSelector('.schematic .module-sheet');
  const sheets = await page.locator('.schematic .module-sheet').count();
  expect(sheets >= 24, `a box for each of the 24 module instances and one for the rest (${sheets})`);
  const titles = await page.locator('.schematic .module-title').allTextContents();
  expect(titles.some((t) => t.startsWith('stage1: LdoStage')), 'a box titled with the instance and its module');
  const shape = await page.evaluate(() => {
    const svg = document.querySelector('svg.schematic');
    const [, , w, h] = svg.getAttribute('data-bounds').split(' ').map(Number);
    return w / h;
  });
  expect(shape > 0.5 && shape < 3, `the drawing is not a strip (${shape.toFixed(2)} wide for 1 high)`);
  await shot('all-modules');
  await page.locator('.schematic .module-sheet[data-module="pulls2"] .module-frame').click({ position: { x: 4, y: 4 }, force: true });
  expect((await page.locator('.sheet-tab.active').innerText()).startsWith('pulls2'), 'a click on the box of pulls2 opened its sheet');

  step('a net between modules is a named stub in each, not a wire across the page');
  await page.locator('.sheet-tab', { hasText: 'All sheets' }).click();
  const stubs = await page.locator('.schematic .net-label[data-net="VIN"]').count();
  expect(stubs >= 8, `the VIN stub in each regulator stage (${stubs})`);
  const wires = await page.locator('.schematic .wire[data-net="VIN"]').count();
  expect(wires >= 8, 'a short stub wire at each');

  step('the Modules list in the details opens a sheet');
  if (!(await page.locator('.outline').count())) await page.getByRole('button', { name: 'Details' }).click();
  const outline = page.locator('.outline button');
  expect((await outline.count()) >= 24, 'an entry for each module');
  await outline.filter({ hasText: 'stage3: LdoStage' }).click();
  expect((await page.locator('.sheet-tab.active').innerText()).startsWith('stage3'), 'stage3 opened');

  step('a sheet remembers its zoom, Fit and F fit it again');
  const zoom = async () => Number((await page.locator('.zoombar span').innerText()).replace('%', ''));
  const area = await page.locator('.schematic-host').boundingBox();
  await page.mouse.move(area.x + area.width / 2, area.y + area.height / 2);
  await page.mouse.wheel(0, -500);
  await page.waitForTimeout(300);
  const zoomed = await zoom();
  expect(zoomed > 110, `zoomed in (${zoomed}%)`);
  await page.getByRole('button', { name: 'Next sheet' }).click();
  expect(Math.abs((await zoom()) - 100) <= 1, 'the next sheet is fitted');
  await page.getByRole('button', { name: 'Previous sheet' }).click();
  expect(Math.abs((await zoom()) - zoomed) <= 1, `stage3 is as it was left (${await zoom()}%)`);
  await page.locator('.schematic-host').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('f');
  expect(Math.abs((await zoom()) - 100) <= 1, 'the key F fits');
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
