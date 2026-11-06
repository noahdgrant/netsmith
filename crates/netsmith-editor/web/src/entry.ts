// The file a person starts in: where the first `board` of a project is written. `src/main.nsm` is what `netsmith init` makes, so it
// is the answer when it is there; else the first source file that sits directly in `src/`; else none.

export function entryFileOf(files: { path: string }[]): string | null {
  const paths = files.map((f) => f.path);
  if (paths.includes('src/main.nsm')) return 'src/main.nsm';
  return paths.find((p) => /^src\/[^/]+\.nsm$/.test(p) && !/\.physical\.nsm$/.test(p)) ?? null;
}
