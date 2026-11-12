// The commands of the editor, registered once: the palette, the cheat sheet, the keyboard and the
// buttons all run these. A command that only the window knows how to do (a view's own key) is
// not here; the cheat sheet lists those from `LOCAL_KEYS` below. (The Schematic's keys are
// commands, defined here and run through the handle the view hands out.)

import { foldAll, unfoldAll } from '@codemirror/language';
import { EditorView, closeHoverTooltips } from '@codemirror/view';

import { askForRef, toggleChanges } from './components/ChangesSwitch';
import { api } from './client';
import { clearCompare, diffTarget, diskCompare, openDiff, refName, refText, runCompare, showChanges } from './compare/store';
import { diffControls } from './compare/collapse';
import { type Command, provide, register, runCommand } from './commands';
import { stepProblem } from './feedback/derive';
import { busy, dock, filter, pending, setDockOpen, toggleDock } from './feedback/state';
import { SECTIONS, canSplit, closePane, splitRight } from './layout';
import { openPicker, toggleCheatSheet } from './quickpick';
import { cardAtCursor } from './source/cm-card';
import { openCard } from './source/cm-card';
import { changeHunks, stepChange } from './source/changeSteps';
import { changeLines } from './source/cm-changes';
import { toggleLineComment } from './source/comment';
import { sourceEditor } from './source/handle';
import { sortImports } from './source/imports';
import { canBack, canForward } from './source/navhistory';
import { COMMENT_FONTS, autosave, commentFont, commentFontOf, hoverCards, inlineHints, outlineOpen, nextCommentFont, rebuildOnSave, resetEditorZoom, sortImportsOnSave, vimMode, wordWrap, zoomEditor } from './source/prefs';
import { openReplace } from './source/cm-search';
import { type Selection, selectAndReveal, selection, setSelection } from './selection';
import { PRESETS, pcbMode, togglePcbMode, view3d } from './pcb3d/mode';
import { THEMES, isDark, theme } from './theme';
import { chooseTheme } from './themeUi';
import {
  activeFile,
  analysis,
  back,
  board,
  chooseBoard,
  cancelAnalysis,
  closeFile,
  job,
  layout,
  navBack,
  navForward,
  navHistory,
  requestBack,
  requestRun,
  revealSection,
  routeBoard,
  runAnalysisAgain,
  say,
  save,
  showInSource,
  showSection,
  visible,
  windowWidth,
} from './store';
import { NUDGE, NUDGE_FAR, PAN_STEP, schematicActions } from './views/schematic-actions';
import { type PickedBus, goToBus } from './views/schematic-selection';

/** The section of the pane that last had the pointer or the keys. */
export function focusedSection() {
  const vis = visible.peek();
  return (vis.find((p) => p.index === layout.peek().focus) ?? vis[0])?.section;
}

const idle = () => job.value === null;

// ---- the Schematic's own keys ----
// They act on the view's zoom, sheets and pins, which the view hands out while it is mounted
// (`views/schematic-actions.ts`). They work only while the Schematic is the pane in use. A plain
// chord already leaves a text field alone (the key handler in `App.tsx`), so none of them needs to
// say so; the keys that other things on the page also use (Tab, Enter, the arrows) are held to the
// drawing by `keyFrom`, so a button or a list that has the focus keeps them. The palette and the
// buttons run a command whatever has the focus.

/** The cheat sheet's group of the keys that arrange the drawing by hand (ADR 0061), apart from the Schematic's own: one group of both would not fit the sheet's window, where the Schematic group is scrolled to. */
const LAYOUT_GROUP = 'Schematic layout';

const schematicHere = () => focusedSection() === 'schematic' && schematicActions.current !== null;
/** The drawing has the key: it was pressed on the drawing or on nothing (the page). A button or a
 * list that has the focus keeps Tab, Enter and the arrows. */
const onDrawing = (t: EventTarget | null) => {
  const el = t instanceof Element ? t : null;
  return !el || el === document.body || !!el.closest('.panzoom');
};
const sheet = () => schematicHere() && !schematicActions.current!.overview();
const sheetOrOverview = () => schematicHere();
const picked = () => selection.value?.kind === 'part' || selection.value?.kind === 'net';
/** A part is selected and its block can be moved: the arrow keys nudge it and no longer pan the view (ADR 0061, stage 4). */
const nudgeable = () => sheet() && selection.value?.kind === 'part' && schematicActions.current!.layout.canMove();
const turnable = () => sheet() && selection.value?.kind === 'part' && schematicActions.current!.layout.canTurn();

