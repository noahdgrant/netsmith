// The first analysis, as the page waits for it. A request that sends `X-Netsmith-Pending: 1` is
// answered at once while the server's first analysis runs: `503` with `{pending: true, elapsedMs}`
// (ADR 0019). Pure functions only: the request is in `client.ts`, the wait in `store.ts`.

/** The header that asks to be told, not to wait. */
export const PENDING_HEADER = 'X-Netsmith-Pending';

/** How long the first analysis has run, if a response is the server's "still analysing": a `503`
 * whose body says `pending`. Any other `503` (a page that is not there) is not it. */
export function pendingElapsed(status: number, body: string): number | null {
  if (status !== 503) return null;
  try {
    const v: unknown = JSON.parse(body);
    if (v && typeof v === 'object' && (v as { pending?: unknown }).pending === true) {
      const ms = Number((v as { elapsedMs?: unknown }).elapsedMs);
      return Number.isFinite(ms) && ms >= 0 ? ms : 0;
    }
  } catch {
    // Not JSON: not ours.
  }
  return null;
}

/** Without a socket the first retry comes after this long, and each waits twice as long as the one
 * before, up to the cap. */
export const RETRY_FIRST_MS = 1000;
export const RETRY_MAX_MS = 5000;
/** With the socket open the server tells the page when the analysis ends (`rebuilt`, and `analysing`
 * with `progress` to a socket that connects mid-way), so a retry is only the safety for a push that
 * is lost. Each retry is a failed request in the browser's console, so it is rare. */
export const RETRY_SAFETY_MS = 30_000;

/** How long to wait before asking again, after waiting `previous` (0 for the first time). The
 * socket's `rebuilt` ends the wait sooner. */
export function nextDelay(previous: number, socketOpen = false): number {
  if (socketOpen) return RETRY_SAFETY_MS;
  return previous <= 0 ? RETRY_FIRST_MS : Math.min(RETRY_MAX_MS, previous * 2);
}
