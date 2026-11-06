// Go to anything (Ctrl+P) and the command palette (Ctrl+Shift+P, Ctrl+K): one input over a list.
// What is typed picks the rows: nothing special searches parts, nets, modules, definitions and
// files; `>` runs a command, `@` goes to a module or definition, `#` to a net, `:` to a line,
// `!` to a problem.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';

import { available, byRecent, chordLabels, runCommand } from '../commands';
import { goToBusPick, goToSelection } from '../commandset';
import { drawnBuses } from '../views/schematic-selection';
import { allRows, revealRow } from '../feedback/derive';
import { sortRows } from '../feedback/problems';
import { pieces } from '../fuzzy';
import { type Hit, type Item, PREFIXES, buildIndex, parseLine, parseQuery, problemItems, recentItems, search } from '../quickopen';
import { closePicker, picker, recentCommands, recentPicks, rememberQuery, usedCommand, usedPick } from '../quickpick';
import { Icon, type IconName, SeverityIcon } from './Icons';
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

/** What a row is, in a word, for the label at its right. */
const KIND_LABEL: Record<string, string> = {
  command: 'Command',
  view: 'View',
  part: 'Part',
  net: 'Net',
  bus: 'Bus',
  module: 'Module',
  definition: 'Definition',
  file: 'File',
  problem: 'Problem',
  line: 'Line',
};

/** The icon of a command: its own where it has one, else its group's. A view is the section's icon. */
function commandIcon(id: string): IconName {
  const exact: Record<string, IconName> = {
    'schematic.sheet.previous': 'chevron-left',
    'schematic.sheet.next': 'chevron-right',
    'schematic.pin.next': 'chevron-right',
    'schematic.pin.previous': 'chevron-left',
    'schematic.source': 'source',
    'schematic.fit': 'expand-all',
    'schematic.fitwidth': 'expand-all',
    'schematic.actual': 'search',
    'schematic.zoomselection': 'search',
    'schematic.zoomin': 'plus',
    'schematic.zoomout': 'minus',
    'nav.back': 'arrow-left',
    'goto.anything': 'search',
    'palette.open': 'command',
    'goto.symbol': 'definition',
    'goto.net': 'net',
    'goto.problem': 'warning',
    'goto.line': 'hash',
    'selection.source': 'source',
    'selection.clear': 'close',
    'file.save': 'download',
    'file.close': 'close',
    'build.kicad': 'kicad',
    'build.fab': 'fab',
    'pcb.route': 'route',
    'pcb.cancel': 'close',
    'theme.toggle': 'theme',
    'help.shortcuts': 'keyboard',
    'pane.toggle': 'split',
  };
  if (exact[id]) return exact[id];
  if (id.startsWith('view.')) {
    const section = id.slice(5);
    return (['source', 'schematic', 'power', 'system', 'bom', 'pcb', 'build'].includes(section) ? section : 'panel') as IconName;
  }
  if (id.startsWith('problems.')) return 'warning';
  if (id.startsWith('theme.')) return 'theme';
  if (id.startsWith('schematic.')) return 'schematic';
  return 'command';
}

/** The kind a row is shown as: a command that opens a section is a view. */
function shownKind(kind: string, item?: Item): string {
  return kind === 'command' && item?.target.type === 'command' && item.target.id.startsWith('view.') ? 'view' : kind;
}

/** One distinct icon for each kind of row, in the colour the badges of the tree use for that kind. */
function RowIcon({ kind, item }: { kind: Item['kind'] | 'line' | 'tip' | 'search'; item?: Item }) {
  if (kind === 'command' && item?.target.type === 'command') return <Icon name={commandIcon(item.target.id)} />;
  switch (kind) {
    case 'search':
    case 'part':
    case 'bus':
    case 'net':
    case 'module':
    case 'file':
    case 'command':
    case 'definition':
      return <Icon name={kind} />;
    case 'line':
      return <Icon name="hash" />;
    case 'problem':
      return <Icon name="warning" />;
    default:
      return <Icon name="note" />;
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
  if (t.type === 'problem') {
    const row = allRows.peek().find((r) => r.id === t.id);
    if (row) setTimeout(() => revealRow(row), 0);
    return;
  }
  usedPick(item.id);
  if (t.type === 'selection') goToSelection(t.sel);
  else if (t.type === 'bus') {
    const bus = drawnBuses.peek().buses.find((b) => b.path === t.path);
    if (bus) goToBusPick(bus);
  }
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
  const drawn = drawnBuses.value;
  // Only the buses of the board in front: the drawing of the board before it is still there for a moment.
  const index = useMemo(() => buildIndex(d, p, drawn.board === d?.name ? drawn.buses : []), [d, p, drawn]);
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
    const found = search(index, commandItems(), query, mode === 'problem' ? problemItems(sortRows(allRows.value)) : []);
    for (const g of found) out.push({ title: g.title, rows: g.hits.map(wrap) });
    return out;
  }, [index, query, mode, text, lines, activeFile.value, recentPicks.value, recentCommands.value, available().length, allRows.value]);

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
      : mode === 'problem'
        ? 'Go to a problem'
      : mode === 'symbol'
        ? 'Go to a module or definition'
        : mode === 'net'
          ? 'Go to a net'
          : mode === 'line'
            ? 'Go to a line of the open file'
            : 'Go to a part, net, module, definition or file';
  const count = rows.length;
  const empty = rows.length === 0 && (text !== '' || mode === 'net' || mode === 'symbol' || mode === 'command' || mode === 'problem');
  let n = -1;
  return (
    <div class="qp-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closePicker()}>
      <div class="qp" role="dialog" aria-modal="true" aria-label={title} data-mode={mode}>
        <div class="qp-field">
          <span class="qp-mode" aria-hidden="true">
            <RowIcon kind={mode === 'command' ? 'command' : mode === 'problem' ? 'problem' : mode === 'net' ? 'net' : mode === 'symbol' ? 'module' : mode === 'line' ? 'line' : 'search'} />
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
                  <span class="qp-group-count">{g.rows.length}</span>
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
                    class={`qp-row k-${shownKind(r.kind, item)} ${i === at ? 'on' : ''}`}
                    onMouseMove={() => i !== at && setActive(i)}
                    onClick={r.run}
                  >
                    <span class={`qp-icon qpk-${shownKind(r.kind, item)} ${item?.tone ?? ''}`}>
                      {r.kind === 'tip' ? null : item?.tone ? <SeverityIcon severity={item.tone} size={16} /> : <RowIcon kind={r.kind} item={item} />}
                    </span>
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
                    {r.kind !== 'tip' && <span class="qp-type">{item?.kind === 'command' ? (item.detail || KIND_LABEL.command) : KIND_LABEL[shownKind(r.kind, item)]}</span>}
                  </div>
                );
              })}
            </div>
          ))}
          {empty && (
            <div class="qp-empty" role="status">
              {mode === 'command' ? 'No command matches' : mode === 'problem' ? (text ? 'No problem matches' : 'No problems') : mode === 'net' ? 'No net matches' : mode === 'symbol' ? 'No module or definition matches' : 'No part, net, module, definition or file matches'}
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
