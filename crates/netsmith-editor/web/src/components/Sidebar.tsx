import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';

import { ApiFailure, api } from '../client';
import { type Hover, hoverEnter, hoverLeave, partsOfDefinition } from '../hover';
import { allowed, folderFor, removalPrompt, starter, stemOf, type Kind, type Template } from '../fileops';
import { load, save } from '../storage';
import { computed } from '@preact/signals';
import { Icon } from './Icons';
import { EmptyState } from './ui';
import {
  activeFile,
  dirty,
  detail,
  dropOpen,
  goTo,
  openFile,
  openFiles,
  problems,
  project,
  refreshAfterChange,
  renameOpen,
  say,
  viewBoardOfDefinition,
} from '../store';
import {
  ancestorIds,
  buildTree,
  type DefNode,
  type FileNode,
  compactOverrides,
  countTitle,
  fileIcon,
  filterTree,
  flatten,
  highlight,
  keyMove,
  kindInfo,
  KINDS,
  parseOverrides,
  readonlyReason,
  revealScroll,
  SECTION_HELP,
  setAll,
  storageKey,
  type Overrides,
  type Row,
  type TreeKey,
} from '../tree';
import {
  Chevron,
  FileTypeIcon,
  FolderIcon,
  KindIcon,
  LockIcon,
} from './TreeIcons';
import { ConfirmRemove, ContextMenu, NameEntry, type Entry, type MenuItem } from './FileOps';

// What the rows are laid out by; style.css (the file tree block) has the same numbers.
const PAD = 8;
const STEP = 16;
const CHEVRON = 20;

/** The height of a section's header, which stays at the top of the list and hides a row under it. */
const STICKY = 32;

const WIDTH_KEY = 'netsmith.sidebar.width';
const WIDTH = { initial: 260, min: 200, max: 560 };

const KEYS: TreeKey[] = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'Enter', ' '];

/** What a click or Enter does on a file (open it) or a definition (go to it, and show its board). */
function activateNode(node: FileNode | DefNode) {
  if (node.type === 'file') {
    openFile(node.file.path).catch((e) => say(String(e), true));
    return;
  }
  // The tree knows the line of a definition and its name, not the column: the editor puts
  // the cursor on the name.
  void goTo({ file: node.file.path, line: node.def.line, column: 1, endLine: node.def.line, endColumn: 1 }, node.name);
  viewBoardOfDefinition(node.def.kind, node.name);
}

/** A name with what the filter matched marked. */
function Marked({ text, filter }: { text: string; filter: string }) {
  return (
    <>
      {highlight(text, filter).map((piece, i) => (piece.hit ? <mark key={i}>{piece.text}</mark> : piece.text))}
    </>
  );
}

/** The errors and warnings of each file, for the badge in its row. */
const fileProblems = computed(() => {
  const by = new Map<string, { error: number; warning: number }>();
  for (const p of problems.value) {
    if (!p.at || p.severity === 'note') continue;
    const e = by.get(p.at.file) ?? { error: 0, warning: 0 };
    e[p.severity]++;
    by.set(p.at.file, e);
  }
  return by;
});

function Pill({ n, title }: { n: number; title: string }) {
  return (
    <span class="ft-count" title={title} aria-label={title}>
      {n}
    </span>
  );
}

