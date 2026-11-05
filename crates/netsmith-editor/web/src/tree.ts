// The project tree: files grouped into sections (by role) and nested folders, the definitions
// of each file under it, what is open, the filter, and the keyboard. All of it is plain data
// and functions; the component only draws the rows.

import type { Def, FileRole, TreeFile } from './api';

// ------------------------------------------------------------------ the nodes

export interface SectionNode {
  type: 'section';
  id: string;
  role: FileRole;
  label: string;
  /** Files below it, at any depth. */
  files: number;
  children: Array<FolderNode | FileNode>;
}

export interface FolderNode {
  type: 'folder';
  id: string;
  /** The folders of a chain of single folders, `['legacy', 'vendor']`: one row with all of them. */
  segments: string[];
  /** Files below it, at any depth. */
  files: number;
  /** Every definition in it is a part: a library, not part of the design's structure. */
  library: boolean;
  /** Its path from the project root: `src/sheets`. The tree is shown below `src/`. */
  dir: string;
  children: Array<FolderNode | FileNode>;
}

export interface FileNode {
  type: 'file';
  id: string;
  name: string;
  file: TreeFile;
  children: DefNode[];
}

export interface DefNode {
  type: 'def';
  id: string;
  name: string;
  def: Def;
  file: TreeFile;
}

export type Node = SectionNode | FolderNode | FileNode | DefNode;

export const SECTION_ORDER: FileRole[] = ['top_level', 'module', 'physical', 'manifest', 'std'];

export const SECTION_LABEL: Record<FileRole, string> = {
  top_level: 'Top level',
  module: 'Modules',
  physical: 'Physical files',
  manifest: 'Project',
  std: 'Standard library',
};

/** What a section holds, for its tooltip. */
export const SECTION_HELP: Record<FileRole, string> = {
  top_level: 'The files the project builds: the boards, systems and panels in them are what the views show',
  module: 'The files under src/ that the top-level files import',
  physical: 'The physical file beside a source: the case, the outline, where parts go',
  manifest: 'The project manifest',
  std: 'The standard library that ships with netsmith (read-only)',
};

/** The kinds of definition the language has, with what to say about each. */
export const KINDS: Array<{ kind: string; label: string; help: string }> = [
  { kind: 'board', label: 'Board', help: 'a printed circuit board: the unit that is laid out and made' },
  { kind: 'system', label: 'System', help: 'boards that are joined to each other' },
  { kind: 'panel', label: 'Panel', help: 'one board repeated on a panel for the factory' },
  { kind: 'module', label: 'Module', help: 'a reusable sub-circuit placed on a board or in a module' },
  { kind: 'part', label: 'Part', help: 'a real component, with its pins and package' },
  { kind: 'interface', label: 'Interface', help: 'a typed bundle of pins and quantities that is connected as one' },
];

export function kindInfo(kind: string) {
  return KINDS.find((k) => k.kind === kind) ?? { kind, label: kind, help: 'a definition' };
}

// ------------------------------------------------------------------ building

/** The part of a path that the section names: files are shown below `src/` (the standard
 * library below `std/`), so the folder is what is left. */
export function baseOf(role: FileRole): string {
  return role === 'std' ? 'std' : 'src';
}

/** The folder of a path below `base` (`src/sheets/adc.nsm` below `src` is `sheets`). */
export function folderOf(path: string, base: string): string {
  const rest = path.startsWith(`${base}/`) ? path.slice(base.length + 1) : path;
  const at = rest.lastIndexOf('/');
  return at < 0 ? '' : rest.slice(0, at);
}

const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true });

/** Case-insensitive, with numbers in order (`adc2` before `adc10`), and the same every time. */
export function compareNames(a: string, b: string): number {
  return collator.compare(a, b) || (a < b ? -1 : a > b ? 1 : 0);
}

/** Folders before files, each by name. */
function compareNodes(a: FolderNode | FileNode, b: FolderNode | FileNode): number {
  if (a.type !== b.type) return a.type === 'folder' ? -1 : 1;
  const name = (n: FolderNode | FileNode) => (n.type === 'folder' ? n.segments.join('/') : n.name);
  return compareNames(name(a), name(b));
}

interface Draft {
  folders: Map<string, Draft>;
  files: FileNode[];
  /** The folder's path from the project root, from a file in it or from the folder list. */
  dir?: string;
}

const draft = (): Draft => ({ folders: new Map(), files: [] });

