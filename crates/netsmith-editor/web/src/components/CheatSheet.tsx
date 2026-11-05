// The keyboard cheat sheet (`?`, or the palette's "Keyboard shortcuts"): every shortcut, grouped.
// The global ones are read from the command registry, so a new command shows up here by itself;
// the keys of one view are listed from `LOCAL_KEYS`.

import { useLayoutEffect, useRef } from 'preact/hooks';

import { type Command, allCommands } from '../commands';
import { LOCAL_KEYS } from '../commandset';
import { cheatSheet, toggleCheatSheet } from '../quickpick';
import { Keys } from './QuickPick';

function groups(): { group: string; commands: Command[] }[] {
  const by = new Map<string, Command[]>();
  for (const c of allCommands()) {
    if (!c.keys) continue;
    by.set(c.group, [...(by.get(c.group) ?? []), c]);
  }
  const order = ['Go to', 'View', 'Selection', 'File', 'Build', 'Help'];
  return [...by.entries()]
    .map(([group, commands]) => ({ group, commands }))
    .sort((a, b) => (order.indexOf(a.group) + 1 || 99) - (order.indexOf(b.group) + 1 || 99));
}

/** `Ctrl+Z · Ctrl+Shift+Z` or `F or 0`: each key in its own cap, the joiners as words. */
function LocalKeys({ text }: { text: string }) {
  const parts = text.split(/( · | or )/);
  return (
    <span class="qp-keys">
      {parts.map((p, i) =>
        i % 2 === 1 ? (
          <span class="or" key={i}>
            {p.trim() === 'or' ? 'or' : '·'}
          </span>
        ) : (
          <kbd key={i}>{p}</kbd>
        ),
      )}
    </span>
  );
}

export function CheatSheet() {
  const box = useRef<HTMLDivElement>(null);
  const open = cheatSheet.value;
  const before = useRef<Element | null>(null);
  useLayoutEffect(() => {
    if (!open) return;
    before.current = document.activeElement;
    box.current?.focus();
    return () => {
      const b = before.current;
      if (b instanceof HTMLElement) b.focus();
    };
  }, [open]);
  if (!open) return null;
  const close = () => toggleCheatSheet(false);
  return (
    <div class="qp-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="cheat"
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        tabIndex={-1}
        ref={box}
        onKeyDown={(e) => {
          if (e.key === 'Escape' || e.key === '?') {
            e.preventDefault();
            e.stopPropagation();
            close();
          } else if (e.key === 'Tab') {
            e.preventDefault();
          }
        }}
      >
        <header>
          <h2>Keyboard shortcuts</h2>
          <span class="faint">Ctrl+Shift+P lists every command, Ctrl+P finds a part, net, module or file.</span>
          <span class="spacer" />
          <button class="tool small" onClick={close} aria-label="Close the shortcuts">
            Close <kbd>Esc</kbd>
          </button>
        </header>
        <div class="cheat-cols">
          {groups().map(({ group, commands }) => (
            <section key={group}>
              <h3>{group}</h3>
              <dl>
                {commands.map((c) => (
                  <div class="cheat-row" key={c.id}>
                    <dt>{c.id === 'pane.toggle' ? 'Split right, or close the second pane' : c.title}</dt>
                    <dd>
                      <Keys chord={c.keys!} />
                      {(c.alsoKeys ?? []).map((k) => (
                        <span class="or" key={k}>
                          or <Keys chord={k} />
                        </span>
                      ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
          {LOCAL_KEYS.map(({ group, keys }) => (
            <section key={group}>
              <h3>{group}</h3>
              <dl>
                {keys.map(([k, what]) => (
                  <div class="cheat-row" key={k}>
                    <dt>{what}</dt>
                    <dd>
                      <LocalKeys text={k} />
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
