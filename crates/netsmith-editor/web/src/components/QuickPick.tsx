// Go to anything (Ctrl+P) and the command palette (Ctrl+Shift+P, Ctrl+K): one input over a list.
// What is typed picks the rows: nothing special searches parts, nets, modules, definitions and
// files; `>` runs a command, `@` goes to a module or definition, `#` to a net, `:` to a line.

import type { ComponentChildren } from 'preact';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';

import { available, byRecent, chordLabels, runCommand } from '../commands';
import { goToSelection } from '../commandset';
import { pieces } from '../fuzzy';
import { type Hit, type Item, PREFIXES, buildIndex, parseLine, parseQuery, recentItems, search } from '../quickopen';
import { closePicker, picker, recentCommands, recentPicks, rememberQuery, usedCommand, usedPick } from '../quickpick';
import { activeFile, detail, goTo, openFile, openFiles, project, revealSection, say, viewBoardOfDefinition } from '../store';

/** A name with the letters the query matched marked. */
function Marked({ text, at }: { text: string; at: number[] | undefined }) {
  return (
    <>
      {pieces(text, at).map((p, i) =>
        p.hit ? (
          <mark key={i} class="qp-hit">
            {p.text}
          </mark>
        ) : (
          p.text
        ),
      )}
    </>
  );
}

const Svg = ({ children }: { children: ComponentChildren }) => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {children}
  </svg>
);

/** One small icon per kind of row, in the colour the badges of the tree use for that kind. */
function KindIcon({ kind }: { kind: Item['kind'] | 'line' | 'tip' | 'search' }) {
  switch (kind) {
    case 'search':
      return (
        <Svg>
          <circle cx="7" cy="7" r="4.6" />
          <path d="M10.6 10.6L14 14" />
        </Svg>
      );
    case 'part':
      return (
        <Svg>
          <rect x="4" y="4" width="8" height="8" rx="1" />
          <path d="M6 1.8v2.2M10 1.8v2.2M6 12v2.2M10 12v2.2M1.8 6h2.2M1.8 10h2.2M12 6h2.2M12 10h2.2" />
        </Svg>
      );
    case 'net':
      return (
        <Svg>
          <circle cx="3.5" cy="4" r="1.4" />
          <circle cx="12.5" cy="12" r="1.4" />
          <path d="M5 4h4.5v8h1.8" />
        </Svg>
      );
    case 'module':
      return (
        <Svg>
          <rect x="2" y="3" width="12" height="10" rx="1.4" />
          <path d="M2 6.2h12M5 3v3.2" />
        </Svg>
      );
    case 'definition':
      return (
        <Svg>
          <path d="M5.5 3C4 3 4 4 4 5.2S4 7 2.8 8c1.2 1 1.2 1.6 1.2 2.8S4 13 5.5 13M10.5 3c1.5 0 1.5 1 1.5 2.2S12 7 13.2 8c-1.2 1-1.2 1.6-1.2 2.8S12 13 10.5 13" />
        </Svg>
      );
    case 'file':
      return (
        <Svg>
          <path d="M3.5 1.8h5.7L12.5 5v9.2h-9z" />
          <path d="M9 1.8V5h3.5" />
        </Svg>
      );
    case 'command':
      return (
        <Svg>
          <path d="M3 4.5l4 3.5-4 3.5M8.5 12h4.5" />
        </Svg>
      );
    case 'line':
      return (
        <Svg>
          <path d="M2.5 4h11M2.5 8h11M2.5 12h6" />
        </Svg>
      );
    default:
      return (
        <Svg>
          <circle cx="8" cy="8" r="5.6" />
          <path d="M8 7.2v3.4M8 5.2v.1" />
        </Svg>
      );
  }
}

