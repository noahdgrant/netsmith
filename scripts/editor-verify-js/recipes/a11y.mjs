// The keyboard and screen reader audit (round 8): landmarks, a name for every control, the order of Tab from the top of the page, every layer reachable and
// closed by Escape from the topmost down, the focus given back to what opened it (never to the page's body), and no layer left open behind the focus.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.reload();
  await page.waitForSelector('.app');
  await page.waitForTimeout(600);
  const settle = (ms = 300) => page.waitForTimeout(ms);
  const active = () =>
    page.evaluate(() => {
      const a = document.activeElement;
      return a && a !== document.body ? `${a.tagName.toLowerCase()}|${a.getAttribute('aria-label') ?? (a.innerText || '').trim().slice(0, 30)}` : 'BODY';
    });
  const visible = (sel) => page.evaluate((s) => [...document.querySelectorAll(s)].some((x) => x.offsetWidth > 0), sel);

  step('landmarks: a banner, the tabs, one main region for the panes, the files and outline, each section by name, the status bar');
  const marks = await page.evaluate(() => ({
    main: document.querySelector('[role="main"]')?.getAttribute('aria-label') ?? null,
    aside: document.querySelector('aside.sidebar')?.getAttribute('aria-label') ?? null,
    banner: !!document.querySelector('header.topbar'),
    tabs: document.querySelector('[role="tablist"]')?.getAttribute('aria-label') ?? null,
    sections: [...document.querySelectorAll('section.section')].every((s) => !!s.getAttribute('aria-label')),
    status: document.querySelector('footer.statusbar')?.getAttribute('aria-label') ?? null,
  }));
  expect(marks.main === 'Workspace' && marks.aside === 'Files and outline' && marks.banner && marks.tabs === 'Sections' && marks.sections && marks.status === 'Status', `landmarks (${JSON.stringify(marks)})`);

  step('every control of every section has a name');
  const unnamed = [];
  for (const id of ['source', 'schematic', 'power', 'system', 'bom', 'pcb', 'build', 'compare']) {
    await section(id);
    await settle(500);
    unnamed.push(
      ...(await page.evaluate((s) =>
        [...document.querySelectorAll(`.section.sec-${s} :is(button, [role=button], a[href], input, select, textarea, [role=tab], [role=switch], [role=checkbox], [role=radio])`)]
          .filter((e) => e.offsetWidth > 0 && !(e.getAttribute('aria-label') || e.getAttribute('aria-labelledby') || e.getAttribute('title') || (e.textContent || '').trim() || e.getAttribute('placeholder') || e.closest('label') || (e.labels && e.labels.length)))
          .map((e) => `${s}: ${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)}`), id)),
    );
  }
  expect(unnamed.length === 0, `no unnamed control (${unnamed.join('; ') || 'none'})`);

  step('Tab from the top of the page: the skip link, then the top bar in order, then the tabs, then the view');
  await section('schematic');
  await page.reload();
  await page.waitForSelector('.app');
  await settle(600);
  await page.evaluate(() => document.activeElement?.blur?.());
  const stops = [];
  for (let i = 0; i < 14; i++) {
    await page.keyboard.press('Tab');
    stops.push(await active());
  }
  expect(/Skip to the view/.test(stops[0]), `the first stop is the skip link (${stops[0]})`);
  expect(stops.slice(1, 8).some((s) => /Go to anything/.test(s)) && stops.slice(1, 10).some((s) => /^button\|Settings/.test(s)) && stops.slice(1, 12).some((s) => /Make fab files/.test(s)), `then the top bar (${stops.slice(1, 9).join(' > ')})`);
  expect(!stops.includes('BODY'), 'and the focus is never lost to the body on the way');

  step('Escape closes the topmost layer: the palette over the settings panel, then the panel; the focus goes back to the button that opened it');
  const gear = page.getByRole('button', { name: /^Settings/ }).first();
  await gear.focus();
  await page.keyboard.press('Control+Shift+p');
  await page.waitForSelector('.qp');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.qp', { state: 'detached' });
  await settle();
  expect(/Settings/.test(await active()), `the palette gives the focus back to the control it was opened from (${await active()})`);
  await gear.click();
  await settle();
  await page.keyboard.press('Control+Shift+p');
  await page.waitForSelector('.qp');
  await page.keyboard.press('Escape');
  await settle();
  expect(!(await visible('.qp')) && (await visible('.theme-option')), 'the first Escape closes the palette and leaves the settings panel');
  await page.keyboard.press('Escape');
  await settle();
  expect(!(await visible('.theme-option')) && /Settings/.test(await active()), `the second closes the panel, and the focus is on its button (${await active()})`);

  step('a dialog opened from a menu gives the focus back to the menu\'s button when it closes, not to the body');
  await page.getByRole('button', { name: /^Help/ }).first().click();
  await page.locator('.menu-row', { hasText: 'About netsmith' }).click();
  await page.waitForSelector('[data-testid="about"]');
  await settle();
  await page.keyboard.press('Escape');
  await settle();
  expect(/Help/.test(await active()), `About: Escape returns to the Help button (${await active()})`);
  await page.getByRole('button', { name: /^Help/ }).first().click();
  await page.locator('.menu-row', { hasText: 'Keyboard shortcuts' }).click();
  await page.waitForSelector('.cheat');
  await settle();
  await page.keyboard.press('Escape');
  await settle();
  expect(/Help/.test(await active()), `the shortcuts: Escape returns to the Help button (${await active()})`);

  step('a menu that the focus leaves by Tab closes: no panel stays open behind the focus');
  await page.getByRole('button', { name: /^Help/ }).first().click();
  await page.waitForSelector('.top-pop-help');
  for (let i = 0; i < 6; i++) await page.keyboard.press('Tab');
  await settle(200);
  expect(!(await visible('.top-pop-help')), `Tab out of the Help menu closes it (focus ${await active()})`);

  step('the ref picker and a status popover close on Escape and give the focus back');
  await section('compare');
  const rb = page.locator('.section:not(.away) [data-testid="ref-button"]');
  if (await rb.count()) {
    await rb.focus();
    await page.keyboard.press('Enter');
    await page.waitForSelector('.ref-pop');
    await page.keyboard.press('Escape');
    await settle();
    expect(!(await visible('.ref-pop')) && /Compare/.test(await active()), `the ref picker (${await active()})`);
  }
  await shot('a11y-end');
}
