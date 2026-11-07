// Sorting a file's imports the way Python's isort does: the standard library first (`std::`), then
// other projects, then this project's own (`src::`), one blank line between the groups and the
// imports of a group in the order of their paths. A comment on the lines just above an import
// travels with it; module docs (`//!`) and everything after the imports stay where they are. A pure
// function of the text, so it can be tested and applied as one undoable change.

export interface ImportEdit {
  from: number;
  to: number;
  insert: string;
}

interface Entry {
  lines: string[];
  path: string;
  group: number;
}

const isImport = (line: string) => /^import\s/.test(line);
const isComment = (line: string) => /^\s*\/\/(?!!)/.test(line);
const isBlank = (line: string) => line.trim() === '';

/** 0 for `std`, 2 for `src`, 1 for a dependency's name. */
export function importGroup(path: string): number {
  const first = path.split('::')[0];
  return first === 'std' ? 0 : first === 'src' ? 2 : 1;
}

/** `import a::b::{C, D}  // note` → `a::b`; the part that orders it. */
function pathOf(statement: string): string {
  const m = /^import\s+([A-Za-z0-9_:]*?)(?:::)?(?:\{|\s|\/\/|$)/.exec(statement);
  return m ? m[1] : statement;
}

/** The change that sorts the imports of `text`, or null when they are sorted already (or there are none). */
export function sortImports(text: string): ImportEdit | null {
  const eol = text.includes('\r\n') ? '\r\n' : '\n';
  const lines = text.split(/\r?\n/);
  // Where each line starts, for the range of the edit.
  const starts: number[] = [];
  let at = 0;
  for (const l of lines) {
    starts.push(at);
    at += l.length + eol.length;
  }

  const entries: Entry[] = [];
  let first = -1;
  let end = 0; // one past the last line the imports use
  let i = 0;
  let pending: string[] = [];
  let pendingFrom = -1;
  while (i < lines.length) {
    const line = lines[i];
    if (isImport(line)) {
      if (first < 0) first = pendingFrom >= 0 ? pendingFrom : i;
      const body = [line];
      // A braced list can run over several lines.
      while (body.join('\n').includes('{') && !body.join('\n').includes('}') && i + 1 < lines.length) body.push(lines[++i]);
      entries.push({ lines: [...pending, ...body], path: pathOf(line), group: importGroup(pathOf(line)) });
      pending = [];
      pendingFrom = -1;
      i++;
      end = i;
    } else if (isComment(line)) {
      if (pendingFrom < 0) pendingFrom = i;
      pending.push(line);
      i++;
    } else if (isBlank(line)) {
      // A blank line cuts a comment loose from what follows. Before the first import it stays where it is; after one, the block ends
      // there, so that no comment is ever lost or moved away from its place.
      if (pending.length && first >= 0) break;
      pending = [];
      pendingFrom = -1;
      i++;
    } else if (line.startsWith('//!') && first < 0) {
      i++;
    } else {
      break;
    }
  }
  if (entries.length === 0) return null;

  entries.sort((a, b) => a.group - b.group || (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  const out: string[] = [];
  entries.forEach((e, n) => {
    if (n > 0 && e.group !== entries[n - 1].group) out.push('');
    out.push(...e.lines);
  });

  // The range: from the first import's first line (with the comments above it) to the end of the last import.
  const from = starts[first];
  const lastLine = end - 1;
  const to = starts[lastLine] + lines[lastLine].length;
  const insert = out.join(eol);
  return text.slice(from, to) === insert ? null : { from, to, insert };
}
