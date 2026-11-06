#!/usr/bin/env node
// scripts/editor-verify: the lifecycle for checking the netsmith editor in a real browser.
//
//   doctor [--no-browser]   what is installed, and whether a browser can be launched
//   launch [--fixture <name>|schematic_bench|big|tree] [--file <path.nsm>]
//                           start `netsmith editor` on a free port over a scratch project
//   drive <recipe>|all      run a recipe in headless Chromium, with screenshots
//   evidence                list the screenshots and logs of the session
//   cleanup [--keep-evidence]
//                           stop the server and remove the scratch project
//
// State is kept in $NETSMITH_VERIFY_DIR (default /tmp/netsmith-verify), never in the repo.

import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { writeBig } from './fixtures/big.mjs';
import { writeRefresh } from './fixtures/refresh.mjs';
import { writeTree } from './fixtures/tree.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const root = process.env.NETSMITH_VERIFY_DIR ?? path.join(os.tmpdir(), 'netsmith-verify');
const statePath = path.join(root, 'state.json');
const evidenceDir = path.join(root, 'evidence');
const FIXTURES = {
  two_boards: 'examples/two_boards.nsm',
  led: 'examples/led.nsm',
  // An invented power architecture for the Power tree: 8 rails, 3 kinds of regulator, 22 zero-current pins.
  power_tree: 'scripts/editor-verify-js/fixtures/power_tree.nsm',
  // An invented board for the System view: 13 modules, a processor with 22 ports, a modem with 10.
  system_view: 'scripts/editor-verify-js/fixtures/system_view.nsm',
  // An invented board for the busses: a processor module and a modem module joined by a UART, an SPI inside one.
  bus_bench: 'scripts/editor-verify-js/fixtures/bus_bench.nsm',
  regulator: 'examples/linear_regulator.nsm',
  four_layer: 'examples/four_layer.nsm',
  outline_and_pours: 'examples/outline_and_pours.nsm',
  carrier_and_cards: 'examples/carrier_and_cards.nsm',
  led_panel: 'examples/led_panel.nsm',
  // A header with a requirement on where it goes and a layout file that breaks it (ADR 0045).
  constrained: 'scripts/editor-verify-js/fixtures/constrained.nsm',
  // A project with no board: the views say where to write the first one.
  empty: 'scripts/editor-verify-js/fixtures/empty.nsm',
  // The LED example with a project catalog and a fake LCSC endpoint (ADR 0039).
  catalog_refresh: 'examples/led.nsm',
};

const [command, ...rest] = process.argv.slice(2);
const flag = (name) => {
  const i = rest.indexOf(`--${name}`);
  return i < 0 ? undefined : (rest[i + 1] ?? true);
};
const has = (name) => rest.includes(`--${name}`);

function readState() {
  try {
    return JSON.parse(fs.readFileSync(statePath, 'utf8'));
  } catch {
    return null;
  }
}

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** The netsmith binary: $NETSMITH_BIN, else the one in $CARGO_TARGET_DIR or target/debug. */
function netsmithBin() {
  if (process.env.NETSMITH_BIN) return process.env.NETSMITH_BIN;
  const target = process.env.CARGO_TARGET_DIR ?? path.join(repo, 'target');
  return path.join(target, 'debug', 'netsmith');
}

async function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
    server.on('error', reject);
  });
}

