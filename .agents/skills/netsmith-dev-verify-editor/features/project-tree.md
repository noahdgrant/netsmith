# Project tree

## How to get to it

The left column of the Source section (key 7), one real tree (`role=tree`). Sections, each with a collapse chevron and a count: Top level, Modules, Physical files, Project (the manifest) and Standard library (closed). Below a section the folders nest inside their parent, 16 px a level, with a thin guide line for each level; a folder that holds only one folder is one row (`legacy / vendor / archive`). Folders come before files, each by name. A file has a chevron when it defines something; its definitions (board, system, panel, module, part, interface, each with its own icon and the kind written after the name) sit below it. The lock on a file says it is read-only (the manifest, the standard library). A pill is a count of files; a dot after a name is unsaved text. The `?` button opens a legend of the icons.

## Driving it

`scripts/editor-verify drive tree` (fixture `two_boards`):

1. The tree lists the project name, `main.nsm` with `LdoStage`, `PowerBoard`, `LedBoard`, `TwoBoards`, the module `power.nsm` and `netsmith.toml`.
2. Open the Standard library section and click `passive.nsm`: a tab opens with `part Resistor` in it. Typing changes nothing (the file is read-only).
3. Click `LedBoard` in the tree: the source scrolls to `board LedBoard` and the board selector at the top switches to `TwoBoards/led`; the cursor is on the name and fewer than five letters are marked.

### The invented greenhouse project: `launch --fixture tree`, then `drive tree-nested`

A generated project (`scripts/editor-verify-js/fixtures/tree.mjs`) with nested folders (`proposed/superseded`, `sheets/power`), a chain of single folders, a library of parts, 45 modules, a physical file and a manifest. The recipe checks, and its screenshots show:

- the sections, the levels of the rows (`aria-level`), no flat `a/b` row, guide lines on a nested file, folders before files;
- icons: folder, `.nsm`, `.physical.nsm`, `.toml`; the chevrons; rows 26 px tall with 13 px text; the count pills and their tooltips; the lock and its tooltip; the kind tooltip of a definition;
- the active file's 3 px accent bar, the mark on an open file, the dot and its tooltip after typing;
- the keyboard: Up, Down, Home, End, Left, Right, Enter;
- Collapse all and Expand all; the filter keeping the folders above a match, open, with the match marked;
- what is open, kept per project across a reload (`localStorage`, `netsmith.tree.open:<project>`); the column dragged to 380 px and kept (`netsmith.sidebar.width`), a double click back to 260 px;
- a click on a definition: the cursor is on the name (`Ln 17, Col 7` for `board SensorHead`), nothing is selected, fewer than five `.cm-selectionMatch` marks, and the line flashes (`.cm-reveal-a`/`-b`).

It takes screenshots at 1366 x 768 and 2000 x 1000.

### A big project: `drive big-tree` after `launch --fixture big`

Modules are grouped by folder below `src/` (`sheets/power`, `sheets/io`). The filter above the tree keeps the files whose path or definitions match, and opens the folders they are in.

## Gotchas

- A file with a syntax error shows no definitions until it parses again.
- The source scrolls by line number; CodeMirror renders only the visible lines, so a recipe cannot find a distant line with a text locator until it has scrolled there. Click a definition in the tree first.
- A board of a system is named `System/member` in the selector, and the tree's definition name is the board's definition (`LedBoard`); the page maps one to the other.
- The tree only knows the line of a definition, so the editor looks for the name on that line.
- The classes are `ft-` ones (`.ft-row`, `.ft-section`, `.ft-folder`, `.ft-file`, `.ft-def`): the page already has a `.section` and a `.row`.
- The tree's open state is in `localStorage`; each recipe has a fresh browser, so it starts from the defaults.
