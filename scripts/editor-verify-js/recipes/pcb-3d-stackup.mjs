// The 3D view's section plane and exploded stackup on a four-layer board (features/pcb-3d.md,
// "Tools"): a cut across Y, set by typing its place in mm, seen from the front shows the stackup
// filled as bands at their true thickness (the mask, the prepreg and the core in two laminate
// tones, as `netsmith render --section` colours them); "copy as command" gives the exact
// `netsmith render` command of the view; E pulls the layers apart, each labelled with its name and
// thickness from the scene file, and puts them back.
import { whenIdle } from '../settle.mjs';
export const fixture = 'four_layer';

const HOST = '[data-testid=pcb-3d]';
const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

async function theme(page, label, id) {
  await page.keyboard.press('Control+Shift+P');
  await page.keyboard.type(`Theme: ${label}`);
  await page.keyboard.press('Enter');
  await page.waitForFunction((t) => document.documentElement.dataset.theme === t, id);
}

export async function run(tools) {
  const { page, shot, step, expect, section } = tools;
  await section('pcb');
  await page.waitForSelector('.pcb-host svg');
  await theme(page, 'Dark', 'dark');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`);
  await page.locator(HOST).focus();
  const info = await ask(page, 'board');
  const board = info.board;
  const slots = await ask(page, 'slots');
  const layers = slots.filter((s) => !s.key.startsWith('silk:'));
  expect(layers.some((l) => l.key === 'In1.Cu') && layers.length >= 9, `the scene file has the stackup: ${layers.map((l) => l.key).join(', ')}`);

  step('Y cuts across Y; a typed place in mm moves the cut');
  await page.keyboard.press('y');
  await page.waitForSelector('[data-testid=section-panel]');
  expect((await page.getByTestId('tool-section').getAttribute('aria-pressed')) === 'true', 'the Section button is pressed');
  const W = info.widthMm;
  const at = Math.round((info.heightMm / 2) * 100) / 100 + 0.37;
  await page.getByTestId('section-at').fill(String(at));
  await page.getByTestId('section-at').press('Enter');
  await page.locator(HOST).focus();

  step('seen from the front, the cut shows the stackup as bands at their true thickness');
  await page.getByTestId('view-front').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'front', HOST);
  await page.keyboard.press('p');
  await whenIdle(page);
  // Zoom at the cut's left part until the board's 1.6 mm fill a good part of the window.
  for (let i = 0; i < 9; i++) {
    const p = await ask(page, 'toScreen', [W * 0.05, at, -0.8]);
    await page.mouse.move(p.x, p.y);
    await page.mouse.wheel(0, -400);
    await whenIdle(page);
  }
  await whenIdle(page);
  const colourOf = async (z) => {
    const p = await ask(page, 'toScreen', [W * 0.05, at, z]);
    return ask(page, 'colourAt', p.x, p.y);
  };
  const thick = layers.filter((l) => l.zTop - l.zBottom > 0.08);
  const bands = [];
  for (const l of thick) bands.push({ key: l.key, detail: l.detail, colour: await colourOf((l.zTop + l.zBottom) / 2) });
  const core = bands.find((b) => b.detail.startsWith('core'));
  const prepreg = bands.find((b) => b.detail.startsWith('prepreg'));
  expect(bands.every((b) => b.colour) && core && prepreg && core.colour !== prepreg.colour, `the core and the prepreg are two tones (${bands.map((b) => `${b.key} ${b.colour}`).join(', ')})`);
  const top = await ask(page, 'toScreen', [W * 0.05, at, 0]);
  const bottom = await ask(page, 'toScreen', [W * 0.05, at, -1.6]);
  expect(bottom.y - top.y > 200 && bottom.y - top.y < 700, `the board's thickness fills ${bottom.y - top.y} pixels`);
  await shot('section-bands');

  step('copy as command: the exact netsmith render command of this view');
  await page.locator('.v3-more .pop-btn').click();
  const cmd = await page.getByTestId('render-command-text').innerText();
  const want = `netsmith render src/main.nsm --board ${board} --view front --section y=${at} --theme dark`;
  expect(cmd === want, `the command: ${cmd} (want ${want})`);
  await page.keyboard.press('Escape');
  await page.mouse.move(5, 5);
  await page.locator(HOST).focus();

  step('E pulls the stackup apart: each layer labelled with its name and thickness');
  await page.keyboard.press('s');
  await page.waitForSelector('[data-testid=section-panel]', { state: 'detached' });
  await page.keyboard.press('p');
  await page.keyboard.press('i');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'iso', HOST);
  await page.keyboard.press('e');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.exploded(), HOST);
  // Apart in full: each layer at its place plus its offset (the ease's last frame), not after a fixed time.
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.slots().every((s) => Math.abs(s.now - ((s.zTop + s.zBottom) / 2 + s.offset)) < 1e-6), HOST);
  await page.keyboard.press('f');
  await whenIdle(page);
  const labels = await page.locator('[data-testid=stack-label]').allInnerTexts();
  expect(labels.length === slots.length, `a label per layer (${labels.length} of ${slots.length})`);
  expect(
    labels.some((l) => l.includes('In1.Cu') && l.includes('copper ·')) && labels.some((l) => l.includes('core ·')) && labels.some((l) => l.includes('prepreg ·')),
    `the labels: ${labels.map((l) => l.replace(/\n/g, ' ')).join(' | ')}`,
  );
  const spread = await ask(page, 'slots');
  expect(spread[0].now - spread[spread.length - 1].now > 10, `the layers are apart (${(spread[0].now - spread[spread.length - 1].now).toFixed(1)} mm from top to bottom)`);
  await shot('exploded');

  step('the light theme');
  await theme(page, 'Light', 'light');
  await whenIdle(page);
  await shot('exploded-light');

  step('E again puts the layers back');
  await page.keyboard.press('e');
  await page.waitForFunction((sel) => !document.querySelector(sel).view3d.exploded(), HOST);
  // The layers ease back over 420 ms of frames; after a theme change the first frames compile the
  // shaders again and come late (SwiftShader in CI), so wait for the layers, not for a time.
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.slots().every((s) => Math.abs(s.now - (s.zTop + s.zBottom) / 2) < 1e-6), HOST);
  expect((await page.locator('[data-testid=stack-label]').count()) === 0, 'no labels once the layers are back');
  const back = await ask(page, 'slots');
  expect(back.every((s) => Math.abs(s.now - (s.zTop + s.zBottom) / 2) < 1e-6), 'every layer is back in its place');
}