// ------------------------------------------------------------------ doctor
async function doctor() {
  let failed = false;
  const line = (ok, name, detail) => {
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? `: ${detail}` : ''}`);
    if (!ok) failed = true;
  };
  const major = Number(process.versions.node.split('.')[0]);
  line(major >= 20, 'node', `v${process.versions.node}${major >= 20 ? '' : ' (20 or newer is needed; `source ~/.nvm/nvm.sh && nvm use 22`)'}`);

  const bin = netsmithBin();
  line(fs.existsSync(bin), 'netsmith binary', fs.existsSync(bin) ? bin : `${bin} is missing: run cargo build -p netsmith (set CARGO_TARGET_DIR or NETSMITH_BIN if it is elsewhere)`);

  // The committed bundle the binary embeds when `web/` has not changed (`web/dist` is only a
  // build directory of a changed `web/`, which needs Node 20+).
  const dist = path.join(repo, 'crates/netsmith-editor/web-dist/index.html');
  line(fs.existsSync(dist), 'web bundle', fs.existsSync(dist) ? dist : 'crates/netsmith-editor/web-dist is missing: restore it from git, or `scripts/web-bundle update` with Node 20+');

  if (!has('no-browser')) {
    let playwright;
    try {
      playwright = await import('playwright');
      line(true, 'playwright', 'imported');
    } catch (e) {
      line(false, 'playwright', `cannot be imported (${e.code ?? e.message}); run npm ci in scripts/editor-verify`);
    }
    if (playwright) {
      try {
        const browser = await playwright.chromium.launch();
        const page = await browser.newPage();
        await page.setContent('<p>ok</p>');
        line(true, 'chromium', `launched ${await browser.version()}`);
        await browser.close();
      } catch (e) {
        line(false, 'chromium', `${String(e.message).split('\n')[0]}; try npx playwright install chromium`);
      }
    }
  }
  process.exit(failed ? 1 : 0);
}

// ------------------------------------------------------------------ launch
async function launch() {
  const old = readState();
  if (old && alive(old.pid)) {
    console.log(`already running: ${old.url} (pid ${old.pid}); run cleanup first to start over`);
    return;
  }
  fs.rmSync(root, { recursive: true, force: true });
  fs.mkdirSync(evidenceDir, { recursive: true });
  const bin = netsmithBin();
  if (!fs.existsSync(bin)) {
    console.error(`${bin} is missing: run cargo build -p netsmith`);
    process.exit(2);
  }
  // A scratch project, so a recipe that types into the editor changes nothing in the repo.
  const project = path.join(root, 'project');
  fs.mkdirSync(project, { recursive: true });
  const run = (args) => spawnSync(bin, args, { cwd: project, encoding: 'utf8' });
  const init = run(['init', '--name', 'verify']);
  if (init.status !== 0) {
    console.error(init.stderr || init.stdout);
    process.exit(2);
  }
  const big = flag('fixture') === 'big' && !flag('file');
  const tree = flag('fixture') === 'tree' && !flag('file');
  // A whole project of the repository's examples (its own symbol library and mapping).
  const projectFixture = !flag('file') && ['schematic_bench'].includes(flag('fixture') ?? '') ? flag('fixture') : null;
  const fixture = big ? 'fixtures/big.mjs' : tree ? 'fixtures/tree.mjs' : (flag('file') ?? path.join(repo, FIXTURES[flag('fixture') ?? 'two_boards'] ?? flag('fixture')));
  if (projectFixture) {
    fs.rmSync(project, { recursive: true, force: true });
    fs.cpSync(path.join(repo, 'examples', projectFixture), project, { recursive: true });
  } else if (big) {
    // A generated 89-part board of 24 module instances, in module files under src/sheets/.
    writeBig(project);
  } else if (tree) {
    // An invented greenhouse project with folders, nested folders and a library of parts, for the file tree.
    writeTree(project);
  } else {
    fs.copyFileSync(fixture, path.join(project, 'src/main.nsm'));
    // The layout file beside the fixture, if it has one (ADR 0032): `x.pcb.toml` is `main.pcb.toml`.
    let layout = fixture.replace(/\.nsm$/, '.pcb.toml');
    // The physical file beside the fixture, if it has one (ADR 0048): `x.physical.nsm` is `main.physical.nsm`.
    const physical = fixture.replace(/\.nsm$/, '.physical.nsm');
    if (physical !== fixture && fs.existsSync(physical)) {
      let text = fs.readFileSync(physical, 'utf8');
      if (path.basename(fixture) === 'outline_and_pours.nsm') {
        // The recipes move the antenna keepout, which only a keepout the source does not place
        // allows: the layout file beside the fixture says where it is.
        text = text.replace('        .x &= 18mm\n        .y &= 2mm\n', '');
        layout = path.join(here, 'fixtures/outline_and_pours.pcb.toml');
      }
      fs.writeFileSync(path.join(project, 'src/main.physical.nsm'), text);
    }
    if (layout !== fixture && fs.existsSync(layout)) fs.copyFileSync(layout, path.join(project, 'src/main.pcb.toml'));
    // The mapping to KiCad beside the fixture, if it has one (ADR 0021): `x.kicad.toml` is the
    // project's `kicad.toml`, and `x.libs/` is its `libs/` (the project's own symbol and
    // footprint libraries), so the PCB view has footprints for the fixture's invented parts.
    const mapping = fixture.replace(/\.nsm$/, '.kicad.toml');
    if (mapping !== fixture && fs.existsSync(mapping)) fs.copyFileSync(mapping, path.join(project, 'kicad.toml'));
    const libs = fixture.replace(/\.nsm$/, '.libs');
    if (libs !== fixture && fs.existsSync(libs)) fs.cpSync(libs, path.join(project, 'libs'), { recursive: true });
    // A module, so the tree and the imports have something to show.
    fs.writeFileSync(
      path.join(project, 'src/power.nsm'),
      '//! A module of the scratch project.\n\nimport std::sources::Rail\n\n/// A rail with a name of its own.\ninterface Supply {\n    .rail: Rail\n}\n',
    );
    if (flag('fixture') === 'catalog_refresh' && !flag('file')) {
      writeRefresh(project, fs.readFileSync(fixture, 'utf8'));
    }
    if ((flag('fixture') ?? 'two_boards') === 'two_boards' && !flag('file')) {
      // An order for the build panel to list and run (ADR 0035).
      fs.appendFileSync(
        path.join(project, 'netsmith.toml'),
        '\n[order.demo.boards]\n"TwoBoards/led" = 5\n"TwoBoards/power" = 2\n',
      );
    }
  }
  const port = await freePort();
  const log = fs.openSync(path.join(root, 'server.log'), 'a');
  // The refresh fixture asks a stand-in for LCSC's endpoint, never the real one, and keeps its
  // cache inside the scratch directory.
  const env = { ...process.env };
  let endpointsPid;
  if (flag('fixture') === 'catalog_refresh' && !flag('file')) {
    const fake = spawn(process.execPath, [path.join(here, 'fixtures/refresh.mjs'), 'serve'], { detached: true, stdio: ['ignore', 'pipe', log] });
    fake.unref();
    const fakePort = await new Promise((resolve) => fake.stdout.once('data', (d) => resolve(String(d).trim())));
    fake.stdout.destroy();
    endpointsPid = fake.pid;
    env.NETSMITH_LCSC_URL = `http://127.0.0.1:${fakePort}/lcsc`;
    env.NETSMITH_CACHE_DIR = path.join(root, 'cache');
  }
  const child = spawn(bin, ['editor', '--port', String(port)], { cwd: project, detached: true, stdio: ['ignore', log, log], env });
  child.unref();
  const url = `http://127.0.0.1:${port}/`;
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`${url}api/analysis`);
      if (r.ok) break;
    } catch {
      // Not up yet.
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  fs.writeFileSync(statePath, JSON.stringify({ pid: child.pid, endpointsPid, port, url, project, fixture, fixtureName: flag('file') ? 'file' : (flag('fixture') ?? 'two_boards') }, null, 2));
  console.log(`launched ${url} (pid ${child.pid}) over ${project}\nlog: ${path.join(root, 'server.log')}`);
}

