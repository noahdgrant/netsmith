// The page's side of the formatter (issue 1103): the server lays a source out as `netsmith project
// fmt` does (`POST /api/format`), and the page applies the answer to the editor as one change, the
// smallest that turns the text into the formatted one, so the editor's undo takes it back in one
// step and the cursor and the folds outside it stay where they are.

export interface TextEdit {
  from: number;
  to: number;
  insert: string;
}

/** The change that turns `before` into `after`: what lies between their common start and their
 * common end. Null when they are the same. */
export function textEdit(before: string, after: string): TextEdit | null {
  if (before === after) return null;
  let start = 0;
  const most = Math.min(before.length, after.length);
  while (start < most && before.charCodeAt(start) === after.charCodeAt(start)) start++;
  let end = 0;
  while (end < most - start && before.charCodeAt(before.length - 1 - end) === after.charCodeAt(after.length - 1 - end)) end++;
  return { from: start, to: before.length - end, insert: after.slice(start, after.length - end) };
}
