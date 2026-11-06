// The file a person starts in: where the first `board` of a project is written. `src/main.nsm` is what `netsmith init` makes, so it
// is the answer when it is there; else the first source file that sits directly in `src/`; else none.

export function entryFileOf(files: { path: string }[]): string | null {
  const paths = files.map((f) => f.path);
  if (paths.includes('src/main.nsm')) return 'src/main.nsm';
  return paths.find((p) => /^src\/[^/]+\.nsm$/.test(p) && !/\.physical\.nsm$/.test(p)) ?? null;
}

/** A project whose only error is "has no `board`" is a project that has not begun: the checker says it as an error (a file the
 * manifest lists must hold a board), the page says it as a place to start. Returns the file the checker names, or null if
 * the project has other errors (or none). */
export function emptyProject(problems: { severity: string; message: string }[]): { file: string | null } | null {
  const errors = problems.filter((p) => p.severity === 'error');
  if (errors.length === 0 || !errors.every((p) => /has no `board`/.test(p.message))) return null;
  return { file: /^(\S+) has no `board`/.exec(errors[0].message)?.[1] ?? null };
}
