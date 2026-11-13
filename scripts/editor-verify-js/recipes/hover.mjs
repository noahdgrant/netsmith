// The shared hover (src/hover.ts, hoverDom.ts): the pointer or the keyboard focus on a thing in one view
// lights the same thing, softly (`.hover-lit`), in the others; it goes out when the pointer leaves, on Escape,
// and a touch never sets it. Only a class is toggled: no drawing is replaced. The board is in the left pane
// and another view in the right one, so that both are on show.
import { whenIdle } from '../settle.mjs';
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  const board = 'TwoBoards/power';
  await page.setViewportSize({ width: 2400, height: 1000 });
  await page.selectOption('.boardpick select', board);
  const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
  const part = detail.parts.find((p) => p.path.includes('.') && p.kind !== 'Header') ?? detail.parts[0];
  const lit = (sel) => page.locator(`${sel}.hover-lit`).count();
  const away = async () => {
    await page.mouse.move(5, 600);
    await whenIdle(page);
  };
  const right = async (id) => {
    await page.locator(`.pane-strip[data-pane="1"] [data-section="${id}"]`).click();
    await page.waitForSelector(`.section.sec-${id}:not(.away)`);
    await whenIdle(page);
  };
  const pcbPart = `.sec-pcb .pcb-host svg g[data-ref="${part.refdes}"]`;

  step('the board on the left, another view on the right');
  await section('pcb');
  await page.waitForSelector(pcbPart);
  await page.getByRole('button', { name: /Split right/ }).click();
  await page.waitForSelector('.divider');

  step('a drawing is not replaced by a hover: a mark on the board survives, and nothing is added or removed in it');
  await page.evaluate(() => {
    const host = document.querySelector('.sec-pcb .pcb-stage');
    host.firstElementChild.dataset.keep = 'yes';
    window.__childList = 0;
    new MutationObserver((m) => (window.__childList += m.filter((x) => x.type === 'childList').length)).observe(host.firstElementChild, { childList: true, subtree: true });
  });

  step('hover a row of the bill of materials: the part is lit on the board and in its panel, and the row is lit');
  await right('bom');
  const row = page.locator(`tr.dr[data-hv-ref~="${part.refdes}"]`).first();
  await row.hover();
  await whenIdle(page);
  expect((await lit(pcbPart)) === 1, 'the part is lit on the board');
  expect((await lit('.sec-pcb .pcb-host svg g[data-ref]')) === 1, 'and only that part');
  expect((await lit(`.sec-pcb .part-row[data-hv-ref="${part.refdes}"]`)) === 1, "its row in the board's panel is lit");
  expect((await lit(`tr.dr[data-hv-ref~="${part.refdes}"]`)) >= 1, 'the row itself is lit');
  await shot('bom-row');
  const kept = await page.evaluate(() => ({ keep: document.querySelector('.sec-pcb .pcb-stage').firstElementChild.dataset.keep, changed: window.__childList }));
  expect(kept.keep === 'yes' && kept.changed === 0, `no re-render of the drawing (${kept.changed} nodes changed)`);

  step('leave the row: the light goes out');
  await away();
  expect((await lit('')) === 0 || (await page.locator('.hover-lit').count()) === 0, 'nothing is lit');

  step('hover a part of the schematic: its BOM row and the board show it');
  await right('schematic');
  await page.locator(`.sec-schematic svg g.part[data-ref="${part.refdes}"]`).first().hover();
  await whenIdle(page);
  expect((await lit(pcbPart)) === 1, 'the part is lit on the board');
  expect((await lit(`.sec-schematic svg g.part[data-ref="${part.refdes}"]`)) === 1, 'and in the schematic itself');
  expect((await lit(`.sec-bom tr.dr[data-hv-ref~="${part.refdes}"]`)) >= 1, 'its row in the bill of materials is lit, shown or not');
  await shot('schematic-part');
  await page.keyboard.press('Escape');
  await whenIdle(page);
  expect((await page.locator('.hover-lit').count()) === 0, 'Escape puts the light out');

  step('hover a net chip of the board\'s inspector: the net is lit in the schematic and on the board, and no part is');
  await page.locator(`.sec-schematic svg g.part[data-ref="${part.refdes}"]`).first().click();
  await page.waitForSelector('.sec-pcb .pin-chip[data-hv^="net:"]');
  const chip = page.locator('.sec-pcb .pin-chip[data-hv^="net:"]').first();
  const netName = (await chip.getAttribute('data-hv')).slice(4);
  await chip.hover();
  await whenIdle(page);
  expect((await lit(`.sec-schematic svg [data-net="${netName}"]:not(.pin)`)) >= 1, `the net ${netName} is lit in the schematic`);
  expect((await lit(`.sec-pcb svg [data-net="${netName}"]`)) >= 1, 'the net is lit on the board');
  expect((await lit('.sec-pcb .pcb-host svg g[data-ref]')) === 0, 'no part of the board is lit by a net');
  await shot('inspector-net');
  await away();

  step('a card of the power tree lights its net on the board; a part row of the board panel lights its card');
  await right('power');
  const rail = page.locator('.sec-power .pt-node[data-hv^="net:"]').first();
  await rail.hover();
  await whenIdle(page);
  const railNet = (await rail.getAttribute('data-hv')).slice(4);
  expect((await lit(`.sec-pcb svg [data-net="${railNet}"]`)) >= 1, `the rail ${railNet} is lit on the board`);
  await shot('power-card');
  await away();
  const prow = page.locator(`.sec-pcb .part-row[data-hv-ref="${part.refdes}"]`);
  await prow.hover();
  await whenIdle(page);
  expect((await lit('.sec-power .pt-node')) >= 1, 'a card is lit in the power tree');
  await away();

  step('the keyboard does the same as the pointer: a BOM row that has the focus lights the part');
  await right('bom');
  await page.locator(`tr.dr[data-hv-ref~="${part.refdes}"]`).first().focus();
  await whenIdle(page);
  expect((await lit(pcbPart)) === 1, 'focus lights the part on the board');
  await page.locator('body').click({ position: { x: 5, y: 500 } });
  await whenIdle(page);
  expect((await page.locator('.hover-lit').count()) === 0, 'focus gone, the light is gone');

  step('a touch sets nothing');
  await page.evaluate((r) => {
    document.querySelector(`tr.dr[data-hv-ref~="${r}"]`).dispatchEvent(new PointerEvent('pointerenter', { pointerType: 'touch' }));
  }, part.refdes);
  await whenIdle(page);
  expect((await page.locator('.hover-lit').count()) === 0, 'nothing is lit by a touch');
}
