// scripts/editor-verify waits: proves, from the text of the recipes and their helpers, that none waits for a time or judges one.
// A recipe waits on what the page or the project does (settle.mjs); the hang guard (`HANG_MS`, set in cli.mjs) is the only timeout,
// and no assertion compares a duration with a limit. It runs without a browser and without packages (CI runs it with the slices).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const files = [
  ...fs.readdirSync(path.join(here, 'recipes')).filter((f) => f.endsWith('.mjs')).map((f) => path.join('recipes', f)),
  ...fs.readdirSync(here).filter((f) => f.endsWith('-helpers.mjs')),
];

const rules = [
  [/waitForTimeout/, 'a pause of a fixed time: wait for what the page signals (whenIdle from settle.mjs) or for the thing expected'],
  [/\btimeout\s*:/, 'a timeout of its own: the hang guard is the default of every wait (HANG_MS), set once in cli.mjs'],
  [/\bsetTimeout\b|\bsetInterval\b/, 'a timer: hold a request with gate() from settle.mjs and open it when the step has seen what it came for'],
  [/\bperformance\.now\b/, 'a clock: no step measures a duration'],
  [/\bDate\.now\(\)/, 'a clock: a duration may be printed but never compared with a limit (read it into a `const` and log it)', (line) => /^\s*const \w+ = Date\.now\(\);\s*$/.test(line) || /console\.log/.test(line)],
];

let bad = 0;
for (const file of files) {
  fs.readFileSync(path.join(here, file), 'utf8')
    .split('\n')
    .forEach((line, i) => {
      if (/^\s*\/\//.test(line)) return;
      for (const [re, why, allowed] of rules) {
        if (re.test(line) && !(allowed && allowed(line))) {
          console.error(`${file}:${i + 1}: ${why}\n    ${line.trim()}`);
          bad++;
        }
      }
    });
}
if (bad) {
  console.error(`\n${bad} wait(s) on a time: a recipe passes or fails by what the page does, never by the speed of the machine (see .agents/skills/netsmith-dev-verify-editor/SKILL.md)`);
  process.exit(1);
}
console.log(`ok: no recipe or helper waits for a time or judges one (${files.length} files)`);