const schematicCommands = (): Command[] => {
  const act = () => schematicActions.current!;
  const pan = (id: string, title: string, key: string, dx: number, dy: number): Command => ({
    id: `schematic.pan.${id}`,
    title,
    group: 'Schematic',
    keys: key,
    aliases: ['scroll', 'move the view'],
    keyFrom: onDrawing,
    // With a part selected whose block can be moved the same keys nudge it (below): the two never apply together.
    when: () => sheet() && !nudgeable(),
    run: () => act().pan(dx * PAN_STEP, dy * PAN_STEP),
  });
  const nudge = (id: string, title: string, key: string, sx: number, sy: number, far: boolean): Command => ({
    id: `schematic.nudge.${id}`,
    title,
    group: LAYOUT_GROUP,
    keys: key,
    aliases: ['move the part', 'arrange', 'layout'],
    keyFrom: onDrawing,
    when: nudgeable,
    run: () => act().layout.nudge(sx * (far ? NUDGE_FAR : NUDGE), sy * (far ? NUDGE_FAR : NUDGE)),
  });
  return [
    {
      id: 'schematic.sheet.previous',
      title: 'Previous module',
      group: 'Schematic',
      keys: '[',
      alsoKeys: ['PageUp'],
      when: () => sheetOrOverview() && act().sheets() > 1,
      run: () => act().sheet(-1),
    },
    {
      id: 'schematic.sheet.next',
      title: 'Next module',
      group: 'Schematic',
      keys: ']',
      alsoKeys: ['PageDown'],
      when: () => sheetOrOverview() && act().sheets() > 1,
      run: () => act().sheet(1),
    },
    {
      id: 'schematic.fit',
      title: 'Fit the module',
      group: 'Schematic',
      keys: 'F',
      alsoKeys: ['0'],
      aliases: ['zoom to fit', 'reset zoom'],
      when: sheet,
      run: () => act().fit(),
    },
    {
      id: 'schematic.fitwidth',
      title: 'Fit the width of the module',
      group: 'Schematic',
      keys: 'W',
      aliases: ['zoom to width'],
      when: sheet,
      run: () => act().fitWidth(),
    },
    {
      id: 'schematic.actual',
      title: 'Zoom to 100%',
      group: 'Schematic',
      keys: 'Shift+1',
      aliases: ['actual size', 'true scale'],
      when: sheet,
      run: () => act().actual(),
    },
    {
      id: 'schematic.zoomselection',
      title: 'Zoom to the selection',
      group: 'Schematic',
      keys: 'Shift+2',
      aliases: ['frame', 'zoom to part', 'zoom to net'],
      when: () => sheet() && picked(),
      run: () => act().zoomSelection(),
    },
    {
      id: 'schematic.zoomin',
      title: 'Zoom in',
      group: 'Schematic',
      keys: '+',
      alsoKeys: ['='],
      when: sheet,
      run: () => act().zoom(1.25),
    },
    {
      id: 'schematic.zoomout',
      title: 'Zoom out',
      group: 'Schematic',
      keys: '-',
      when: sheet,
      run: () => act().zoom(0.8),
    },
    pan('left', 'Pan left', 'ArrowLeft', 1, 0),
    pan('right', 'Pan right', 'ArrowRight', -1, 0),
    pan('up', 'Pan up', 'ArrowUp', 0, 1),
    pan('down', 'Pan down', 'ArrowDown', 0, -1),
    nudge('left', 'Nudge the selected part left one grid step', 'ArrowLeft', -1, 0, false),
    nudge('right', 'Nudge the selected part right one grid step', 'ArrowRight', 1, 0, false),
    nudge('up', 'Nudge the selected part up one grid step', 'ArrowUp', 0, -1, false),
    nudge('down', 'Nudge the selected part down one grid step', 'ArrowDown', 0, 1, false),
    nudge('left10', 'Nudge the selected part left ten grid steps', 'Shift+ArrowLeft', -1, 0, true),
    nudge('right10', 'Nudge the selected part right ten grid steps', 'Shift+ArrowRight', 1, 0, true),
    nudge('up10', 'Nudge the selected part up ten grid steps', 'Shift+ArrowUp', 0, -1, true),
    nudge('down10', 'Nudge the selected part down ten grid steps', 'Shift+ArrowDown', 0, 1, true),
    {
      id: 'schematic.rotate',
      title: 'Rotate the selected part a quarter turn',
      group: LAYOUT_GROUP,
      keys: 'R',
      aliases: ['turn', 'orientation', 'layout'],
      keyFrom: onDrawing,
      when: turnable,
      run: () => act().layout.turn(1),
    },
    {
      id: 'schematic.rotate.back',
      title: 'Rotate the selected part a quarter turn the other way',
      group: LAYOUT_GROUP,
      keys: 'Shift+R',
      aliases: ['turn', 'orientation', 'layout'],
      keyFrom: onDrawing,
      when: turnable,
      run: () => act().layout.turn(-1),
    },
    {
      id: 'schematic.mirror',
      title: 'Mirror the selected part',
      group: LAYOUT_GROUP,
      keys: 'M',
      aliases: ['flip', 'orientation', 'layout'],
      keyFrom: onDrawing,
      when: turnable,
      run: () => act().layout.mirror(),
    },
    {
      id: 'schematic.layout.clearpart',
      title: "Clear the selected part's layout entry",
      group: LAYOUT_GROUP,
      keys: 'Backspace',
      alsoKeys: ['Delete'],
      aliases: ['put back', 'reset position', 'forget the move', 'layout'],
      keyFrom: onDrawing,
      when: () => sheet() && selection.value?.kind === 'part' && schematicActions.current!.layout.hinted(),
      run: () => act().layout.clearPart(),
    },
    {
      id: 'schematic.layout.clearsheet',
      title: 'Clear the layout of this module',
      group: LAYOUT_GROUP,
      aliases: ['reset the arrangement', 'put every part back', 'layout'],
      when: () => sheet() && schematicActions.current!.layout.sheetEntries() > 0,
      run: () => act().layout.clearSheet(),
    },
    {
      id: 'schematic.layout.clearall',
      title: 'Clear every layout entry of the file',
      group: LAYOUT_GROUP,
      aliases: ['reset the arrangement', 'delete the layout file', 'layout'],
      when: () => schematicHere() && schematicActions.current!.layout.allEntries() > 0,
      run: () => act().layout.clearAll(),
    },
    {
      id: 'schematic.pin.next',
      title: 'Next pin of the selected net',
      group: 'Schematic',
      keys: 'Tab',
      aliases: ['walk the net', 'step through the pins'],
      keyFrom: onDrawing,
      when: () => sheetOrOverview() && selection.value?.kind === 'net',
      run: () => act().pin(1),
    },
    {
      id: 'schematic.pin.previous',
      title: 'Previous pin of the selected net',
      group: 'Schematic',
      keys: 'Shift+Tab',
      aliases: ['walk the net', 'step through the pins'],
      keyFrom: onDrawing,
      when: () => sheetOrOverview() && selection.value?.kind === 'net',
      run: () => act().pin(-1),
    },
    {
      id: 'schematic.source',
      title: 'Open the selected part in Source',
      group: 'Schematic',
      keys: 'Enter',
      aliases: ['go to source', 'declaration'],
      keyFrom: onDrawing,
      when: () => sheet() && selection.value?.kind === 'part',
      run: () => act().source(),
    },
  ];
};

