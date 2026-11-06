// The read-only comparison of one file at the ref against the working file: a CodeMirror view,
// inline or side by side, with the changed lines coloured by the page's tokens and the words that
// changed marked inside the line. It opens over the Compare section's list or over the editor of the
// Source section, and closes with Escape.

import type { EditorView } from '@codemirror/view';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { api } from '../../client';
import { Icon } from '../../components/Icons';
import { Segmented, Spinner } from '../../components/ui';
import { type ViewRow, createDiffView, scrollToRow, syncScroll } from '../../compare/cm-diff';
import { changeStarts, inlineRows, sideRows, splitLines } from '../../compare/linediff';
import { baseText, closeDiff, compare, diffTarget, openDiff, type DiffTarget } from '../../compare/store';
import { goTo, openFiles, revealSection } from '../../store';
import { load, save } from '../../storage';

type Mode = 'inline' | 'side';
const MODE_KEY = 'netsmith.compare.diffmode';

/** The text on disk, or what the editor holds when the file is open (so unsaved edits are compared too). */
async function headText(path: string): Promise<{ text: string; open: boolean } | null> {
  const f = openFiles.peek()[path];
  if (f) return { text: f.text, open: true };
  try {
    return { text: (await api.file(path)).text, open: false };
  } catch {
    return null;
  }
}

const viewRows = (rows: ReturnType<typeof inlineRows>): ViewRow[] => rows.map((r) => ({ text: r.text, kind: r.kind, a: r.aLine, b: r.bLine, marks: r.marks }));

function Views({ base, head, path, mode, line, side, onCount, jump }: { base: string; head: string; path: string; mode: Mode; line?: number; side?: 'base' | 'head'; onCount: (n: number) => void; jump: { to: number; nonce: number } | null }) {
  const left = useRef<HTMLDivElement>(null);
  const right = useRef<HTMLDivElement>(null);
  const views = useRef<EditorView[]>([]);
  const a = useMemo(() => splitLines(base), [base]);
  const b = useMemo(() => splitLines(head), [head]);
  const inline = useMemo(() => inlineRows(a, b), [a, b]);
  const sides = useMemo(() => sideRows(a, b), [a, b]);
  const starts = useMemo(() => (mode === 'inline' ? changeStarts(inline) : changeStarts(sides)), [mode, inline, sides]);
  useEffect(() => onCount(starts.length), [starts.length]);

  // The row of the line that was asked for.
  const focus = useMemo(() => {
    if (line === undefined) return undefined;
    if (mode === 'inline') {
      const at = inline.findIndex((r) => (side === 'head' ? r.bLine === line : r.aLine === line));
      return at < 0 ? undefined : at;
    }
    const at = sides.findIndex((r) => (side === 'head' ? r.right?.bLine === line : r.left?.aLine === line));
    return at < 0 ? undefined : at;
  }, [mode, inline, sides, line, side]);

  useEffect(() => {
    const made: EditorView[] = [];
    let unsync = () => {};
    if (mode === 'inline' && left.current) {
      made.push(createDiffView(left.current, viewRows(inline), path, { columns: 'both', focus, label: `Changes in ${path}` }));
    } else if (left.current && right.current) {
      const fill = (r: ViewRow | null | undefined): ViewRow => r ?? { text: '', kind: 'filler', marks: [] };
      const l = createDiffView(left.current, sides.map((r) => fill(r.left && { text: r.left.text, kind: r.left.kind, a: r.left.aLine, marks: r.left.marks })), path, { columns: 'a', focus, label: `${path} at the ref` });
      const r = createDiffView(right.current, sides.map((s) => fill(s.right && { text: s.right.text, kind: s.right.kind, b: s.right.bLine, marks: s.right.marks })), path, { columns: 'b', focus, label: `${path} on disk` });
      made.push(l, r);
      unsync = syncScroll(l, r);
    }
    views.current = made;
    // A change to look at: the one asked for, else the first.
    const at = focus ?? starts[0];
    if (at !== undefined) requestAnimationFrame(() => made[0] && scrollToRow(made[0], at, focus === undefined ? 24 : 140));
    return () => {
      unsync();
      for (const v of made) v.destroy();
      views.current = [];
    };
  }, [mode, inline, sides, path, focus]);

  useEffect(() => {
    if (!jump) return;
    const at = starts[jump.to];
    if (at !== undefined && views.current[0]) scrollToRow(views.current[0], at, 60);
  }, [jump?.nonce]);

  return mode === 'inline' ? (
    <div class="diff-cols inline" ref={left} data-testid="diff-inline" />
  ) : (
    <div class="diff-cols side" data-testid="diff-side">
      <div class="diff-col" ref={left} />
      <div class="diff-col" ref={right} />
    </div>
  );
}

