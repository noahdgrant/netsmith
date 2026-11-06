// Cancelling the analysis (ADR 0062): a file is changed so that the analysis of it is long, the status bar says how far the
// solver has got (parts chosen, with the total), Cancel stops it, the views go on showing the reading before it, the bar says
// so, and the next change to a file (or "Run again") analyses again. The fixture is the `big` board; the file written is the
// same board with forty rails, which the solver works at for much longer than a person needs to press a button.
import fs from 'node:fs';
import path from 'node:path';

import { bigSource } from '../fixtures/big.mjs';
import { sheetNames } from '../schematic-helpers.mjs';

export const fixture = 'big';

export async function run({ page, shot, step, expect, section, project }) {
  const main = path.join(project, 'src/main.nsm');
  const small = fs.readFileSync(main, 'utf8');
  const bar = '.statusbar';
  const analysingText = () => page.locator(`${bar} .analysing-text`).first().innerText();
  const api = (route) => page.evaluate((r) => fetch(r, { cache: 'no-store' }).then((x) => x.json()), route);
  const idle = () => page.waitForSelector(`${bar} .item.idle`, { timeout: 120000 });
  const startLong = async () => {
    fs.writeFileSync(main, bigSource(40));
    await page.waitForSelector(`${bar} .cancel-analysis`, { timeout: 30000 });
  };

  await section('schematic');
  await idle();
  step('the first reading: eight rails, the drawing of stage8 and no stage9');
  const before = await sheetNames(page);
  expect(before.includes('stage8') && !before.includes('stage9'), `the sheets of the small board (${before.length})`);
  const read = await api('/api/analysis');
  expect(read.cancelled === undefined, 'the server does not call it cancelled');
  const version = read.version;
  await shot('analysed');

  step('a file changes and the analysis of it is long: the bar says how far the solver has got, and offers Cancel');
  await startLong();
  await page.waitForFunction(() => /\d+ of \d+ parts chosen/.test(document.querySelector('.statusbar .analysing-text')?.textContent ?? ''), null, { timeout: 60000 });
  const first = await analysingText();
  const count = (t) => Number(/(\d+) of (\d+) parts chosen/.exec(t)?.[1]);
  const total = Number(/of (\d+) parts chosen/.exec(first)?.[1]);
  expect(/Solving/.test(first) && total > 100, `the stage, what it counts and the total (${first})`);
  expect(!/%/.test(first), 'never a percentage');
  await page.waitForTimeout(1500);
  const second = await analysingText();
  expect(count(second) >= count(first) && Number(/of (\d+) parts/.exec(second)?.[1]) === total, `the count only grows and the total is the same (${first} -> ${second})`);
  expect((await page.locator(`${bar} .cancel-analysis`).innerText()).includes('Cancel'), 'the bar has a Cancel button');
  await shot('solving');

  step('Cancel: "Cancelling…", then "Analysis cancelled"; the old drawing stays; the version did not move');
  const asked = Date.now();
  await page.locator(`${bar} .cancel-analysis`).click();
  await page.waitForSelector(`${bar} .item.cancelled`, { timeout: 15000 });
  const latency = Date.now() - asked;
  console.log(`    cancel heard after ${latency} ms`);
  expect(latency < 3000, `the analysis stopped within a few seconds of the click (${latency} ms)`);
  expect((await page.locator(`${bar} .item.cancelled`).innerText()).includes('Analysis cancelled'), 'the bar says it was cancelled');
  expect((await page.locator(`${bar} .item.cancelled`).innerText()).includes('Run again'), 'and offers to run it again');
  expect((await page.locator(`${bar} .analysing`).count()) === 0, 'it is no longer analysing');
  const after = await sheetNames(page);
  expect(after.length === before.length && after.includes('stage8') && !after.includes('stage9'), 'the views still show the small board: the reading before the cancelled analysis');
  const cancelled = await api('/api/analysis');
  expect(cancelled.cancelled === true && cancelled.version === version, `the server says it was cancelled and its version is the one before (${cancelled.version})`);
  expect(cancelled.boards.length === read.boards.length && cancelled.errors === 0, 'with the same boards and no error');
  // Not begun again behind the page's back.
  await page.waitForTimeout(1500);
  expect((await page.locator(`${bar} .analysing`).count()) === 0, 'the watcher does not start it again');
  await shot('cancelled');

  step('the next change to a file analyses again, and the note goes');
  fs.writeFileSync(main, `${small}\n// an edit after the cancel\n`);
  await idle();
  expect((await page.locator(`${bar} .item.cancelled`).count()) === 0, 'no note of the cancelled analysis');
  const next = await api('/api/analysis');
  expect(next.version > version && next.cancelled === undefined && next.errors === 0, `a new reading (version ${next.version})`);
  await shot('analysed-again');

  step('Run again: cancel the long analysis, press the note, the analysis begins again, and is cancelled once more');
  await startLong();
  await page.locator(`${bar} .cancel-analysis`).click();
  await page.waitForSelector(`${bar} .item.cancelled`, { timeout: 15000 });
  await page.locator(`${bar} .item.cancelled`).click();
  await page.waitForSelector(`${bar} .cancel-analysis`, { timeout: 15000 });
  expect((await page.locator(`${bar} .item.cancelled`).count()) === 0, 'the note is gone while it runs');
  await page.locator(`${bar} .cancel-analysis`).click();
  await page.waitForSelector(`${bar} .item.cancelled`, { timeout: 15000 });
  expect(true, 'cancelled again');

  step('put the small board back: it analyses, and the page is as it was');
  fs.writeFileSync(main, small);
  await idle();
  const last = await sheetNames(page);
  expect(last.includes('stage8') && !last.includes('stage9'), 'the small board');
}