// ---- the source's commands ----
// They act on the editor that is on show (`source/handle.ts`), the history and the preferences
// (`source/prefs.ts`). The switches that a person turns on and off read as the thing to do next
// ("Show inline hints" while they are hidden); both are one command, found by its id.

const sourceHere = () => focusedSection() === 'source';
const editorOn = () => sourceHere() && sourceEditor.view !== null && activeFile.value !== null;

/** Next change and Previous change: the cursor goes to the start of the next run of lines the comparison marks in the open file, from
 * the cursor's line (the runs the margin and the ruler show). Without a comparison nothing moves and a toast says so. */
function stepSourceChange(dir: 1 | -1) {
  const v = sourceEditor.view;
  if (!v) return;
  if (refName.peek() === null) {
    say('No comparison active');
    return;
  }
  const marks = changeLines(v.state);
  if (!marks) {
    say('The changes are not shown: turn on Show changes to step through them');
    return;
  }
  const state = v.state;
  const to = stepChange(changeHunks(state.doc.lines, marks), state.doc.lineAt(state.selection.main.head).number, dir);
  if (!to) {
    say(`No changes in this file since ${refText.peek() || 'the ref'}`);
    return;
  }
  const at = state.doc.line(Math.min(to.line, state.doc.lines)).from;
  v.focus();
  v.dispatch({ selection: { anchor: at }, effects: EditorView.scrollIntoView(at, { y: 'center' }), scrollIntoView: false, userEvent: 'select' });
  if (to.wrapped) say(dir === 1 ? 'Back at the first change' : 'Back at the last change');
}

const sourceCommands = (): Command[] => [
  {
    id: 'source.comment.toggle',
    title: 'Source: Comment or uncomment the lines',
    group: 'Source',
    keys: 'Mod+/',
    inFields: true,
    aliases: ['toggle comment', 'comment out', 'uncomment', 'block comment'],
    when: editorOn,
    run: () => {
      const v = sourceEditor.view;
      if (!v || v.state.readOnly) return;
      v.focus();
      toggleLineComment(v);
    },
  },
  {
    id: 'source.card.show',
    title: 'Show the hover card at the cursor',
    group: 'Source',
    keys: 'Mod+Alt+I',
    inFields: true,
    aliases: ['hover', 'quick info', 'describe', 'peek', 'tooltip'],
    when: editorOn,
    run: () => {
      const v = sourceEditor.view;
      const path = sourceEditor.path;
      if (!v || !path) return;
      if (!cardAtCursor(v.state, path)) {
        say('Nothing to describe at the cursor: put it on a part, a type or a net name.');
        return;
      }
      v.focus();
      // A card the pointer opened is closed first: two cards over one word would stack.
      v.dispatch({ effects: [closeHoverTooltips, openCard.of(v.state.selection.main.head)] });
    },
  },
  {
    id: 'source.breadcrumbs.focus',
    title: 'Open the breadcrumb of the cursor',
    group: 'Source',
    keys: 'Mod+Alt+B',
    inFields: true,
    aliases: ['siblings', 'navigate', 'path'],
    when: () => sourceHere() && activeFile.value !== null,
    run: () => window.dispatchEvent(new Event('netsmith:crumbs')),
  },
  {
    id: 'source.fold.all',
    title: 'Fold all',
    group: 'Source',
    keys: 'Mod+Alt+[',
    inFields: true,
    aliases: ['collapse', 'fold every block'],
    when: editorOn,
    run: () => {
      if (sourceEditor.view) foldAll(sourceEditor.view);
    },
  },
  {
    id: 'source.fold.unfold',
    title: 'Unfold all',
    group: 'Source',
    keys: 'Mod+Alt+]',
    inFields: true,
    aliases: ['expand', 'unfold every block'],
    when: editorOn,
    run: () => {
      if (sourceEditor.view) unfoldAll(sourceEditor.view);
    },
  },
  {
    id: 'source.zoom.in',
    title: 'Zoom in the editor',
    group: 'Source',
    keys: 'Mod+=',
    alsoKeys: ['Mod++'],
    inFields: true,
    aliases: ['bigger text', 'font size', 'larger'],
    when: sourceHere,
    run: () => void zoomEditor(1),
  },
  {
    id: 'source.zoom.out',
    title: 'Zoom out the editor',
    group: 'Source',
    keys: 'Mod+-',
    inFields: true,
    aliases: ['smaller text', 'font size'],
    when: sourceHere,
    run: () => void zoomEditor(-1),
  },
  {
    id: 'source.zoom.reset',
    title: 'Reset the editor zoom',
    group: 'Source',
    keys: 'Mod+0',
    inFields: true,
    aliases: ['font size', 'default text size'],
    when: sourceHere,
    run: () => void resetEditorZoom(),
  },
  {
    id: 'source.change.next',
    title: 'Next change',
    group: 'Source',
    keys: 'Alt+F5',
    inFields: true,
    aliases: ['next hunk', 'next difference', 'changed since the ref', 'diff', 'compare'],
    when: editorOn,
    run: () => stepSourceChange(1),
  },
  {
    id: 'source.change.prev',
    title: 'Previous change',
    group: 'Source',
    keys: 'Shift+Alt+F5',
    inFields: true,
    aliases: ['previous hunk', 'previous difference', 'changed since the ref', 'diff', 'compare'],
    when: editorOn,
    run: () => stepSourceChange(-1),
  },
  {
    id: 'source.replace',
    title: 'Find and replace',
    group: 'Source',
    keys: 'Mod+H',
    inFields: true,
    aliases: ['search', 'substitute'],
    when: editorOn,
    run: () => {
      const v = sourceEditor.view;
      if (!v) return;
      v.focus();
      openReplace(v);
    },
  },
];

