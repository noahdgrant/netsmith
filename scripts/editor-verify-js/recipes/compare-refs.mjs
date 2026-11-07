// Two refs compared with each other (ADR 0059, "Built in the extensions"): the picker's second choice, `Against`, the
// answer that reads no file on disk, the views that stay unmarked, the head side of a file at the second ref, the way back
// to the working tree and a second ref that does not analyse. It needs `launch --git` (see compare.mjs and
// fixtures/gitrepo.mjs): `main` is the earlier design, `feature` the project as launched, `broken` has a typo in a type, and
// an untracked NOTES.md makes the work tree differ from `feature`, which two refs must not show.
export const fixture = 'two_boards';
export const needs = 'git';

export async function run({ page, shot, step, expect, section, resetRepo }) {
  resetRepo();
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await page.waitForSelector('.sidebar > *', { state: 'attached' });
  await page.setViewportSize({ width: 1920, height: 1080 });
  const settle = (ms = 300) => page.waitForTimeout(ms);
  const text = (sel) => page.locator(sel).first().innerText();
  const visible = (sel) => page.locator(`.section:not(.away) ${sel}`);
  const summary = async () => {
    await page.waitForSelector('[data-testid="cmp-summary"]', { timeout: 60000 });
    return text('[data-testid="cmp-summary"]');
  };
  const openPicker = async (slot) => {
    if (!(await page.locator('[data-testid="ref-popover"]').count())) await visible('[data-testid="ref-button"]').click();
    await page.waitForSelector('[data-testid="ref-popover"]');
    if (slot) await page.click(`[data-testid="ref-slot-${slot}"]`);
  };
  const choose = async (slot, name) => {
    await openPicker(slot);
    await page.locator(`[data-testid="ref-item"][data-name="${name}"]`).click();
  };
  const tab = async (id) => {
    await page.click(`.cmp-tab[data-tab="${id}"]`);
    await settle(150);
  };
  const titles = () => page.locator('[data-testid="cmp-row"] .cr-title').allInnerTexts();
  const marksOnPage = () => page.evaluate(() => document.querySelectorAll('[class*="cmp-changed"], [class*="cmp-added"], [class*="cmp-net-"]').length);

  step('main against the working tree, as before: the button, the chip and the dirty notice have no second ref');
  await page.keyboard.press('8');
  await page.waitForSelector('[data-testid="cmp-start"]');
  await choose(null, 'main');
  const onDisk = await summary();
  expect(/1 part added, 1 removed, 2 values changed/.test(onDisk), `the sentence against the files on disk (${onDisk})`);
  expect((await text('[data-testid="ref-button"]')).replace(/\s+/g, ' ').trim() === 'vs main', 'the button says vs main and no second ref');
  expect(/uncommitted changes/.test(await text('.cmp-meta')), 'the work tree has changes: said');
  expect((await page.locator('[data-testid="cmp-against"]').count()) === 0, 'no second commit in the header');
  await tab('source');
  const filesOnDisk = await page.locator('[data-testid="cmp-file"] .cr-title').allInnerTexts();
  expect(filesOnDisk.includes('NOTES.md') && filesOnDisk.includes('src/main.nsm'), `the files on disk include the untracked notes (${filesOnDisk.join(', ')})`);

  step('the picker has two slots: Base is on, and Against is the working tree');
  await openPicker();
  expect((await page.getAttribute('[data-testid="ref-slot-base"]', 'aria-pressed')) === 'true', 'Base is the slot that is on');
  expect(/main/.test(await text('[data-testid="ref-slot-base"]')), 'Base says main');
  expect(/working tree/.test(await text('[data-testid="ref-slot-against"]')), 'Against says the working tree');
  expect((await page.locator('[data-testid="ref-worktree"]').count()) === 0, 'the working tree is listed under Against only');
  await page.click('[data-testid="ref-slot-against"]');
  expect((await page.getAttribute('[data-testid="ref-worktree"]', 'aria-selected')) === 'true', 'under Against the working tree is the chosen one');
  expect(
    (await page.locator('[data-testid="ref-item"]').evaluateAll((els) => els[0].getAttribute('data-name'))) === 'HEAD',
    'the refs follow it',
  );
  await shot('picker-against');

  step('feature as the second ref: the same circuit changes, read from two commits, and the notes are not there');
  await page.locator('[data-testid="ref-item"][data-name="feature"]').click();
  await page.waitForFunction(() => document.querySelector('[data-testid="cmp-against"]'), null, { timeout: 60000 });
  const two = await summary();
  expect(/1 part added, 1 removed, 2 values changed/.test(two) && /headroom shrank from 70 mA to 50 mA/.test(two), `the sentence of two refs (${two})`);
  expect((await text('[data-testid="ref-button"]')).replace(/\s+/g, ' ').trim() === 'vs main and feature', 'the button names both refs');
  expect((await text('[data-testid="chip-pair"]')).trim().startsWith('main and feature'), 'the status bar chip names both refs');
  expect((await page.locator('.cmp-dirty').count()) === 0, 'no uncommitted-changes notice: the files on disk are not read');
  expect(/\w{7}/.test(await text('[data-testid="cmp-against"]')), 'the header has the second commit');
  expect((await page.locator('.cmp-head [data-testid="changes-switch"]').count()) === 0, 'the section has no switch for the views');
  await tab('source');
  const filesRefs = await page.locator('[data-testid="cmp-file"] .cr-title').allInnerTexts();
  expect(filesRefs.length === 1 && filesRefs[0] === 'src/main.nsm', `only the committed file differs, not the notes (${filesRefs.join(', ')})`);
  await tab('parts');
  await shot('two-refs');

  step('the views stay unmarked and their switch says why it is off');
  expect((await marksOnPage()) === 0, 'nothing carries a mark of the comparison');
  await section('bom');
  await page.waitForSelector('.section:not(.away) [data-testid="changes-switch"]');
  const toggle = page.locator('.section:not(.away) [data-testid="changes-switch"]').first();
  expect((await toggle.getAttribute('aria-disabled')) === 'true' && (await toggle.getAttribute('aria-pressed')) === 'false', 'the switch of a view is off, and says so');
  expect(/files on disk/.test((await toggle.getAttribute('data-tip')) ?? ''), 'its tooltip says the views draw the files on disk');
  await toggle.click({ force: true });
  await settle(300);
  expect((await marksOnPage()) === 0 && !(await page.evaluate(() => document.documentElement.classList.contains('cmp-on'))), 'a click marks nothing');
  await section('compare');
  await page.waitForSelector('[data-testid="cmp-summary"]');

  step('a row opens the file at both refs, and the head side is the second ref');
  await tab('parts');
  const r1 = page.locator('[data-testid="cmp-row"]', { has: page.locator('.cr-title', { hasText: /^r1$/ }) });
  await r1.locator('[data-testid="link-head"]').click();
  await page.waitForSelector('[data-testid="diff-panel"]');
  await settle(500);
  const side = await text('[data-testid="diff-head-side"]');
  expect(/^[0-9a-f]{7}$/.test(side.trim()), `the head side is a commit (${side})`);
  const diffText = await page.locator('[data-testid="diff-side"]').innerText();
  expect(/150ohm/.test(diffText) && /\.r1: Resistor \{\}/.test(diffText), 'the earlier resistor on the left, the later on the right');
  expect(await page.locator('[data-testid="diff-in-source"]').isDisabled(), 'the file cannot be shown in the source, which holds the files on disk');
  await shot('diff-two-refs');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid="diff-panel"]', { state: 'detached' });

  step('a reload keeps both refs');
  await page.reload();
  await page.waitForSelector('.sidebar > *', { state: 'attached' });
  await page.keyboard.press('8');
  await summary();
  await page.waitForSelector('[data-testid="cmp-against"]', { timeout: 60000 });
  expect((await text('[data-testid="ref-button"]')).replace(/\s+/g, ' ').trim() === 'vs main and feature', 'the button still names both refs');

  step('the other way round: feature against main renames the net back');
  const buttonSays = (re) => page.waitForFunction((src) => new RegExp(src).test(document.querySelector('[data-testid="ref-button"]')?.innerText?.replace(/\s+/g, ' ') ?? ''), re, { timeout: 60000 });
  await choose('against', 'main');
  await buttonSays('vs main and main');
  expect(/No circuit changes/.test(await summary()), 'a ref against itself changes nothing');
  await choose('base', 'feature');
  await buttonSays('vs feature and main');
  await page.waitForFunction(() => !/No circuit changes/.test(document.querySelector('[data-testid="cmp-summary"]')?.textContent ?? ''), null, { timeout: 60000 });
  await tab('nets');
  const nets = await titles();
  expect(nets.includes('VOUT'), `the net that was 3V3 is now VOUT (${nets.join(', ')})`);

  step('a second ref that does not analyse is an answer that names that commit');
  await choose('base', 'main');
  await buttonSays('vs main and main');
  await choose('against', 'broken');
  await page.waitForSelector('[data-testid="cmp-broken"]', { timeout: 60000 });
  const broken = await text('[data-testid="cmp-error"]');
  expect(/does not analyse at/.test(broken) && /Resistr/.test(broken), `it says why (${broken})`);
  await shot('second-ref-broken');

  step('the working tree again: the notes come back, the views can be marked');
  await openPicker('against');
  await page.click('[data-testid="ref-worktree"]');
  await page.waitForSelector('[data-testid="cmp-summary"]', { timeout: 60000 });
  await settle();
  expect((await text('[data-testid="ref-button"]')).replace(/\s+/g, ' ').trim() === 'vs main', 'one ref again');
  expect((await page.locator('.cmp-head [data-testid="changes-switch"]').count()) === 1, 'the switch is back');
  await tab('source');
  const back = await page.locator('[data-testid="cmp-file"] .cr-title').allInnerTexts();
  expect(back.includes('NOTES.md'), 'the untracked notes are in the source again');
  await page.evaluate(() => localStorage.clear());
}
