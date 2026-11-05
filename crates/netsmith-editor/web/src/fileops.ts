// What the tree's file operations need to decide, apart from drawing it: whether a name is
// good, what a new file starts with, where "new here" is, and what each row may do. The server
// checks the same rules again (crates/netsmith/src/editor/fileops.rs); this is so that a
// mistake is said while it is typed.

import type { Node } from './tree';

export type Kind = 'file' | 'folder';

const MAX_NAME = 64;
const IDENTIFIER = /^[\p{L}_][\p{L}\p{N}_]*$/u;
const WINDOWS_RESERVED = new Set(['con', 'prn', 'aux', 'nul', ...[1, 2, 3, 4, 5, 6, 7, 8, 9].flatMap((n) => [`com${n}`, `lpt${n}`])]);

export const NAME_RULE = 'letters, digits and _, not starting with a digit';

export type Checked = { ok: true; name: string } | { ok: false; message: string };

/** The name a person typed for a new file or folder: what it becomes (`power` is `power.nsm`),
 * or what is wrong with it. `siblings` are the names already in the folder. */
export function validateName(raw: string, kind: Kind, siblings: string[] = []): Checked {
  const typed = raw.trim();
  if (!typed) return { ok: false, message: 'Type a name.' };
  if (typed.includes('/') || typed.includes('\\')) {
    return { ok: false, message: 'A name has no slash: make the folder first, then the file in it.' };
  }
  if (typed.includes('..') || typed === '.') return { ok: false, message: 'A name cannot have `..` in it.' };
  if (typed.startsWith('.')) return { ok: false, message: 'A name cannot start with a dot: such a file is hidden.' };
  if (/[\u0000-\u001f]/.test(typed)) return { ok: false, message: 'A name cannot have control characters.' };
  let name = typed;
  let stem = typed;
  if (kind === 'file') {
    if (!name.toLowerCase().endsWith('.nsm')) {
      if (/\.[^.]*$/.test(name) && !name.toLowerCase().endsWith('.physical')) {
        return { ok: false, message: 'A source file ends in .nsm: leave the ending out and it is added.' };
      }
      name += '.nsm';
    }
    stem = name.slice(0, -4);
    if (stem.toLowerCase().endsWith('.physical')) stem = stem.slice(0, -9);
  }
  if (name.length > MAX_NAME) return { ok: false, message: `A name is at most ${MAX_NAME} characters.` };
  if (!IDENTIFIER.test(stem)) return { ok: false, message: `A name below src/ is a module name: ${NAME_RULE}.` };
  if (WINDOWS_RESERVED.has(stem.toLowerCase())) return { ok: false, message: `\`${stem}\` is a name some systems keep for themselves.` };
  if (siblings.some((s) => s.toLowerCase() === name.toLowerCase())) {
    return { ok: false, message: `\`${name}\` is already there.` };
  }
  return { ok: true, name };
}

/** Whether the name is a physical file's: `power.physical.nsm`. */
export const isPhysicalName = (name: string) => name.toLowerCase().endsWith('.physical.nsm');

/** The part of a file name before `.nsm` and `.physical`. */
export function stemOf(name: string): string {
  const base = name.replace(/\.nsm$/i, '');
  return base.replace(/\.physical$/i, '');
}

// ------------------------------------------------------------------ what a new file starts with

export type Template = 'empty' | 'module' | 'board' | 'part' | 'physical';

/** The choices for a new source file, in the order they are offered. */
export const TEMPLATES: Array<{ id: Exclude<Template, 'physical'>; label: string; help: string }> = [
  { id: 'empty', label: 'Empty', help: 'only a comment' },
  { id: 'module', label: 'Module', help: 'a module with a power rail' },
  { id: 'board', label: 'Board', help: 'a board with a two-layer process' },
  { id: 'part', label: 'Part', help: 'a part with two pins' },
];

/** `power_stage` is `PowerStage`: the name a definition gets from its file's. */
export function definitionName(stem: string): string {
  const words = stem.split('_').filter(Boolean);
  const name = words.map((w) => w[0].toUpperCase() + w.slice(1)).join('');
  return name || 'Item';
}

