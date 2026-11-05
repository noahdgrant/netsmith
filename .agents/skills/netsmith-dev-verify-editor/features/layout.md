# Sections, panes and the split

## How to get to it

The strip of seven sections under the top bar: Source, Schematic, Power tree, System, BOM, PCB, Build (the keys 1 to 7, plain when the focus is not in a text field, Alt+1 to Alt+7 or Ctrl+1 to Ctrl+7 anywhere). One section fills the window; the status bar is in all of them. The top bar has the board selector and, on the right, apart from the sections, the two actions "Write KiCad files" and "Make fab files".

A window 1400 px wide or more can be split in two panes, each with its own strip: "Split right" at the end of the strip, Ctrl+\ (again to close it), a right click on a tab ("Split: open on the right/left", or in two panes "Show in the left/right pane"), or a drag of a tab onto the left or right half of the window (a highlighted zone shows where it lands; Escape cancels). A tab dragged or chosen into the other pane moves there: the same section is never in both panes, the two swap. The divider between the panes is dragged (each pane keeps at least 380 px), moved with the arrow keys when focused, and a double click makes the panes equal. "Close pane" on the second pane returns to one. A click-through to the source (a part, a load line, a problem) replaces the section of the pane that was clicked, and "Back to ..." in the source's file tabs (or Escape, Alt+Left, the browser's Back) puts it back: it never makes a second pane. Only in a split the person made (Split right, Ctrl+\\, a drag of a tab or of the divider, the tab menu: `layout.chosen`) does it open in the other pane, and then the second pane's strip says once "Opened in a second pane. Close pane returns to one." with a Close pane button and "Got it" (remembered in localStorage; Close pane or Got it ends it). A split saved before `chosen` existed counts as not chosen. The tab whose section is open in the other pane has a small two-pane icon (the filled half is the pane that has it; aria-label and tooltip "Open in the other pane: click to move it here"); the tab is not dimmed.

The window opens on the Schematic, or on the sections last used (the saved layout; Source being first in the strip does not change that). The layout (the sections, the divider, the pane in focus) and the board and the sheet of each board are kept in localStorage. In a window narrower than 1400 px a saved split is not shown (one pane, the one in focus) and is restored when the window is wide enough again; the "Split right" tooltip says so.

## Driving it

`scripts/editor-verify drive sections split overview`

- `sections`: each key opens its section, which fills the width at 1366 x 768 and 2000 x 1000 with no scrollbar on the page (look at the screenshots, `sections-*`); a plain digit typed into the tree filter stays text; Alt+2 works there; the section is kept across a reload.
- `split`: the button, the divider (drag, minimum, arrow key, double click), a tab dragged onto a half of the window (screenshots `dragging-a-tab` and `drop-zone`), the menu, Ctrl+\, a reload, closing the pane and the narrow window.
- A click-through and Back are in `schematic` (Escape) and `power` (a load line opens the source).
- `split` also checks the icon on the tab of the other pane, that a click-through leaves one pane, that a saved split that was not chosen is not opened into, and the one-time notice of a split the person made.

## Gotchas

- The page keeps the sections it has shown in the DOM, hidden (`.section.away`, `visibility: hidden`, so layout, scroll and zoom survive), except the PCB: its keys (arrows, R, F, Delete, Ctrl+Z) work on the whole window, so it leaves the page when it is not shown and finds its zoom again (`memoKey`). The source is always there. A Playwright locator that matches something in two sections is a strict-mode error: scope it (`.sec-power .graph-host`).
- A drag of a tab starts after 8 px of movement and uses pointer events, not HTML drag and drop, so `page.mouse.down/move/up` drives it. A click on a tab right after a drag is swallowed.
- A tab's right-click menu has the first enabled entry focused, so the keyboard (Shift+F10 on a focused tab) reaches the same choices as a drag.
- A toast (bottom right, a few seconds; a failure stays until dismissed) can sit over a canvas and take a click: wait for `.toast` to go before a drag near the bottom right.
- The problems dock (`problems-dock.md`) is under the panes, so it takes height from every section when it is open; a recipe that measures a canvas opens or closes it first, or leaves it closed (the default).