function TreeRow({
  row,
  filter,
  focused,
  onFocus,
  onToggle,
  onMenu,
}: {
  row: Row;
  filter: string;
  focused: boolean;
  onFocus: (id: string) => void;
  onToggle: (id: string, open: boolean) => void;
  onMenu: (e: MouseEvent, id: string) => void;
}) {
  const node = row.node;
  const active = node.type === 'file' && activeFile.value === node.file.path;
  const state = node.type === 'file' ? openFiles.value[node.file.path] : undefined;
  const isDirty = !!state && dirty(state);
  const guides = [];
  for (let p = 2; p < row.level; p++) {
    guides.push(<span key={p} class="ft-guide" style={{ left: `${PAD + (p - 2) * STEP + CHEVRON / 2}px` }} />);
  }
  const pad = row.level >= 2 ? PAD + (row.level - 2) * STEP : 10;
  const toggle = row.open === null ? undefined : () => onToggle(row.id, !row.open);

  let title = '';
  let icon;
  let label;
  let meta = null;
  let activate: () => void;
  switch (node.type) {
    case 'section':
      title = SECTION_HELP[node.role];
      label = node.label;
      meta = <Pill n={node.files} title={countTitle(node)} />;
      activate = () => toggle?.();
      break;
    case 'folder':
      title = node.segments.join('/');
      icon = <FolderIcon open={row.open === true} library={node.library} />;
      label = node.segments.map((s, i) => (
        <span key={i}>
          {i > 0 && <span class="ft-sep">/</span>}
          <Marked text={s} filter={filter} />
        </span>
      ));
      meta = <Pill n={node.files} title={countTitle(node)} />;
      activate = () => toggle?.();
      break;
    case 'file':
      title = node.file.path;
      icon = <FileTypeIcon icon={fileIcon(node.name)} />;
      label = <Marked text={node.name} filter={filter} />;
      {
        const bad = fileProblems.value.get(node.file.path);
        meta = (
          <>
            {bad && (
              <span
                class={`ft-problems ${bad.error ? 'error' : 'warning'}`}
                title={`${bad.error ? `${bad.error} error${bad.error === 1 ? '' : 's'}` : ''}${bad.error && bad.warning ? ', ' : ''}${bad.warning ? `${bad.warning} warning${bad.warning === 1 ? '' : 's'}` : ''} in this file`}
                aria-label={`${bad.error} errors, ${bad.warning} warnings`}
              >
                {bad.error + bad.warning}
              </span>
            )}
            {node.file.readonly && (
              <span class="ft-lock" title={readonlyReason(node.file)} aria-label={readonlyReason(node.file)}>
                <LockIcon />
              </span>
            )}
          </>
        );
      }
      activate = () => activateNode(node);
      break;
    case 'def': {
      const info = kindInfo(node.def.kind);
      title = `${info.label} ${node.name}, line ${node.def.line}: ${info.help}`;
      icon = <KindIcon kind={node.def.kind} />;
      label = <Marked text={node.name} filter={filter} />;
      meta = <span class="ft-kind">{info.label.toLowerCase()}</span>;
      activate = () => activateNode(node);
      break;
    }
  }

  // A definition lights the parts it declares (in the schematic and on the board) while the pointer or focus is on it.
  const defHover = (): Hover | null => {
    if (node.type !== 'def') return null;
    const defs = [...node.file.defs].sort((a, b) => a.line - b.line);
    const next = defs.find((d) => d.line > node.def.line)?.line ?? null;
    const refs = partsOfDefinition(detail.peek(), node.file.path, node.def.line, next);
    return refs.length ? { kind: 'module', key: `def:${node.file.path}:${node.name}`, refs } : null;
  };
  const isOpenFile = node.type === 'file' && !!state;
  const cls = ['ft-row', `ft-${node.type}`, active ? 'active' : '', isOpenFile && !active ? 'opened' : ''].filter(Boolean).join(' ');
  return (
    <div
      role="treeitem"
      aria-level={row.level}
      aria-expanded={row.open === null ? undefined : row.open}
      aria-selected={node.type === 'file' ? active : undefined}
      data-id={row.id}
      data-path={node.type === 'file' ? node.file.path : undefined}
      tabIndex={focused ? 0 : -1}
      class={cls}
      style={`--pad:${pad}px`}
      title={title}
      onFocus={() => {
        onFocus(row.id);
        const h = defHover();
        if (h) hoverEnter(h);
      }}
      onBlur={() => {
        const h = defHover();
        if (h) hoverLeave(h);
      }}
      onPointerEnter={(e) => {
        const h = e.pointerType !== 'touch' && defHover();
        if (h) hoverEnter(h);
      }}
      onPointerLeave={(e) => {
        const h = e.pointerType !== 'touch' && defHover();
        if (h) hoverLeave(h);
      }}
      onContextMenu={(e) => onMenu(e, row.id)}
      onClick={() => {
        onFocus(row.id);
        activate();
      }}
    >
      {guides}
      {row.level >= 2 && (
        <span
          class={`ft-twisty ${row.open === null ? 'none' : ''}`}
          onClick={
            node.type === 'file' && toggle
              ? (e) => {
                  e.stopPropagation();
                  onFocus(row.id);
                  toggle();
                }
              : undefined
          }
        >
          {row.open !== null && <Chevron open={row.open} />}
        </span>
      )}
      {node.type === 'section' && <Chevron open={row.open === true} />}
      {icon}
      <span class="ft-label">{label}</span>
      {isDirty && <span class="ft-dot" title="unsaved changes" aria-label="unsaved changes" />}
      <span class="ft-spacer" />
      {meta}
    </div>
  );
}

