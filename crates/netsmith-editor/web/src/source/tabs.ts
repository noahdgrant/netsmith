// The order of the file tabs: a tab dragged to a place goes before the tab it is dropped on, or to
// the end. Pure.

/** `list` with `path` moved before `before` (null: to the end). An unknown path or the same place
 * gives the list as it was. */
export function reorder(list: string[], path: string, before: string | null): string[] {
  if (!list.includes(path) || path === before) return list;
  const rest = list.filter((p) => p !== path);
  const at = before === null ? rest.length : rest.indexOf(before);
  if (at < 0) return list;
  const next = [...rest.slice(0, at), path, ...rest.slice(at)];
  return next.every((p, i) => p === list[i]) ? list : next;
}

/** Where a drag over a tab would drop: before it when the pointer is in its left half, else before
 * the next one (null at the end). */
export function dropBefore(list: string[], over: string, leftHalf: boolean): string | null {
  const i = list.indexOf(over);
  if (i < 0) return null;
  return leftHalf ? over : (list[i + 1] ?? null);
}
