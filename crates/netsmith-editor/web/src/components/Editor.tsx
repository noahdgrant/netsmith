import { closeBrackets } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, indentOnInput, indentUnit } from '@codemirror/language';
import { type Diagnostic, lintGutter, setDiagnostics } from '@codemirror/lint';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { Annotation, EditorState, type Extension } from '@codemirror/state';
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from '@codemirror/view';
import { useEffect, useRef } from 'preact/hooks';

import { api } from '../client';
import { languageExtensions } from '../languages';
import { flashLine, nextFlash, revealFlash, revealTarget } from '../reveal';
import {
  activeFile,
  cursor,
  edit,
  goTo,
  openFiles,
  project,
  reveal,
  save,
  say,
  shown,
} from '../store';

/** Marks a change made by the page, not typed, so it is not an edit. */
const External = Annotation.define<boolean>();

const theme = EditorView.theme(
  {
    '&': { height: '100%', color: 'var(--text)', backgroundColor: 'var(--editor)' },
    '.cm-content': { fontFamily: 'var(--mono)', fontSize: '13px', caretColor: 'var(--accent)' },
    '.cm-gutters': { backgroundColor: 'var(--editor)', color: 'var(--faint)', border: 'none' },
    '.cm-activeLine': { backgroundColor: 'rgba(255,255,255,0.045)' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text)' },
    '&.cm-focused': { outline: 'none' },
    '.cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground': {
      backgroundColor: 'rgba(232,135,58,0.28)',
    },
    '.cm-cursor': { borderLeftColor: 'var(--accent)' },
    '.cm-tooltip': { backgroundColor: 'var(--panel-2)', border: '1px solid var(--line)', color: 'var(--text)' },
    '.cm-diagnostic-error': { borderLeft: '3px solid var(--bad)' },
    '.cm-diagnostic-warning': { borderLeft: '3px solid var(--warn)' },
    '.cm-lintRange-error': { backgroundImage: 'none', textDecoration: 'underline wavy var(--bad)', textUnderlineOffset: '3px' },
    '.cm-lintRange-warning': { backgroundImage: 'none', textDecoration: 'underline wavy var(--warn)', textUnderlineOffset: '3px' },
  },
  { dark: true },
);

function extensionsFor(path: string, readonly: boolean): Extension[] {
  return [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightActiveLine(),
    drawSelection(),
    history(),
    bracketMatching(),
    closeBrackets(),
    indentOnInput(),
    indentUnit.of('    '),
    // A word of three letters or more, whole: a letter or two selected must not mark every other one.
    highlightSelectionMatches({ minSelectionLength: 3, wholeWords: true }),
    revealFlash,
    lintGutter(),
    ...languageExtensions(path, () => shown.peek()?.problems ?? []),
    theme,
    EditorState.readOnly.of(readonly),
    EditorView.editable.of(!readonly),
    keymap.of([
      {
        key: 'Mod-s',
        preventDefault: true,
        run: () => {
          void save(path);
          return true;
        },
      },
      {
        key: 'F12',
        run: (view) => {
          void definitionAt(path, view, view.state.selection.main.head);
          return true;
        },
      },
      indentWithTab,
      ...searchKeymap,
      ...historyKeymap,
      ...defaultKeymap,
    ]),
    EditorView.domEventHandlers({
      mousedown(event, view) {
        if (!(event.ctrlKey || event.metaKey) || event.button !== 0) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos == null) return false;
        event.preventDefault();
        void definitionAt(path, view, pos);
        return true;
      },
    }),
    EditorView.updateListener.of((update) => {
      if (update.docChanged && !update.transactions.some((t) => t.annotation(External))) {
        edit(path, update.state.doc.toString());
      }
      if (update.docChanged || update.selectionSet) {
        const head = update.state.selection.main.head;
        const line = update.state.doc.lineAt(head);
        cursor.value = { file: path, line: line.number, column: head - line.from + 1 };
      }
    }),
  ];
}

async function definitionAt(path: string, view: EditorView, pos: number) {
  try {
    const found = await api.definition(path, view.state.doc.toString(), pos);
    if (found) await goTo(found.at);
    else say('No definition found there');
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
  }
}

