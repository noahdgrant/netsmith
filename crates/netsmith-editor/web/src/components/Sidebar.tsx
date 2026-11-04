import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { load, save } from '../storage';
import {
  activeFile,
  dirty,
  goTo,
  openFile,
  openFiles,
  project,
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
  SECTION_HELP,
  setAll,
  storageKey,
  type Overrides,
  type Row,
  type TreeKey,
} from '../tree';
import {
  Chevron,
  CollapseIcon,
  ExpandIcon,
  FileTypeIcon,
  FolderIcon,
  HelpIcon,
  KindIcon,
  LockIcon,
} from './TreeIcons';

// What the rows are laid out by; style.css (the file tree block) has the same numbers.
const PAD = 8;
const STEP = 16;
const CHEVRON = 20;

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
}: {
  row: Row;
  filter: string;
  focused: boolean;
  onFocus: (id: string) => void;
  onToggle: (id: string, open: boolean) => void;
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
      meta = node.file.readonly ? (
        <span class="ft-lock" title={readonlyReason(node.file)} aria-label={readonlyReason(node.file)}>
          <LockIcon />
        </span>
      ) : null;
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
      onFocus={() => onFocus(row.id)}
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

export function Sidebar() {
  const p = project.value;
  const [filter, setFilter] = useState('');
  const [width, setWidth] = useWidth();
  const [legend, setLegend] = useState(false);
  const [focusId, setFocusId] = useState<string | null>(null);
  const treeRef = useRef<HTMLDivElement>(null);
  const key = storageKey(p?.title ?? '');
  // What the project had open last time; it is read again when another project is loaded.
  const [saved, setSaved] = useState<{ key: string; map: Overrides }>(() => ({ key, map: parseOverrides(load(key, {})) }));
  const overrides = saved.key === key ? saved.map : parseOverrides(load(key, {}));

  const sections = useMemo(() => buildTree(p?.files ?? []), [p?.files]);
  const shown = useMemo(() => filterTree(sections, filter), [sections, filter]);
  const rows = useMemo(() => flatten(shown, overrides, filter), [shown, overrides, filter]);
  const filtering = filter.trim() !== '';

  const remember = (next: Overrides) => {
    setSaved({ key, map: next });
    save(key, compactOverrides(sections, next));
  };
  const toggle = (id: string, open: boolean) => remember({ ...overrides, [id]: open });

  // The file that is on show is in the tree: its folders open, and the row in view.
  const path = activeFile.value;
  useEffect(() => {
    if (!path || filtering) return;
    const closed = new Set(flatten(sections, overrides).filter((r) => r.open === false).map((r) => r.id));
    const need = ancestorIds(sections, path).filter((id) => closed.has(id));
    if (need.length) remember({ ...overrides, ...Object.fromEntries(need.map((id) => [id, true])) });
  }, [path, sections]);
  useEffect(() => {
    const el = [...(treeRef.current?.querySelectorAll<HTMLElement>('[data-path]') ?? [])].find((e) => e.dataset.path === path);
    el?.scrollIntoView({ block: 'nearest' });
  }, [path, rows.length]);

  if (!p) return <aside class="sidebar" />;

  const focus = rows.some((r) => r.id === focusId) ? focusId : (rows[0]?.id ?? null);
  const move = (id: string) => {
    setFocusId(id);
    requestAnimationFrame(() => {
      [...(treeRef.current?.querySelectorAll<HTMLElement>('[role=treeitem]') ?? [])].find((e) => e.dataset.id === id)?.focus();
    });
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey || !(KEYS as string[]).includes(e.key)) return;
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

  return (
    <aside class="sidebar" style={{ width: `${width}px` }}>
      <div class="project-name" title={p.title}>
        <span class="cube" />
        {p.title}
      </div>
      <div class="ft-tools">
        <input
          class="tree-filter"
          type="search"
          placeholder="Filter files and definitions"
          aria-label="Filter files and definitions"
          value={filter}
          onInput={(e) => setFilter((e.target as HTMLInputElement).value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') setFilter('');
            else if (e.key === 'ArrowDown' && rows.length) {
              e.preventDefault();
              move(focus ?? rows[0].id);
            }
          }}
        />
        <div class="ft-actions">
          <button type="button" class="ft-btn" title="Open every folder" aria-label="Expand all" onClick={() => remember(setAll(sections, true, overrides))}>
            <ExpandIcon />
            Expand all
          </button>
          <button type="button" class="ft-btn" title="Close every folder" aria-label="Collapse all" onClick={() => remember(setAll(sections, false, overrides))}>
            <CollapseIcon />
            Collapse all
          </button>
          <span class="ft-spacer" />
          <button
            type="button"
            class={`ft-btn icon ${legend ? 'on' : ''}`}
            title="What the icons mean"
            aria-label="What the icons mean"
            aria-expanded={legend}
            onClick={() => setLegend(!legend)}
          >
            <HelpIcon />
          </button>
        </div>
      </div>
      <div class="ft-wrap">
        {legend && <Legend onClose={() => setLegend(false)} />}
        <div ref={treeRef} class="ft-tree" role="tree" aria-label="Project files" onKeyDown={onKeyDown}>
          {rows.map((row) => (
            <TreeRow key={row.id} row={row} filter={filter} focused={row.id === focus} onFocus={setFocusId} onToggle={toggle} />
          ))}
          {!rows.length && <div class="ft-empty">{filtering ? `Nothing matches “${filter.trim()}”.` : 'No files yet.'}</div>}
        </div>
      </div>
      <Resizer width={width} set={setWidth} />
    </aside>
  );
}