function Body({ target }: { target: DiffTarget }) {
  const c = compare.value;
  const [mode, setMode] = useState<Mode>(() => (load<Mode>(MODE_KEY, 'side') === 'inline' ? 'inline' : 'side'));
  const [texts, setTexts] = useState<{ base: string | null; head: { text: string; open: boolean } | null } | 'loading'>('loading');
  const [count, setCount] = useState(0);
  const [at, setAt] = useState(0);
  const [jump, setJump] = useState<{ to: number; nonce: number } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const path = target.path;
  const hash = c?.base.hash;

  useEffect(() => {
    let live = true;
    setTexts('loading');
    void Promise.all([baseText(path), headText(path)]).then(([base, head]) => live && setTexts({ base, head }));
    return () => {
      live = false;
    };
  }, [path, hash]);
  useEffect(() => root.current?.focus(), [path]);

  const ref = c?.base.short ?? 'the ref';
  const step = (d: 1 | -1) => {
    const n = Math.min(Math.max(at + d, 0), Math.max(0, count - 1));
    setAt(n);
    setJump({ to: n, nonce: (jump?.nonce ?? 0) + 1 });
  };
  const loaded = texts === 'loading' ? null : texts;
  const isNew = !!loaded && loaded.base === null;
  const isGone = !!loaded && loaded.head === null;
  const where = target.where;
  return (
    <div
      class={`diff-panel ${where}`}
      ref={root}
      tabIndex={-1}
      role="dialog"
      aria-label={`Changes in ${path} since ${ref}`}
      data-testid="diff-panel"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          closeDiff();
        } else if (e.key === 'F7' || (e.altKey && e.key === 'ArrowDown')) {
          e.preventDefault();
          step(e.shiftKey ? -1 : 1);
        }
      }}
    >
      <header class="diff-head">
        <button class="icon-btn" data-tip="Close the comparison (Esc)" aria-label="Close the comparison" data-testid="diff-close" onClick={closeDiff}>
          <Icon name="close" size={15} />
        </button>
        <div class="diff-title">
          <Icon name="file" size={14} />
          <span class="mono" data-testid="diff-path">{path}</span>
          <span class="diff-versus">
            <span class="diff-side base">{ref}</span>
            <Icon name="arrow-right" size={12} />
            <span class="diff-side head">{loaded?.head?.open ? 'working file' : 'on disk'}</span>
          </span>
          {isNew && <span class="chip ok">new file</span>}
          {isGone && <span class="chip bad">deleted</span>}
          <span class="chip read-only">read only</span>
        </div>
        <span class="spacer" />
        <span class="diff-count" data-testid="diff-count" aria-live="polite">
          {count === 0 ? 'no changes' : `${Math.min(at + 1, count)} of ${count} ${count === 1 ? 'change' : 'changes'}`}
        </span>
        <button class="icon-btn" data-tip="Previous change (Shift+F7)" aria-label="Previous change" disabled={count === 0 || at === 0} data-testid="diff-prev" onClick={() => step(-1)}>
          <Icon name="chevron-up" size={15} />
        </button>
        <button class="icon-btn" data-tip="Next change (F7)" aria-label="Next change" disabled={count === 0 || at >= count - 1} data-testid="diff-next" onClick={() => step(1)}>
          <Icon name="chevron-down" size={15} />
        </button>
        <Segmented<Mode>
          label="How to show the changes"
          value={mode}
          onChange={(m) => {
            setMode(m);
            save(MODE_KEY, m);
            setAt(0);
          }}
          options={[
            { value: 'side', label: 'Side by side', tip: 'The file at the ref on the left, the file on disk on the right', testid: 'diff-mode-side' },
            { value: 'inline', label: 'Inline', tip: 'Removed and added lines in one column', testid: 'diff-mode-inline' },
          ]}
        />
        {where === 'compare' ? (
          <button
            class="tool small"
            data-testid="diff-in-source"
            disabled={isGone}
            onClick={() => {
              openDiff({ ...target, where: 'source' });
              revealSection('source');
              void goTo({ file: path, line: target.line ?? 1, column: 1, endLine: target.line ?? 1, endColumn: 1 }, undefined, true);
            }}
          >
            <Icon name="source" size={13} />
            Show changes in source
          </button>
        ) : (
          <button class="tool small" data-testid="diff-in-compare" onClick={() => (openDiff({ ...target, where: 'compare' }), revealSection('compare'))}>
            <Icon name="compare" size={13} />
            Show in Compare
          </button>
        )}
      </header>
      <div class="diff-body">
        {loaded === null ? (
          <div class="diff-loading" role="status">
            <Spinner size={14} /> Reading {path} at {ref}
          </div>
        ) : (
          <Views base={loaded.base ?? ''} head={loaded.head?.text ?? ''} path={path} mode={mode} line={target.line} side={target.side} onCount={setCount} jump={jump} />
        )}
      </div>
    </div>
  );
}

/** The panel, where `where` is the host that shows it: the Compare section or the Source section. */
export function DiffPanel({ where }: { where: DiffTarget['where'] }) {
  const t = diffTarget.value;
  if (!t || t.where !== where) return null;
  return <Body key={`${t.path}:${t.line ?? ''}:${t.side ?? ''}`} target={t} />;
}
