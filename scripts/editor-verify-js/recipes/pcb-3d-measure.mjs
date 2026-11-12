// The 3D view's measure tool (features/pcb-3d.md, "Tools"): M turns it on, a point snaps to a pad's
// centre (and says so while the pointer is over it), two clicks measure the distance with its dx,
// dy and dz, which is the distance of the two pads in the 2D scene; a measurement stays when the
// camera moves and when the tool is left; a picked part shows its height above its face and its
// margin to the height limit above it (the fixture's tall part is 6 mm over the lid's 3 mm); Esc
// leaves the tool and the clear-all control removes the measurements.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, openPcb, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'height_limits';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lidded';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

async function theme(page, label, id) {
  await page.keyboard.press('Control+Shift+P');
  await page.keyboard.type(`Theme: ${label}`);
  await page.keyboard.press('Enter');
  await page.waitForFunction((t) => document.documentElement.dataset.theme === t, id);
}

/** The pads of the 2D scene in the scene's frame of docs/BOARD3D.md (the sheet's y is down, from the outline's top-left corner). */
function padsOf(s) {
  const o = s.outline;
  return s.parts.flatMap((p) => p.pads.map((pad) => ({ part: p, pad, at: [pad.x - o.x, o.y + o.height - pad.y, 0] })));
}

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/height_limits.pcb.toml'));
  await openPcb(page, BOARD);
  await theme(page, 'Dark', 'dark');
  const s = await scene(page, BOARD);
  const tall = partOf(s, 'tall');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`, { timeout: 30000 });

  step('M turns the measure tool on: its panel says what to do');
  await page.locator(HOST).focus();
  await page.keyboard.press('t');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'top', HOST);
  await page.waitForTimeout(250);
  await page.keyboard.press('m');
  await page.waitForSelector('[data-testid=measure-panel]');
  expect((await page.getByTestId('tool-measure').getAttribute('aria-pressed')) === 'true', 'the Measure button is pressed');

  step("a point near a pad snaps to the pad's centre, and the snap target is shown while hovering");
  // A pad under a body is not snapped to from above (the body is in front of it): take two that are not, the farthest apart.
  const snapping = [];
  for (const p of padsOf(s)) {
    const at = await ask(page, 'toScreen', p.at);
    await page.mouse.move(at.x + 4, at.y + 3);
    const snap = await page
      .waitForSelector('[data-testid=measure-snap]', { timeout: 1500 })
      .then(() => page.locator('[data-testid=measure-snap]').evaluate((e) => ({ kind: e.dataset.kind, text: e.innerText })))
      .catch(() => null);
    if (snap?.kind === 'pad' && snap.text.includes(`Pad ${p.part.refdes}.${p.pad.number}`)) snapping.push({ ...p, screen: at });
  }
  let found = [];
  for (const x of snapping) for (const y of snapping) if (x !== y && (!found.length || Math.hypot(x.at[0] - y.at[0], x.at[1] - y.at[1]) > Math.hypot(found[0].at[0] - found[1].at[0], found[0].at[1] - found[1].at[1]))) found = [x, y];
  expect(found.length === 2, `pads snap where nothing stands over them (${snapping.map((f) => `${f.part.refdes}.${f.pad.number}`).join(', ')})`);
  await page.mouse.move(found[0].screen.x + 4, found[0].screen.y + 3);
  await page.waitForTimeout(100);
  await shot('measure-snap-pad');

  step('two clicks measure from one pad centre to the other: the distance and dx, dy, dz, as the 2D scene has them');
  for (const f of found) {
    await page.mouse.move(f.screen.x + 3, f.screen.y - 2);
    await page.waitForTimeout(80);
    await page.mouse.click(f.screen.x + 3, f.screen.y - 2);
  }
  await page.waitForSelector('[data-testid=measure-label]');
  const [a, b] = found;
  const dx = b.at[0] - a.at[0];
  const dy = b.at[1] - a.at[1];
  const d = Math.hypot(dx, dy);
  const label = await page.getByTestId('measure-label').innerText();
  const want = `${d.toFixed(3).replace(/0$/, '')} mm`;
  expect(label.includes(want) && label.includes(`dx ${dx.toFixed(3).replace(/0$/, '')}`) && label.includes('dz 0.00'), `the label: ${label.replace(/\n/g, ' | ')} (want ${want})`);
  const listed = await page.getByTestId('measure-list').innerText();
  expect(listed.includes(`Pad ${a.part.refdes}.${a.pad.number}`) && listed.includes(`Pad ${b.part.refdes}.${b.pad.number}`), `the panel lists it: ${listed.replace(/\n/g, ' | ')}`);

  step('the measurement stays when the camera moves');
  await page.keyboard.press('i');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'iso', HOST);
  await page.waitForTimeout(300);
  expect((await page.getByTestId('measure-label').count()) === 1, 'the label is still there in the isometric view');
  await shot('measure-pads');

  step('Shift+click picks a part: its height above the face and its margin to the limit above it');
  const at = await ask(page, 'project', tall.path);
  await page.keyboard.down('Shift');
  await page.mouse.click(at.x, at.y);
  await page.keyboard.up('Shift');
  await page.waitForSelector('[data-testid=measure-readout]');
  const readout = await page.getByTestId('measure-readout').innerText();
  expect(readout.includes('9.00 mm above the top face') && readout.includes('6.00 mm over the limit lid (3.00 mm)'), `the readout: ${readout.replace(/\n/g, ' | ')}`);
  const height = await page.getByTestId('measure-height').innerText();
  const limit = await page.getByTestId('measure-limit').innerText();
  expect(height.includes('9.00 mm') && limit.includes('6.00 mm over'), `the lines on the part: ${height.replace(/\n/g, ' ')} / ${limit.replace(/\n/g, ' ')}`);
  await shot('measure-height');

  step('Esc leaves the tool and the measurement stays; the clear-all control removes it');
  await page.mouse.move(5, 5);
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=measure-panel]', { state: 'detached' });
  expect((await page.getByTestId('measure-label').count()) === 1, 'the measurement is still drawn');
  await page.keyboard.press('m');
  await page.getByTestId('measure-clear').click();
  expect((await page.getByTestId('measure-label').count()) === 0, 'cleared');

  step('the light theme');
  await page.keyboard.press('t');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'top', HOST);
  await page.waitForTimeout(400);
  for (const f of found) {
    const p = await ask(page, 'toScreen', f.at);
    await page.mouse.move(p.x + 3, p.y - 2);
    await page.waitForTimeout(80);
    await page.mouse.click(p.x + 3, p.y - 2);
  }
  await page.keyboard.press('i');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'iso', HOST);
  await theme(page, 'Light', 'light');
  await page.waitForTimeout(1200);
  const relabel = await page.getByTestId('measure-label').innerText();
  expect(relabel.includes(want), `measured again in the light theme: ${relabel.replace(/\n/g, ' | ')}`);
  await shot('measure-light');
}