/** The switches, with the title that says what turning each does now. */
const sourceSwitches = (): Command[] => [
  {
    id: 'source.hints.toggle',
    title: inlineHints.value ? 'Source: Hide inline hints' : 'Source: Show inline hints',
    group: 'Source',
    aliases: ['values', 'lcsc', 'part numbers', 'annotations', 'hints'],
    run: () => {
      inlineHints.value = !inlineHints.peek();
    },
  },
  {
    id: 'source.cards.toggle',
    title: hoverCards.value ? 'Source: Hide hover cards' : 'Source: Show hover cards',
    group: 'Source',
    aliases: ['tooltip', 'hover', 'quick info'],
    run: () => {
      hoverCards.value = !hoverCards.peek();
    },
  },
  {
    id: 'source.outline.toggle',
    title: outlineOpen.value ? 'Source: Hide outline' : 'Source: Show outline',
    group: 'Source',
    aliases: ['symbols', 'structure', 'sidebar'],
    run: () => {
      outlineOpen.value = !outlineOpen.peek();
    },
  },
  {
    id: 'source.imports.sort',
    title: 'Source: Sort the imports',
    group: 'Source',
    keys: 'Shift+Alt+O',
    inFields: true,
    aliases: ['organize imports', 'order imports', 'isort', 'std src'],
    when: () => editorOn() && activeFile.value?.endsWith('.nsm') === true,
    run: () => {
      const v = sourceEditor.view;
      if (!v || v.state.readOnly) return;
      const edit = sortImports(v.state.doc.toString());
      if (!edit) {
        say('The imports are sorted already');
        return;
      }
      v.focus();
      v.dispatch({ changes: edit, userEvent: 'input.sort' });
      say('Imports sorted: std, other projects, then src');
    },
  },
  {
    id: 'analysis.rebuild',
    title: 'Rebuild the project now',
    group: 'Build',
    keys: 'Mod+B',
    inFields: true,
    aliases: ['analyse', 'analyze', 'check', 'refresh', 'run again', 'read the files'],
    run: () => {
      void runAnalysisAgain();
    },
  },
  {
    id: 'analysis.stop',
    title: 'Stop the rebuild',
    group: 'Build',
    aliases: ['cancel', 'cancel analysis', 'stop analysis', 'abort'],
    when: () => busy.value && !!pending.value.server,
    run: () => {
      void cancelAnalysis();
    },
  },
  {
    id: 'source.rebuild.toggle',
    title: rebuildOnSave.value ? 'File: Do not rebuild when a file is saved' : 'File: Rebuild when a file is saved',
    group: 'File',
    aliases: ['build on save', 'analyse on save', 'manual build', 'rebuild on save'],
    run: () => {
      rebuildOnSave.value = !rebuildOnSave.peek();
      say(rebuildOnSave.peek() ? 'A saved file is rebuilt at once' : 'A saved file is not rebuilt: use Rebuild (Ctrl+B) when you want the views to catch up');
    },
  },
  {
    id: 'source.sortsave.toggle',
    title: sortImportsOnSave.value ? 'File: Do not sort the imports when saving' : 'File: Sort the imports when saving',
    group: 'File',
    aliases: ['sort imports on save', 'organize imports on save', 'isort'],
    run: () => {
      sortImportsOnSave.value = !sortImportsOnSave.peek();
      say(sortImportsOnSave.peek() ? 'Ctrl+S sorts the imports first (autosave does not)' : 'Saving leaves the imports as they are');
    },
  },
  {
    id: 'source.autosave.toggle',
    title: autosave.value ? 'File: Turn autosave off' : 'File: Turn autosave on',
    group: 'File',
    aliases: ['auto save', 'save automatically', 'save after delay'],
    run: () => {
      autosave.value = !autosave.peek();
      say(autosave.peek() ? 'Autosave on: a file is saved a moment after your last edit' : 'Autosave off: save with Ctrl+S');
    },
  },
  {
    id: 'source.commentfont.next',
    title: `Source: Comment font (now ${commentFontOf(commentFont.value).label.toLowerCase()}; ${COMMENT_FONTS.length} to choose from)`,
    group: 'Source',
    aliases: ['comment style', 'doc comment font', 'italic comments', 'font'],
    run: () => {
      commentFont.value = nextCommentFont(commentFont.peek());
      say(`Comments in the ${commentFontOf(commentFont.peek()).label.toLowerCase()}`);
    },
  },
  {
    id: 'source.vim.toggle',
    title: vimMode.value ? 'Source: Turn Vim keys off' : 'Source: Turn Vim keys on',
    group: 'Source',
    aliases: ['vim', 'vi', 'modal', 'keybindings', 'normal mode'],
    run: () => {
      vimMode.value = !vimMode.peek();
      say(vimMode.peek() ? 'Vim keys on: Esc for normal mode, :w saves' : 'Vim keys off');
    },
  },
  {
    id: 'source.wrap.toggle',
    title: wordWrap.value ? 'Source: Do not wrap long lines' : 'Source: Wrap long lines',
    group: 'Source',
    keys: 'Alt+Z',
    inFields: true,
    aliases: ['word wrap', 'soft wrap'],
    when: sourceHere,
    run: () => {
      wordWrap.value = !wordWrap.peek();
    },
  },
];