function fileNode(file: TreeFile, name: string): FileNode {
  return {
    type: 'file',
    id: `f:${file.path}`,
    name,
    file,
    children: file.defs.map((def) => ({ type: 'def', id: `e:${file.path}:${def.kind}:${def.name}:${def.line}`, name: def.name, def, file })),
  };
}

function finish(role: FileRole, prefix: string, d: Draft): Array<FolderNode | FileNode> {
  const folders = [...d.folders.entries()].map(([name, sub]): FolderNode => {
    const path = prefix ? `${prefix}/${name}` : name;
    const children = finish(role, path, sub);
    const all = collect(children);
    return {
      type: 'folder',
      id: `d:${role}:${path}`,
      segments: [name],
      files: all.length,
      library: all.length > 0 && all.every((f) => f.file.defs.length > 0 && f.file.defs.every((x) => x.kind === 'part')),
      dir: sub.dir ?? path,
      children,
    };
  });
  return [...folders, ...d.files].sort(compareNodes);
}

function collect(nodes: Array<FolderNode | FileNode>): FileNode[] {
  return nodes.flatMap((n) => (n.type === 'file' ? [n] : collect(n.children)));
}

/** A folder that holds only one folder is shown as one row, `vendor/archive`. */
function compress(nodes: Array<FolderNode | FileNode>): Array<FolderNode | FileNode> {
  const out = nodes.map((n) => {
    if (n.type !== 'folder') return n;
    let folder = n;
    while (folder.children.length === 1 && folder.children[0].type === 'folder') {
      const only: FolderNode = folder.children[0];
      folder = { ...only, segments: [...folder.segments, ...only.segments] };
    }
    return { ...folder, children: compress(folder.children) };
  });
  return out.sort(compareNodes);
}

/** The tree of a project's files: a section for each role that has files, in a fixed order,
 * folders inside folders, folders before files, each by name. */
export function buildTree(files: TreeFile[], folders: string[] = []): SectionNode[] {
  const sections: SectionNode[] = [];
  for (const role of SECTION_ORDER) {
    const mine = files.filter((f) => f.role === role);
    // The folders with no file of the tree in them are in the modules' section, where a file would go.
    const empty = role === 'module' ? folders.filter((d) => !files.some((f) => f.path.startsWith(`${d}/`))) : [];
    if (!mine.length && !empty.length) continue;
    const base = baseOf(role);
    const root = draft();
    const into = (parts: string[], dirs: string[]) => {
      let at = root;
      parts.forEach((part, i) => {
        let next = at.folders.get(part);
        if (!next) at.folders.set(part, (next = draft()));
        next.dir ??= dirs[i];
        at = next;
      });
      return at;
    };
    for (const f of mine) {
      const rest = f.path.startsWith(`${base}/`) ? f.path.slice(base.length + 1) : f.path;
      const parts = rest.split('/');
      const name = parts.pop() ?? rest;
      const prefix = f.path.startsWith(`${base}/`) ? `${base}/` : '';
      const dirs = parts.map((_, i) => `${prefix}${parts.slice(0, i + 1).join('/')}`);
      into(parts, dirs).files.push(fileNode(f, name));
    }
    for (const dir of empty) {
      const parts = dir.startsWith(`${base}/`) ? dir.slice(base.length + 1).split('/') : dir.split('/');
      into(parts, parts.map((_, i) => `${base}/${parts.slice(0, i + 1).join('/')}`));
    }
    const children = compress(finish(role, '', root));
    sections.push({ type: 'section', id: `s:${role}`, role, label: SECTION_LABEL[role], files: mine.length, children });
  }
  return sections;
}

/** The files of the tree, in the order it shows them. */
export function filesOf(nodes: Array<SectionNode | FolderNode | FileNode>): FileNode[] {
  return nodes.flatMap((n) => (n.type === 'file' ? [n] : filesOf(n.children)));
}

// ------------------------------------------------------------------ what is open

/** What is open or closed where the person said so: an id that is not here has its default. */
export type Overrides = Record<string, boolean>;

/** Whether a node that has children starts open: the sections are, except the standard
 * library; a library of parts is closed; the definitions of a top-level file are shown. */
export function defaultOpen(node: SectionNode | FolderNode | FileNode): boolean {
  switch (node.type) {
    case 'section':
      return node.role !== 'std';
    case 'folder':
      return !node.library;
    case 'file':
      return node.file.role === 'top_level';
  }
}

export function hasChildren(node: Node): node is SectionNode | FolderNode | FileNode {
  return node.type !== 'def' && node.children.length > 0;
}