function Legend({ onClose }: { onClose: () => void }) {
  return (
    <div class="ft-legend" role="dialog" aria-label="What the icons mean" onKeyDown={(e) => e.key === 'Escape' && onClose()}>
      <div class="ft-legend-head">
        <span>What the icons mean</span>
        <button type="button" class="ft-btn" onClick={onClose} aria-label="Close the legend">
          Close
        </button>
      </div>
      <div class="ft-legend-group">Definitions</div>
      {KINDS.map((k) => (
        <div class="ft-legend-row" key={k.kind}>
          <KindIcon kind={k.kind} />
          <span>
            <b>{k.label}</b>: {k.help}
          </span>
        </div>
      ))}
      <div class="ft-legend-group">Files</div>
      {(
        [
          ['nsm.nsm', 'netsmith source'],
          ['x.physical.nsm', 'physical file: the case, the outline, where parts go'],
          ['netsmith.toml', 'the project manifest'],
          ['parts.csv', 'a table'],
          ['x.kicad_pcb', 'a KiCad file'],
        ] as const
      ).map(([name, what]) => (
        <div class="ft-legend-row" key={name}>
          <FileTypeIcon icon={fileIcon(name)} />
          <span>{what}</span>
        </div>
      ))}
      <div class="ft-legend-row">
        <FolderIcon open={false} library />
        <span>A folder of parts only: a library, closed until you open it</span>
      </div>
      <div class="ft-legend-row">
        <span class="ft-lock">
          <LockIcon />
        </span>
        <span>Read-only: edit it outside the editor</span>
      </div>
      <div class="ft-legend-row">
        <span class="ft-dot" />
        <span>Unsaved changes</span>
      </div>
    </div>
  );
}

/** The width of the column, which the person can drag, and what it was last time. */
function useWidth() {
  const [width, setWidth] = useState(() => {
    const w = load<number>(WIDTH_KEY, WIDTH.initial);
    return typeof w === 'number' ? Math.min(Math.max(w, WIDTH.min), WIDTH.max) : WIDTH.initial;
  });
  const set = (w: number, keep = true) => {
    const next = Math.min(Math.max(Math.round(w), WIDTH.min), WIDTH.max);
    setWidth(next);
    if (keep) save(WIDTH_KEY, next);
    return next;
  };
  return [width, set] as const;
}

function Resizer({ width, set }: { width: number; set: (w: number, keep?: boolean) => number }) {
  const drag = useRef<{ x: number; width: number } | null>(null);
  return (
    <div
      class="ft-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the file tree"
      aria-valuenow={width}
      aria-valuemin={WIDTH.min}
      aria-valuemax={WIDTH.max}
      tabIndex={0}
      title="Drag to resize; double-click to reset"
      onPointerDown={(e) => {
        drag.current = { x: e.clientX, width };
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        if (drag.current) set(drag.current.width + e.clientX - drag.current.x, false);
      }}
      onPointerUp={(e) => {
        if (drag.current) set(drag.current.width + e.clientX - drag.current.x);
        drag.current = null;
      }}
      onDblClick={() => set(WIDTH.initial)}
      onKeyDown={(e) => {
        if (e.key === 'ArrowLeft') set(width - 16);
        else if (e.key === 'ArrowRight') set(width + 16);
        else return;
        e.preventDefault();
      }}
    />
  );
}

/** The level and the padding of a row, and the guide lines at its left, for a row that is not
 * one of the tree's (the name being typed). */
function place(level: number) {
  const guides = [];
  for (let p = 2; p < level; p++) {
    guides.push(<span key={p} class="ft-guide" style={{ left: `${PAD + (p - 2) * STEP + CHEVRON / 2}px` }} />);
  }
  return { guides, pad: level >= 2 ? PAD + (level - 2) * STEP : 10 };
}

const dirOf = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf('/')));
const baseOfPath = (path: string) => path.slice(path.lastIndexOf('/') + 1);

