// The commands of the editor, registered once: the palette, the cheat sheet, the keyboard and the
// buttons all run these. A command that only the window knows how to do (a view's own key) is
// not here; the cheat sheet lists those from `LOCAL_KEYS` below. (The Schematic's keys are
// commands, defined here and run through the handle the view hands out.)

import { foldAll, unfoldAll } from '@codemirror/language';

import { api } from './client';
import { type Command, provide, register, runCommand } from './commands';
import { stepProblem } from './feedback/derive';
import { dock, filter, setDockOpen, toggleDock } from './feedback/state';
import { SECTIONS, canSplit, closePane, splitRight } from './layout';
import { openPicker, toggleCheatSheet } from './quickpick';
import { cardAtCursor } from './source/cm-card';
import { openCard } from './source/cm-card';
import { sourceEditor } from './source/handle';
import { canBack, canForward } from './source/navhistory';
import { hoverCards, inlineHints, outlineOpen, resetEditorZoom, wordWrap, zoomEditor } from './source/prefs';
import { openReplace } from './source/cm-search';
import { type Selection, selectAndReveal, selection, setSelection } from './selection';
import { THEMES, isDark, theme } from './theme';
import { chooseTheme } from './themeUi';
import {
  activeFile,
  analysis,
  back,
  board,
  chooseBoard,
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
  say,
  save,
  showInSource,
  showSection,
  visible,
  windowWidth,
} from './store';
import { PAN_STEP, schematicActions } from './views/schematic-actions';
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

const schematicCommands = (): Command[] => {
  const act = () => schematicActions.current!;
  const pan = (id: string, title: string, key: string, dx: number, dy: number): Command => ({
    id: `schematic.pan.${id}`,
    title,
    group: 'Schematic',
    keys: key,
    aliases: ['scroll', 'move the view'],
    keyFrom: onDrawing,
    when: sheet,
    run: () => act().pan(dx * PAN_STEP, dy * PAN_STEP),
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

const sourceCommands = (): Command[] => [
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
      v.dispatch({ effects: openCard.of(v.state.selection.main.head) });
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
      when: () => back.value !== null && focusedSection() === 'source',
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
      alsoKeys: ['Mod+/'],
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
    group: 'Source',
    keys: [
      ['F12 or Ctrl+click', 'go to the definition under the cursor'],
      ['Hover, or Ctrl+Alt+I', 'a card for the part, net, module or type under the pointer or cursor'],
      ['Alt+Left · Alt+Right', 'back and forward through the places the cursor was taken to'],
      ['Ctrl+Shift+O', 'go to a symbol in the open file'],
      ['Ctrl+Alt+[ · Ctrl+Alt+]', 'fold all · unfold all'],
      ['Ctrl+F · Ctrl+G', 'find · find next'],
      ['Ctrl+Z · Ctrl+Shift+Z', 'undo / redo (survives a file switch)'],
      ['Tab · Shift+Tab', 'indent / outdent'],
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
