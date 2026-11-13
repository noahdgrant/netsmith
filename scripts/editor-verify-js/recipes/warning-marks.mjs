// The warning triangle everywhere warnings are shown: the file tree, a file's tab, the outline, the status bar, the Problems dock (its toggle
// and its rows) and the source's gutter and overview ruler. One icon (the same drawing), one colour (the `--warning` token, in every theme),
// one meaning (a name that says `warning`) for a screen reader.
import { whenIdle } from '../settle.mjs';
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'warn_bench';

const THEMES = ['dark', 'light', 'hc-dark', 'hc-light'];
// the drawing of `warning` in components/Icons.tsx
const TRIANGLE = 'M8 2.2l6.2 11H1.8z';

export async function run({ page, shot, step, expect, project, section }) {
  step('open the Source with the file tree, the outline and the dock: the board has two warnings, no error');
  await section('source');
  await page.waitForSelector('.cm-editor');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('.dock .problem.warning');
  await page.waitForSelector('.ft-row .sev-mark');
  await page.waitForSelector('.cm-sev.warning');
  await page.waitForSelector('.tabbar.files .tab .sev-mark');

  const spots = {
    tree: '.ft-row .sev-mark.warning',
    tab: '.tabbar.files .tab .sev-mark.warning',
    outline: '.ol-row .sev-mark.warning',
    statusBar: '.statusbar .counts .w',
    dockToggle: '.dock .sev-toggle.warning',
    dockRow: '.dock .problem.warning .sev',
    gutter: '.cm-sev.warning',
  };

  const audit = () =>
    page.evaluate(
      ({ spots, TRIANGLE }) => {
        const probe = document.createElement('i');
        probe.style.color = 'var(--warning)';
        document.body.append(probe);
        const token = getComputedStyle(probe).color;
        probe.remove();
        const out = { token, spots: {} };
        for (const [name, sel] of Object.entries(spots)) {
          const el = document.querySelector(sel);
          if (!el) {
            out.spots[name] = { missing: true };
            continue;
          }
          const icon = el.matches('svg') ? el : el.querySelector('svg[data-icon="warning"]');
          const accessible = [el.getAttribute('aria-label'), el.getAttribute('title'), el.closest('[aria-label]')?.getAttribute('aria-label'), el.textContent].filter(Boolean).join(' | ');
          out.spots[name] = {
            path: icon?.querySelector('path')?.getAttribute('d') ?? null,
            colour: icon ? getComputedStyle(icon).color : null,
            name: accessible,
            role: el.getAttribute('role'),
          };
        }
        // the ruler's triangle is drawn by the stylesheet, not an icon: its fill is the same token
        const tick = document.querySelector('.cm-ruler-tick.kind-warning');
        out.ruler = tick ? getComputedStyle(tick).backgroundColor : null;
        return out;
      },
      { spots, TRIANGLE },
    );

  step('in every theme: the same triangle, the token\'s colour, and a name that says warning');
  for (const id of THEMES) {
    await page.evaluate((t) => { document.documentElement.dataset.theme = t; }, id);
    await whenIdle(page);
    const r = await audit();
    for (const [name, s] of Object.entries(r.spots)) {
      expect(!s.missing, `${id}: ${name} is on screen`);
      if (s.missing) continue;
      expect(s.path === TRIANGLE, `${id}: ${name} draws the warning triangle (${s.path})`);
      expect(s.colour === r.token, `${id}: ${name} is the --warning colour (${s.colour} against ${r.token})`);
      expect(/warning/i.test(s.name), `${id}: ${name} says warning to a screen reader (${s.name.replace(/\s+/g, ' ').slice(0, 80)})`);
    }
    expect(r.ruler === r.token, `${id}: the ruler's triangle is the --warning colour (${r.ruler})`);
    await shot(id);
  }
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });

  step('the marks that count are one component: an icon and the number, named with the file, and a screen reader hears the count');
  const names = await page.evaluate(() => ({
    tree: document.querySelector('.ft-row .sev-mark')?.getAttribute('aria-label'),
    tab: document.querySelector('.tabbar.files .tab .sev-mark')?.getAttribute('aria-label'),
    role: document.querySelector('.ft-row .sev-mark')?.getAttribute('role'),
    n: document.querySelector('.ft-row .sev-mark .n')?.textContent,
    tabHasNumber: !!document.querySelector('.tabbar.files .tab .sev-mark .n'),
  }));
  expect(names.tree === '2 warnings in this file' && names.n === '2', `the tree says "${names.tree}" with 2`);
  expect(names.tab === '2 warnings in this file', `the tab says "${names.tab}"`);
  expect(names.role === 'img', 'as an image with a name, not a bare number');

  step('an error in the file changes every mark to the error\'s: the cross, the danger colour, the word error');
  const file = path.join(project, 'src/main.nsm');
  const good = fs.readFileSync(file, 'utf8');
  try {
    fs.writeFileSync(file, good.replace('.supply: Header1x2', '.supply: Header1x2 {\n        .nonsense &= 1V\n    }'));
    await page.waitForSelector('.ft-row .sev-mark.error');
    const e = await page.evaluate(() => {
      const probe = document.createElement('i');
      probe.style.color = 'var(--danger)';
      document.body.append(probe);
      const token = getComputedStyle(probe).color;
      probe.remove();
      const mark = document.querySelector('.ft-row .sev-mark.error');
      const tab = document.querySelector('.tabbar.files .tab .sev-mark.error');
      return {
        tree: mark?.getAttribute('aria-label'),
        tab: tab?.getAttribute('aria-label'),
        icon: mark?.querySelector('svg')?.getAttribute('data-icon'),
        colour: mark ? getComputedStyle(mark.querySelector('svg')).color : null,
        token,
      };
    });
    expect(e.icon === 'error' && e.colour === e.token, `the tree's mark is the error cross in --danger (${e.icon}, ${e.colour})`);
    expect(/error/.test(e.tree) && /error/.test(e.tab ?? ''), `and says error (${e.tree} / ${e.tab})`);
    await shot('with-error');
  } finally {
    fs.writeFileSync(file, good);
    await page.waitForFunction(() => !document.querySelector('.ft-row .sev-mark.error'), null);
  }
}
