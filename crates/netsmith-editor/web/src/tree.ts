// The project tree's folders: files grouped by the folder they are in, and which folders are
// the library of parts (collapsed until asked for).

import type { TreeFile } from './api';

export interface Folder {
  /** `sheets`, `parts/vendor`; empty for the files beside the root of the section. */
  name: string;
  files: TreeFile[];
  /** Every definition in it is a part: a library, not part of the design's structure. */
  library: boolean;
}

/** The folder of a path below `base` (`src/sheets/adc.nsm` below `src` is `sheets`). */
export function folderOf(path: string, base: string): string {
  const rest = path.startsWith(`${base}/`) ? path.slice(base.length + 1) : path;
  const at = rest.lastIndexOf('/');
  return at < 0 ? '' : rest.slice(0, at);
}

/** The files grouped by folder, the ones with no folder first, then the folders by name. */
export function groupByFolder(files: TreeFile[], base = 'src'): Folder[] {
  const groups = new Map<string, TreeFile[]>();
  for (const f of files) {
    const name = folderOf(f.path, base);
    groups.set(name, [...(groups.get(name) ?? []), f]);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a === '' ? -1 : b === '' ? 1 : a.localeCompare(b)))
    .map(([name, list]) => ({
      name,
      files: list.sort((a, b) => a.path.localeCompare(b.path, undefined, { numeric: true })),
      library: list.every((f) => f.defs.length > 0 && f.defs.every((d) => d.kind === 'part')),
    }));
}

/** Whether a file matches what was typed in the filter: its path or one of its definitions. */
export function matches(file: TreeFile, filter: string): boolean {
  const needle = filter.trim().toLowerCase();
  if (!needle) return true;
  return file.path.toLowerCase().includes(needle) || file.defs.some((d) => d.name.toLowerCase().includes(needle));
}