export function isOpen(node: SectionNode | FolderNode | FileNode, overrides: Overrides): boolean {
  return overrides[node.id] ?? defaultOpen(node);
}

function expandable(nodes: Array<Node>): Array<SectionNode | FolderNode | FileNode> {
  return nodes.flatMap((n) => (n.type === 'def' ? [] : [...(hasChildren(n) ? [n] : []), ...expandable(n.children)]));
}

/** Everything open, or everything closed. The sections stay as they are: closing them too
 * would leave five lines and nothing to read. */
export function setAll(sections: SectionNode[], open: boolean, overrides: Overrides): Overrides {
  const next: Overrides = { ...overrides };
  for (const n of expandable(sections.flatMap((s) => s.children))) next[n.id] = open;
  return next;
}

/** The ids that must be open for the file to be seen: its section and its folders. */
export function ancestorIds(sections: SectionNode[], path: string): string[] {
  const walk = (nodes: Array<FolderNode | FileNode>, trail: string[]): string[] | null => {
    for (const n of nodes) {
      if (n.type === 'file') {
        if (n.file.path === path) return trail;
      } else {
        const found = walk(n.children, [...trail, n.id]);
        if (found) return found;
      }
    }
    return null;
  };
  for (const s of sections) {
    const found = walk(s.children, [s.id]);
    if (found) return found;
  }
  return [];
}

/** Drops what is not in the tree any more and what says what the default says, so what is
 * kept stays small. */
export function compactOverrides(sections: SectionNode[], overrides: Overrides): Overrides {
  const known = new Map<string, boolean>();
  for (const n of [...sections, ...expandable(sections.flatMap((s) => s.children))]) known.set(n.id, defaultOpen(n));
  const next: Overrides = {};
  for (const [id, open] of Object.entries(overrides)) if (known.has(id) && known.get(id) !== open) next[id] = open;
  return next;
}

const KEY = 'netsmith.tree.open:';

/** Reads what a project had open. Anything that is not a map of booleans is ignored. */
export function parseOverrides(value: unknown): Overrides {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  const out: Overrides = {};
  for (const [id, open] of Object.entries(value)) if (typeof open === 'boolean') out[id] = open;
  return out;
}

export const storageKey = (project: string) => `${KEY}${project}`;

// ------------------------------------------------------------------ the filter

/** Whether a file matches what was typed in the filter: its path or one of its definitions. */
export function matches(file: TreeFile, filter: string): boolean {
  const needle = filter.trim().toLowerCase();
  if (!needle) return true;
  return file.path.toLowerCase().includes(needle) || file.defs.some((d) => d.name.toLowerCase().includes(needle));
}

/** The tree with only what the filter finds: a file whose path has it (with no definitions,
 * unless one has it too) or a definition that has it, and every folder and section above.
 * With no filter, the tree itself. */
export function filterTree(sections: SectionNode[], filter: string): SectionNode[] {
  const needle = filter.trim().toLowerCase();
  if (!needle) return sections;
  const keep = (nodes: Array<FolderNode | FileNode>): Array<FolderNode | FileNode> =>
    nodes.flatMap((n): Array<FolderNode | FileNode> => {
      if (n.type === 'file') {
        if (!matches(n.file, needle)) return [];
        return [{ ...n, children: n.children.filter((d) => d.name.toLowerCase().includes(needle)) }];
      }
      const children = keep(n.children);
      return children.length ? [{ ...n, children, files: filesOf(children).length }] : [];
    });
  return sections.flatMap((s) => {
    const children = keep(s.children);
    return children.length ? [{ ...s, children, files: filesOf(children).length }] : [];
  });
}

/** A name in pieces, those that the filter matched marked, for the row to draw. */
export function highlight(text: string, filter: string): Array<{ text: string; hit: boolean }> {
  const needle = filter.trim().toLowerCase();
  const at = needle ? text.toLowerCase().indexOf(needle) : -1;
  if (at < 0) return [{ text, hit: false }];
  return [
    ...(at > 0 ? [{ text: text.slice(0, at), hit: false }] : []),
    { text: text.slice(at, at + needle.length), hit: true },
    ...(at + needle.length < text.length ? [{ text: text.slice(at + needle.length), hit: false }] : []),
  ];
}

// ------------------------------------------------------------------ the rows

export interface Row {
  id: string;
  node: Node;
  /** 1 for a section, 2 for what is directly in it, and so on: `aria-level`. */
  level: number;
  /** The id of the row above in the tree, or null for a section. */
  parent: string | null;
  /** True or false for a row with children, null for one without. */
  open: boolean | null;
}

