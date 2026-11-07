// The right-click menu of a file tab: the comparison of that file with the ref (read-only, the file at the ref beside
// the file on disk), and closing it.

import { signal } from '@preact/signals';
import { useEffect, useRef } from 'preact/hooks';

import { diskCompare, openDiff, refText } from '../compare/store';
import { closeFile, revealSection } from '../store';
import { askForRef } from './ChangesSwitch';

export const fileTabMenu = signal<{ path: string; x: number; y: number } | null>(null);

export function FileTabMenu() {
  const m = fileTabMenu.value;
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    box.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, [m?.path, m?.x, m?.y]);
  if (!m) return null;
  const close = () => (fileTabMenu.value = null);
  const c = diskCompare.value;
  const listed = !!c?.source.files.some((f) => f.path === m.path);
  const known = !!c && /\.(nsm|toml|csv)$/.test(m.path);
  return (
    <>
      <div class="menu-shield" onPointerDown={close} onContextMenu={(e) => (e.preventDefault(), close())} />
      <div class="menu" ref={box} role="menu" aria-label={`The tab ${m.path}`} style={{ left: Math.min(m.x, window.innerWidth - 270), top: m.y }} onKeyDown={(e) => e.key === 'Escape' && close()}>
        <div class="menu-head">{m.path}</div>
        {c ? (
          <button
            role="menuitem"
            data-testid="tab-show-changes"
            disabled={!known}
            title={!known ? 'Only the project’s own sources (.nsm, .toml, .csv) are in the repository’s comparison' : listed ? undefined : `${m.path} is the same as at ${refText.value}`}
            onClick={() => {
              close();
              openDiff({ path: m.path, where: 'source' });
              revealSection('source');
            }}
          >
            Show changes vs {refText.value}
          </button>
        ) : (
          <button
            role="menuitem"
            data-testid="tab-compare"
            onClick={() => {
              close();
              askForRef();
            }}
          >
            Compare with a git ref…
          </button>
        )}
        <button
          role="menuitem"
          onClick={() => {
            close();
            closeFile(m.path);
          }}
        >
          Close
        </button>
      </div>
    </>
  );
}
