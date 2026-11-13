// How a recipe waits (this file is not a recipe: recipes are in recipes/). A recipe waits for something the page or the project
// does, never for a time: a pause of a fixed time passes or fails by the speed of the machine it runs on.
//
// - `whenIdle(page)`: the page is done with what it just did. The page says when it is busy
//   (crates/netsmith-editor/web/src/idle.ts): `<html data-busy>` while a request, an analysis, a pause before an action (a
//   debounce, a hover's rest) or a transition is under way, and `window.__netsmith.idle()` resolves once none has been under way
//   for three frames. Use it after an action, before reading what the action changed. A page that holds a request on purpose
//   (a `route` that waits for a `gate()`) is busy until the gate opens: wait for what the held request shows, not for idle.
// - `waitFor(what, check)`: poll something outside the page (a file the page writes, a server) until `check()` is true.
// - `gate()`: a request a recipe holds with `page.route` until the recipe opens the gate, so the state the held request shows is
//   there for as long as the recipe needs it, on a machine of any speed.
// - A wait that never ends fails after the hang guard (HANG_MS, also the default timeout of every `waitFor...` of the page): the
//   failure says the page hung. The guard is no limit on speed; no step compares a duration with a limit.

/** How long a wait may go on before the recipe fails as hung. Long on purpose: it only says that waiting longer is pointless.
 * `NETSMITH_VERIFY_HANG_MS` shortens it while a recipe is being written, to find a hang sooner; CI uses the default. */
export const HANG_MS = Number(process.env.NETSMITH_VERIFY_HANG_MS) || 300_000;

/** The error of a wait that outlasted the hang guard; the driver reports it as a hang. */
export class Hung extends Error {
  constructor(what) {
    super(`${what}`);
    this.name = 'TimeoutError';
  }
}

/** Wait until the page has nothing in flight and nothing moving. */
export async function whenIdle(page) {
  await page.waitForFunction(() => window.__netsmith?.idle().then(() => true), null, { polling: 'raf' });
}

/** Poll `check` (it may be async) until it returns something true, and return that. `what` names the state, for the failure of a hang. */
export async function waitFor(what, check) {
  const guard = Date.now() + HANG_MS;
  for (;;) {
    const found = await check();
    if (found) return found;
    if (Date.now() > guard) throw new Hung(`waiting for ${what}`);
    // The pause between two looks: how often, not how long (nothing here is judged by it).
    await new Promise((r) => setTimeout(r, 20));
  }
}

/** A gate for a request a recipe holds: `await g.opened` inside a `page.route` handler, `g.open()` when the recipe has seen what it came for. */
export function gate() {
  let open;
  const opened = new Promise((resolve) => (open = resolve));
  return { opened, open };
}

/** The version of the analysis the page has taken (the server counts them), read before a change is made. */
export const pageVersion = (page) => page.evaluate(() => Number(document.documentElement.dataset.analysis || 0));

/** After a recipe changed a file of the project: wait until the page has taken an analysis newer than `before` (`pageVersion`, read before the change) and is idle again. */
export async function analysedSince(page, before) {
  await page.waitForFunction((v) => Number(document.documentElement.dataset.analysis || 0) > v, before);
  await whenIdle(page);
}
