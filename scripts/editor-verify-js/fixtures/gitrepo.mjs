// A throwaway git repository for the Compare recipe, made from the scratch project (`launch --git`). The files on
// disk are the project as launched, so every other recipe sees the project it expects; the history is what differs:
// an earlier design of the same boards. Mirrors what crates/netsmith-workspace/tests/editor_compare.rs does for the server.
//
//   main     first   the earlier design (tag v1.0): a debug header on the LED board, no input capacitor on the regulator,
//                    the LED's resistor fixed at 150 ohm, the output rail named VOUT and drawing up to 30 mA
//   broken   a typo in a type (it does not analyse)
//   feature  a second commit: the project as launched. `feature` is checked out and is HEAD.
//   (work)   an untracked NOTES.md, so the working tree is not clean and the circuit is the same as HEAD's
//
// Compared with `main` the circuit has: a part removed (the debug header), one added (the input capacitor), two values
// changed (the resistor, and the LED the solver then picks), a net renamed (VOUT to 3V3), nets that gained pins and a rail
// whose headroom shrank from 70 mA to 50 mA. Compared with HEAD it has none: only the source differs (NOTES.md).

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

function replaceOnce(text, from, to) {
  if (!text.includes(from)) throw new Error(`the fixture has no \`${from.trim()}\``);
  return text.replace(from, to);
}

/** The earlier design: `text` is the project as launched. */
export function earlierDesign(text) {
  let t = text;
  t = replaceOnce(t, '.r1: Resistor {}', '.r1: Resistor { .resistance &= 150ohm +/- 1% }');
  t = replaceOnce(t, '    .power_in: Socket1x2\n', '    .debug: Header1x2\n    .power_in: Socket1x2\n');
  t = replaceOnce(t, '"3V3"', '"VOUT"');
  t = replaceOnce(t, '.current &= 0mA..50mA', '.current &= 0mA..30mA');
  t = replaceOnce(t, '    .input_cap:  Capacitor { .effective_capacitance >= 1uF }\n', '');
  t = replaceOnce(t, '    .input_cap.max_voltage  >= .vin.voltage * 1.5\n', '');
  t = replaceOnce(t, '    .vin.pos  ~> .input_cap  ~> .vin.neg\n', '');
  return t;
}

const NOTES = '# Notes\n\nCheck the LED current against the datasheet before the first order.\n';

/** Make the repository: the project as it is now is the second commit, an earlier design the first. */
export function makeCompareRepo(project) {
  const main = path.join(project, 'src/main.nsm');
  const launched = fs.readFileSync(main, 'utf8');
  git(project, 'init', '-q');
  git(project, 'symbolic-ref', 'HEAD', 'refs/heads/main');
  git(project, 'config', 'user.name', 'Ada Verify');
  git(project, 'config', 'user.email', 'ada@example.com');
  git(project, 'config', 'commit.gpgsign', 'false');
  git(project, 'config', 'tag.gpgsign', 'false');
  fs.writeFileSync(path.join(project, '.gitignore'), 'out/\nfab/\n.netsmith/\n*.log\n');
  fs.writeFileSync(main, earlierDesign(launched));
  git(project, 'add', '-A');
  git(project, 'commit', '-q', '-m', 'First design: a debug header, no input capacitor');
  git(project, 'tag', 'v1.0');
  // A branch whose project does not build.
  git(project, 'checkout', '-q', '-b', 'broken');
  fs.writeFileSync(main, replaceOnce(fs.readFileSync(main, 'utf8'), '.r1: Resistor {', '.r1: Resistr {'));
  git(project, 'commit', '-q', '-am', 'A typo in the type of the resistor');
  // The branch that is worked on: the project as it was launched.
  git(project, 'checkout', '-q', 'main');
  git(project, 'checkout', '-q', '-b', 'feature');
  fs.writeFileSync(main, launched);
  git(project, 'commit', '-q', '-am', 'Input capacitor, 3V3 for the output, no debug header');
  fs.writeFileSync(path.join(project, 'NOTES.md'), NOTES);
}

/** Put the project back as `makeCompareRepo` left it: another recipe may have changed files. */
export function resetCompareRepo(project) {
  git(project, 'checkout', '-q', '-f', 'feature');
  git(project, 'reset', '-q', '--hard', 'feature');
  git(project, 'clean', '-fdq');
  fs.writeFileSync(path.join(project, 'NOTES.md'), NOTES);
}
