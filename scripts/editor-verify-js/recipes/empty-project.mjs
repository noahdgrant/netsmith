// A project with no board yet: every view that needs a board says "No boards yet", names the file to write it in, and the button opens it.
export const fixture = 'empty';

export async function run({ page, shot, step, expect, section }) {
  await page.setViewportSize({ width: 1280, height: 800 });
  for (const id of ['schematic', 'power', 'system', 'bom', 'pcb']) {
    step(`${id}: the empty state`);
    await section(id);
    await page.waitForSelector(`.sec-${id} [data-testid="no-board"]`, { timeout: 15000 });
    const text = await page.locator(`.sec-${id} [data-testid="no-board"]`).innerText();
    expect(text.includes('No boards yet') && text.includes('src/main.nsm'), `${id} says No boards yet and names src/main.nsm`);
    if (id === 'schematic') await shot('schematic');
  }
  step('the button opens src/main.nsm in the Source');
  await section('bom');
  await page.click('.sec-bom [data-testid="open-entry"]');
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForSelector('.tabbar.files .tab.active');
  expect((await page.locator('.tabbar.files .tab.active').innerText()).includes('main.nsm'), 'the file tab main.nsm is active');
  await shot('source');
}
