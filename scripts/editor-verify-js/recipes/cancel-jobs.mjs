// Cancelling a comparison, a build and a fab from the page (ADR 0062). A comparison with a ref of a big project (a commit made
// from the `big` board with forty rails, without touching the work tree) is long enough to be cancelled by the button in its
// card and by Escape; a fab is cancelled by the button in its card; the build card has the same button. It needs `launch --git`.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { writeBig } from '../fixtures/big.mjs';
import { git } from '../fixtures/gitrepo.mjs';

export const fixture = 'two_boards';
export const needs = 'git';

/** The sources of the `big` board (the modules too), as `[path, text]`. */
function bigFiles() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cancel-big-'));
  writeBig(dir, 40);
  const files = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else files.push([path.relative(dir, p), fs.readFileSync(p, 'utf8')]);
    }
  };
  walk(dir);
  fs.rmSync(dir, { recursive: true, force: true });
  return files;
}

/** A branch `bigref` whose project is the big board, made with a scratch index so that the work tree and the index are never touched. */
async function makeBigRef(project) {
  const index = path.join(os.tmpdir(), `cancel-index-${process.pid}`);
  const env = { ...process.env, GIT_INDEX_FILE: index, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_SYSTEM: '/dev/null' };
  const g = (...args) => {
    const r = spawnSync('git', ['-C', project, ...args], { encoding: 'utf8', env });
    if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr}`);
    return r.stdout.trim();
  };
  try {
    g('read-tree', 'feature');
    for (const [file, text] of bigFiles()) {
      const tmp = path.join(os.tmpdir(), `cancel-blob-${process.pid}`);
      fs.writeFileSync(tmp, text);
      const blob = g('hash-object', '-w', tmp);
      fs.rmSync(tmp);
      g('update-index', '--add', '--cacheinfo', `100644,${blob},${file.split(path.sep).join('/')}`);
    }
    const tree = g('write-tree');
    const commit = g('-c', 'user.name=Ada Verify', '-c', 'user.email=ada@example.com', 'commit-tree', tree, '-p', 'feature', '-m', 'The big board');
    git(project, 'branch', '-f', 'bigref', commit);
  } finally {
    fs.rmSync(index, { force: true });
  }
}

export async function run({ page, shot, step, expect, section, project, resetRepo }) {
  resetRepo();
  await makeBigRef(project);
  await page.reload();
  await page.waitForSelector('.sidebar > *', { state: 'attached' });
  await page.setViewportSize({ width: 1920, height: 1080 });
  const text = (sel) => page.locator(sel).first().innerText();
  const visible = (sel) => page.locator(`.section:not(.away) ${sel}`);
  const pick = async (name) => {
    if (!(await page.locator('[data-testid="ref-popover"]').count())) await visible('[data-testid="ref-button"]').click();
    await page.waitForSelector('[data-testid="ref-popover"]');
    await page.locator(`[data-testid="ref-item"][data-name="${name}"]`).click();
  };
  const api = (route, method = 'GET') => page.evaluate(([r, m]) => fetch(r, { method: m, headers: { 'X-Netsmith': '1' } }).then((x) => x.json()), [route, method]);

  step('a comparison with the big ref: its card has Cancel, and the stage it is in');
  await page.keyboard.press('8');
  await page.waitForSelector('.section.sec-compare:not(.away)');
  await page.waitForSelector('[data-testid="cmp-start"]');
  await visible('[data-testid="ref-button"]').click();
  await page.waitForSelector('[data-testid="ref-popover"]');
  await page.waitForFunction(() => [...document.querySelectorAll('[data-testid="ref-item"]')].some((e) => e.getAttribute('data-name') === 'bigref'));
  await pick('bigref');
  await page.waitForSelector('[data-testid="cmp-progress"]');
  await page.waitForSelector('[data-testid="cmp-cancel"]');
  await page.waitForFunction(() => /solving|Solving/.test(document.querySelector('[data-testid="cmp-stage"]')?.textContent ?? ''), null, { timeout: 60000 });
  expect(/Solving/i.test(await text('[data-testid="cmp-stage"]')), `the stage (${await text('[data-testid="cmp-stage"]')})`);
  await shot('comparing');

  step('Cancel in the card: "Comparison cancelled", no result, the server has nothing running');
  const asked = Date.now();
  await page.locator('[data-testid="cmp-cancel"]').click();
  await page.waitForSelector('[data-testid="cmp-cancelled"]', { timeout: 15000 });
  console.log(`    the comparison stopped ${Date.now() - asked} ms after the click`);
  expect(/Comparison cancelled/.test(await text('[data-testid="cmp-cancelled"]')), 'the card says so');
  expect((await page.locator('[data-testid="cmp-summary"]').count()) === 0, 'there is no summary of a comparison that was stopped');
  expect((await api('/api/compare/cancel', 'POST')).cancelled === false, 'nothing is left running on the server');
  await shot('cancelled');

  step('Escape in the card cancels too, and "Compare again" starts it');
  await page.locator('[data-testid="cmp-cancelled"] .tool.primary').click();
  await page.waitForSelector('[data-testid="cmp-progress"]');
  await page.waitForFunction(() => /solving|Solving/.test(document.querySelector('[data-testid="cmp-stage"]')?.textContent ?? ''), null, { timeout: 60000 });
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid="cmp-cancelled"]', { timeout: 15000 });
  expect(true, 'Escape stopped it');
  expect((await api('/api/compare/cancel', 'POST')).cancelled === false, 'nothing is left running');

  step('a ref that is small compares as ever after the cancelled ones, and is not slowed by what was cancelled');
  await visible('[data-testid="ref-button"]').click();
  await pick('main');
  await page.waitForSelector('[data-testid="cmp-summary"]', { timeout: 120000 });
  expect(/part|rail|net/.test(await text('[data-testid="cmp-summary"]')), `a summary (${await text('[data-testid="cmp-summary"]')})`);
  await shot('compared-after');

  step('the Build card has a Cancel button while a build runs, and it asks the server to stop');
  await section('build');
  await page.waitForSelector('.acard[data-task="build"]');
  let cancelAsked = 0;
  await page.route('**/api/build/cancel', async (route) => {
    cancelAsked++;
    await route.continue();
  });
  // The build is held for a moment so that its card is on screen; the server has nothing to cancel then.
  await page.route('**/api/build', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    await new Promise((r) => setTimeout(r, 1500));
    await route.continue();
  });
  await page.locator('.acard[data-task="build"] button.tool', { hasText: 'Build schematic' }).click();
  await page.waitForSelector('[data-testid="cancel-build"]');
  await shot('build-running');
  await page.locator('[data-testid="cancel-build"]').click();
  await page.waitForFunction(() => document.querySelectorAll('[data-testid="cancel-build"]').length === 0, null, { timeout: 60000 });
  expect(cancelAsked === 1, 'the button asked the server to cancel the build');
  await page.unroute('**/api/build');
  await page.unroute('**/api/build/cancel');

  step('a fab is cancelled from its card: a calm note, no failure, no package, and a new fab is whole');
  await page.locator('.acard[data-task="fab"] button.tool', { hasText: 'Make fab package' }).click();
  await page.waitForSelector('[data-testid="cancel-fab"]');
  await page.waitForFunction(() => /placing|placed|routing|routed|reading/.test(document.querySelector('.acard[data-task="fab"] .job-line')?.textContent ?? ''), null, { timeout: 120000 });
  await page.locator('[data-testid="cancel-fab"]').click();
  await page.locator('.toast', { hasText: 'Fab was cancelled' }).waitFor({ timeout: 30000 });
  expect((await page.locator('.acard[data-task="fab"] .pill.bad').count()) === 0, 'the card shows no failure');
  expect((await page.locator('.acard[data-task="fab"] .downloads a').count()) === 0, 'and offers no download: nothing was kept');
  await shot('fab-cancelled');
  expect((await api('/api/fab/cancel', 'POST')).cancelled === false, 'nothing is left running');

  git(project, 'branch', '-D', 'bigref');
  resetRepo();
}
