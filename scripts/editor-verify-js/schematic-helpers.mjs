// What the schematic recipes share (this file is not a recipe: recipes are in recipes/).

/** The sheet the toolbar's picker names: `light1`. */
export const activeSheet = (page) => page.locator('.sheetpick .crumb-sheet').innerText();

/** Open the toolbar's sheet picker and return the names of its sheets (the overview first). */
export async function sheetNames(page) {
  await page.locator('.sheetpick .pick-btn').click();
  const names = await page.locator('.pick-pop .sl-row .sl-name').allInnerTexts();
  await page.keyboard.press('Escape');
  return names;
}

/** Open a sheet by its name (`modem`, or `All sheets`) from the toolbar's picker. */
export async function openSheet(page, name) {
  await page.locator('.sheetpick .pick-btn').click();
  await page.locator('.pick-pop .sl-row', { has: page.locator('.sl-name', { hasText: new RegExp(`^${name.replace(/[()/]/g, '\\$&')}$`) }) }).first().click();
  await page.waitForTimeout(250);
}

/** The zoom the zoom bar shows, in percent. */
export async function zoomPercent(page) {
  return Number((await page.locator('.zoombar span:not(.zsep)').first().innerText()).replace('%', ''));
}
