// The schematic layout file (ADR 0061, stage 3): the server reads `src/main.sch.toml`, the schematic is drawn with it, what is
// wrong with it is in the Problems list (stage `layout`, the file and line), and `GET` / `PUT /api/schematic-hints` read and write it.
// The page has no UI for it yet (stage 4), so the recipe drives the API with `fetch` from the page, and writes and deletes the file
// as another program would. A request that is refused is made from the recipe's own `fetch`, not the page's: the page would log it as
// a console error, which fails a recipe.
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project }) {
  const file = path.join(project, 'src/main.sch.toml');
  const board = 'TwoBoards/led';
  fs.rmSync(file, { force: true });
  const base = new URL(page.url()).origin;

  const get = () => page.evaluate(async (b) => (await fetch(`/api/schematic-hints?board=${encodeURIComponent(b)}`, { headers: { Accept: 'application/json' } })).json(), board);
  const put = (text) =>
    page.evaluate(
      async ([b, body]) => {
        const r = await fetch(`/api/schematic-hints?board=${encodeURIComponent(b)}`, { method: 'PUT', headers: { 'X-Netsmith': '1', Accept: 'application/json', 'Content-Type': 'text/plain' }, body });
        return { status: r.status, body: await r.json() };
      },
      [board, text],
    );
  const svg = () => page.evaluate(async (b) => (await fetch(`/api/schematic.svg?board=${encodeURIComponent(b)}`, { cache: 'no-store' })).text(), board);
  const analysis = () => page.evaluate(async () => (await fetch('/api/analysis', { cache: 'no-store' })).json());
  const layoutProblems = async () => (await analysis()).problems.filter((p) => p.stage === 'layout');

  await page.waitForSelector('.schematic');
  step('pick the LED board and find its connector by its path, not by a designator');
  await page.selectOption('.boardpick select', board);
  await page.waitForSelector('.schematic[data-board="LedBoard"]');
  const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
  const connector = detail.parts.find((p) => p.path === 'power_in');
  const resistor = detail.parts.find((p) => p.path === 'r1');
  expect(!!connector && !!resistor, 'the LED board has a connector `power_in` and a resistor `r1`');
  // Where the connector is on the page, from the resistor: a move of the connector's block moves it and not the resistor.
  const gap = () =>
    page.evaluate(
      ([a, b]) => {
        const box = (ref) => document.querySelector(`.schematic .part[data-ref="${ref}"]`)?.getBoundingClientRect();
        const [p, q] = [box(a), box(b)];
        return p && q ? { x: Math.round((p.left + p.width / 2) - (q.left + q.width / 2)), y: Math.round((p.top + p.height / 2) - (q.top + q.height / 2)) } : null;
      },
      [connector.refdes, resistor.refdes],
    );
  const before = await gap();
  const drawingBefore = await svg();
  await shot('without-a-file');

  step('with no file there are no hints and nothing is wrong');
  const none = await get();
  expect(none.file === 'src/main.sch.toml' && !none.exists && none.stamp === 'none' && none.text === '', `no file: ${none.file}, exists ${none.exists}, stamp ${none.stamp}`);
  expect(none.errors.length === 0 && none.findings.length === 0 && Object.keys(none.boards).length === 0, 'no faults, no findings, no tables');
  expect((await layoutProblems()).length === 0, 'no problem of stage layout');

  step('a PUT writes the canonical text, the drawing has it, and the page redraws without a reload');
  const written = await put('# a comment\nversion=1\n[LedBoard.parts.power_in]\ndx = 25.40\ndy = 0\n');
  expect(written.status === 200, `the PUT is answered 200 (${written.status})`);
  const disk = fs.readFileSync(file, 'utf8');
  expect(disk === 'version = 1\n\n[LedBoard.parts.power_in]\ndx = 25.4\ndy = 0\n', `the file is the canonical text, with no comment (${JSON.stringify(disk)})`);
  expect(written.body.exists && written.body.text === disk && written.body.boards.LedBoard.parts.power_in.dx === 25.4, 'the answer is the file as the drawing now has it');
  const moved = await svg();
  expect(moved !== drawingBefore, 'the drawing from the server changed');
  await page.waitForFunction(([a, b, x]) => {
    const box = (ref) => document.querySelector(`.schematic .part[data-ref="${ref}"]`)?.getBoundingClientRect();
    const [p, q] = [box(a), box(b)];
    return p && q && Math.round((p.left + p.width / 2) - (q.left + q.width / 2)) !== x;
  }, [connector.refdes, resistor.refdes, before.x], { timeout: 15000 });
  const after = await gap();
  expect(after.x !== before.x, `the page redrew, and the connector is ${after.x - before.x} px from where it was, relative to the resistor`);
  await shot('moved');

  step('a file edited by another program is read too, and a finding for an entry that matches nothing is a note at its line');
  const text = 'version = 1\n\n[LedBoard.parts.power_in]\ndx = 25.4\ndy = 0\n\n[LedBoard.parts.gone]\ndx = 1.27\ndy = 0\n\n[Retired.parts.x]\ndx = 1.27\ndy = 0\n';
  fs.writeFileSync(file, text);
  await page.waitForFunction(async () => (await (await fetch('/api/analysis', { cache: 'no-store' })).json()).problems.some((p) => p.stage === 'layout'), null, { timeout: 15000 });
  const lineOf = (needle) => text.split('\n').findIndex((l) => l.includes(needle)) + 1;
  const found = await layoutProblems();
  expect(found.length === 2 && found.every((p) => p.severity === 'note' && p.at.file === 'src/main.sch.toml'), `two notes at the file (${found.map((p) => p.message).join(' | ')})`);
  const gone = found.find((p) => p.message.includes('`gone`'));
  const retired = found.find((p) => p.message.includes('`Retired`'));
  expect(gone?.at.line === lineOf('LedBoard.parts.gone') && gone.board === board, `the stale entry is at line ${gone?.at.line}, of this board`);
  expect(retired?.at.line === lineOf('Retired.parts.x') && !retired.board, `the table of a board the source lacks is at line ${retired?.at.line}, and no board's`);
  const read = await get();
  expect(read.text === text && read.findings.length === 2 && read.findings[0].kind === 'stale_entry' && read.findings[1].kind === 'unknown_board', 'GET has the text and the same two findings');
  expect(read.boards.LedBoard.parts.gone.line === lineOf('LedBoard.parts.gone'), 'and each entry with its line');
  expect((await svg()) === moved, 'the entries that match nothing leave the drawing as it was');

  step('a file that does not read: a warning in the dock at its line, and the drawing is the one without a file');
  const broken = 'version = 1\n\n[LedBoard.parts.power_in]\ndx = 25.4\ndy = 0\nspin = 3\n';
  fs.writeFileSync(file, broken);
  await page.waitForFunction(async () => (await (await fetch('/api/analysis', { cache: 'no-store' })).json()).problems.some((p) => p.stage === 'layout' && p.severity === 'warning'), null, { timeout: 15000 });
  expect((await svg()) === drawingBefore, 'the drawing is byte for byte the one without a file');
  const bad = (await layoutProblems())[0];
  expect(bad.at.file === 'src/main.sch.toml' && bad.at.line === 6 && bad.message.includes('the file is ignored'), `at line ${bad.at.line}: ${bad.message}`);
  expect((await analysis()).errors === 0, 'it is no error: the project builds');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('.dock .problem.warning');
  const row = (await page.locator('.dock .problem.warning').first().innerText()).replace(/\s+/g, ' ');
  expect(row.includes('Layout') && row.includes('main.sch.toml:6'), `the dock row says the stage and the place (${row})`);
  await shot('dock-with-a-broken-file');
  await page.keyboard.press('Control+j');

  step('a refused PUT: no header is 403, text that does not read is 422 with its line, a stale stamp is 409; the file stays');
  const request = (method, headers, body, query = `board=${encodeURIComponent(board)}`) => fetch(`${base}/api/schematic-hints?${query}`, { method, headers, body });
  const noHeader = await request('PUT', { 'Content-Type': 'text/plain' }, 'version = 1\n');
  expect(noHeader.status === 403, `a PUT without X-Netsmith is refused (${noHeader.status})`);
  const invalid = await request('PUT', { 'X-Netsmith': '1', Accept: 'application/json' }, 'version = 1\n\n[LedBoard.parts.power_in]\ndx = 1.3\ndy = 0\n');
  const detail422 = await invalid.json();
  expect(invalid.status === 422 && detail422.code === 'schematic_hints_invalid' && detail422.line === '4' && detail422.file === 'src/main.sch.toml', `422 ${JSON.stringify(detail422)}`);
  const stale = await request('PUT', { 'X-Netsmith': '1', 'X-Stamp': '1-1' }, 'version = 1\n');
  expect(stale.status === 409, `a stale stamp is a conflict (${stale.status})`);
  expect(fs.readFileSync(file, 'utf8') === broken, 'the file was not touched by any of the three');
  expect((await request('GET', {}, undefined, 'board=Nope')).status === 404, 'an unknown board is 404');

  step('deleting the file brings the first drawing back');
  fs.rmSync(file);
  await page.waitForFunction(async () => (await (await fetch('/api/analysis', { cache: 'no-store' })).json()).problems.every((p) => p.stage !== 'layout'), null, { timeout: 15000 });
  expect((await svg()) === drawingBefore, 'the drawing is byte for byte the first one');
  await page.waitForFunction(([a, b, x]) => {
    const box = (ref) => document.querySelector(`.schematic .part[data-ref="${ref}"]`)?.getBoundingClientRect();
    const [p, q] = [box(a), box(b)];
    return p && q && Math.round((p.left + p.width / 2) - (q.left + q.width / 2)) === x;
  }, [connector.refdes, resistor.refdes, before.x], { timeout: 15000 });
  expect(!(await get()).exists, 'GET says there is no file');
  await shot('back');
}
