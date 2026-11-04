# Project tree

## How to get to it

The left column of the Source section (key 7). Sections: top-level files (with the boards, systems and modules defined in each, letter badges B, S, M), modules under `src/`, the manifest (`ro`), and the standard library (collapsed, read-only).

## Driving it

`scripts/editor-verify drive tree`

1. The tree lists the project name, `main.nsm` with `LdoStage`, `PowerBoard`, `LedBoard`, `TwoBoards`, the module `power.nsm` and `netsmith.toml`.
2. Expand "Standard library" and click `passive.nsm`: a tab opens with `part Resistor` in it. Typing changes nothing (the file is read-only).
3. Click `LedBoard` in the tree: the source scrolls to `board LedBoard` and the board selector at the top switches to `TwoBoards/led`.

### A big project: `drive big-tree` after `launch --fixture big`

Modules are grouped by folder below `src/` (`power`, `io`; the files directly in `src` first). A folder whose files define only parts is closed until clicked, with its file count and a `parts` tag. The filter above the tree keeps the files whose path or definitions match, and opens the folders they are in.

## Gotchas

- A file with a syntax error shows no definitions until it parses again.
- The source scrolls by line number; CodeMirror renders only the visible lines, so a recipe cannot find a distant line with a text locator until it has scrolled there. Click a definition in the tree first.
- A board of a system is named `System/member` in the selector, and the tree's definition name is the board's definition (`LedBoard`); the page maps one to the other.