/** Where a problem is, as document positions. */
function range(state: EditorState, at: { line: number; column: number; endLine: number; endColumn: number }) {
  const doc = state.doc;
  const clamp = (line: number) => Math.min(Math.max(line, 1), doc.lines);
  const first = doc.line(clamp(at.line));
  const last = doc.line(clamp(at.endLine));
  const from = Math.min(first.from + at.column - 1, first.to);
  let to = Math.min(last.from + at.endColumn - 1, last.to);
  if (to <= from) to = Math.min(from + 1, doc.length);
  return { from, to };
}

/** Lines that stand where the code will be while the project is first read. */
function CodeSkeleton() {
  const widths = [38, 62, 24, 70, 54, 0, 46, 66, 30, 58, 42];
  return (
    <div class="code-skeleton" aria-hidden="true" data-testid="code-skeleton">
      {widths.map((w, i) => (w ? <div key={i} class="sk sk-line" style={{ width: `${w}%`, marginLeft: `${(i % 3) * 3}%` }} /> : <div key={i} class="sk-gap" />))}
    </div>
  );
}

export function Editor() {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const states = useRef(new Map<string, EditorState>());
  const current = useRef<string | null>(null);
  // The `rev` of each file's text that the editor has, so a replaced text is noticed and typing is not.
  const revs = useRef(new Map<string, number>());

  const path = activeFile.value;
  const file = path ? openFiles.value[path] : undefined;

  useEffect(() => {
    view.current = new EditorView({ parent: host.current!, state: EditorState.create({ doc: '' }) });
    return () => view.current?.destroy();
  }, []);

  // Switching file keeps each file's own undo history and cursor.
  useEffect(() => {
    const v = view.current;
    if (!v) return;
    if (current.current && current.current !== path) states.current.set(current.current, v.state);
    if (!path || !file) {
      current.current = null;
      v.setState(EditorState.create({ doc: '' }));
      return;
    }
    if (current.current !== path) {
      const kept = revs.current.get(path) === file.rev ? states.current.get(path) : undefined;
      v.setState(kept ?? EditorState.create({ doc: file.text, extensions: extensionsFor(path, file.readonly) }));
      revs.current.set(path, file.rev);
      current.current = path;
      const head = v.state.selection.main.head;
      const line = v.state.doc.lineAt(head);
      cursor.value = { file: path, line: line.number, column: head - line.from + 1 };
    }
  }, [path, file === undefined]);

  // The text was replaced under the editor (read again from disk), which typing never does.
  useEffect(() => {
    const v = view.current;
    if (!v || !file || !path || current.current !== path) return;
    if (revs.current.get(path) === file.rev) return;
    revs.current.set(path, file.rev);
    v.dispatch({
      changes: { from: 0, to: v.state.doc.length, insert: file.text },
      annotations: External.of(true),
    });
  }, [file?.rev]);

  // The problems of this file, as squiggles.
  const problems = shown.value?.problems;
  useEffect(() => {
    const v = view.current;
    if (!v || !path || current.current !== path) return;
    const diagnostics: Diagnostic[] = (problems ?? [])
      // Notes are listed in the problems panel and are not drawn on the source.
      .filter((p) => p.at && p.at.file === path && p.severity !== 'note')
      .map((p) => {
        const { from, to } = range(v.state, p.at!);
        return { from, to, severity: p.severity === 'error' ? ('error' as const) : ('warning' as const), message: p.message };
      });
    v.dispatch(setDiagnostics(v.state, diagnostics));
  }, [problems, path, file === undefined, file?.rev]);

  // A request to put the cursor somewhere.
  const ask = reveal.value;
  useEffect(() => {
    const v = view.current;
    if (!v || !ask || ask.at.file !== path || current.current !== path) return;
    const target = revealTarget(v.state.doc, ask.at, ask.name);
    v.dispatch({
      selection: { anchor: target.anchor, head: target.head },
      effects: [EditorView.scrollIntoView(target.anchor, { y: 'center' }), flashLine.of(nextFlash(target.anchor))],
    });
    v.focus();
  }, [ask, path, file === undefined]);

  return (
    <div class="editor-host">
      <div ref={host} class="cm-wrap" style={{ display: file ? 'block' : 'none' }} />
      {!file && (project.value ? <div class="empty">Open a file from the project tree.</div> : <CodeSkeleton />)}
    </div>
  );
}
