// A big board drawn by module: a box for each module instance, an outline to jump to one,
// a click on a box zooms into it, and nets between modules are stubs with names.
export const fixture = 'big';

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic');
  await page.getByRole('button', { name: 'Source' }).click();
  step('the whole board is in boxes, one for each module instance');
  await page.waitForSelector('.schematic .module-sheet');
  const sheets = await page.locator('.schematic .module-sheet').count();
  expect(sheets >= 24, `a box for each of the 24 module instances and one for the rest (${sheets})`);
  const titles = await page.locator('.schematic .module-title').allTextContents();
  expect(titles.some((t) => t.startsWith('stage1: LdoStage')), 'a box titled with the instance and its module');
  const box = await page.locator('.schematic-host').boundingBox();
  const shape = await page.evaluate(() => {
    const svg = document.querySelector('svg.schematic');
    const [, , w, h] = svg.getAttribute('data-bounds').split(' ').map(Number);
    return w / h;
  });
  expect(shape > 0.5 && shape < 3, `the drawing is not a strip (${shape.toFixed(2)} wide for 1 high)`);
  expect(box.width > 300, 'the drawing has room');
  await shot('all-modules');

  step('a net between modules is a named stub in each, not a wire across the page');
  const stubs = await page.locator('.schematic .net-label[data-net="VIN"]').count();
  expect(stubs >= 8, `the VIN stub in each regulator stage (${stubs})`);
  const wires = await page.locator('.schematic .wire[data-net="VIN"]').count();
  expect(wires >= 8, 'a short stub wire at each');

  step('the outline lists the modules; a click zooms to one');
  const zoom = async () => Number((await page.locator('.zoombar span').innerText()).replace('%', ''));
  const fit = await zoom();
  const outline = page.locator('.outline button');
  expect((await outline.count()) >= 24, 'an outline entry for each module');
  await outline.filter({ hasText: 'stage3: LdoStage' }).click();
  await page.waitForTimeout(300);
  const zoomed = await zoom();
  expect(zoomed > fit * 2, `zoomed in (${fit}% to ${zoomed}%)`);
  await shot('outline-jump');

  step('Fit shows the whole board again; a click on a box zooms into it');
  await page.getByRole('button', { name: 'Fit' }).click();
  await page.waitForTimeout(300);
  expect(Math.abs((await zoom()) - fit) <= 1, 'back to the fit');
  await page.locator('.schematic .module-sheet[data-module="pulls2"] .module-frame').click({ position: { x: 4, y: 4 }, force: true });
  await page.waitForTimeout(300);
  expect((await zoom()) > fit * 2, 'a click on the box of pulls2 zoomed to it');
  await shot('box-click');

  step('wheel zooms and a drag pans');
  const before = await page.locator('.canvas').first().evaluate((e) => e.style.transform);
  const area = await page.locator('.schematic-host').boundingBox();
  await page.mouse.move(area.x + area.width / 2, area.y + area.height / 2);
  await page.mouse.down();
  await page.mouse.move(area.x + area.width / 2 + 80, area.y + area.height / 2 + 40, { steps: 4 });
  await page.mouse.up();
  const after = await page.locator('.canvas').first().evaluate((e) => e.style.transform);
  expect(before !== after, 'a drag moved the drawing');
}