/** The key chord of a command, as keycaps. */
export function Keys({ chord }: { chord: string }) {
  return (
    <span class="qp-keys" aria-label={chordLabels(chord).join(' ')}>
      {chordLabels(chord).map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
    </span>
  );
}

interface Row {
  id: string;
  kind: Item['kind'] | 'line' | 'tip';
  item?: Item;
  hit?: Hit;
  /** For rows that are not items: what they say and do. */
  label?: string;
  detail?: string;
  run: () => void;
}

interface RowGroup {
  title: string;
  rows: Row[];
}

function commandItems(): Item[] {
  return available().map((c) => ({
    id: `cmd:${c.id}`,
    kind: 'command' as const,
    label: c.title,
    detail: c.group,
    keys: c.keys,
    fields: [
      { name: 'label', text: c.title, weight: 2 },
      ...(c.aliases ?? []).map((a) => ({ name: 'alias', text: a, weight: 1 })),
      { name: 'detail', text: c.group, weight: 0.5 },
    ],
    target: { type: 'command' as const, id: c.id },
  }));
}

/** Do what choosing a row of go to anything does. */
function choose(item: Item) {
  closePicker();
  const t = item.target;
  if (t.type === 'command') {
    usedCommand(t.id);
    // After the window is gone, so the command's own focus change is the last.
    setTimeout(() => runCommand(t.id), 0);
    return;
  }
  usedPick(item.id);
  if (t.type === 'selection') goToSelection(t.sel);
  else if (t.type === 'definition') {
    void goTo({ file: t.file, line: t.line, column: 1, endLine: t.line, endColumn: 1 }, t.name);
    viewBoardOfDefinition(t.defKind, t.name);
  } else {
    openFile(t.path)
      .then(() => revealSection('source'))
      .catch((e) => say(String(e), true));
  }
}

function Dialog({ initial }: { initial: string }) {
  const [query, setQuery] = useState(initial);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const before = useRef<Element | null>(document.activeElement);
  const d = detail.value;
  const p = project.value;
  const index = useMemo(() => buildIndex(d, p), [d, p]);
  const { mode, text } = parseQuery(query);

  useLayoutEffect(() => {
    const el = input.current;
    if (el) {
      el.focus();
      // A bare mode (`>`) keeps the caret after it; the last search is selected, so typing replaces it.
      if (initial.length > 1) el.select();
      else el.setSelectionRange(initial.length, initial.length);
    }
    return () => {
      // Back to where the keys were, unless the choice moved them (the editor took the cursor).
      const b = before.current;
      setTimeout(() => {
        if ((document.activeElement === document.body || !document.activeElement) && b instanceof HTMLElement) b.focus();
      }, 20);
    };
  }, []);

  const file = activeFile.value ? openFiles.value[activeFile.value] : undefined;
  const lines = file ? file.text.split('\n').length : null;

  const groups: RowGroup[] = useMemo(() => {
    const out: RowGroup[] = [];
    const wrap = (hit: Hit): Row => ({ id: hit.item.id, kind: hit.item.kind, item: hit.item, hit, run: () => choose(hit.item) });
    if (mode === 'line') {
      const r = parseLine(text, lines);
      const path = activeFile.value;
      out.push({
        title: '',
        rows: [
          {
            id: 'line',
            kind: 'line',
            label: r.message,
            detail: path ?? '',
            run: () => {
              if (r.line == null || !path) return;
              closePicker();
              void goTo({ file: path, line: r.line, column: 1, endLine: r.line, endColumn: 1 });
            },
          },
        ],
      });
      return out;
    }
    if (mode === 'all' && !text) {
      const recent = recentItems(index, recentPicks.value).slice(0, 8);
      if (recent.length) out.push({ title: 'Recently chosen', rows: recent.map((item) => wrap({ item, score: 0, marks: {} })) });
      out.push({
        title: 'Narrow the search',
        rows: PREFIXES.map((x) => ({
          id: `tip:${x.prefix}`,
          kind: 'tip' as const,
          label: x.prefix,
          detail: `${x.title}: ${x.hint}`,
          run: () => {
            setQuery(x.prefix);
            setActive(0);
            input.current?.focus();
          },
        })),
      });
      return out;
    }
    if (mode === 'command' && !text) {
      // Recently used commands first, most recent first; then the others in the order they were added.
      const recent = recentCommands.value.map((id) => `cmd:${id}`);
      const known = new Set(recent);
      const ordered = byRecent(commandItems(), recent);
      const used = ordered.filter((i) => known.has(i.id)).map((item) => wrap({ item, score: 0, marks: {} }));
      const rest = ordered.filter((i) => !known.has(i.id)).map((item) => wrap({ item, score: 0, marks: {} }));
      if (used.length) out.push({ title: 'Recently used', rows: used });
      if (rest.length) out.push({ title: used.length ? 'Other commands' : '', rows: rest });
      return out;
    }
    const found = search(index, commandItems(), query);
    for (const g of found) out.push({ title: g.title, rows: g.hits.map(wrap) });
    return out;
  }, [index, query, mode, text, lines, activeFile.value, recentPicks.value, recentCommands.value, available().length]);

  const rows = groups.flatMap((g) => g.rows);
  const at = Math.min(active, Math.max(rows.length - 1, 0));

  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [at, query]);

  const move = (by: number) => {
    if (!rows.length) return;
    setActive((at + by + rows.length) % rows.length);
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      move(1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      move(-1);
    } else if (e.key === 'PageDown') {
      e.preventDefault();
      setActive(Math.min(at + 8, rows.length - 1));
    } else if (e.key === 'PageUp') {
      e.preventDefault();
      setActive(Math.max(at - 8, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      rows[at]?.run();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      closePicker();
    } else if (e.key === 'Tab') {
      // The window is one stop: Tab does not leave it.
      e.preventDefault();
      move(e.shiftKey ? -1 : 1);
    }
  };

  const title = mode === 'command' ? 'Command palette' : 'Go to anything';
  const placeholder =
    mode === 'command'
      ? 'Type a command'
      : mode === 'symbol'
        ? 'Go to a module or definition'
        : mode === 'net'
          ? 'Go to a net'
          : mode === 'line'
            ? 'Go to a line of the open file'
            : 'Go to a part, net, module, definition or file';
  const count = rows.length;
  const empty = rows.length === 0 && (text !== '' || mode === 'net' || mode === 'symbol' || mode === 'command');
  let n = -1;
  return (
    <div class="qp-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closePicker()}>
      <div class="qp" role="dialog" aria-modal="true" aria-label={title} data-mode={mode}>
        <div class="qp-field">
          <span class="qp-mode" aria-hidden="true">
            <KindIcon kind={mode === 'command' ? 'command' : mode === 'net' ? 'net' : mode === 'symbol' ? 'module' : mode === 'line' ? 'line' : 'search'} />
          </span>
          <input
            ref={input}
            type="text"
            role="combobox"
            aria-expanded="true"
            aria-controls="qp-list"
            aria-autocomplete="list"
            aria-activedescendant={rows[at] ? `qp-opt-${at}` : undefined}
            aria-label={title}
            placeholder={placeholder}
            spellcheck={false}
            autocomplete="off"
            value={query}
            onInput={(e) => {
              const v = (e.currentTarget as HTMLInputElement).value;
              setQuery(v);
              setActive(0);
              rememberQuery(v);
            }}
            onKeyDown={onKey}
          />
          <div class="qp-modes" role="group" aria-label="Narrow the search">
            {PREFIXES.map((x) => (
              <button
                key={x.prefix}
                type="button"
                tabIndex={-1}
                class={`qp-pill ${mode === x.mode ? 'on' : ''}`}
                title={`${x.title}: ${x.hint}`}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => {
                  setQuery(mode === x.mode ? '' : x.prefix);
                  setActive(0);
                  input.current?.focus();
                }}
              >
                {x.prefix}
              </button>
            ))}
          </div>
        </div>
        <div class="qp-list" id="qp-list" role="listbox" aria-label="Results" ref={list}>
          {groups.map((g) => (
            <div key={g.title || 'rows'} role="group" aria-label={g.title || 'Results'}>
              {g.title && (
                <div class="qp-group" role="presentation">
                  {g.title}
                </div>
              )}
              {g.rows.map((r) => {
                n++;
                const i = n;
                const hit = r.hit;
                const item = r.item;
                return (
                  <div
                    key={r.id}
                    id={`qp-opt-${i}`}
                    role="option"
                    aria-selected={i === at}
                    class={`qp-row ${i === at ? 'on' : ''}`}
                    onMouseMove={() => i !== at && setActive(i)}
                    onClick={r.run}
                  >
                    <span class={`qp-icon qpk-${r.kind}`}>{r.kind !== 'tip' && <KindIcon kind={r.kind} />}</span>
                    <span class="qp-main">
                      {item ? (
                        <>
                          <span class="qp-label">
                            <Marked text={item.label} at={hit?.marks.label} />
                          </span>
                          {item.sub && (
                            <span class="qp-sub">
                              <Marked text={item.sub} at={hit?.marks.sub} />
                            </span>
                          )}
                          <span class="qp-detail">
                            <Marked text={item.detail} at={hit?.marks.detail} />
                          </span>
                        </>
                      ) : (
                        <>
                          <span class={`qp-label ${r.kind === 'tip' ? 'mono' : ''}`}>{r.label}</span>
                          {r.detail && <span class="qp-detail">{r.detail}</span>}
                        </>
                      )}
                    </span>
                    {item?.keys && <Keys chord={item.keys} />}
                  </div>
                );
              })}
            </div>
          ))}
          {empty && (
            <div class="qp-empty" role="status">
              {mode === 'command' ? 'No command matches' : mode === 'net' ? 'No net matches' : mode === 'symbol' ? 'No module or definition matches' : 'No part, net, module, definition or file matches'}
              {text && <> “{text}”</>}
              {mode === 'all' && <div class="qp-empty-hint">Try a designator like c25, a value like 100n, or a module path. Type # for nets, @ for definitions.</div>}
            </div>
          )}
        </div>
        <div class="qp-foot" aria-hidden="true">
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> move
          </span>
          <span>
            <kbd>Enter</kbd> {mode === 'command' ? 'run' : 'go'}
          </span>
          <span>
            <kbd>Esc</kbd> close
          </span>
          <span class="spacer" />
          <span>{mode === 'line' ? '' : `${count} ${count === 1 ? 'result' : 'results'}`}</span>
        </div>
        <div class="sr-only" aria-live="polite">
          {count} {count === 1 ? 'result' : 'results'}
        </div>
      </div>
    </div>
  );
}

export function QuickPick() {
  const p = picker.value;
  if (!p) return null;
  return <Dialog key={p.nonce} initial={p.initial} />;
}
