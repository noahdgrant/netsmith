// The first moment: until the server has answered with the project, the window is the mark and one line of what is happening
// (`Reading the project…`, then the analysis's own words and clock). `index.html` holds the same markup before any script has run,
// so what the person sees does not change when the page starts; it leaves with a short fade when the project is there, and never
// stays when the server does not answer (the views say that themselves).

import { useEffect, useState } from 'preact/hooks';

import { analysisSince, connection, now, pending, runClock } from '../feedback/state';
import { clock, progressText } from '../feedback/status';
import { reducedMotion } from '../settings';
import { project } from '../store';

/** The net-anvil without its wordmark, in the brand's orange: the same drawing as the favicon. */
export function Mark({ size = 56 }: { size?: number }) {
  return (
    <svg class="splash-mark" viewBox="0 0 128 128" width={size} height={size} aria-hidden="true" focusable="false">
      <path d="M33.7 70.5L92.3 70.5M63 70.5L63 92.4M63 92.4L48.4 112.9M63 92.4L77.6 112.9M48.4 112.9L77.6 112.9M74.5 30.2L40.8 6.6" fill="none" stroke="var(--brand)" stroke-width="8.2" stroke-linecap="round" />
      <g fill="var(--brand)">
        <circle cx="33.7" cy="70.5" r="9.6" />
        <circle cx="63" cy="70.5" r="9.6" />
        <circle cx="92.3" cy="70.5" r="9.6" />
        <circle cx="63" cy="92.4" r="9.6" />
        <circle cx="48.4" cy="112.9" r="9.6" />
        <circle cx="77.6" cy="112.9" r="9.6" />
        <rect x="62.7" y="10.1" width="23.8" height="40.2" rx="3.2" transform="rotate(35 74.5 30.2)" />
      </g>
    </svg>
  );
}

export function Splash() {
  const waiting = project.value === null && connection.value !== 'offline';
  const [shown, setShown] = useState(true);
  const [leaving, setLeaving] = useState(false);
  useEffect(() => (waiting ? runClock() : undefined), [waiting]);
  useEffect(() => {
    if (waiting || !shown) return;
    setLeaving(true);
    const t = window.setTimeout(() => setShown(false), reducedMotion() ? 0 : 200);
    return () => window.clearTimeout(t);
  }, [waiting]);
  if (!shown) return null;
  const since = analysisSince.value;
  const progress = pending.value.server?.progress;
  const text =
    since === null
      ? 'Reading the project…'
      : `Analysing the project… ${[clock(Math.max(0, now.value - since)), progress ? progressText(progress) : ''].filter(Boolean).join(' · ')}`;
  return (
    <div class={`splash ${leaving ? 'leaving' : ''}`} role="status" aria-live="polite" data-testid="splash">
      <Mark />
      <div class="splash-text">{text}</div>
    </div>
  );
}
