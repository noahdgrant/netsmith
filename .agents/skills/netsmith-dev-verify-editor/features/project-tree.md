# Project tree

## How to get to it

The left column of the Source section (key 1), one real tree (`role=tree`). Sections, each with a collapse chevron and a count: Top level, Imported files, Physical files, Project (the manifest) and Standard library (closed). Below a section the folders nest inside their parent, 16 px a level, with a thin guide line for each level; a folder that holds only one folder is one row (`legacy / vendor / archive`). Folders come before files, each by name. A file has a chevron when it defines something; its definitions (board, system, panel, module, part, interface, each with its own icon and the kind written after the name) sit below it. The lock on a file says it is read-only (the manifest, the standard library). A pill is a count of files; a dot after a name is unsaved text. The `?` button opens a legend of the icons.

## Driving it

`scripts/editor-verify drive tree` (fixture `two_boards`):

1. The tree lists the project name, `main.nsm` with `LdoStage`, `PowerBoard`, `LedBoard`, `TwoBoards`, the module `power.nsm` and `netsmith.toml`.
2. Open the Standard library section and click `passive.nsm`: a tab opens with `generic Resistor` in it. Typing changes nothing (the file is read-only).
3. Click `LedBoard` in the tree: the source scrolls to `board LedBoard` and the board selector at the top switches to `TwoBoards/led`; the cursor is on the name and fewer than five letters are marked.

### The invented greenhouse project: `launch --fixture tree`, then `drive tree-nested`

A generated project (`scripts/editor-verify-js/fixtures/tree.mjs`) with nested folders (`proposed/superseded`, `sheets/power`), a chain of single folders, a library of parts, 45 modules, a physical file and a manifest. The recipe checks, and its screenshots show:

- the sections, the levels of the rows (`aria-level`), no flat `a/b` row, guide lines on a nested file, folders before files;
- icons: folder, `.nsm`, `.physical.nsm`, `.toml`; the chevrons; rows 26 px tall with 13 px text; the count pills and their tooltips; the lock and its tooltip; the kind tooltip of a definition;
- the active file's 2 px bar in the selection colour (blue), the mark on an open file, the dot and its tooltip after typing;
- the keyboard: Up, Down, Home, End, Left, Right, Enter;
- Collapse all and Expand all; the filter keeping the folders above a match, open, with the match marked;
- what is open, kept per project across a reload (`localStorage`, `netsmith.tree.open:<project>`); the column dragged to 380 px and kept (`netsmith.sidebar.width`), a double click back to 260 px;
- a click on a definition: the cursor is on the name (`Ln 17, Col 7` for `board SensorHead`), nothing is selected, fewer than five `.cm-selectionMatch` marks, and the line flashes (`.cm-reveal-a`/`-b`).

It takes screenshots at 1366 x 768 and 2000 x 1000.

### Making, renaming and deleting files: `drive tree-files` after `launch --fixture tree`

The header above the tree is `Files` and the project's name with five icon buttons (New file, New folder, Expand all, Collapse all, what the icons mean; tooltips from `data-tip`, `aria-label`s as before), then the filter with its icon; a file with errors or warnings has a count in its row (`.ft-problems`). The header has New file and New folder; a right click on a row (or the Menu key, Shift+F10) opens New file here, New folder here, Rename (F2) and Delete (Del); Ctrl+N makes a file in the selected folder (Ctrl+Shift+N a folder). A name is typed in the tree, in place, like in an editor's explorer: Enter makes it, Esc drops it, a mistake is said under the name (`a b`, `a..b`, a slash, a name that is there in any case, a physical file with no source beside it). `power` becomes `power.nsm`. A new source file offers `Empty / Module / Board / Part` (a name with `board` in it starts as a board; the others as a module); `x.physical` gets `physical for <Board> { case { } }` for the board of `x.nsm` beside it. The new file opens in the Source section and is the selected row. Delete asks first (Cancel has the focus); a top-level file and a folder with files in it also need a box ticked; the manifest and the standard library cannot be renamed or deleted (the menu says why).

The server side (`POST /api/fs/create-file`, `create-dir`, `rename`, `delete`, with the `X-Netsmith` header) works only below the project's `src/`, with module names (letters, digits, `_`), never overwrites (409), asks 428 for a delete that was not confirmed, and refuses `..`, absolute paths, `.git`, `target`, `out`, `fab`, hidden names and links that leave `src/` (403). `tree-files` checks the files on the disk after each step, and that the check ran again.

### A big project: `drive big-tree` after `launch --fixture big`

Imported files are grouped by folder below `src/` (`sheets/power`, `sheets/io`). The filter above the tree keeps the files whose path or definitions match, and opens the folders they are in.

## Gotchas

- A file with a syntax error shows no definitions until it parses again.
- The source scrolls by line number; CodeMirror renders only the visible lines, so a recipe cannot find a distant line with a text locator until it has scrolled there. Click a definition in the tree first.
- A board of a system is named `System/member` in the selector, and the tree's definition name is the board's definition (`LedBoard`); the page maps one to the other.
- The tree only knows the line of a definition, so the editor looks for the name on that line.
- The classes are `ft-` ones (`.ft-row`, `.ft-section`, `.ft-folder`, `.ft-file`, `.ft-def`): the page already has a `.section` and a `.row`.
- The folders with no file in them come from `Project.folders`, which the server lists from the disk.
- A real browser keeps Ctrl+N for itself (a new window) unless the page is an installed app; the toolbar and the menu do the same. Headless Chromium passes the key to the page, which is how the recipe tests it.
- The tree's open state is in `localStorage`; each recipe has a fresh browser, so it starts from the defaults.
