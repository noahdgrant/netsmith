// The padlock and the move of the pin map write the pin file's `lock`, `pin` and `source` lines and no other (ADR 0101, ED2 of the pin
// epic). The project has a part with three requests: `status` (locked by hand in the file), `vbat` (held by the file, picked by the
// tool) and `spare` (no entry in the file yet). The recipe clicks the padlock of `spare` (the entry is written), unlocks `status`,
// moves `vbat` with the keyboard (Tab to the pin, Enter), is refused by the server for a pin another request holds (a toast with the
// server's words, nothing written), drags `status` onto a free pin, and asserts the text of the pin file on disk after each step. At
// the end it puts the file back, so the other recipes of this fixture find it as it was.
import { Hung, whenIdle } from '../settle.mjs';
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'pin_map';

export async function run({ page, shot, step, expect, project, allowConsole }) {
  // The browser logs each refusal (422, 404, 403) the recipe makes on purpose; only those routes are let through.
  allowConsole(/status of 4\d\d .*\/api\/pins\/(move|lock)/);
  const src = (name) => path.join(project, 'src', name);
  const read = (name = 'main.pins.toml') => fs.readFileSync(src(name), 'utf8');
  const original = read();
  const nsm = read('main.nsm');
  const lockFile = read('main.lock.toml');
  const detail = () => page.evaluate(async () => (await fetch('/api/board?name=PinMapBoard', { cache: 'no-store' })).json());
  const until = (fn, what, arg) =>
    page.waitForFunction(fn, arg).catch((e) => {
      throw e.name === 'TimeoutError' ? new Hung(what) : e;
    });
  const row = (request) => page.locator(`[data-testid="pin-map"] .pm-row[data-request="${request}"]`);
  const state = () =>
    page.evaluate(() =>
      Object.fromEntries(
        [...document.querySelectorAll('[data-testid="pin-map"] .pm-row')].map((r) => [
          r.getAttribute('data-request'),
          {
            pin: r.querySelector('.pm-name')?.textContent ?? '',
            pressed: r.querySelector('[data-testid="pm-padlock"]')?.getAttribute('aria-pressed'),
            tag: !!r.querySelector('.pm-lock'),
            origin: r.querySelector('.pm-origin')?.textContent ?? '',
          },
        ]),
      ),
    );
  // The toast of a refusal: the server's own words.
  const toastText = () => page.locator('.toast.bad').first().innerText();

  await page.waitForSelector('.schematic[data-board="PinMapBoard"]');
  step('select the microcontroller by its path in the source');
  const part = (await detail()).parts.find((p) => p.path === 'u');
  await page.locator(`.schematic .part[data-ref="${part.refdes}"]`).click();
  if (!(await page.locator('.sinspector').count())) await page.getByRole('button', { name: 'Details' }).click();
  await page.waitForSelector('[data-testid="pin-map"]');
  let s = await state();
  expect(s.status.pressed === 'true' && s.status.tag && s.status.pin === 'p01', `status is locked on p01 (${JSON.stringify(s.status)})`);
  expect(s.spare.pressed === 'false' && !s.spare.tag && s.spare.origin === 'picked now', `spare is not locked, picked now (${JSON.stringify(s.spare)})`);
  expect((await row('spare').locator('[data-testid="pm-padlock"]').getAttribute('aria-label')) === 'Lock the pin of spare', 'the padlock is named for its request');
  expect(
    (await row('spare').locator('[data-testid="pm-padlock"]').getAttribute('title')).includes('Click to lock it (writes `lock = true` in src/main.pins.toml)'),
    'the padlock\'s tooltip says in plain words what a click writes',
  );
  await shot('before');

  step('the padlock of a request the file has no entry for: it writes the entry, locked, with the pin the run picked');
  await row('spare').locator('[data-testid="pm-padlock"]').click();
  await until(() => document.querySelector('.pm-row[data-request="spare"] .pm-lock'), 'spare shows locked');
  const entry = '\n[PinMapBoard.pins.spare]\npin = "p00"\nfunction = "GPIO"\nlock = true\nsource = "auto"\n';
  expect(read() === original + entry, `the file is the old file and one new entry, byte for byte:\n${read()}`);
  s = await state();
  expect(s.spare.pressed === 'true' && s.spare.pin === 'p00' && s.spare.origin === 'file, auto', `spare is locked on p00, from the file (${JSON.stringify(s.spare)})`);
  await shot('spare-locked');

  step('unlock `status`: only its `lock` line goes');
  const afterLock = read();
  await row('status').locator('[data-testid="pm-padlock"]').click();
  await until(() => !document.querySelector('.pm-row[data-request="status"] .pm-lock'), 'status shows unlocked');
  expect(read() === afterLock.replace('pin = "p01"\nfunction = "GPIO"\nlock = true\n', 'pin = "p01"\nfunction = "GPIO"\n'), `only the lock line of status went:\n${read()}`);
  s = await state();
  expect(s.status.pressed === 'false' && s.status.pin === 'p01', `status is unlocked on p01 (${JSON.stringify(s.status)})`);

  step('move `vbat` with the keyboard: Enter on Move opens the list of pins that offer ANALOG, Tab walks it, Enter moves');
  const afterUnlock = read();
  await row('vbat').locator('[data-testid="pm-move"]').focus();
  await page.keyboard.press('Enter');
  await page.waitForSelector('.pm-row[data-request="vbat"] .pm-targets');
  const targets = await row('vbat').locator('.pm-target').evaluateAll((els) => els.map((e) => [e.getAttribute('data-pin'), e.getAttribute('data-state'), e.getAttribute('aria-label')]));
  expect(JSON.stringify(targets.map((t) => t.slice(0, 2))) === '[["p02","current"],["p03","free"]]', `ANALOG is offered by p02 (now) and p03 (free): ${JSON.stringify(targets)}`);
  expect(targets[1][2] === 'p03, pad 6', `a target is named by its pin and pad (${targets[1][2]})`);
  await shot('move-list');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  expect((await page.evaluate(() => document.activeElement?.getAttribute('data-pin'))) === 'p03', 'Tab reaches p03');
  await page.keyboard.press('Enter');
  await until(() => document.querySelector('.pm-row[data-request="vbat"] .pm-name')?.textContent === 'p03', 'vbat is on p03');
  expect(
    read() === afterUnlock.replace('pin = "p02"\nfunction = "ANALOG"\nsource = "auto"', 'pin = "p03"\nfunction = "ANALOG"\nsource = "hand"'),
    `the pin and source lines of vbat changed, nothing else:\n${read()}`,
  );
  s = await state();
  expect(s.vbat.pin === 'p03' && s.vbat.origin === 'file, hand', `vbat is on p03, chosen by hand (${JSON.stringify(s.vbat)})`);
  expect((await row('vbat').locator('.pm-targets').count()) === 0, 'the list closed once the move was written');
  expect((await page.evaluate(() => document.activeElement?.getAttribute('data-testid'))) === 'pm-move', 'the keyboard is back on the Move button');
  await shot('vbat-moved');

  step('a pin another request holds is refused with the server\'s message, and the file is not written');
  const afterMove = read();
  await row('status').locator('[data-testid="pm-move"]').click();
  await page.waitForSelector('.pm-row[data-request="status"] .pm-targets');
  const held = row('status').locator('.pm-target[data-pin="p00"]');
  expect((await held.getAttribute('data-state')) === 'held', 'p00 is shown as held (spare has it)');
  await held.click();
  await page.waitForSelector('.toast.bad');
  const said = await toastText();
  expect(said.includes('`status`: the pin file was not written') && said.includes('p00 is held by `spare`, which is locked'), `the toast has the server's words (${said})`);
  expect(read() === afterMove, 'the file keeps its bytes');
  // The toast slides in; look at it settled.
  await whenIdle(page);
  await shot('refused');

  step('a locked request has no move: its Move button says why');
  const spareMove = row('spare').locator('[data-testid="pm-move"]');
  expect((await spareMove.getAttribute('aria-disabled')) === 'true' && (await spareMove.getAttribute('title')) === 'Unlock `spare` to move it.', 'a locked request cannot be moved');

  step('drag the pin of `status` onto a free pin of its list');
  const free = await row('status').locator('.pm-target[data-state="free"]').evaluateAll((els) => els.map((e) => e.getAttribute('data-pin')));
  expect(free.join() === 'p02', `p02 is the free pin (${free})`);
  await row('status').locator('[data-testid="pm-pin"]').dragTo(row('status').locator('.pm-target[data-pin="p02"]'));
  await until(() => document.querySelector('.pm-row[data-request="status"] .pm-name')?.textContent === 'p02', 'status is on p02');
  expect(read() === afterMove.replace('pin = "p01"\nfunction = "GPIO"', 'pin = "p02"\nfunction = "GPIO"'), `only the pin line of status changed (it was chosen by hand already):\n${read()}`);
  await shot('status-dragged');

  step('the server refuses what the page does not offer, and a change with no guard header');
  const afterDrag = read();
  const post = (url, body, headers = { 'X-Netsmith': '1' }) =>
    page.evaluate(
      async ([u, b, h]) => {
        const r = await fetch(u, { method: 'POST', headers: { ...h, 'Content-Type': 'application/json' }, body: JSON.stringify(b) });
        return { status: r.status, text: (await r.text()).trim() };
      },
      [url, body, headers],
    );
  const move = (request, pin) => post('/api/pins/move', { board: 'PinMapBoard', request, pin });
  let r = await move('vbat', 'p01');
  expect(r.status === 422 && r.text === 'p01 does not offer ANALOG, which `vbat` asks `u` for', `a pin that does not offer the function (${r.status} ${r.text})`);
  r = await move('vbat', 'p99');
  expect(r.status === 422 && r.text === '`u` has no pin p99', `an unknown pin (${r.status} ${r.text})`);
  r = await move('status', 'p03');
  expect(r.status === 422 && r.text.startsWith('p03 is held by `vbat`'), `a taken pin (${r.status} ${r.text})`);
  r = await move('spare', 'p01');
  expect(r.status === 422 && r.text === '`spare` is locked: unlock it before moving it', `a locked request (${r.status} ${r.text})`);
  r = await move('nope', 'p01');
  expect(r.status === 404, `a request that is not there (${r.status})`);
  r = await post('/api/pins/lock', { board: 'PinMapBoard', request: 'status', locked: true }, {});
  expect(r.status === 403, `a write with no X-Netsmith header (${r.status})`);
  expect(read() === afterDrag, 'none of the refusals wrote the file');

  step('no other file was written: the source and the pick lock have the bytes they had');
  expect(read('main.nsm') === nsm && read('main.lock.toml') === lockFile, 'the source and the lock are as they were');

  step('put the pin file back, as a person would with their editor: the map shows the first state again');
  fs.writeFileSync(src('main.pins.toml'), original);
  await until(() => document.querySelector('.pm-row[data-request="status"] .pm-name')?.textContent === 'p01' && document.querySelector('.pm-row[data-request="status"] .pm-lock'), 'the map is as it was');
  s = await state();
  expect(s.spare.pressed === 'false' && s.spare.origin === 'picked now' && s.vbat.pin === 'p02', 'spare is picked now again, vbat is on p02');
}