/** What a file named so is most likely to hold. */
export function suggestTemplate(stem: string): Exclude<Template, 'physical'> {
  const s = stem.toLowerCase();
  if (/(^|_)board($|_)/.test(s) || s.endsWith('board')) return 'board';
  if (/(^|_)part($|_)/.test(s)) return 'part';
  return 'module';
}

/** The text of a new file: a skeleton that builds, following docs/LANGUAGE.md. A physical file
 * is for a board of the source beside it (`board` is its name). */
export function starter(template: Template, stem: string, board?: string): string {
  const name = definitionName(stem);
  switch (template) {
    case 'empty':
      return `//! ${name}.\n`;
    case 'module':
      return `//! ${name}.\n\nimport std::sources::Rail\n\n/// ${name}.\nmodule ${name} {\n    .power: Rail\n}\n`;
    case 'board':
      return `//! ${name}.\n\nimport std::mfg::jlcpcb::TwoLayer\n\n/// ${name}.\nboard ${name} {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n}\n`;
    case 'part':
      return `//! ${name}.\n\n/// ${name}.\npart ${name} {\n    .designator &= "U"\n\n    pins {\n        .a passive "A" "1"\n        .b passive "B" "2"\n    }\n}\n`;
    case 'physical':
      return `//! The physical intent of ${board ?? name}.\n\nphysical for ${board ?? name} {\n    case {\n    }\n}\n`;
  }
}

// ------------------------------------------------------------------ where, and what a row may do

const parentOf = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '');

/** The folder that "new file here" means for a row: the folder itself, or the one a file is in,
 * when it is below `src/`; else `src/`. */
export function folderFor(node: Node | null): string {
  let dir = 'src';
  if (node?.type === 'folder') dir = node.dir;
  else if (node?.type === 'file' || node?.type === 'def') dir = parentOf(node.file.path);
  return dir === 'src' || dir.startsWith('src/') ? dir : 'src';
}

export interface Allowed {
  newHere: boolean;
  rename: string | null;
  remove: string | null;
}

/** What a row may do, and for what it may not, why: a string is the reason, null means it may. */
export function allowed(node: Node | null): Allowed {
  if (!node) return { newHere: true, rename: 'Select a file or folder.', remove: 'Select a file or folder.' };
  if (node.type === 'folder') {
    const inSrc = node.dir.startsWith('src/');
    const reason = inSrc ? null : 'Only folders below src/ can be changed.';
    return { newHere: true, rename: reason, remove: reason };
  }
  if (node.type === 'section') return { newHere: true, rename: 'Select a file or folder.', remove: 'Select a file or folder.' };
  const file = node.file;
  if (file.role === 'std') {
    const why = 'The standard library is built into netsmith: it cannot be changed.';
    return { newHere: true, rename: why, remove: why };
  }
  if (file.role === 'manifest') {
    const why = 'netsmith.toml is edited outside the editor.';
    return { newHere: true, rename: why, remove: why };
  }
  if (file.role === 'top_level') {
    return { newHere: true, rename: 'A top-level file is named in netsmith.toml: change the list there first.', remove: null };
  }
  if (file.role === 'physical') {
    return { newHere: true, rename: 'A physical file goes with its source: rename the source.', remove: null };
  }
  return { newHere: true, rename: null, remove: null };
}

/** What the person is asked before something is removed, and whether it needs the stronger
 * yes (a top-level file, or a folder with files in it). */
export function removalPrompt(node: Exclude<Node, { type: 'section' }>, openUnsaved: string[]): { title: string; lines: string[]; strong: boolean } {
  const lines: string[] = [];
  let strong = false;
  let title: string;
  if (node.type === 'folder') {
    title = `Delete the folder ${node.dir}?`;
    if (node.files > 0) {
      lines.push(`It has ${node.files} ${node.files === 1 ? 'file' : 'files'} in it, and they are deleted with it.`);
      strong = true;
    }
  } else {
    const file = node.file;
    title = `Delete ${file.path}?`;
    if (file.role === 'top_level') {
      lines.push('This is a top-level file: netsmith.toml lists it, and the project does not build without it until the list is changed.');
      strong = true;
    }
    if (file.role !== 'physical') lines.push('Its physical file beside it, if it has one, is deleted too.');
  }
  lines.push('This cannot be undone.');
  if (openUnsaved.length) lines.push(`Unsaved changes in ${openUnsaved.join(', ')} are lost.`);
  return { title, lines, strong };
}
