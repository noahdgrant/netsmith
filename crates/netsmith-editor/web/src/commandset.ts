// The commands of the editor, registered once: the palette, the cheat sheet, the keyboard and the
// buttons all run these. A command that only the window knows how to do (a view's own key) is
// not here; the cheat sheet lists those from `LOCAL_KEYS` below.

import { api } from './client';
import { type Command, provide, register, runCommand } from './commands';
import { stepProblem } from './feedback/derive';
import { dock, filter, setDockOpen, toggleDock } from './feedback/state';
import { SECTIONS, canSplit, closePane, splitRight } from './layout';
import { openPicker, toggleCheatSheet } from './quickpick';
import { type Selection, selectAndReveal, selection, setSelection } from './selection';
import {
  activeFile,
  analysis,
  back,
  board,
  chooseBoard,
  closeFile,
  job,
  layout,
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

/** The section of the pane that last had the pointer or the keys. */
export function focusedSection() {
  const vis = visible.peek();
  return (vis.find((p) => p.index === layout.peek().focus) ?? vis[0])?.section;
}

const idle = () => job.value === null;

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
      title: 'Back',
      group: 'Go to',
      keys: 'Alt+Left',
      alsoKeys: ['Escape'],
      inFields: true,
      when: () => back.value !== null && focusedSection() === 'source',
      run: () => requestBack(),
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
      title: 'Go to module or definition…',
      group: 'Go to',
      keys: 'Mod+Shift+O',
      aliases: ['symbol', 'outline'],
      run: () => openPicker('@'),
    },
    {
      id: 'goto.net',
      title: 'Go to net…',
      group: 'Go to',
      aliases: ['rail', 'signal'],
      run: () => openPicker('#'),
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
      when: () => selection.value !== null,
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
  register(...list);
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
    return [toggle, ...boards];
  });
}

/** Run a command from a button: the same command the palette and the key run. */
export const run = runCommand;

/** The keys that belong to one view and are handled there, for the cheat sheet. */
export const LOCAL_KEYS: { group: string; keys: [string, string][] }[] = [
  {
    group: 'Schematic',
    keys: [
      ['← → · [ ]', 'previous / next sheet'],
      ['F or 0', 'fit the sheet'],
      ['+ · −', 'zoom'],
      ['Click a part', 'select it everywhere and open its source (Back returns)'],
      ['Click a wire', 'select the net: its pins are listed and the rest is dimmed'],
      ['Esc', 'clear the selection'],
    ],
  },
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