let done = false;

/** Register the editor's commands (once). */
export function registerAppCommands() {
  if (done) return;
  done = true;
  const list: Command[] = [];
  SECTIONS.forEach((s, i) => {
    list.push({
      id: `view.${s.id}`,
      title: `Show ${s.label}`,
      group: 'View',
      keys: `${i + 1}`,
      alsoKeys: [`Alt+${i + 1}`],
      run: () => showSection(s.id),
    });
  });
  list.push(
    {
      id: 'nav.back',
      title: 'Back to the view you came from',
      group: 'Go to',
      keys: 'Escape',
      inFields: true,
      // A card, a list, the find panel, the comparison of a file or a menu of the status bar that is open takes Escape first
      // (the ones that are not the editor's say so with `data-esc-closes`: the window's key runs before their own).
      when: () => back.value !== null && focusedSection() === 'source' && !document.querySelector('.sp, .cm-tooltip-autocomplete, [data-testid="hover-card"], [data-testid="crumb-menu"], [data-testid="tab-menu"], [data-esc-closes]'),
      run: () => requestBack(),
    },
    {
      id: 'nav.history.back',
      title: 'Back',
      group: 'Go to',
      keys: 'Alt+Left',
      inFields: true,
      aliases: ['previous location', 'go back', 'history'],
      // The places the cursor was taken to, one step at a time; at the first one, back to the view the
      // click-through came from.
      when: () => focusedSection() === 'source' && (canBack(navHistory.value) || back.value !== null),
      run: () => {
        if (canBack(navHistory.peek())) void navBack();
        else requestBack();
      },
    },
    {
      id: 'nav.history.forward',
      title: 'Forward',
      group: 'Go to',
      keys: 'Alt+Right',
      inFields: true,
      aliases: ['next location', 'go forward', 'history'],
      when: () => focusedSection() === 'source' && canForward(navHistory.value),
      run: () => void navForward(),
    },
    {
      id: 'goto.anything',
      title: 'Go to anything…',
      group: 'Go to',
      keys: 'Mod+P',
      aliases: ['quick open', 'find part', 'search'],
      run: () => openPicker(),
    },
    {
      id: 'palette.open',
      title: 'Show all commands',
      group: 'Go to',
      keys: 'Mod+Shift+P',
      alsoKeys: ['Mod+K'],
      aliases: ['command palette'],
      run: () => openPicker('>'),
    },
    {
      id: 'goto.symbol',
      title: 'Go to symbol…',
      group: 'Go to',
      keys: 'Mod+Shift+O',
      inFields: true,
      aliases: ['symbol', 'outline', 'module', 'definition'],
      // In the editor it lists what the open file defines; elsewhere, the project's modules and definitions.
      run: () => openPicker('@'),
    },
    {
      id: 'goto.symbol.file',
      title: 'Go to symbol in the open file…',
      group: 'Go to',
      aliases: ['outline', 'definition in file'],
      when: () => activeFile.value !== null,
      run: () => openPicker('@', true),
    },
    {
      id: 'goto.symbol.project',
      title: 'Go to module or definition in the project…',
      group: 'Go to',
      aliases: ['module', 'definition', 'library'],
      run: () => openPicker('@', false),
    },
    {
      id: 'goto.net',
      title: 'Go to net…',
      group: 'Go to',
      aliases: ['rail', 'signal'],
      run: () => openPicker('#'),
    },
    {
      id: 'goto.problem',
      title: 'Go to problem…',
      group: 'Go to',
      aliases: ['error', 'warning', 'issue'],
      run: () => openPicker('!'),
    },
    {
      id: 'goto.line',
      title: 'Go to line…',
      group: 'Go to',
      when: () => activeFile.value !== null,
      run: () => openPicker(':'),
    },
    {
      id: 'selection.source',
      title: 'Show the selection in Source',
      group: 'Selection',
      keys: 'Mod+Enter',
      inFields: false,
      aliases: ['declaration', 'jump to source'],
      when: () => !!selection.value && selection.value.kind !== 'net',
      run: () => {
        if (selection.value) showInSource(selection.value);
      },
    },
    {
      id: 'selection.clear',
      title: 'Clear the selection',
      group: 'Selection',
      keys: 'Escape',
      passive: true,
      // A popup of a view that Esc closes (the schematic's flag menu and key list) goes first.
      when: () => selection.value !== null && !document.querySelector('[data-esc-closes]'),
      run: () => setSelection(null),
    },
    {
      id: 'problems.toggle',
      title: 'Toggle Problems',
      group: 'Problems',
      keys: 'Mod+J',
      aliases: ['problems panel', 'errors', 'warnings', 'dock'],
      run: () => toggleDock(),
    },
    {
      id: 'problems.next',
      title: 'Next problem',
      group: 'Problems',
      keys: 'F8',
      inFields: true,
      aliases: ['next error', 'next warning'],
      run: () => stepProblem(1),
    },
    {
      id: 'problems.previous',
      title: 'Previous problem',
      group: 'Problems',
      keys: 'Shift+F8',
      inFields: true,
      aliases: ['previous error', 'previous warning'],
      run: () => stepProblem(-1),
    },
    {
      id: 'problems.thisview',
      title: 'Show problems of this view only',
      group: 'Problems',
      aliases: ['filter problems', 'scope'],
      when: () => !dock.value.open || !filter.value.thisView,
      run: () => {
        filter.value = { ...filter.value, thisView: true };
        setDockOpen(true);
      },
    },
    {
      id: 'problems.wholeproject',
      title: 'Show problems of the whole project',
      group: 'Problems',
      aliases: ['filter problems', 'scope', 'this view only'],
      when: () => filter.value.thisView,
      run: () => {
        filter.value = { ...filter.value, thisView: false };
        setDockOpen(true);
      },
    },
    {
      id: 'file.save',
      title: 'Save the file',
      group: 'File',
      keys: 'Mod+S',
      when: () => activeFile.value !== null,
      run: () => {
        if (activeFile.value) void save(activeFile.value);
      },
    },
    {
      id: 'file.close',
      title: 'Close the file tab',
      group: 'File',
      when: () => activeFile.value !== null,
      run: () => {
        if (activeFile.value) closeFile(activeFile.value);
      },
    },
    {
      id: 'build.kicad',
      title: 'Write KiCad files',
      group: 'Build',
      aliases: ['export', 'schematic', 'netsmith build'],
      when: idle,
      run: () => requestRun('build'),
    },
    {
      id: 'build.fab',
      title: 'Make fab files',
      group: 'Build',
      aliases: ['gerber', 'jlcpcb', 'order', 'netsmith fab'],
      when: idle,
      run: () => requestRun('fab'),
    },
    {
      id: 'pcb.route',
      title: 'Place and route the board',
      group: 'Build',
      aliases: ['router', 'autoroute'],
      when: () => board.value !== null && idle(),
      run: () => {
        revealSection('pcb');
        if (board.value) void routeBoard(board.value);
      },
    },
    {
      id: 'pcb.cancel',
      title: 'Cancel the router',
      group: 'Build',
      when: () => job.value?.task === 'route',
      run: () => void api.cancelRoute(),
    },
    ...pcb3dCommands(),
    ...THEMES.map<Command>((t) => ({
      id: `theme.${t.id}`,
      title: `Theme: ${t.label}`,
      group: 'View',
      aliases: ['colour', 'color', 'appearance', 'dark mode', 'light mode', 'contrast', t.label],
      run: () => chooseTheme(t.id),
    })),
    {
      id: 'theme.toggle',
      title: 'Theme: Toggle light and dark',
      group: 'View',
      aliases: ['colour', 'color', 'appearance', 'dark mode', 'light mode'],
      run: () => chooseTheme(isDark(theme.peek()) ? 'light' : 'dark'),
    },
    {
      id: 'help.shortcuts',
      title: 'Keyboard shortcuts',
      group: 'Help',
      keys: '?',
      // `?` types a ? in a text field and in the editor; F1 opens the list from there too.
      alsoKeys: ['F1'],
      aliases: ['cheat sheet', 'keys', 'help'],
      run: () => toggleCheatSheet(),
    },
  );
  register(...list, ...schematicCommands(), ...sourceCommands());
  // These depend on the window and the project, so they are asked for each time.
  provide(() => {
    const two = layout.value.panes.length === 2 && visible.value.length === 2;
    const toggle: Command = {
      id: 'pane.toggle',
      title: two ? 'Close the second pane' : 'Split right: show a second section',
      group: 'View',
      keys: 'Mod+\\',
      inFields: true,
      aliases: ['split', 'pane', 'side by side'],
      when: () => two || canSplit(windowWidth.value),
      run: () => {
        const l = layout.peek();
        if (l.panes.length === 2 && visible.peek().length === 2) layout.value = closePane(l, 1);
        else if (canSplit(windowWidth.peek())) layout.value = splitRight(l);
      },
    };
    const boards: Command[] = (analysis.value?.boards ?? [])
      .filter((b) => b.name !== board.value)
      .map((b) => ({
        id: `board.${b.name}`,
        title: `Switch to board ${b.name}`,
        group: 'Board',
        aliases: ['board'],
        run: () => chooseBoard(b.name),
      }));
    return [toggle, ...boards, ...sourceSwitches()];
  });
}

