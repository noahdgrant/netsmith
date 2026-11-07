import { closeBrackets } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, foldKeymap, indentOnInput, indentUnit } from '@codemirror/language';
import { type Diagnostic, setDiagnostics } from '@codemirror/lint';
import { highlightSelectionMatches } from '@codemirror/search';
import { Annotation, Compartment, EditorState, type Extension } from '@codemirror/state';
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
import { EmptyState } from './ui';
import { languageExtensions } from '../languages';
import { flashLine, nextFlash, revealFlash, revealTarget } from '../reveal';
import { ensurePower, power } from '../source/boarddata';
import { baseText, diskCompare, showChanges } from '../compare/store';
import { cardExtension } from '../source/cm-card';
import { changesExtension, setBaseText } from '../source/cm-changes';
import { rulerExtension } from '../source/cm-ruler';
import { completeExtension } from '../source/cm-complete';
import { foldExtension } from '../source/cm-fold';
import { guidesExtension } from '../source/cm-guides';
import { hintsExtension, setHints } from '../source/cm-hints';
import { diagnosticOf } from '../source/cm-lint';
import { searchExtension, searchKeys } from '../source/cm-search';
import { severityGutter } from '../source/cm-sev';
import { cmTheme, lintCardTheme } from '../source/cm-theme';
import { sourceEditor } from '../source/handle';
import { inlineHints as hintsFor } from '../source/hints';
import { fontSize, inlineHints, vimMode, wordWrap } from '../source/prefs';
import { vimExtension } from '../source/cm-vim';
import {
  activeFile,
  cursor,
  detail,
  dirty,
  edit,
  goTo,
  openFiles,
  project,
  reveal,
  save,
  say,
  shown,
} from '../store';
import { type Theme, isDark, theme as colourTheme } from '../theme';

/** Marks a change made by the page, not typed, so it is not an edit. */
const External = Annotation.define<boolean>();

/** CodeMirror's own defaults differ for a dark and a light page (search panel, completion list, the
 * cursor when nothing overrides it): it is told which one this is, and again when the theme changes. */
const darkness = new Compartment();
const darknessFor = (t: Theme): Extension => EditorView.darkTheme.of(isDark(t));

/** Long lines wrap or run on, as chosen; set again for a state that was kept while the choice changed. */
const wrapping = new Compartment();
const wrapFor = (on: boolean): Extension => (on ? EditorView.lineWrapping : []);

/** Vim keys, or none, as chosen; set again for a state that was kept while the choice changed. */
const vimKeys = new Compartment();

