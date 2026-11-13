// The server's additive fields in the page (ADR 0019's table): the outline and the breadcrumbs from `Def.endLine` and `Def.children`,
// a definition card from `/api/definition` (no library file), pin names on a net card, a part card's solved block, the boards that use
// a module (`Analysis.moduleUses`) with a card for each, and a thin space in every range. The About dialog and the planes are in the
// `settings` and `four-layer` recipes. Fixture: one module, LdoStage, used by two boards.
import { whenIdle } from '../settle.mjs';
export const fixture = 'shared_module';

export async function run({ page, shot, step, expect, section }) {
  await page.setViewportSize({ width: 1700, height: 1000 });
  const requests = [];
  page.on('request', (r) => requests.push(new URL(r.url()).pathname + new URL(r.url()).search));
  await section('source');
  await page.waitForSelector('.cm-content');
  const goDef = async (name) => {
    await page.locator('.ft-def', { hasText: name }).first().click();
    await whenIdle(page);
  };
  const lineOf = (text) => page.locator('.cm-line', { hasText: text }).first();
  const hoverToken = async (line, word, nth = 0) => lineOf(line).locator('span', { hasText: word }).nth(nth).hover();
  const away = async () => {
    await page.mouse.move(1500, 960);
    await whenIdle(page);
  };
  const analysis = await page.evaluate(async () => (await fetch('/api/analysis')).json());
  const shown = await page.evaluate(() => document.querySelector('.boardpick select').value);
  const boards = analysis.boards.map((b) => b.name);
  expect(boards.length === 2 && analysis.moduleUses?.[0]?.usedBy.length === 2, `two boards share the module (${boards.join(', ')}; ${JSON.stringify(analysis.moduleUses)})`);

  step('the outline and the breadcrumbs take the server\'s ends and children; edited text that moved them is read by the page');
  await goDef('LdoStage');
  const outline = page.locator('[data-testid="outline"]');
  await page.waitForSelector('.ol-row[data-name="input_cap"]');
  expect((await outline.getAttribute('data-source')) === 'server', 'the symbols are the server\'s');
  const rows = await page.locator('.ol-row').evaluateAll((els) => els.map((e) => `${e.dataset.kind}:${e.dataset.name}:${e.dataset.line}`));
  const file = await page.evaluate(async () => (await fetch('/api/project')).json());
  const defs = file.files.find((f) => f.path === 'src/main.nsm').defs;
  expect(defs.every((d) => d.endLine !== undefined) && rows.some((r) => r.includes(`:LdoStage:${defs[0].line}`)), `a row for LdoStage on the server's line (${rows.slice(0, 4).join(', ')})`);
  await lineOf('.input_cap:').click();
  await whenIdle(page);
  const crumbs = await page.locator('.bc-crumb').allInnerTexts();
  expect(crumbs.length === 3 && crumbs[1].includes('LdoStage') && crumbs[2].startsWith('input_cap'), `main.nsm > LdoStage > input_cap (${crumbs.join(' > ').replace(/\s+/g, ' ')})`);
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('\n');
  await whenIdle(page);
  expect((await outline.getAttribute('data-source')) === 'text', 'unsaved text that moved the lines is read by the page');
  await lineOf('.input_cap:').click();
  await whenIdle(page);
  const moved = await page.locator('.bc-crumb').allInnerTexts();
  expect(moved.length === 3 && moved[2].startsWith('input_cap'), 'and the breadcrumbs still follow the cursor');
  await page.keyboard.press('Control+z');
  await whenIdle(page);
  expect((await outline.getAttribute('data-source')) === 'server', 'back to the server\'s once the text is as it was');

  step('a part card has the solved block and the boards that use the module; the library file is not read');
  await hoverToken('.input_cap:', 'input_cap');
  await page.waitForSelector('[data-testid="hover-card"][data-kind="part"]');
  let card = await page.locator('[data-testid="hover-card"]').innerText();
  expect(card.includes('Solved') && card.includes('1.72 uF .. 2.916 uF'), `the capacitance in use, with thin spaces (${card.replace(/\s+/g, ' ').slice(0, 260)})`);
  expect(card.includes('+/- 20 %') && card.includes('max voltage 10 V') && card.includes('CL05A106MP5NUNC'), 'the tolerance, the ratings and the catalog row');
  const chips = await page.locator('[data-testid="hc-boards"] .hc-board').allInnerTexts();
  expect(chips.length === 2 && boards.every((b) => chips.some((c) => c.includes(b))), `the boards that use LdoStage as chips (${chips.join(' | ').replace(/\s+/g, ' ')})`);
  expect((await page.locator('.hc-board[aria-pressed="true"]').innerText()).includes(shown), `the board shown is pressed (${shown})`);
  await shot('hover-shared-part');
  const other = boards.find((b) => b !== shown);
  await page.locator('.hc-board', { hasText: other }).click();
  await page.waitForFunction((b) => document.querySelector('[data-testid="hover-card"]')?.dataset.board === b, other);
  card = await page.locator('[data-testid="hover-card"]').innerText();
  expect(card.includes('stage.input_cap') && (await page.locator('[data-testid="hc-open-board"]').count()) === 1, `the card is for ${other}, and says how to show it in the views`);
  await shot('hover-shared-other-board');
  await page.locator('[data-testid="hc-open-board"]').click();
  await whenIdle(page);
  expect((await page.evaluate(() => document.querySelector('.boardpick select').value)) === other, 'the views now show that board');
  await away();
  await page.selectOption('.boardpick select', shown);
  await whenIdle(page);

  step('a net card names the pins the way the datasheet does');
  await goDef(shown === 'PowerBoard' ? 'PowerBoard' : 'SpareBoard');
  await hoverToken('.pos.net &= "3V3"', '"3V3"');
  await page.waitForSelector('[data-testid="hover-card"][data-kind="net"]');
  await page.waitForFunction(() => document.querySelector('[data-testid="hover-card"]')?.innerText.includes('Voltage'), null);
  card = await page.locator('[data-testid="hover-card"]').innerText();
  const pinNames = await page.locator('[data-testid="hover-card"] .hc-pname').allInnerTexts();
  expect(pinNames.includes('OUT') && pinNames.includes('P1'), `pin names on the chips (${pinNames.join(', ')})`);
  expect(/Voltage\s+3\.135 V \.\. 3\.465 V/.test(card), `the rail's voltage with thin spaces (${card.replace(/\s+/g, ' ').slice(0, 160)})`);
  await shot('hover-net-pins');
  await away();

  step('a definition card has the server\'s signature and doc, and no library file is fetched for it');
  const before = requests.length;
  await goDef('LdoStage');
  await hoverToken('.regulator:', 'LinearRegulator');
  await page.waitForSelector('[data-testid="hover-card"][data-kind="definition"]');
  await page.waitForFunction(() => /generic LinearRegulator \{/.test(document.querySelector('[data-testid="hover-card"] .hc-sig')?.innerText ?? ''), null);
  const sig = await page.locator('[data-testid="hover-card"] .hc-sig').innerText();
  expect(/^generic LinearRegulator \{.*\}$/s.test(sig.trim()), `the signature is the server's (${sig.replace(/\s+/g, ' ')})`);
  const since = requests.slice(before);
  expect(since.some((r) => r.startsWith('/api/definition')), 'it asked /api/definition');
  expect(!since.some((r) => r.startsWith('/api/file')), `and read no file (${since.filter((r) => r.startsWith('/api/f')).join(', ') || 'none'})`);
  await shot('hover-definition-served');
  await away();
}
