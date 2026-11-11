// SPIKE (#430 P0, not for merge): WebGL in the headless Chromium, and the three.js chunk loaded on demand by `?probe3d`.
export const fixture = 'led';

export async function run({ page, url, shot, step, expect }) {
  step('open the page with ?probe3d: the three.js chunk loads and renders a test scene');
  const chunks = [];
  page.on('response', (r) => r.url().includes('/assets/') && r.url().endsWith('.js') && chunks.push(`${r.status()} ${r.headers()['content-type']} ${r.url()}`));
  await page.goto(`${url.replace(/\/$/, '')}/?probe3d`);
  await page.waitForFunction(() => window.__probe3d, null, { timeout: 60000 });
  const r = await page.evaluate(() => window.__probe3d);
  const browser = page.context().browser();
  console.log(`PROBE3D chromium ${browser ? browser.version() : '?'}`);
  console.log(`PROBE3D chunks ${JSON.stringify(chunks)}`);
  console.log(`PROBE3D result ${JSON.stringify(r)}`);
  expect(r.ok, `WebGL rendered the scene (${r.error ?? `${r.nonBackgroundPixels} of ${r.totalPixels} pixels are not background`})`);
  expect(r.glbMeshes === 1, 'the GLB loaded');
  await shot('scene');
}