function extensionsFor(path: string, readonly: boolean): Extension[] {
  return [
    // First: Vim's keys come before every other keymap's.
    vimKeys.of(vimExtension(vimMode.peek())),
    severityGutter,
    lineNumbers(),
    changesExtension(path),
    rulerExtension,
    foldExtension,
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
    hintsExtension,
    guidesExtension,
    searchExtension,
    ...languageExtensions(path, () => shown.peek()?.problems ?? []),
    cardExtension(path),
    completeExtension(path),
    cmTheme,
    lintCardTheme,
    darkness.of(darknessFor(colourTheme.peek())),
    wrapping.of(wrapFor(wordWrap.peek())),
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
      ...searchKeys,
      ...foldKeymap,
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
    sourceEditor.view = view.current;
    return () => {
      sourceEditor.view = null;
      sourceEditor.path = null;
      view.current?.destroy();
    };
  }, []);

  // Switching file keeps each file's own undo history and cursor.
  useEffect(() => {
    const v = view.current;
    if (!v) return;
    if (current.current && current.current !== path) states.current.set(current.current, v.state);
    if (!path || !file) {
      current.current = null;
      sourceEditor.path = null;
      v.setState(EditorState.create({ doc: '' }));
      return;
    }
    if (current.current !== path) {
      const kept = revs.current.get(path) === file.rev ? states.current.get(path) : undefined;
      v.setState(kept ?? EditorState.create({ doc: file.text, extensions: extensionsFor(path, file.readonly) }));
      // A kept state was made under the theme and the wrapping of its day.
      v.dispatch({ effects: [darkness.reconfigure(darknessFor(colourTheme.peek())), wrapping.reconfigure(wrapFor(wordWrap.peek())), vimKeys.reconfigure(vimExtension(vimMode.peek()))] });
      revs.current.set(path, file.rev);
      current.current = path;
      sourceEditor.path = path;
      const head = v.state.selection.main.head;
      const line = v.state.doc.lineAt(head);
      cursor.value = { file: path, line: line.number, column: head - line.from + 1 };
    }
  }, [path, file === undefined]);

  // The theme changed.
  const colours = colourTheme.value;
  useEffect(() => {
    view.current?.dispatch({ effects: darkness.reconfigure(darknessFor(colours)) });
  }, [colours]);

  // Wrapping and the size of the text changed.
  const wrap = wordWrap.value;
  useEffect(() => {
    view.current?.dispatch({ effects: wrapping.reconfigure(wrapFor(wrap)) });
  }, [wrap]);
  const vimOn = vimMode.value;
  useEffect(() => {
    view.current?.dispatch({ effects: vimKeys.reconfigure(vimExtension(vimOn)) });
  }, [vimOn]);
  const size = fontSize.value;
  useEffect(() => {
    view.current?.requestMeasure();
  }, [size]);

  // The text was replaced under the editor (read again from disk), which typing never does.
  useEffect(() => {
    const v = view.current;
    if (!v || !file || !path || current.current !== path) return;
    if (revs.current.get(path) === file.rev) return;
    revs.current.set(path, file.rev);
    // The cursor stays where it was (as far as the new text reaches): a whole-text replacement alone would put it at the top, and with it
    // the line a reveal had just taken it to.
    const { anchor, head } = v.state.selection.main;
    const end = file.text.length;
    v.dispatch({
      changes: { from: 0, to: v.state.doc.length, insert: file.text },
      selection: { anchor: Math.min(anchor, end), head: Math.min(head, end) },
      annotations: External.of(true),
    });
  }, [file?.rev]);

  // The problems of this file, as squiggles, marks in the gutter and cards.
  const problems = shown.value?.problems;
  useEffect(() => {
    const v = view.current;
    if (!v || !path || current.current !== path) return;
    const diagnostics: Diagnostic[] = (problems ?? [])
      // Notes are listed in the problems panel and are not drawn on the source.
      .filter((p) => p.at && p.at.file === path && p.severity !== 'note')
      .map((p) => {
        const { from, to } = range(v.state, p.at!);
        return diagnosticOf(p, from, to);
      });
    v.dispatch(setDiagnostics(v.state, diagnostics));
  }, [problems, path, file === undefined, file?.rev]);

  // The hints at the end of a line: chosen from the board when the file is as it is on disk (the board's
  // lines are the disk's); while it has edits the hints stay on their lines, moved with the text.
  const wantHints = inlineHints.value;
  const board = detail.value;
  const rails = power.value;
  useEffect(() => {
    const v = view.current;
    if (!v || !path || current.current !== path) return;
    if (!wantHints) {
      v.dispatch({ effects: setHints.of([]) });
      return;
    }
    ensurePower();
    const f = openFiles.peek()[path];
    if (f && dirty(f)) return;
    const lines = v.state.doc.toString().split('\n');
    v.dispatch({ effects: setHints.of(hintsFor(path, lines, board, rails)) });
  }, [wantHints, board, rails, path, file === undefined, file?.rev]);

  // The marks of the comparison with a git ref: the file at the ref is the base the margin diffs the text against.
  const marking = showChanges.value;
  const cmp = diskCompare.value;
  useEffect(() => {
    const v = view.current;
    if (!v || !path || current.current !== path) return;
    if (!marking || !cmp) {
      v.dispatch({ effects: setBaseText.of(null) });
      return;
    }
    let live = true;
    const entry = cmp.source.files.find((f) => f.path === path);
    const put = (base: string | null) => {
      if (live && current.current === path) view.current?.dispatch({ effects: setBaseText.of(base) });
    };
    // Only a file that differs is asked of the server: one that is not listed is as it was at the ref (so what is on disk is the
    // base), a new one is all new, and a file the server does not give (a note, a library) has no marks.
    if (!entry) put(openFiles.peek()[path]?.saved ?? null);
    else if (entry.status === 'added' || entry.status === 'untracked') put('');
    else if (entry.status === 'deleted' || !/\.(nsm|toml|csv)$/.test(path)) put(null);
    else void baseText(path).then(put);
    return () => {
      live = false;
    };
  }, [marking, cmp, path, file === undefined]);

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
    // A quiet ask (go to anything from another view) moves the cursor and leaves the keys where they were.
    if (!ask.quiet) v.focus();
  }, [ask, path, file === undefined]);

  return (
    <div class="editor-host">
      <div ref={host} class="cm-wrap" style={{ display: file ? 'block' : 'none' }} />
      {!file && (project.value ? <EmptyState icon="file" title="No file is open" help="Choose one in the project tree, or press Ctrl+P and type a name." /> : <CodeSkeleton />)}
    </div>
  );
}