type Typing = { kind: Kind; dir: string } | { kind: Kind; dir: string; rename: string; id: string };

export function Sidebar() {
  const p = project.value;
  const [filter, setFilter] = useState('');
  const [width, setWidth] = useWidth();
  const [legend, setLegend] = useState(false);
  const [focusId, setFocusId] = useState<string | null>(null);
  const [typing, setTyping] = useState<Typing | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: string | null } | null>(null);
  const [removing, setRemoving] = useState<{ id: string; error: string | null; busy: boolean } | null>(null);
  const treeRef = useRef<HTMLDivElement>(null);
  // Where the list is scrolled, kept by the page: nothing that changes the rows moves it.
  const scrolled = useRef(0);
  const revealed = useRef<string | null>(null);
  // Where the list was before a filter was typed, which it goes back to when the filter is cleared.
  const beforeFilter = useRef(0);
  const key = storageKey(p?.title ?? '');
  // What the project had open last time; it is read again when another project is loaded.
  const [saved, setSaved] = useState<{ key: string; map: Overrides }>(() => ({ key, map: parseOverrides(load(key, {})) }));
  const overrides = saved.key === key ? saved.map : parseOverrides(load(key, {}));

  const sections = useMemo(() => buildTree(p?.files ?? [], p?.folders ?? []), [p?.files, p?.folders]);
  const shown = useMemo(() => filterTree(sections, filter), [sections, filter]);
  const rows = useMemo(() => flatten(shown, overrides, filter), [shown, overrides, filter]);
  const filtering = filter.trim() !== '';
  const setFilterKeeping = (next: string) => {
    if (filter === '' && next !== '') beforeFilter.current = scrolled.current;
    else if (filter !== '' && next === '') scrolled.current = beforeFilter.current;
    setFilter(next);
  };

  const remember = (next: Overrides) => {
    setSaved({ key, map: next });
    save(key, compactOverrides(sections, next));
  };
  const toggle = (id: string, open: boolean) => remember({ ...overrides, [id]: open });

  // After every draw the list is where the person left it, whatever the draw did to it.
  useLayoutEffect(() => {
    const el = treeRef.current;
    if (el && Math.abs(el.scrollTop - scrolled.current) > 1) el.scrollTop = scrolled.current;
  });

  // The file that is on show is in the tree: its folders open, and the row in view if it is not.
  const path = activeFile.value;
  useEffect(() => {
    if (!path || filtering) return;
    const closed = new Set(flatten(sections, overrides).filter((r) => r.open === false).map((r) => r.id));
    const need = ancestorIds(sections, path).filter((id) => closed.has(id));
    if (need.length) remember({ ...overrides, ...Object.fromEntries(need.map((id) => [id, true])) });
  }, [path, sections]);
  useEffect(() => {
    const tree = treeRef.current;
    if (!tree || !path || revealed.current === path) return;
    const el = [...tree.querySelectorAll<HTMLElement>('[data-path]')].find((e) => e.dataset.path === path);
    if (!el) return;
    // Once for each file that becomes the active one, and only if it is not in view.
    revealed.current = path;
    const top = el.getBoundingClientRect().top - tree.getBoundingClientRect().top + tree.scrollTop;
    const to = revealScroll(top, el.offsetHeight, tree.scrollTop, tree.clientHeight, STICKY);
    if (to !== tree.scrollTop) {
      tree.scrollTop = to;
      scrolled.current = tree.scrollTop;
    }
  }, [path, rows]);

  const canEdit = !!p?.hasManifest;
  const noProject = 'File operations need a project: there is no netsmith.toml here.';
  const focus = rows.some((r) => r.id === focusId) ? focusId : (rows[0]?.id ?? null);
  const nodeOf = (id: string | null) => rows.find((r) => r.id === id)?.node ?? null;

  /** Puts the focus on a row, once it is drawn. */
  const move = (id: string) => {
    setFocusId(id);
    let tries = 0;
    const find = () => {
      const el = [...(treeRef.current?.querySelectorAll<HTMLElement>('[role=treeitem]') ?? [])].find((e) => e.dataset.id === id);
      if (el) el.focus({ preventScroll: false });
      else if (tries++ < 20) requestAnimationFrame(find);
    };
    requestAnimationFrame(find);
  };

  // ---- file operations ----

  const opened = (dir: string) => {
    // The folders on the way, and the section, are open, so the row for the name is seen.
    const rel = dir === 'src' ? [] : dir.slice(4).split('/');
    const ids = ['s:module', ...rel.map((_, i) => `d:module:${rel.slice(0, i + 1).join('/')}`)];
    remember({ ...overrides, ...Object.fromEntries(ids.map((id) => [id, true])) });
  };

  const startNew = (kind: Kind, dirIn?: string) => {
    if (!canEdit) return say(noProject, true);
    const dir = dirIn ?? folderFor(nodeOf(focus));
    setMenu(null);
    setFilterKeeping('');
    opened(dir);
    setTyping({ kind, dir });
  };

  const startRename = (id: string | null) => {
    const node = nodeOf(id);
    if (!node || node.type === 'section') return say('Select a file or folder to rename.', true);
    if (!canEdit) return say(noProject, true);
    const why = allowed(node).rename;
    if (why) return say(why, true);
    setMenu(null);
    if (node.type === 'folder') setTyping({ kind: 'folder', dir: dirOf(node.dir), rename: baseOfPath(node.dir), id: node.id });
    else setTyping({ kind: 'file', dir: dirOf(node.file.path), rename: baseOfPath(node.file.path), id: id! });
  };

  const askRemove = (id: string | null) => {
    const node = nodeOf(id);
    if (!node || node.type === 'section') return say('Select a file or folder to delete.', true);
    if (!canEdit) return say(noProject, true);
    const why = allowed(node).remove;
    if (why) return say(why, true);
    setMenu(null);
    setRemoving({ id: id!, error: null, busy: false });
  };

  /** A message for a refusal, to show under the name or in the question. */
  const said = (e: unknown) => (e instanceof ApiFailure ? e.message : e instanceof Error ? e.message : String(e));

  const entryFor = (t: Typing): Entry => {
    const files = p?.files ?? [];
    const here = files.filter((f) => dirOf(f.path) === t.dir);
    const siblings = [...here.map((f) => baseOfPath(f.path)), ...(p?.folders ?? []).filter((d) => dirOf(d) === t.dir).map(baseOfPath)];
    const boards: Record<string, string | undefined> = {};
    for (const f of here) boards[baseOfPath(f.path)] = f.defs.find((d) => d.kind === 'board')?.name;
    return { dir: t.dir, kind: t.kind, renaming: 'rename' in t ? t.rename : undefined, siblings, boards };
  };

  const finishNew = async (t: Typing, name: string, template: Template | null): Promise<string | null> => {
    const target = `${t.dir}/${name}`;
    try {
      if ('rename' in t) {
        await api.renamePath(`${t.dir}/${t.rename}`, target);
        renameOpen(`${t.dir}/${t.rename}`, target, t.kind === 'folder');
      } else if (t.kind === 'folder') {
        await api.createFolder(target);
      } else {
        const stem = stemOf(name);
        const board = entryFor(t).boards[`${stem}.nsm`];
        await api.createFile(target, starter(template ?? 'module', stem, board));
      }
    } catch (e) {
      return said(e);
    }
    setTyping(null);
    try {
      await refreshAfterChange();
      if (t.kind === 'file' && !('rename' in t)) await openFile(target);
    } catch (e) {
      say(said(e), true);
    }
    if (t.kind === 'folder') {
      opened(dirOf(target));
      move(`d:module:${target.slice(4)}`);
    } else move(`f:${target}`);
    say(`${'rename' in t ? 'Renamed to' : 'Made'} ${target}`);
    return null;
  };

  const doRemove = async () => {
    const node = nodeOf(removing?.id ?? null);
    if (!removing || !node || node.type === 'section') return;
    const folder = node.type === 'folder';
    const target = folder ? node.dir : node.file.path;
    setRemoving({ ...removing, busy: true, error: null });
    try {
      await api.deletePath(target, true);
    } catch (e) {
      setRemoving({ ...removing, busy: false, error: said(e) });
      return;
    }
    dropOpen(target, folder);
    setRemoving(null);
    const near = rows[Math.max(0, rows.findIndex((r) => r.id === removing.id) - 1)]?.id;
    try {
      await refreshAfterChange();
    } catch (e) {
      say(said(e), true);
    }
    if (near) move(near);
    say(`Deleted ${target}`);
  };

  const menuItems = (id: string | null): MenuItem[] => {
    const node = nodeOf(id);
    const can = allowed(node);
    const off = canEdit ? null : noProject;
    return [
      { id: 'new-file', label: 'New file here', keys: 'Ctrl+N', disabled: off },
      { id: 'new-folder', label: 'New folder here', disabled: off },
      { id: 'rename', label: 'Rename', keys: 'F2', disabled: off ?? can.rename },
      { id: 'delete', label: 'Delete', keys: 'Del', disabled: off ?? can.remove },
    ];
  };

  const pick = (action: string, id: string | null) => {
    setMenu(null);
    if (action === 'new-file') startNew('file', folderFor(nodeOf(id)));
    else if (action === 'new-folder') startNew('folder', folderFor(nodeOf(id)));
    else if (action === 'rename') startRename(id);
    else if (action === 'delete') askRemove(id);
  };

  const onMenu = (e: MouseEvent, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    setFocusId(id);
    setMenu({ x: e.clientX, y: e.clientY, id });
  };

  if (!p) return <aside class="sidebar" />;

  const onKeyDown = (e: KeyboardEvent) => {
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === 'F2' && !mod) {
      e.preventDefault();
      return startRename(focus);
    }
    if (e.key === 'Delete' || (e.metaKey && e.key === 'Backspace')) {
      e.preventDefault();
      return askRemove(focus);
    }
    if (mod && !e.altKey && e.key.toLowerCase() === 'n') {
      e.preventDefault();
      return startNew(e.shiftKey ? 'folder' : 'file');
    }
    if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      e.preventDefault();
      const el = [...(treeRef.current?.querySelectorAll<HTMLElement>('[role=treeitem]') ?? [])].find((r) => r.dataset.id === focus);
      const box = el?.getBoundingClientRect();
      return setMenu({ x: (box?.left ?? 0) + 40, y: (box?.bottom ?? 0) - 4, id: focus });
    }
    if (mod || e.altKey || !(KEYS as string[]).includes(e.key)) return;
    const result = keyMove(rows, focus, e.key as TreeKey);
    e.preventDefault();
    e.stopPropagation();
    if (!result) return;
    if ('focus' in result) move(result.focus);
    else if ('toggle' in result) {
      if (!filtering) toggle(result.toggle, result.open);
    } else {
      const node = rows.find((r) => r.id === result.activate)?.node;
      if (node && (node.type === 'file' || node.type === 'def')) activateNode(node);
    }
  };

  // The rows on screen, with the name being typed in its place: a new one goes first below the
  // folder it is in, a renamed one takes the place of its row.
  const entry = typing ? entryFor(typing) : null;
  const items: preact.ComponentChildren[] = [];
  const typedRow = (level: number) => {
    const { guides, pad } = place(level);
    const t = typing!;
    return (
      <NameEntry
        key={`entry:${'rename' in t ? t.id : `${t.dir}:${t.kind}`}`}
        entry={entry!}
        level={level}
        pad={pad}
        guides={guides}
        onDone={(name, template) => finishNew(t, name, template)}
        onCancel={() => {
          setTyping(null);
          if ('rename' in t) move(t.id);
        }}
      />
    );
  };
  const parent =
    typing && !('rename' in typing)
      ? typing.dir === 'src'
        ? rows.findIndex((r) => r.id === 's:module')
        : rows.findIndex((r) => r.node.type === 'folder' && r.node.dir === typing.dir)
      : -1;
  if (typing && !('rename' in typing) && parent < 0) items.push(typedRow(2));
  rows.forEach((row, i) => {
    if (typing && 'rename' in typing && typing.id === row.id) items.push(typedRow(row.level));
    else {
      items.push(<TreeRow key={row.id} row={row} filter={filter} focused={row.id === focus} onFocus={setFocusId} onToggle={toggle} onMenu={onMenu} />);
    }
    if (i === parent) items.push(typedRow(row.level + 1));
  });

  const removingNode = removing ? nodeOf(removing.id) : null;
  const prompt =
    removingNode && removingNode.type !== 'section'
      ? removalPrompt(
          removingNode,
          Object.values(openFiles.value)
            .filter((f) => dirty(f) && (removingNode.type === 'folder' ? f.path.startsWith(`${removingNode.dir}/`) : f.path === removingNode.file.path))
            .map((f) => f.path),
        )
      : null;

  return (
    <aside class="sidebar" style={{ width: `${width}px` }}>
      <div class="sb-head">
        <span class="sb-title" title={p.title}>
          Files
          <span class="sb-project">{p.title}</span>
        </span>
        <span class="sb-actions" role="toolbar" aria-label="Files">
          <button type="button" class="icon-btn" data-tip={canEdit ? 'New file' : noProject} data-tip-keys={canEdit ? 'Ctrl+N' : undefined} aria-label="New file" disabled={!canEdit} onClick={() => startNew('file')}>
            <Icon name="file-plus" size={15} />
          </button>
          <button type="button" class="icon-btn" data-tip={canEdit ? 'New folder' : noProject} aria-label="New folder" disabled={!canEdit} onClick={() => startNew('folder')}>
            <Icon name="folder-plus" size={15} />
          </button>
          <span class="sb-sep" aria-hidden="true" />
          <button type="button" class="icon-btn" data-tip="Expand all folders" aria-label="Expand all" onClick={() => remember(setAll(sections, true, overrides))}>
            <Icon name="expand-all" size={15} />
          </button>
          <button type="button" class="icon-btn" data-tip="Collapse all folders" aria-label="Collapse all" onClick={() => remember(setAll(sections, false, overrides))}>
            <Icon name="collapse-all" size={15} />
          </button>
          <button
            type="button"
            class={`icon-btn ${legend ? 'on' : ''}`}
            data-tip="What the icons mean"
            aria-label="What the icons mean"
            aria-expanded={legend}
            onClick={() => setLegend(!legend)}
          >
            <Icon name="help" size={15} />
          </button>
        </span>
      </div>
      <div class="ft-tools">
        <label class="field-icon">
          <Icon name="search" size={13} />
          <input
            class="tree-filter"
            type="search"
            placeholder="Filter files and definitions"
            aria-label="Filter files and definitions"
            value={filter}
            onInput={(e) => setFilterKeeping((e.target as HTMLInputElement).value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setFilterKeeping('');
              else if (e.key === 'ArrowDown' && rows.length) {
                e.preventDefault();
                move(focus ?? rows[0].id);
              }
            }}
          />
        </label>
      </div>
      <div class="ft-wrap">
        {legend && <Legend onClose={() => setLegend(false)} />}
        <div
          ref={treeRef}
          class="ft-tree"
          role="tree"
          aria-label="Project files"
          onKeyDown={onKeyDown}
          onScroll={(e) => {
            scrolled.current = (e.currentTarget as HTMLElement).scrollTop;
          }}
          onContextMenu={(e) => {
            // Beside the rows: the menu of the project's src folder.
            e.preventDefault();
            setMenu({ x: e.clientX, y: e.clientY, id: null });
          }}
        >
          {items}
          {!rows.length && !typing && !project.value && (
            <div class="ft-skeleton" aria-hidden="true" data-testid="tree-skeleton">
              {[60, 40, 52, 34, 46].map((w, i) => (
                <div key={i} class="sk sk-line" style={{ width: `${w}%` }} />
              ))}
            </div>
          )}
          {!rows.length && !typing && project.value && (
            <EmptyState
              compact
              class="ft-empty"
              icon={filtering ? 'search' : 'folder'}
              title={filtering ? 'No file or definition matches' : 'No files yet'}
              help={filtering ? `Nothing in the project fits “${filter.trim()}”.` : 'Add a source file to the project; the tree lists it and what it defines.'}
            />
          )}
        </div>
      </div>
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.id)} onPick={(a) => pick(a, menu.id)} onClose={() => setMenu(null)} />}
      {removing && prompt && (
        <ConfirmRemove
          title={prompt.title}
          lines={prompt.lines}
          strong={prompt.strong}
          busy={removing.busy}
          error={removing.error}
          onYes={() => void doRemove()}
          onNo={() => {
            const id = removing.id;
            setRemoving(null);
            move(id);
          }}
        />
      )}
      <Resizer width={width} set={setWidth} />
    </aside>
  );
}