// ---- the board in 3D (pcb3d/) ----
// The keys work while the PCB is the pane in use; the palette runs them from anywhere.

const pcbHere = () => focusedSection() === 'pcb';
const in3d = () => pcbMode.value === '3d' && view3d.current !== null;

function pcb3dCommands(): Command[] {
  const act = () => view3d.current!;
  return [
    {
      id: 'pcb.3d.toggle',
      title: 'PCB: Switch between 2D and 3D',
      group: 'PCB',
      keys: 'D',
      aliases: ['3d', 'three dimensions', '2d', 'heights', 'board view'],
      keyFrom: pcbHere,
      when: () => board.value !== null,
      run: () => {
        if (!pcbHere()) {
          revealSection('pcb');
          pcbMode.value = '3d';
        } else togglePcbMode();
      },
    },
    ...PRESETS.map<Command>((p) => ({
      id: `pcb.3d.view.${p.id}`,
      title: `3D view: ${p.id === 'iso' ? 'isometric' : `from the ${p.id}`}`,
      group: 'PCB',
      keys: p.key,
      aliases: ['3d', 'camera', 'view', p.label],
      keyFrom: pcbHere,
      when: in3d,
      run: () => act().preset(p.id),
    })),
    {
      id: 'pcb.3d.flip',
      title: '3D view: Flip the board over',
      group: 'PCB',
      keys: 'Shift+F',
      aliases: ['3d', 'bottom', 'other side', 'turn over'],
      keyFrom: pcbHere,
      when: in3d,
      run: () => act().flip(),
    },
    {
      id: 'pcb.3d.projection',
      title: '3D view: Perspective or orthographic',
      group: 'PCB',
      keys: 'P',
      aliases: ['3d', 'ortho', 'orthographic', 'perspective', 'camera'],
      keyFrom: pcbHere,
      when: in3d,
      run: () => act().toggleProjection(),
    },
    {
      id: 'pcb.3d.fit',
      title: '3D view: Fit the board',
      group: 'PCB',
      aliases: ['3d', 'zoom to fit', 'reset view'],
      when: in3d,
      run: () => act().fit(),
    },
  ];
}

