// The pin map of the selected part and the pick lock of the board, read only (ADR 0101, ADR 0100). The project has a part with three
// pin requests (one locked in the pin file by hand, one held by it, one picked by this reading) and a pick lock that is not current.
// Select the part in the schematic and read the panel's rows; run `netsmith lock --update` in the project as a person would, and the
// lock panel says it is current without the page being touched (the lock file is watched); delete it and it says it is absent; and a
// request nothing offers shows the check's own message in the pin map. The editor writes neither file: the recipe checks their bytes.
import { Hung, whenIdle } from '../settle.mjs';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'pin_map';

export async function run({ page, shot, step, expect, project, binary }) {
  const src = (name) => path.join(project, 'src', name);
  const original = fs.readFileSync(src('main.nsm'), 'utf8');
  const pinsBefore = fs.readFileSync(src('main.pins.toml'), 'utf8');
  const lockBefore = fs.readFileSync(src('main.lock.toml'), 'utf8');
  const detail = () => page.evaluate(async () => (await fetch('/api/board?name=PinMapBoard', { cache: 'no-store' })).json());
  const until = (fn, what) =>
    page.waitForFunction(fn, null).catch((e) => {
      throw e.name === 'TimeoutError' ? new Hung(what) : e;
    });

  await page.waitForSelector('.schematic[data-board="PinMapBoard"]');
  step('find the microcontroller by its path in the source, not by a designator');
  const part = (await detail()).parts.find((p) => p.path === 'u');
  expect(!!part, 'the board has a part `u`');
  await page.locator(`.schematic .part[data-ref="${part.refdes}"]`).click();
  // The inspector is behind the Details button when the window is narrow.
  if (!(await page.locator('.sinspector').count())) await page.getByRole('button', { name: 'Details' }).click();
  await page.waitForSelector('[data-testid="pin-map"]');

  step('the pin map lists the three requests in the order of their paths, each with its pin and pad');
  const rows = () =>
    page.evaluate(() =>
      [...document.querySelectorAll('[data-testid="pin-map"] .pm-row')].map((r) => ({
        request: r.getAttribute('data-request'),
        pin: r.querySelector('.pm-name')?.textContent ?? '',
        text: r.textContent.replace(/\s+/g, ' ').trim(),
        locked: !!r.querySelector('.pm-lock'),
        origin: r.querySelector('.pm-origin')?.textContent ?? '',
        message: r.querySelector('.pm-msg')?.textContent ?? '',
      })),
    );
  let list = await rows();
  expect(list.map((r) => r.request).join() === 'spare,status,vbat', `the requests are in path order (${list.map((r) => r.request)})`);
  const byName = (name) => list.find((r) => r.request === name);
  expect(byName('status').pin === 'p01' && byName('status').text.includes('pad 4'), `status has p01, pad 4 (${byName('status').text})`);
  expect(byName('status').locked && byName('status').origin === 'file, hand', `status is locked, from the file, by hand (${byName('status').origin})`);
  expect(byName('vbat').pin === 'p02' && !byName('vbat').locked && byName('vbat').origin === 'file, auto', `vbat has p02, unlocked, from the file (${byName('vbat').origin})`);
  expect(byName('spare').pin === 'p00' && byName('spare').origin === 'picked now', `spare has p00, picked now (${byName('spare').origin})`);
  expect(list.every((r) => r.message === ''), 'no request lacks a pin');
  expect((await page.locator('.insp-top').innerText()).includes('Pin map'), 'the section is in the part inspector');
  await shot('pin-map');

  step('a part no request asks has no pin map');
  const other = (await detail()).parts.find((p) => p.path === 'load');
  const pick = (refdes) => page.locator('table.parts tr', { has: page.locator('td.ref', { hasText: new RegExp(`^${refdes}$`) }) }).click();
  await pick(other.refdes);
  await page.waitForSelector(`section[aria-label="Part ${other.refdes}"]`);
  expect((await page.locator('[data-testid="pin-map"]').count()) === 0, 'the resistor has no pin map');
  await pick(part.refdes);
  await page.waitForSelector('[data-testid="pin-map"]');

  step('the pick lock is not current, and lists what `lock --check` would say');
  const lock = () =>
    page.evaluate(() => {
      const state = document.querySelector('[data-testid="pick-lock-state"]')?.getAttribute('data-state');
      const p = document.querySelector('[data-testid="pick-lock"]');
      return state ? { state, file: p?.querySelector('.defs')?.textContent ?? '', lines: p ? [...p.querySelectorAll('.lock-diff li')].map((l) => l.textContent) : [] } : null;
    });
  await page.waitForSelector('[data-testid="pick-lock-state"]');
  let state = await lock();
  expect(state.state === 'not current' && state.file.includes('src/main.lock.toml'), `the lock is not current and names its file (${JSON.stringify(state)})`);
  expect(state.lines.length === 1 && state.lines[0].startsWith('PinMapBoard: `pullup`: dropped'), `one line: the part that is gone (${state.lines})`);
  expect((await page.locator('.pm-row').count()) === 3 && (await page.locator('[data-testid="pick-lock"]').isVisible()), 'the lock section is open beside the pin map, with the part selected');
  await shot('lock-not-current');

  step('the editor writes neither file');
  expect(fs.readFileSync(src('main.pins.toml'), 'utf8') === pinsBefore, 'the pin file has the bytes it had');
  expect(fs.readFileSync(src('main.lock.toml'), 'utf8') === lockBefore, 'the lock has the bytes it had');

  step('`netsmith lock --update` in the project, as a person runs it: the page says the lock is current without being touched');
  execFileSync(binary, ['lock', '--update'], { cwd: project, stdio: 'pipe' });
  await page.waitForSelector('[data-testid="pick-lock-state"][data-state="current"]');
  state = await lock();
  expect(state.lines.length === 0, 'a current lock has no diff');
  expect((await page.locator('[data-testid="pick-lock"]').count()) === 0, 'a lock that is current is folded');
  await page.locator('.isec', { hasText: 'Pick lock' }).locator('.disc-toggle').click();
  await page.waitForSelector('[data-testid="pick-lock"]');
  await shot('lock-current');

  step('delete the lock: it is absent, and the editor does not write it back');
  fs.rmSync(src('main.lock.toml'));
  await page.waitForSelector('[data-testid="pick-lock-state"][data-state="absent"]');
  await page.waitForSelector('[data-testid="pick-lock"]');
  expect((await page.locator('.lock-says').innerText()).includes('netsmith build'), 'it says what writes the lock');
  await whenIdle(page);
  expect(!fs.existsSync(src('main.lock.toml')), 'the lock is still absent');
  await shot('lock-absent');

  step('a request nothing offers shows the check\'s message in the pin map, with the others still there');
  fs.writeFileSync(src('main.nsm'), original.replace('.spare ~ .pull.a', '.nope: .u.GPIOO\n    .nope ~ .pull.b\n    .spare ~ .pull.a'));
  await page.waitForSelector('[data-testid="pin-map"] .pm-row[data-request="nope"] .pm-msg');
  list = await rows();
  expect(list.map((r) => r.request).join() === 'nope,spare,status,vbat', `four requests (${list.map((r) => r.request)})`);
  expect(byName('nope').pin === '' && byName('nope').message.includes('`nope` asks for GPIOO on `u` (`Zmcu4`), and no pin of `Zmcu4` offers it'), `the check's own words (${byName('nope').message})`);
  expect(byName('status').pin === 'p01', 'the other requests keep their pins');
  const problems = await page.evaluate(async () => (await (await fetch('/api/analysis', { cache: 'no-store' })).json()).problems.map((p) => p.message));
  expect(problems.includes(byName('nope').message), 'the same message is in the Problems list');
  expect((await page.locator('[data-testid="pick-lock-state"]').count()) === 0, 'a source that does not build has no lock to compare: the lock section is not shown');
  await shot('request-without-pin');

  step('mend the source: the message goes');
  fs.writeFileSync(src('main.nsm'), original);
  await until(() => !document.querySelector('[data-testid="pin-map"] .pm-msg'), 'the message is gone');
  await page.waitForSelector('[data-testid="pick-lock-state"]');
  expect(fs.readFileSync(src('main.pins.toml'), 'utf8') === pinsBefore, 'the pin file still has the bytes it had');
  // Put the lock back, so the recipe can run again in this session.
  fs.writeFileSync(src('main.lock.toml'), lockBefore);
}
