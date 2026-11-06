// A column of help at the right edge of a page-like section (Compare, the bill of materials, Build) when the pane is wide enough to have the
// room: "What does this view show?", a few plain sentences about how to read the page. It stands in the margin the centred page leaves, so
// it takes nothing from the page; on a pane without that margin (a split, a small window) it is not in the page at all. One click folds it to a
// tab, and the choice is remembered for each view.

import { signal } from '@preact/signals';
import type { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';

import { load, save } from '../storage';
import { Icon } from './Icons';

const KEY = 'netsmith.viewhelp.v1';
/** Which views have their help folded (by view id). Open is the default. */
export const helpFolded = signal<Record<string, boolean>>(load<Record<string, boolean>>(KEY, {}));

export function setHelpFolded(id: string, folded: boolean) {
  const next = { ...helpFolded.peek(), [id]: folded };
  helpFolded.value = next;
  save(KEY, next);
}

/** The width of the column and the air round it, in rem; the page's own width is `--page-w`. */
export const HELP_REM = 17;
const GAP_REM = 1.5;

/** Whether a pane of `width` px, whose centred page is `page` px wide, has margin enough on its right for the help (and the same on its left, so the page stays centred). */
export const hasRoomForHelp = (width: number, page: number, rem: number): boolean => width >= page + 2 * (HELP_REM + GAP_REM) * rem;

export function ViewHelp({ id, children }: { id: string; children: ComponentChildren }) {
  const holder = useRef<HTMLDivElement>(null);
  const [room, setRoom] = useState(false);
  useEffect(() => {
    const host = holder.current?.parentElement;
    if (!host) return;
    const measure = () => {
      const root = getComputedStyle(document.documentElement);
      const rem = parseFloat(root.fontSize) || 16;
      const probe = document.createElement('div');
      probe.style.cssText = 'position:absolute;visibility:hidden;width:var(--page-w)';
      host.appendChild(probe);
      const page = probe.getBoundingClientRect().width;
      probe.remove();
      setRoom(hasRoomForHelp(host.clientWidth, page, rem));
    };
    measure();
    const obs = typeof ResizeObserver === 'function' ? new ResizeObserver(measure) : null;
    obs?.observe(host);
    window.addEventListener('resize', measure);
    return () => {
      obs?.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, []);
  const folded = !!helpFolded.value[id];
  return (
    <div ref={holder} class="vh-holder">
      {room && (
        <aside class={`vh ${folded ? 'folded' : ''}`} aria-label="About this view" data-testid="view-help" data-view={id}>
          <button type="button" class="vh-toggle" aria-expanded={!folded} aria-controls={`vh-body-${id}`} data-testid="view-help-toggle" onClick={() => setHelpFolded(id, !folded)}>
            <Icon name="chevron-right" size={12} class="vh-chev" />
            <span>What does this view show?</span>
          </button>
          {!folded && (
            <div class="vh-body" id={`vh-body-${id}`}>
              {children}
            </div>
          )}
        </aside>
      )}
    </div>
  );
}