/** Run a command from a button: the same command the palette and the key run. */
export const run = runCommand;

/** The keys that belong to one view and are handled there, for the cheat sheet. */
export const LOCAL_KEYS: { group: string; keys: [string, string][] }[] = [
  {
    group: 'Power tree and System',
    keys: [
      ['Arrow keys', 'move the selection along the rails'],
      ['Enter or Space', 'list the loads of a rail (Power tree)'],
      ['F or 0', 'fit; + − zoom'],
      ['Esc', 'clear the selection'],
    ],
  },
  {
    group: 'PCB',
    keys: [
      ['R · Shift+R', 'rotate the selected parts 90°'],
      ['F', 'flip to the other side'],
      ['L', 'lock or unlock'],
      ['K', 'keepout tool; Esc returns to Select'],
      ['Arrow keys', 'nudge (Shift for a larger step)'],
      ['Ctrl+Z · Ctrl+Shift+Z', 'undo / redo a move'],
      ['Ctrl+A · Delete', 'select all · remove the keepout or release the parts'],
    ],
  },
  {
    group: 'PCB in 3D',
    keys: [
      ['Drag · right-drag · scroll', 'orbit · pan · zoom'],
      ['Click · double-click', 'select the part · show it in the source'],
      ['Arrow keys', 'orbit (Shift to pan)'],
      ['F or 0; + −', 'fit the board; zoom'],
      ['Esc', 'clear the selection'],
    ],
  },
  {
    group: 'Source',
    keys: [
      ['F12 or Ctrl+click', 'go to the definition under the cursor'],
      ['Hover, or Ctrl+Alt+I', 'a card for the part, net, module or type under the pointer or cursor'],
      ['Alt+Left · Alt+Right', 'back and forward through the places the cursor was taken to'],
      ['Ctrl+Shift+O', 'go to a symbol in the open file'],
      ['Ctrl+Alt+[ · Ctrl+Alt+]', 'fold all · unfold all'],
      ['Ctrl+F · Ctrl+G', 'find · find next'],
      ['Ctrl+Z · Ctrl+Shift+Z', 'undo / redo (survives a file switch)'],
      ['Ctrl+/', 'comment or uncomment the selected lines'],
      ['Shift+Alt+O', 'sort the imports: std, other projects, then src'],
      ['Tab · Shift+Tab', 'indent / outdent'],
    ],
  },
  {
    group: 'Comparison of a file',
    keys: [
      ['Esc', 'close the comparison and return to the list or the editor'],
      ['Enter or Space', 'on a bar of unchanged lines: show them'],
      ['Alt+Down', 'next change, from the keyboard of the text'],
    ],
  },
  {
    group: 'Project tree',
    keys: [
      ['↑ ↓ ← →', 'move, open and close folders'],
      ['Enter', 'open the file or go to the definition'],
      ['F2 · Delete', 'rename · remove'],
      ['Ctrl+N · Ctrl+Shift+N', 'new file · new folder'],
    ],
  },
];

