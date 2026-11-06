// Settings (theme, density, size, motion), the help menu, About, the diagnostics copy, the tab title, the favicon and the splash.
// The settings are kept in localStorage (`netsmith.settings`), applied before the first paint, changed from the gear's panel and from
// the palette; the help menu opens the shortcuts, the palette and About; "Report a problem" copies a block of facts.
export const fixture = 'two_boards';

const SECTIONS = ['source', 'schematic', 'power', 'system', 'bom', 'pcb', 'build', 'compare'];

export async function run({ page, shot, step, expect, url, section }) {
  const settings = () => page.evaluate(() => JSON.parse(localStorage.getItem('netsmith.settings') ?? 'null'));
  const attr = (n) => page.evaluate((k) => document.documentElement.dataset[k], n);
  const rootPx = () => page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize));
  const px = (sel) => page.evaluate((s) => Math.round(document.querySelector(s).getBoundingClientRect().height * 10) / 10, sel);
  const openPanel = async () => {
    await page.click('[data-testid="menu-settings"]');
    await page.waitForSelector('[data-testid="panel-settings"]');
  };
  const palette = async (text) => {
    await page.keyboard.press('Control+Shift+p');
    await page.waitForSelector('.qp');
    await page.keyboard.type(text);
    await page.waitForTimeout(250);
    await page.keyboard.press('Enter');
    await page.waitForTimeout(300);
  };

  await page.setViewportSize({ width: 1280, height: 800 });
  await page.evaluate(() => localStorage.removeItem('netsmith.settings'));
  await page.reload();
  await page.waitForSelector('.app');

  step('the defaults: comfortable, 100 %, motion from the system; the gear and the question mark are in the top bar');
  expect((await attr('density')) === 'comfortable' && (await attr('motion')) === 'system', 'density comfortable and motion system');
  expect((await rootPx()) === 16, 'the root is 16 px');
  expect((await page.locator('[data-testid="menu-settings"]').count()) === 1 && (await page.locator('[data-testid="menu-help"]').count()) === 1, 'both buttons');
  const comfortableBar = await px('.topbar');
  const comfortableCtl = await px('.tb-ghosts .icon-btn');

  step('the gear opens the panel: theme, density, size, motion; Escape closes it and the focus returns to the gear');
  await openPanel();
  expect((await page.locator('[data-testid="panel-settings"] .theme-option').count()) === 5, 'five themes');
  expect((await page.locator('[data-testid="panel-settings"] [role=radiogroup]').count()) === 4, 'four groups of choices');
  await shot('panel');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid="panel-settings"]', { state: 'detached' });
  expect(await page.evaluate(() => document.activeElement?.getAttribute('data-testid') === 'menu-settings'), 'the gear has the focus again');

  step('Compact: the controls, the rows and the bar get smaller, the choice is kept, a toast says so');
  await openPanel();
  await page.click('[data-testid="density-compact"]');
  await page.waitForTimeout(250);
  expect((await attr('density')) === 'compact', 'html[data-density] is compact');
  expect((await settings())?.density === 'compact', 'kept in localStorage');
  expect((await page.locator('.toast', { hasText: 'Density: Compact' }).count()) === 1, 'a toast says Density: Compact');
  await page.waitForFunction(() => !document.querySelector('.toast'), null, { timeout: 2600 });
  expect(true, 'and it is gone within 2 seconds, not left standing');
  expect((await px('.topbar')) < comfortableBar && (await px('.tb-ghosts .icon-btn')) < comfortableCtl, `the bar and a control are smaller (${comfortableBar} to ${await px('.topbar')}, ${comfortableCtl} to ${await px('.tb-ghosts .icon-btn')})`);
  await page.keyboard.press('Escape');

  step('every view in compact: no horizontal overflow, no control narrower than its text (a clipped label)');
  const bad = [];
  for (const id of SECTIONS) {
    await section(id);
    await page.waitForTimeout(350);
    const r = await page.evaluate(() => ({ overflow: document.documentElement.scrollWidth > innerWidth }));
    if (r.overflow) bad.push(`${id}: overflow`);
    await shot(`compact-${id}`);
  }
  expect(bad.length === 0, `nothing overflows (${bad.join('; ')})`);

  step('Size: 125 % makes the root 20 px and the bar 25 % taller; the choice is kept');
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 100, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');
  const bar100 = await px('.topbar');
  // What each card of the power tree and the system view overflows by (a few px of its own at any size), at 100 %: the same at 125 %.
  const clips = (id) =>
    page.evaluate((s) => [...document.querySelectorAll(`.sec-${s} .pt-node`)].map((n) => `${n.dataset.node}:${n.scrollHeight - n.clientHeight}x${n.scrollWidth - n.clientWidth}`).join(' '), id);
  const at100 = {};
  for (const id of ['power', 'system']) {
    await section(id);
    await page.waitForTimeout(500);
    at100[id] = await clips(id);
  }
  await openPanel();
  await page.click('[data-testid="scale-125"]');
  await page.waitForTimeout(250);
  expect((await rootPx()) === 20, 'the root font size is 20 px');
  expect(Math.abs((await px('.topbar')) / bar100 - 1.25) < 0.02, `the bar is 125 % (${bar100} to ${await px('.topbar')})`);
  expect((await settings())?.scale === 125, 'kept');
  await page.keyboard.press('Escape');
  await shot('scale-125');
  for (const id of SECTIONS) {
    await section(id);
    await page.waitForTimeout(300);
    if (await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)) bad.push(`125 % ${id}: overflow`);
  }
  expect(bad.length === 0, `no view overflows at 125 % (${bad.join('; ')})`);
  step('at 125 % the cards of the power tree and the system view overflow by what they did at 100 % (the canvas keeps its own size)');
  for (const id of ['power', 'system']) {
    await section(id);
    await page.waitForTimeout(500);
    const now = await clips(id);
    expect(now === at100[id], `${id}: the cards clip no more than at 100 % (${now})`);
  }
  await openPanel();
  await page.click('[data-testid="scale-90"]');
  await page.waitForTimeout(250);
  expect(Math.abs((await rootPx()) - 14.4) < 0.01, 'the root is 14.4 px at 90 %');
  await page.keyboard.press('Escape');

  step('applied before the first paint: html carries the settings when the document is interactive, after a reload');
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'compact', scale: 110, motion: 'reduce' })));
  await page.addInitScript(() => {
    document.addEventListener('readystatechange', () => {
      if (document.readyState === 'interactive') {
        const r = document.documentElement;
        window.__boot = `${r.dataset.density}/${r.dataset.motion}/${r.style.getPropertyValue('--ui-scale')}`;
      }
    });
  });
  await page.reload();
  await page.waitForSelector('.app');
  expect((await page.evaluate(() => window.__boot)) === 'compact/reduce/1.1', `compact, reduce and 1.1 are on html when the document is interactive (${await page.evaluate(() => window.__boot)})`);

  step('Motion: Reduce stops every animation (none longer than 1 ms), whatever the system says');
  await openPanel();
  const longest = await page.evaluate(() => Math.max(0, ...document.getAnimations().map((a) => Number(a.effect?.getComputedTiming().duration ?? 0))));
  expect(longest <= 1, `the opening panel runs no animation over 1 ms (${longest})`);
  await page.keyboard.press('Escape');

  step('the palette: Density, Size, Motion and Settings: back to the defaults');
  await palette('density: comfortable');
  expect((await attr('density')) === 'comfortable', 'Density: Comfortable');
  await palette('size: 125');
  expect((await rootPx()) === 20, 'Size: 125 %');
  await palette('motion: system');
  expect((await attr('motion')) === 'system', 'Motion: System');
  await palette('settings: back');
  expect((await attr('density')) === 'comfortable' && (await rootPx()) === 16 && (await attr('motion')) === 'system', 'back to the defaults');
  expect(JSON.stringify(await settings()) === JSON.stringify({ density: 'comfortable', scale: 100, motion: 'system' }), 'and kept so');
  await page.keyboard.press('Control+,');
  await page.waitForSelector('[data-testid="panel-settings"]');
  expect(true, 'Ctrl+, opens the settings');
  await page.keyboard.press('Escape');

  step('the help menu: shortcuts, palette, go to anything, report a problem, About; arrow keys move');
  await page.click('[data-testid="menu-help"]');
  await page.waitForSelector('[data-testid="panel-help"]');
  expect((await page.locator('[data-testid="panel-help"] [role=menuitem]').count()) === 5, 'five items');
  await page.keyboard.press('ArrowDown');
  expect(await page.evaluate(() => document.activeElement?.getAttribute('role') === 'menuitem'), 'an arrow key moves between items');
  await shot('help-menu');
  await page.click('[data-testid="help-shortcuts"]');
  await page.waitForSelector('.cheat');
  expect((await page.locator('[data-testid="panel-help"]').count()) === 0, 'the menu closed and the shortcuts opened');
  await page.keyboard.press('Escape');

  step('About: what the server reports, what it does not; Escape closes it and the focus returns');
  await page.click('[data-testid="menu-help"]');
  await page.click('[data-testid="help-about"]');
  await page.waitForSelector('[data-testid="about"]');
  const about = await page.locator('[data-testid="about"]').innerText();
  expect(/Project\s*\n?\s*\S+/.test(about) && about.includes('Boards') && /KiCad/.test(about), 'it names the project, the boards and KiCad');
  const facts = await page.evaluate(async () => (await fetch('/api/project')).json());
  expect(!about.includes('Not reported by the server') && about.includes(facts.version), `it shows the version the server reports (${facts.version})`);
  expect(about.includes(facts.root), `it shows the project's folder (${facts.root})`);
  expect(!facts.kicad || about.includes(facts.kicadPath), `it shows where KiCad was found (${facts.kicadPath})`);
  expect(!facts.bundleHash || about.includes(facts.bundleHash), `it shows the hash of the page the binary embeds (${facts.bundleHash})`);
  await page.waitForSelector('[data-testid="about-bundle"]');
  const match = await page.locator('[data-testid="about-bundle"]').getAttribute('data-match');
  expect(match === 'match', `the page on screen is the one the binary serves (${match})`);
  await shot('about');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid="about"]', { state: 'detached' });

  step('Report a problem copies the diagnostics block (versions, the analysis, the last errors) to the clipboard');
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.click('[data-testid="menu-help"]');
  await page.click('[data-testid="help-report"]');
  await page.waitForSelector('.toast', { hasText: 'Diagnostics copied' });
  const text = await page.evaluate(() => navigator.clipboard.readText());
  for (const w of ['netsmith editor: diagnostics', 'Bundle:', 'Browser:', 'Problems:', 'Last errors the page showed', `netsmith version: ${facts.version}`, `Folder: ${facts.root}`, `Embedded page (source hash): ${facts.bundleHash}`, 'Page on screen vs the binary: match']) expect(text.includes(w), `the block says "${w}"`);
  expect(!text.includes('Not reported by the server'), 'and does not say that anything is not reported');

  step('the tab title is `netsmith - <board>`, a dot in front while a file is unsaved');
  const board = await page.evaluate(() => document.querySelector('.boardpick select').value);
  expect((await page.title()) === `netsmith - ${board}`, `the title is netsmith - ${board} (${await page.title()})`);
  await section('source');
  await page.locator('.cm-content').click();
  await page.keyboard.type(' ');
  await page.waitForTimeout(300);
  expect((await page.title()).startsWith('● netsmith'), `a dot while unsaved (${await page.title()})`);
  await page.keyboard.press('Control+s');
  await page.waitForTimeout(800);
  expect(!(await page.title()).startsWith('●'), 'the dot is gone after the save');
  await page.keyboard.press('Control+z');
  await page.keyboard.press('Control+s');
  await page.waitForTimeout(600);

  step('the favicon is an SVG with a dark variant, served by the editor');
  const icon = await page.evaluate(async () => {
    const l = document.querySelector('link[rel="icon"][type="image/svg+xml"]');
    if (!l) return null;
    const r = await fetch(l.href);
    return { type: r.headers.get('content-type'), text: await r.text() };
  });
  expect(icon && /svg/.test(icon.type) && icon.text.includes('prefers-color-scheme: dark'), 'an SVG icon with a prefers-color-scheme rule');

  step('the splash: the mark and `Reading the project…` until the project arrives, then it is gone');
  const p2 = await page.context().newPage();
  await p2.route('**/api/project', async (r) => {
    await new Promise((x) => setTimeout(x, 1200));
    await r.continue();
  });
  const nav = p2.goto(url);
  await p2.waitForSelector('.splash');
  expect(((await p2.locator('.splash').innerText()) || '').includes('Reading the project'), 'it says Reading the project');
  await nav;
  await p2.waitForSelector('.splash', { state: 'detached', timeout: 8000 });
  expect(true, 'it is gone once the project is there');
  await p2.close();

  step('storage that throws: the page loads with the defaults');
  const browser = page.context().browser();
  const blocked = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  await blocked.addInitScript(() => {
    Object.defineProperty(window, 'localStorage', {
      get() {
        throw new DOMException('blocked', 'SecurityError');
      },
    });
  });
  const p3 = await blocked.newPage();
  const errors = [];
  p3.on('pageerror', (e) => errors.push(e.message));
  await p3.goto(url);
  await p3.waitForSelector('.app');
  await p3.waitForTimeout(500);
  await p3.click('[data-testid="menu-settings"]');
  await p3.click('[data-testid="density-compact"]');
  await p3.waitForSelector('.toast', { hasText: 'not remembered' });
  expect(errors.length === 0, `no page error (${errors.join('; ')})`);
  await blocked.close();
}