// ------------------------------------------------------------------ drive
async function drive() {
  const state = readState();
  if (!state || !alive(state.pid)) {
    console.error('no editor is running: run `scripts/editor-verify launch` first');
    process.exit(2);
  }
  const { chromium } = await import('playwright');
  const wanted = rest.filter((a) => !a.startsWith('--'));
  const names = wanted.length === 0 || wanted.includes('all')
    ? fs.readdirSync(path.join(here, 'recipes')).filter((f) => f.endsWith('.mjs')).map((f) => f.replace('.mjs', '')).sort()
    : wanted;
  const browser = await chromium.launch();
  let failed = 0;
  for (const name of names) {
    const file = path.join(here, 'recipes', `${name}.mjs`);
    if (!fs.existsSync(file)) {
      console.error(`there is no recipe ${name}`);
      process.exit(2);
    }
    const { run, fixture } = await import(pathToFileURL(file).href);
    if (fixture && fixture !== state.fixtureName) {
      console.log(`${wanted.length === 0 || wanted.includes('all') ? 'skip' : 'FAIL'} ${name}: it needs \`launch --fixture ${fixture}\` (this session has ${state.fixtureName})`);
      if (!(wanted.length === 0 || wanted.includes('all'))) failed++;
      continue;
    }
    const context = await browser.newContext({ viewport: { width: 1500, height: 900 }, acceptDownloads: true });
    const page = await context.newPage();
    const problems = [];
    page.on('pageerror', (e) => problems.push(`page error: ${e.message}`));
    page.on('console', (m) => m.type() === 'error' && problems.push(`console error: ${m.text()}`));
    let shots = 0;
    const steps = [];
    const tools = {
      page,
      url: state.url,
      project: state.project,
      async shot(label) {
        const file = path.join(evidenceDir, `${name}-${String(++shots).padStart(2, '0')}-${label}.png`);
        await page.screenshot({ path: file });
        console.log(`     screenshot ${file}`);
        return file;
      },
      /** Open a section (schematic, power, system, bom, pcb, build, source) in the first pane. */
      async section(id) {
        await page.locator(`.pane-strip [data-section="${id}"]`).first().click();
        await page.waitForSelector(`.section.sec-${id}:not(.away)`);
      },
      step(text) {
        steps.push(text);
        console.log(`  - ${text}`);
      },
      expect(condition, what) {
        if (!condition) throw new Error(`expected ${what}`);
        console.log(`    ok ${what}`);
      },
    };
    console.log(`recipe ${name}`);
    try {
      await page.goto(state.url);
      await page.waitForSelector('.app', { timeout: 10000 });
      // `.app` is there before the project has loaded (a big one takes a moment): wait for the
      // tree (the source section is in the page, if hidden), and for the board list when the project has a board, so a recipe starts on a page
      // that has something in it. A project that never gets a board (a broken one) just moves on.
      await page.waitForSelector('.sidebar > *', { state: 'attached', timeout: 20000 });
      await page.waitForSelector('.boardpick select option', { state: 'attached', timeout: 5000 }).catch(() => {});
      await run(tools);
      if (problems.length) throw new Error(problems.join('\n'));
      console.log(`PASS ${name}`);
    } catch (e) {
      failed++;
      console.log(`FAIL ${name}: ${String(e.message).slice(0, 700)}`);
      try {
        await tools.shot('failure');
      } catch {
        // The page may be gone.
      }
    }
    await context.close();
  }
  await browser.close();
  process.exit(failed ? 1 : 0);
}