/** What a go to anything choice of a part, net or module does: it is selected in every view and
 * brought into view in the one on screen. Where the view in front shows no selection (the source,
 * the build) the source opens at its declaration; elsewhere the source's cursor moves quietly, so
 * it is there when the person looks. */
export function goToSelection(sel: Selection) {
  selectAndReveal(sel);
  const section = focusedSection();
  const shows = section !== undefined && ['schematic', 'power', 'system', 'bom', 'pcb'].includes(section);
  if (sel.kind === 'net') {
    if (!shows) say(`Selected net ${sel.key}. A net has no line in the source; the Schematic shows it.`);
    return;
  }
  showInSource(sel, shows);
}

/** What go to anything's choice of a bus does: the bus is picked (its first net is the shared
 * selection) and the Schematic frames its line. Where that is not the view in front, it says so. */
export function goToBusPick(bus: PickedBus) {
  goToBus(bus);
  if (focusedSection() !== 'schematic') say(`Selected bus ${bus.path.slice(bus.path.lastIndexOf('.') + 1)}. The Schematic shows it.`);
}

// ---- the comparison with a git ref (a block of its own, registered by `registerCompareCommands`) ----

let compareDone = false;

/** Register the commands of the Compare section (once): choose a ref, turn the marks on the views on and off,
 * compare again, stop, and show the changes of the open file in the source. */
export function registerCompareCommands() {
  if (compareDone) return;
  compareDone = true;
  register(
    {
      id: 'compare.open',
      title: 'Compare with a ref…',
      group: 'Compare',
      aliases: ['git', 'branch', 'tag', 'commit', 'what changed', 'since', 'diff', 'base'],
      run: () => askForRef(),
    },
    {
      id: 'compare.refresh',
      title: 'Compare again',
      group: 'Compare',
      aliases: ['refresh the comparison'],
      when: () => refName.value !== null,
      run: () => {
        const ref = refName.peek();
        if (ref) void runCompare(ref);
      },
    },
    {
      id: 'compare.stop',
      title: 'Stop comparing with a ref',
      group: 'Compare',
      aliases: ['clear the comparison', 'forget the ref'],
      when: () => refName.value !== null,
      run: () => {
        clearCompare();
        showChanges.value = false;
      },
    },
    {
      id: 'compare.diff.file',
      title: 'Show changes in source',
      group: 'Compare',
      aliases: ['diff', 'what changed in this file', 'base side', 'file at the ref'],
      when: () => diskCompare.value !== null && activeFile.value !== null,
      run: () => {
        const path = activeFile.peek();
        if (!path) return;
        openDiff({ path, where: 'source' });
        revealSection('source');
      },
    },
  );
  // The open comparison of a file (views/compare/DiffPanel.tsx): the keys work wherever the focus is while it is open, a diff view being a text view too.
  register(
    {
      id: 'compare.diff.next',
      title: 'Next change in the comparison',
      group: 'Compare',
      keys: 'F7',
      inFields: true,
      aliases: ['next hunk', 'next difference', 'diff'],
      when: () => diffTarget.value !== null && (diffControls.value?.changes ?? 0) > 0,
      run: () => diffControls.peek()?.step(1),
    },
    {
      id: 'compare.diff.prev',
      title: 'Previous change in the comparison',
      group: 'Compare',
      keys: 'Shift+F7',
      inFields: true,
      aliases: ['previous hunk', 'previous difference', 'diff'],
      when: () => diffTarget.value !== null && (diffControls.value?.changes ?? 0) > 0,
      run: () => diffControls.peek()?.step(-1),
    },
  );
  provide(() => [
    {
      id: 'compare.diff.unchanged',
      title: diffControls.value?.hiding ? 'Show unchanged lines in the comparison' : 'Hide unchanged lines in the comparison',
      group: 'Compare',
      keys: 'Mod+Alt+H',
      inFields: true,
      aliases: ['collapse unchanged', 'context', 'fold unchanged', 'diff'],
      when: () => diffTarget.value !== null && diffControls.value !== null,
      run: () => diffControls.peek()?.toggleHide(),
    },
  ]);
  // The switch reads as what turning it does next, as the source's switches do.
  provide(() => [
    {
      id: 'compare.marks.toggle',
      title: showChanges.value ? `Hide changes${refText.value ? ` vs ${refText.value}` : ''}` : `Show changes${refText.value ? ` vs ${refText.value}` : ' vs a git ref'}`,
      group: 'Compare',
      aliases: ['marks', 'highlight changes', 'what changed', 'diff', 'outline changed parts'],
      run: () => toggleChanges(),
    },
  ]);
}