/** The rows on screen, top to bottom. While a filter is on, everything that has a match is open. */
export function flatten(sections: SectionNode[], overrides: Overrides, filter = ''): Row[] {
  const filtering = filter.trim() !== '';
  const rows: Row[] = [];
  const walk = (node: Node, level: number, parent: string | null) => {
    const kids = node.type === 'def' ? [] : node.children;
    const open = kids.length ? filtering || isOpen(node as SectionNode | FolderNode | FileNode, overrides) : null;
    rows.push({ id: node.id, node, level, parent, open });
    if (open) for (const c of kids) walk(c, level + 1, node.id);
  };
  for (const s of sections) walk(s, 1, null);
  return rows;
}

// ------------------------------------------------------------------ the keyboard

export type TreeKey = 'ArrowUp' | 'ArrowDown' | 'ArrowLeft' | 'ArrowRight' | 'Home' | 'End' | 'Enter' | ' ';

export type Move = { focus: string } | { toggle: string; open: boolean } | { activate: string } | null;

/** What a key does with the focus on row `focus`, as the keys of a tree do in a code editor:
 * Up and Down move, Left closes or goes to the parent, Right opens or goes to the first child,
 * Home and End go to the ends, Enter opens a file or toggles a folder. */
export function keyMove(rows: Row[], focus: string | null, key: TreeKey): Move {
  if (!rows.length) return null;
  const at = Math.max(0, rows.findIndex((r) => r.id === focus));
  const row = rows[at];
  switch (key) {
    case 'ArrowDown':
      return { focus: rows[Math.min(at + 1, rows.length - 1)].id };
    case 'ArrowUp':
      return { focus: rows[Math.max(at - 1, 0)].id };
    case 'Home':
      return { focus: rows[0].id };
    case 'End':
      return { focus: rows[rows.length - 1].id };
    case 'ArrowRight':
      if (row.open === false) return { toggle: row.id, open: true };
      if (row.open === true) {
        const next = rows[at + 1];
        return next && next.parent === row.id ? { focus: next.id } : null;
      }
      return null;
    case 'ArrowLeft':
      if (row.open === true) return { toggle: row.id, open: false };
      return row.parent ? { focus: row.parent } : null;
    case 'Enter':
    case ' ':
      if (row.node.type === 'section' || row.node.type === 'folder') return { toggle: row.id, open: !row.open };
      return { activate: row.id };
  }
}

// ------------------------------------------------------------------ what a file is

export type FileIcon = 'nsm' | 'physical' | 'toml' | 'csv' | 'kicad' | 'file';

/** The icon of a file by its name: `.physical.nsm` before `.nsm`. */
export function fileIcon(name: string): FileIcon {
  const n = name.toLowerCase();
  if (n.endsWith('.physical.nsm')) return 'physical';
  if (n.endsWith('.nsm')) return 'nsm';
  if (n.endsWith('.toml')) return 'toml';
  if (n.endsWith('.csv')) return 'csv';
  if (/\.kicad_[a-z]+$/.test(n)) return 'kicad';
  return 'file';
}

/** Why the editor does not change a file, for the lock's tooltip. */
export function readonlyReason(file: TreeFile): string {
  if (file.role === 'manifest') return 'Read-only: edit it outside the editor';
  if (file.role === 'std') return 'Read-only: part of the standard library that ships with netsmith';
  return 'Read-only: edit it outside the editor';
}

/** The tooltip of a folder's count. */
export function countTitle(folder: FolderNode | SectionNode): string {
  if (folder.files === 0) return 'An empty folder';
  const files = `${folder.files} ${folder.files === 1 ? 'file' : 'files'}`;
  return folder.type === 'folder' && folder.library ? `${files}, all of them parts: a library` : files;
}

// ------------------------------------------------------------------ keeping the place

/** Where to scroll a list so that a row can be seen, moving it as little as can be: the new
 * `scrollTop`, which is the one it has if the row is all in view. `covered` is the height at
 * the top that something sticky hides (a section's header). */
export function revealScroll(rowTop: number, rowHeight: number, scrollTop: number, viewHeight: number, covered = 0): number {
  if (rowTop - covered < scrollTop) return Math.max(0, rowTop - covered);
  if (rowTop + rowHeight > scrollTop + viewHeight) return rowTop + rowHeight - viewHeight;
  return scrollTop;
}
