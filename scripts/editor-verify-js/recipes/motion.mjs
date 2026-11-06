// Motion: the active section's line glides, a view fades in, a popover scales in, the dock opens by height, a toast comes in and
// goes out (it fades and folds its height away), a row eases on hover; every one takes 120 to 200 ms, and with
// prefers-reduced-motion none of them has a duration over 1 ms and the toast is gone at once.
export const fixture = 'two_boards';

export async function run({ page, step, expect, section }) {
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await section('source');
  await page.waitForTimeout(500);

  /** What is animating just after `trigger` (an in-page function), as `name duration`. */
  const running = async (trigger) => {
    if (typeof trigger === 'function' && trigger.constructor.name === 'AsyncFunction') await trigger();
    else await page.evaluate(trigger);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    await page.waitForTimeout(30);
    return page.evaluate(() =>
      document
        .getAnimations()
        .map((a) => ({ name: a.animationName || a.transitionProperty, ms: a.effect.getComputedTiming().duration, target: a.effect.target?.className?.toString?.() ?? '' }))
        .filter((a) => a.ms > 1),
    );
  };
  const find = (list, name) => list.find((a) => a.name === name);
  const quick = (a) => a && a.ms >= 100 && a.ms <= 200;

  step('the line under the active section glides (transform and width), the new view fades in');
  let a = await running(() => document.querySelector('.pane-strip [data-section="bom"]').click());
  expect(quick(find(a, 'transform')) && quick(find(a, 'width')), 'the line glides: transform and width, 120 to 200 ms');
  expect(quick(find(a, 'view-in')), 'the view fades in over 150 ms');

  step('a popover scales in');
  a = await running(() => document.querySelector('.theme-item').click());
  expect(quick(find(a, 'pop-in')), 'the theme menu scales in over 120 ms');
  await page.keyboard.press('Escape');

  step('the dock opens by height');
  a = await running(() => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', ctrlKey: true, bubbles: true })));
  expect(quick(find(a, 'dock-open')), 'the dock opens over 160 ms');
  await page.keyboard.press('Control+j');

  step('a toast comes in, and goes out the way it came: it fades and folds its height away');
  a = await running(async () => {
    await page.locator('.statusbar .theme-item').click();
    await page.locator('.theme-option').nth(2).click();
  });
  expect(quick(find(a, 'toast-in')), 'the toast comes in over 160 ms');
  await page.waitForSelector('.toast');
  await page.waitForTimeout(300);
  a = await running(() => document.querySelector('.toast .icon-btn').click());
  expect(a.some((x) => x.name === 'opacity' && x.target.includes('toast')) && a.some((x) => x.name === 'max-height'), 'it fades and folds: opacity and max-height run');
  await page.waitForFunction(() => !document.querySelector('.toast'), null, { timeout: 1000 });
  expect(true, 'and it is gone within a second, not left behind');

  step('a row eases on hover');
  await section('bom');
  await page.waitForSelector('tr.dr');
  await page.mouse.move(5, 5);
  const row = page.locator('tr.dr').nth(1);
  const before = await row.evaluate((e) => getComputedStyle(e).backgroundColor);
  await row.hover();
  await page.waitForTimeout(250);
  const after = await row.evaluate((e) => getComputedStyle(e).backgroundColor);
  expect(before !== after, `an even row changes on hover too (zebra ${before}, hover ${after})`);

  step('with prefers-reduced-motion nothing runs longer than 1 ms and a toast leaves at once');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await section('power');
  a = await running(() => document.querySelector('.pane-strip [data-section="system"]').click());
  expect(a.length === 0, `no animation over 1 ms after a section change (${a.map((x) => x.name).join(', ') || 'none'})`);
  a = await running(() => document.querySelector('.theme-item').click());
  expect(a.length === 0, 'a popover opens with none');
  await page.keyboard.press('Escape');
  await page.locator('.statusbar .theme-item').click();
  await page.locator('.theme-option').nth(1).click();
  await page.waitForSelector('.toast');
  await page.locator('.toast .icon-btn').first().click();
  await page.waitForFunction(() => !document.querySelector('.toast'), null, { timeout: 200 });
  expect(true, 'the toast is gone within 200 ms');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
}
