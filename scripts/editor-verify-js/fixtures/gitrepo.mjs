// A throwaway git repository for the Compare recipe, made from the scratch project (`launch --git`):
// the project as it was committed, a branch that changes the circuit, a branch that does not build, and one edit
// that is not committed. Mirrors what crates/netsmith/tests/editor_compare.rs does for the server.
//
//   main     first   the project as launched, with a debug header on the LED board (tag v1.0)
//   broken   a typo in a type (does not analyse)
//   feature  a second commit: the LED's resistor is 150 ohm, a bulk capacitor on the regulator's input,
//            the output rail 3V3 is VOUT, and the debug header is gone
//   (work)   the output's current is 80 mA where the commit has 50 mA: the rail's headroom shrank
//
// `feature` is checked out. The ref to compare with is `main` (or `v1.0`).

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const ENV = { ...process.env, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_SYSTEM: '/dev/null', GIT_OPTIONAL_LOCKS: '0' };

export function git(dir, ...args) {
  const r = spawnSync('git', ['-C', dir, ...args], { encoding: 'utf8', env: ENV });
  if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr || r.stdout}`);
  return r.stdout.trim();
}

/** Whether git is on the PATH. */
export function gitHere() {
  return spawnSync('git', ['--version'], { encoding: 'utf8' }).status === 0;
}

function edit(project, file, from, to) {
  const p = path.join(project, file);
  const text = fs.readFileSync(p, 'utf8');
  if (!text.includes(from)) throw new Error(`${file} has no \`${from}\``);
  fs.writeFileSync(p, text.replace(from, to));
}

const INPUT_CAP = '    .input_cap:  Capacitor { .effective_capacitance >= 1uF }\n';
const DEBUG = '    .debug: Header1x2\n';
const BULK = '    .bulk: Capacitor { .effective_capacitance >= 10uF }\n    .vin.pos ~> .bulk ~> .vin.neg\n';

/** The change a person would make on a branch (committed on `feature`). */
export function applyFeature(project) {
  edit(project, 'src/main.nsm', '.r1: Resistor {}', '.r1: Resistor { .resistance &= 150ohm +/- 1% }');
  edit(project, 'src/main.nsm', INPUT_CAP, BULK + INPUT_CAP);
  edit(project, 'src/main.nsm', '"3V3"', '"VOUT"');
  edit(project, 'src/main.nsm', DEBUG, '');
}

/** What is left uncommitted: the output draws up to 80 mA where the commit has 50. */
export function applyWorkEdit(project) {
  edit(project, 'src/main.nsm', '.current &= 0mA..50mA', '.current &= 0mA..80mA');
}

/** Make the repository: the project as it is now is the first commit. Needs the two_boards fixture (its text is edited). */
export function makeCompareRepo(project) {
  git(project, 'init', '-q');
  git(project, 'symbolic-ref', 'HEAD', 'refs/heads/main');
  git(project, 'config', 'user.name', 'Ada Verify');
  git(project, 'config', 'user.email', 'ada@example.com');
  git(project, 'config', 'commit.gpgsign', 'false');
  git(project, 'config', 'tag.gpgsign', 'false');
  // a part on the LED board that the branch takes away, so the comparison has something removed
  edit(project, 'src/main.nsm', '    .power_in: Socket1x2\n', `    .power_in: Socket1x2\n${DEBUG}`);
  fs.writeFileSync(path.join(project, '.gitignore'), 'out/\nfab/\n.netsmith/\n*.log\n');
  git(project, 'add', '-A');
  git(project, 'commit', '-q', '-m', 'First: two boards, a regulator and an LED');
  git(project, 'tag', 'v1.0');
  // A branch whose project does not build.
  git(project, 'checkout', '-q', '-b', 'broken');
  edit(project, 'src/main.nsm', '.r1: Resistor {}', '.r1: Resistr {}');
  git(project, 'commit', '-q', '-am', 'A typo in the type of the resistor');
  // The branch that is worked on.
  git(project, 'checkout', '-q', 'main');
  git(project, 'checkout', '-q', '-b', 'feature');
  applyFeature(project);
  git(project, 'commit', '-q', '-am', 'Tune the LED, add a bulk capacitor, rename the output rail');
  applyWorkEdit(project);
}

/** Put the project back as `makeCompareRepo` left it: another recipe may have changed files. */
export function resetCompareRepo(project) {
  git(project, 'checkout', '-q', '-f', 'feature');
  git(project, 'reset', '-q', '--hard', 'feature');
  git(project, 'clean', '-fdq');
  applyWorkEdit(project);
}
