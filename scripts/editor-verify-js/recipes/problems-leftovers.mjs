// The Problems dock's small leftovers: the place of every problem is a button (Tab reaches it, Enter and a click open the source there, and
// the row's own click still picks the thing), the row that is picked stays in view when the list moves under it, a fixed order of groups
// (severity, stage), and a count for each severity in the dock's head and in the status bar.
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'warn_bench';

export async function run({ page, shot, step, expect, project, section }) {
  const file = path.join(project, 'src/main.nsm');
  const good = fs.readFileSync(file, 'utf8');
  // twelve boards with the same fault: 24 warnings, more rows than the dock shows (the first keeps its name: the page is showing it)
  const many = Array.from({ length: 12 }, (_, i) => good.replace('board Heater {', i === 0 ? 'board Heater {' : `board Heater${String(i + 1).padStart(2, '0')} {`).replace(/^\/\/.*\n/gm, '').replace(/^import .*\n/gm, '')).join('\n');
  const header = good.split('\n').filter((l) => l.startsWith('import ')).join('\n');
  try {
    await section('source');
    await page.waitForSelector('.cm-editor');
    await page.keyboard.press('Control+j');
    await page.waitForSelector('.dock .problem.warning', { timeout: 15000 });

    step('the place of a row is a button with a name: Tab reaches it, Enter opens the source on that line');
    const first = page.locator('.dock .problem').first();
    const place = first.locator('[data-testid="problem-place"]');
    expect((await place.count()) === 1, 'a row with a place has its button');
    const label = await place.getAttribute('aria-label');
    expect(/^Open src\/main\.nsm:\d+ in the source$/.test(label), `named for where it goes (${label})`);
    expect((await place.evaluate((e) => e.tagName)) === 'BUTTON', 'a real button');
    await page.locator('.dock .problem').first().focus();
    await page.keyboard.press('Tab');
    expect(await page.evaluate(() => document.activeElement?.matches('[data-testid="problem-place"]')), 'one Tab from the row lands on its place');
    // somewhere else first, so the move to the line is seen
    await page.evaluate(() => document.querySelector('.cm-content')?.focus());
    await page.keyboard.press('Control+Home');
    await page.locator('.dock .problem').first().focus();
    await page.keyboard.press('Tab');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => /Ln 12/.test(document.querySelector('.statusbar .cursor')?.textContent ?? ''), null, { timeout: 5000 });
    expect(true, 'the cursor is on line 12 of the file (the status bar says Ln 12)');
    expect((await page.locator('.dock .problem.active').count()) === 1, 'and the row is the picked one');
    await shot('place-focus');

    step('a click on the place does the same, and not the row\'s other work');
    await page.evaluate(() => document.querySelector('.cm-content')?.focus());
    await page.keyboard.press('Control+Home');
    await place.click();
    await page.waitForFunction(() => /Ln 12/.test(document.querySelector('.statusbar .cursor')?.textContent ?? ''), null, { timeout: 5000 });
    expect(true, 'the cursor is on line 12 again');

    step('arrows move between rows from the place\'s button too (not back to the first row)');
    await page.locator('.dock .problem').first().focus();
    await page.keyboard.press('Tab');
    await page.keyboard.press('ArrowDown');
    expect(await page.evaluate(() => document.activeElement?.matches('.problem') && document.activeElement === document.querySelectorAll('.dock .problem')[1]), 'ArrowDown from the place of row 1 focuses row 2');

    step('a severity and a stage have a fixed place: the head counts each severity, the status bar the same, the stage grouping is in the chips\' order');
    const head = await page.locator('.dock .sev-toggle').evaluateAll((els) => els.map((e) => `${e.className.split(' ')[1]}:${e.querySelector('.n').textContent}`));
    expect(head.join(' ').startsWith('error:0 warning:2 note:'), `the head counts errors, warnings and notes (${head.join(' ')})`);
    const bar = await page.locator('.statusbar .counts').innerText();
    expect(/0\s+2/.test(bar.replace(/\s+/g, ' ')), `the status bar counts errors and warnings (${bar.replace(/\s+/g, ' ')})`);
    await page.locator('.dock-group').selectOption('stage');
    await page.waitForSelector('.dock-group-head');
    const heads = await page.locator('.dock-group-head').evaluateAll((els) => els.map((e) => e.textContent.replace(/\s+/g, ' ').trim()));
    expect(heads.length === 1 && /^Guideline\s*2$/.test(heads[0]), `grouped by stage: ${heads.join(' | ')}`);
    expect((await page.locator('.dock-group option').allTextContents()).join(',') === 'No grouping,Group by file,Group by severity,Group by stage,Group by cause', 'five groupings in the menu');
    await page.locator('.dock-group').selectOption('cause');
    expect((await page.locator('.dock-group-head').count()) === 0, 'by cause: two warnings and no note have no head, as with no grouping');
    await page.locator('.dock-group').selectOption('none');

    step('many warnings: F8 steps through them and the picked row is always in view');
    fs.writeFileSync(file, `${header}\n${many}`);
    await page.waitForFunction(() => document.querySelectorAll('.dock .problem.warning').length >= 24, null, { timeout: 60000 });
    const inView = () =>
      page.evaluate(() => {
        const row = document.querySelector('.dock .problem.active');
        const box = document.querySelector('.dock-rows');
        if (!row || !box) return false;
        const r = row.getBoundingClientRect();
        const b = box.getBoundingClientRect();
        return r.top >= b.top - 1 && r.bottom <= b.bottom + 1;
      });
    const rowsDrawn = await page.locator('.dock .problem').count();
    const scrolls = await page.evaluate(() => { const b = document.querySelector('.dock-rows'); return b.scrollHeight > b.clientHeight; });
    expect(scrolls, `the list scrolls (${rowsDrawn} rows)`);
    let ok = true;
    for (let i = 0; i < 14; i++) {
      await page.keyboard.press('F8');
      await page.waitForTimeout(80);
      ok = ok && (await inView());
    }
    expect(ok, 'after each of fourteen F8 the picked row is inside the list\'s box');
    await shot('f8-in-view');
    // the list moves under the picked row: another grouping, a filter, a smaller dock
    await page.locator('.dock-group').selectOption('stage');
    await page.waitForTimeout(150);
    expect(await inView(), 'grouped by stage: still in view');
    await page.locator('.dock-group').selectOption('severity');
    await page.waitForTimeout(150);
    expect(await inView(), 'grouped by severity: still in view');
    const active = await page.locator('.dock .problem.active .msg').innerText();
    await page.locator('.dock-search input').fill('Heater0');
    await page.waitForTimeout(200);
    const still = await page.locator('.dock .problem.active').count();
    expect(still === 0 || (await inView()), 'a filter that keeps the picked row keeps it in view');
    await page.locator('.dock-search input').fill('');
    await page.locator('.dock-group').selectOption('none');
    // shrink the dock: the picked row is still seen
    await page.locator('.dock-resize').focus();
    for (let i = 0; i < 8; i++) await page.keyboard.press('ArrowDown');
    await page.waitForTimeout(250);
    expect(await inView(), 'a smaller dock: still in view');
    expect(active.length > 0, 'the picked row has a message');
    await shot('smaller-dock');

    step('in every theme the place reads at 4.5:1 or more, at rest and with the focus on it');
    for (const id of ['dark', 'light', 'hc-dark', 'hc-light']) {
      await page.evaluate((t) => { document.documentElement.dataset.theme = t; }, id);
      await page.waitForTimeout(200);
      const r = await page.evaluate(() => {
        const lum = (c) => {
          const m = c.match(/[\d.]+/g).map(Number);
          const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
          return 0.2126 * f(m[0]) + 0.7152 * f(m[1]) + 0.0722 * f(m[2]);
        };
        const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
        const link = document.querySelector('.dock .problem .where-link');
        const row = link.closest('.problem');
        const surface = (el) => { for (let e = el; e; e = e.parentElement) { const c = getComputedStyle(e).backgroundColor; if (c !== 'rgba(0, 0, 0, 0)' && !/, 0\)$/.test(c)) return c; } return 'rgb(0, 0, 0)'; };
        const rest = ratio(getComputedStyle(link).color, surface(row));
        link.focus();
        const focused = ratio(getComputedStyle(link).color, surface(row));
        return { rest, focused };
      });
      expect(r.rest >= 4.5 && r.focused >= 4.5, `${id}: ${r.rest.toFixed(1)}:1 at rest, ${r.focused.toFixed(1)}:1 focused`);
    }
    await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  } finally {
    fs.writeFileSync(file, good);
    await page.locator('.dock-resize').dblclick().catch(() => {});
  }
}
