// Files are coloured by their extension: .nsm in the netsmith language, .toml as TOML, a catalog's
// anything else as plain text (the tree lists only .nsm files and the manifest, so a catalog's .csv
// cannot be opened in the editor; the choice for it is in src/languages.test.ts). Nothing but .nsm may
// show the red wavy mark of an invalid token, and there it comes with a note that says what it is.
export const fixture = 'catalog_refresh';

export async function run({ page, shot, step, expect, section, project }) {
  await page.setViewportSize({ width: 2000, height: 1000 });
  await section('source');
  await page.waitForSelector('.sidebar .ft-row.ft-file');
  const wavy = () =>
    page.evaluate(() => {
      const bad = [];
      for (const e of document.querySelectorAll('.cm-content *')) {
        const c = getComputedStyle(e);
        if (/wavy/.test(c.textDecorationStyle) || /wavy/.test(c.textDecoration)) bad.push(e.textContent.slice(0, 20));
      }
      return { bad, lint: document.querySelectorAll('.cm-content .cm-lintRange').length };
    });

  step('open netsmith.toml: coloured as TOML, no squiggles');
  await page.locator('.ft-row.ft-file', { hasText: 'netsmith.toml' }).click();
  await page.waitForSelector('.tab.active:has-text("netsmith.toml")');
  await page.waitForFunction(() => document.querySelector('.cm-content')?.innerText.includes('['));
  await page.waitForTimeout(500);
  let w = await wavy();
  expect(w.bad.length === 0 && w.lint === 0, `no wavy underline and no lint mark in the manifest (${JSON.stringify(w)})`);
  const colours = await page.evaluate(() => new Set([...document.querySelectorAll('.cm-content .cm-line span')].map((s) => getComputedStyle(s).color)).size);
  expect(colours >= 3, `the manifest has several colours: it is highlighted as TOML (${colours})`);
  await shot('manifest-toml');

  step('an .nsm file: a token the editor marks as invalid has a note that says what it is');
  await page.locator('.ft-row.ft-file', { hasText: 'main.nsm' }).first().click();
  await page.waitForSelector('.tab.active:has-text("main.nsm")');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\n]');
  await page.waitForSelector('.cm-content .cm-lintRange-error', { timeout: 15000 });
  const line = page.locator('.cm-content .cm-line', { hasText: /^\]$/ });
  const box = await line.boundingBox();
  await page.mouse.move(box.x + 3, box.y + box.height / 2);
  await page.waitForSelector('.cm-tooltip', { timeout: 10000 });
  const tip = (await page.locator('.cm-tooltip').first().innerText()).trim();
  expect(tip.length > 10, `a tooltip with words, not an unexplained mark (${tip.slice(0, 140)})`);
  await shot('nsm-invalid-tooltip');
  // Put the file back, so nothing is left unsaved.
  await page.keyboard.press('Control+z');
}
