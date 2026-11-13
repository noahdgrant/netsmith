// The issues strip and the badges of the power tree. The server works the issues out (`issues` of
// the tree: `issues_of` in crates/netsmith-pipeline/src/power.rs, and `netsmith build` and the analysis
// report the same), and a board whose supply can be overdrawn does not build, so the answer is
// changed in flight to show what the page draws from what the server sends.
export const fixture = 'power_tree';

export async function run({ page, shot, step, expect, section }) {
  step('no issues on the invented board');
  await page.setViewportSize({ width: 2000, height: 1000 });
  await section('power');
  await page.waitForSelector('.sec-power .pt-node');
  expect(/No issues/.test(await page.locator('.pt-issues').innerText()), 'the strip says there are none');
  expect((await page.locator('.pt-badge').count()) === 0, 'no badge on any card');

  step('an overdrawn supply, a rail with no source, an unsettled current and an unsettled efficiency');
  await page.route('**/api/power?*', async (route) => {
    const response = await route.fetch();
    const tree = await response.json();
    const v3 = tree.nodes.find((n) => n.net === 'V3V3');
    v3.budget = 'over';
    v3.demand = { lo: 0.0025, hi: 0.12, text: '2.5mA..120mA' };
    const ana = tree.nodes.find((n) => n.net === 'VANA');
    ana.supplies = [];
    tree.edges = tree.edges.filter((e) => e.to !== 'VANA');
    const sens = tree.nodes.find((n) => n.net === 'VSENS');
    sens.loads.find((d) => d.who === 'imu.power').range = null;
    sens.budget = 'unknown';
    sens.demand = null;
    tree.stages.find((s) => s.label.startsWith('boost')).efficiency = null;
    const boost = tree.stages.find((s) => s.label.startsWith('boost'));
    boost.efficiencyUnsettled = true;
    tree.issues = [
      { id: v3.id, severity: 'error', subject: 'V3V3', text: 'can draw up to 120 mA, more than the 100 mA its supply can give' },
      { id: ana.id, severity: 'note', subject: 'VANA', text: 'has no source: nothing supplies it' },
      { id: sens.id, severity: 'note', subject: 'VSENS', text: 'has a load with a current the solver has not settled' },
      { id: `stage:${boost.label}`, severity: 'note', subject: boost.label.replace(/ \(.*$/, ''), text: 'has an efficiency the solver has not settled' },
    ];
    await route.fulfill({ response, json: tree });
  });
  await page.reload();
  await section('power');
  await page.waitForSelector('.pt-issue');
  const issues = (await page.locator('.pt-issue').allInnerTexts()).map((i) => i.replace(/\s+/g, ' '));
  expect(issues.length === 4, `four issues (${issues.join(' | ')})`);
  expect(/^V3V3/.test(issues[0]) && /more than the 100 mA/.test(issues[0]), 'the overdrawn supply comes first');
  expect((await page.locator('.pt-issue.bad').count()) === 1 && (await page.locator('.pt-issue.warn').count()) === 3, 'one red, three amber');
  expect(issues.some((i) => /VANA.*no source/.test(i)) && issues.some((i) => /VSENS.*not settled/.test(i)) && issues.some((i) => /boost.*efficiency/.test(i)), 'each of the others is named');
  expect((await page.locator('.pt-rail.bad .pt-badge.bad').innerText()).trim() === 'Over limit', 'a red badge on the overdrawn card');
  expect((await page.locator('[data-node="VANA"] .pt-badge').innerText()).trim() === 'No source', 'a badge "No source"');
  expect((await page.locator('[data-node="VSENS"] .pt-badge.warn').count()) === 1, 'an amber badge on the rail with the unknown current');
  expect((await page.locator('.pt-stage .pt-eff.unknown').innerText()).includes('not settled'), 'the stage says its efficiency is not settled');
  await shot('issues');

  step('an issue jumps to its card');
  await page.getByRole('button', { name: /VANA/ }).click();
  await page.waitForSelector('[data-node="VANA"].selected');
  const box = await page.locator('[data-node="VANA"]').boundingBox();
  expect(box.x >= 0 && box.x + box.width <= 2000 && box.y >= 0 && box.y + box.height <= 1000, 'it is selected and inside the window');
}
