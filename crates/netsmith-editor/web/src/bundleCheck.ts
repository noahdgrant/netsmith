// Whether the page on screen is the one the binary serves now: asked of the server (`index.html`, not from the cache) when About opens
// or the diagnostics are copied, and kept as a signal for both. A long-lived tab can be older than the netsmith that serves it.

import { signal } from '@preact/signals';

import { bundleInHtml, bundleMatch, bundleOf } from './diagnostics';

export const bundleCheck = signal<'match' | 'differs' | 'unknown' | undefined>(undefined);

/** Ask the server which page it serves and compare it with the script of this one. Never throws: no answer is `unknown`. */
export async function checkBundle(): Promise<'match' | 'differs' | 'unknown'> {
  const script = document.querySelector<HTMLScriptElement>('script[type="module"][src]');
  const loaded = bundleOf(script?.src);
  let served: string | null = null;
  try {
    const r = await fetch('/', { cache: 'no-store' });
    if (r.ok) served = bundleInHtml(await r.text());
  } catch {
    // No answer is not a difference.
  }
  const result = bundleMatch(loaded, served);
  bundleCheck.value = result;
  return result;
}