// ------------------------------------------------------------------ evidence, cleanup
function evidence() {
  if (!fs.existsSync(evidenceDir)) {
    console.log('no evidence yet');
    return;
  }
  for (const f of fs.readdirSync(evidenceDir).sort()) console.log(path.join(evidenceDir, f));
  const log = path.join(root, 'server.log');
  if (fs.existsSync(log)) console.log(log);
}

function cleanup() {
  const state = readState();
  if (state && alive(state.pid)) {
    try {
      process.kill(state.pid);
    } catch {
      // Already gone.
    }
  }
  if (state?.endpointsPid && alive(state.endpointsPid)) {
    try {
      process.kill(state.endpointsPid);
    } catch {
      // Already gone.
    }
  }
  if (has('keep-evidence')) {
    fs.rmSync(path.join(root, 'project'), { recursive: true, force: true });
    fs.rmSync(statePath, { force: true });
    console.log(`stopped; evidence kept in ${evidenceDir}`);
  } else {
    fs.rmSync(root, { recursive: true, force: true });
    console.log('stopped and removed everything');
  }
}

const commands = { doctor, launch, drive, evidence, cleanup };
if (!commands[command]) {
  console.error('usage: scripts/editor-verify doctor [--no-browser] | launch [--fixture two_boards|led|regulator|four_layer|outline_and_pours|carrier_and_cards|led_panel|catalog_refresh|constrained|schematic_bench|big|tree] [--file f.nsm] | drive <recipe>|all | evidence | cleanup [--keep-evidence]');
  process.exit(2);
}
await commands[command]();
