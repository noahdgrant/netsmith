# Editor UI benchmark: what to take from EDA tools and code editors

Status: research, 2026-11-05. Not a decision (ADR 0019 holds the editor's design). Method: official
docs fetched or searched on the dates above, plus a read of `crates/netsmith-editor/web/src` and
`.agents/skills/netsmith-dev-verify-editor/features/*.md`. Nothing was built or run. Evidence
grades: **[doc]** read in the vendor's own documentation, **[forum]** user report, **[src]** read in
netsmith's source, **[unverified]** from memory or an indirect result; do not rely on those.
The doc fetcher summarised long pages, so quotes are from its extracts, not whole pages.

## 1. Verdict

netsmith's editor is already ahead of every code-first EDA tool I could verify on the one thing that
matters most to its audience: the source and the drawing are tied both ways by declaration spans
(click a part, land on its line; put the cursor on a line, the parts light up), with a Back that
returns, and views computed from the model (power tree, system, BOM) that no schematic-first tool
has. What it lacks is the layer every IDE-grade tool puts on top of its views: **one selection
that follows the person across views, a keyboard-first way to reach anything (command palette,
go to part/net/module), one problems list that is always in reach, a shared inspector, and hover
cards in the source.** KiCad and Altium prove the value of net highlight, cross-probe, a net
navigator and a violation list that jumps to the marker; VS Code proves the palette, quick open,
breadcrumbs, peek, F8 and a status bar with a clear left/right rule. All of those fit a model
that is text-first and computed, and most cost days, not weeks. The two things worth building that
nobody does well are a **git-aware compare view** (what changed in the board since a ref: parts,
nets, power budget, BOM) and **module-level wayfinding for big designs** (breadcrumbs plus outline
plus semantic zoom). The things to leave alone are the ones that make Altium heavy: dozens of
panels, modal check dialogs, and mouse-driven editing of a drawing as the truth.

## 2. Products

### Altium Designer
- **Cross-probing is a first-class mode with a global switch.** Ctrl+click jumps and activates the
  other document; "Cross Select Mode" (Shift+Ctrl+X) mirrors selection between the documents;
  zoom/dim behaviour is a preference. Why it works: selection is the one shared object. [doc]
  https://www.altium.com/documentation/altium-designer/cross-probing-selecting-schematics-pcb
- **Navigator panel = the compiled design as a tree**: Documents (sheets, flattened hierarchy, full
  design hierarchy), components with pins, nets with pins. Click locates on the sheet; an
  "Interactive Navigation" button inverts it (click on the sheet, find in the panel); back/forward
  buttons keep a navigation history; Alt+click also locates on the PCB. Why: one place to answer
  "where is this net/part" on a big design, and it has history. The panel is blank until the
  project compiles, which is the failure to avoid. [doc]
  https://www.altium.com/documentation/altium-designer/navigator-panel
- **Four highlight methods, selectable**: Select, Connective Graph (lines drawn between the
  members of a net, green for part navigation, red for net navigation), Zoom, and Dim ("Mask
  Objects", keeps matches at full colour and dims the rest). Why: masking is the cheapest way to
  show fan-out without moving the camera. [doc] same Navigator page.
- **Filter panel with query language** (F12): type a query, a prompt list suggests keywords as you
  type, Apply / Apply to All / Apply to Selected; "Find Similar Objects" with Any/Same per
  attribute generates the expression for you. Why: three entry points (type, build, click an
  example) to one query. [doc] https://www.altium.com/documentation/altium-designer/using-queries ,
  https://www.altium.com/documentation/altium-designer/find-similar-objects-tools
- **Properties panel is context-sensitive**: its content follows the selected document or object,
  edits several objects at once, collapsible regions (General, Location, Parameters, Graphical),
  F11 or double-click opens it. Why: one place for "what is this and what can I change". [doc]
  https://www.altium.com/documentation/altium-designer/properties-panel
- **Violation severity is configured per rule, not hard-coded**: Connection Matrix and Error
  Reporting set each check to No Report / Warning / Error / Fatal; only reported items reach the
  Messages panel; double-click or Cross Probe on a message jumps to the object, only when the
  message has a document. Why: teams tune noise once. Cost: two tabs of matrices most users never
  open. [doc]
  https://www.altium.com/documentation/altium-designer/project-options-error-reporting?version=19
- **Messages panel** lists warnings, errors and status from validation, DRC and simulation in one
  place; selecting a message highlights the responsible object. [doc, thin; column set and
  grouping not verified]
  https://www.altium.com/documentation/altium-designer/messages-panel
- **Draftsman** (drawing documents): not researched. [unverified]

### KiCad 9 (Eeschema and pcbnew)
- **Net highlight with cross-probe**: the Highlight Net tool (hotkey `` ` ``, clear with `~` or Esc)
  colours a net's wires and labels, and with cross-probing on, the same net lights in the PCB
  editor; in pcbnew the net is brighter and everything else dims. Why: one gesture, both views,
  immediate undo with Esc. [doc]
  https://docs.kicad.org/9.0/en/eeschema/eeschema.html ,
  https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html
- **Net Navigator panel** lists every occurrence of the highlighted net (labels, pins, sheet pins)
  grouped by sheet; with a net highlighted, Tab / Shift+Tab step through its items; with none, it
  lists all nets. KiCad 9 made it list all nets. Why: fan-out becomes a list you can walk with the
  keyboard. [doc] same eeschema page; 9.0 notes https://www.kicad.org/blog/2025/02/Version-9.0.0-Released/
- **Search panel** (Ctrl+G) lists symbols, labels, text across the whole schematic (or footprints,
  zones, nets in pcbnew), wildcard and `/regex/` filter, sortable columns, a click selects and pans,
  double-click opens properties. Why: a result list with sheet/position is the cheapest "go to
  anything". [doc] same pages.
- **ERC / DRC dialog**: severity-filter checkboxes with live counts, markers in the drawing (yellow
  arrow warning, red error, green excluded), selecting a row jumps to the marker, right-click
  "exclude with comment" (persists between runs), "change severity", "ignore all of this kind"
  (moves to an Ignored Tests tab); DRC has tabs for violations, unconnected, schematic parity.
  Selecting a clearance violation highlights the minimum-clearance path. Why: waivers are recorded
  with a reason, not deleted. No progress indicator for a long run is described. [doc]
- **Net Inspector (new in 9)**: table with per-net pad count, via count, track length per layer, die
  length; filter, grouping by net class or regex, CSV export; double-click highlights. [doc]
  https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html
- **Selection filter** (lower-left): checkboxes by object type, right-click an entry to select
  only that type, Ctrl/Shift/Ctrl+Shift modifiers; added to the schematic editor in 9. Why: in a
  dense board it removes accidents. [doc]
- **Properties manager** applies edits immediately; numeric fields take expressions
  (`1in + 1mm`); multi-selection shows only shared properties. **Symbol fields table** is a
  spreadsheet of every part's fields with Group By, scope (sheet, subsheets, project), saved
  presets, and row selection cross-probes. [doc] eeschema page.
- **Appearance panel** (layers, objects, nets), saved layer presets and viewports; status bar shows
  cursor, zoom, relative delta, grid, units (Space zeroes the delta). Interactive router has
  push, walkaround and highlight-collision modes and "local ratsnest" for the selected footprint
  only. [doc] pcbnew page. Router status feedback detail not verified.

### atopile (VS Code extension, `.ato`)
Evidence here is weak; I could not read the extension's own documentation.
- The extension adds an atopile sidebar icon, language services, a build button in an `ato` menu
  bar, and an "Open Example" command; its release notes record the 0.14.0 extension UI, a
  migration flow, project selection and file explorer changes. [doc/release notes]
  https://github.com/atopile/atopile/releases ,
  https://atopile.mintlify.app/atopile-0.14.x/quickstart/1-installation
- Build outputs include BOM, fab and assembly data and reports; the layout lives in KiCad (the
  build updates the `.kicad_pcb`), so atopile does **not** own a live layout view. [doc]
  https://raw.githubusercontent.com/atopile/atopile/main/README.md
- A browser workspace exists (no install). https://docs.atopile.io/ [doc]
- A community fork describes an activity-bar panel to build a target, browse parts and nets,
  inspect pinouts, open PCB and 3D viewers, and an inline connection preview with net highlight
  and links to the source declaration and the PCB. Useful as an existence proof of the
  "source declaration link from a net list" idea; it is a third-party fork, not upstream.
  https://github.com/just1jak/atopile [doc, fork]
- Named views (schematic, layout, pinout, BOM, part picker) in the upstream extension, and how
  code and views relate: **[unverified]**. Lesson regardless: the code editor is VS Code's, so
  atopile gets palette, problems, outline and git for free; netsmith has to build those.

### VS Code
- **Palette and quick open are the one front door.** Ctrl+Shift+P runs any command (`?` lists
  them), Ctrl+P opens files and symbols, Ctrl+Shift+O goes to a symbol in the file (`:` groups by
  kind), Ctrl+T in the workspace, repeated Ctrl+P cycles recent files. Why: one input, prefix to
  switch intent, shortcut shown next to the command so the mouse path teaches the key. [doc]
  https://code.visualstudio.com/docs/getstarted/userinterface ,
  https://code.visualstudio.com/docs/getstarted/tips-and-tricks
  (showing shortcuts in the palette: **[unverified]** from these pages, true in use)
- **Layout rules**: Activity Bar chooses a view container, Primary Sidebar shows it, Panel holds
  Problems/Output/Terminal as splittable tabs, Status Bar left = workspace scope and right = active
  file scope, notifications can carry progress, walkthroughs give a multi-step first run. Why: a
  rule about *where* each kind of information lives. [doc]
  https://code.visualstudio.com/api/ux-guidelines/overview
- **Wayfinding in a file**: breadcrumbs show file and symbol path and a click on a segment lists
  siblings; Outline view with error/warning markers; sticky scroll pins enclosing scopes; Peek
  Definition (Alt+F12) and Find All References in an inline editor; F12 and Ctrl+hover preview. [doc]
  https://code.visualstudio.com/docs/editing/editingevolved ,
  https://code.visualstudio.com/docs/getstarted/userinterface
- **Problems panel**: Ctrl+Shift+M opens it, F8 / Shift+F8 cycle problems without leaving the
  editor, counts in the status bar, lightbulb quick fixes. [doc] tips-and-tricks page.
- **Keybinding design**: a searchable Keyboard Shortcuts editor (search by command or by pressing
  the keys), right-click any action to "Configure Keybinding", chords (`ctrl+k ctrl+c`), `when`
  clauses so one key means different things per context, "Show Same Keybindings" for conflicts, a
  troubleshooting log of each keydown. Why: the model stays discoverable as it grows. [doc]
  https://code.visualstudio.com/docs/getstarted/keybindings
- **Theming**: colour themes with automatic light/dark following the OS, separate preferred dark,
  light, high-contrast dark and high-contrast light themes, per-theme overrides, file-icon theme.
  [doc] https://code.visualstudio.com/docs/getstarted/themes
- **Accessibility**: screen-reader support, Alt+F1 help per surface, Accessible View (Alt+F2) to
  read hovers and notifications line by line, audio cues for errors, F6 to cycle workbench areas,
  visible focus ring on every focusable element. [doc]
  https://code.visualstudio.com/docs/editor/accessibility

### Flux.ai
- Schematic and layout are **bidirectionally synchronised in real time**, no netlist or ECO step:
  a change in one is in the other. Why: removes the "update PCB from schematic" ceremony. netsmith
  already has the same property (one source); the lesson is to make it visible in the UI. [doc]
  https://www.flux.ai/docs/getting-started/kicad-to-flux/layout--kicad-to-flux-
- Right-hand **Inspector** with Properties, Assets, Pricing & Availability, Simulation panels; PCB
  rules edited in the inspector; layers panel opened from a button in the canvas corner with an
  eye toggle per layer; a PCB **object tree** by type (components, nets, traces, vias, rules). [doc]
  https://www.flux.ai/docs/reference/reference-object-tree-pcb
- Copilot opens with Alt+C or "Ask Copilot" in an element's context menu (the right-click-to-ask
  entry is the transferable part). [doc] https://www.flux.ai/docs/copilot/overview
- Version history, branching and comments: not verified in the docs I could read. **[unverified]**

### EasyEDA / JLCEDA Pro
- Cross-probe is explicit and small: select a pin, net, or component, run Cross Select (Design
  menu), or Shift+X from the PCB; a selected pad highlights the same-numbered pin of the part in the
  schematic; parts are matched by unique ID. A filter panel decides which object kinds a
  cross-select touches. Why: cheap to learn, one key. Only available when both documents are in
  the same project. [doc]
  https://prodocs.easyeda.com/en/schematic/design-cross-probe/ ,
  https://prodocs.easyeda.com/en/pcb/design-cross-probe/
- Editor layout (navigation tree, DRC panel, properties): **[unverified]**, the overview page 404ed.

### Cadence OrCAD X
- Not researched; no documentation was reachable. **[unverified]** Skip as a source.

### Autodesk Fusion Electronics
- Useful as a negative example: users report ERC rows that jump to a zoomed-out document without
  highlighting the object, and weak schematic/PCB cross-probing; support advice is "run ERC to list
  consistency errors". Lesson: a problem that does not highlight where it is has no value. [forum]
  https://forums.autodesk.com/t5/fusion-electronics-forum/schematic-pcb-sync-deactivated-erc-sync-function-not-working/m-p/10686685

### tscircuit
- Code-first, same shape as netsmith: browser editor with live schematic, PCB and 3D previews, a
  `tsci dev` local server on port 3020, `tsci build`/`export` for SVGs and 3D, and an embeddable
  RunFrame (editor + viewers). The PCB viewer has layer visibility, a bounds tool, a context menu
  on pads (part number, link to the schematic), DRC visualisation, a WebGPU renderer with canvas
  fallback. Net hover highlighting in the schematic viewer was a bounty claim; merge not verified.
  [doc, partly unverified] https://docs.tscircuit.com/llms.txt ,
  https://github.com/tscircuit/pcb-viewer

### Wokwi
- Visual editor and `diagram.json` are two views of one file; the page states which things only the
  JSON can do (45 degree rotation, some boards), a clear "escape hatch" message. Fast keyboard set
  for a canvas: A add, D duplicate, R rotate, F fit, G grid, `?` opens the selected part's docs,
  Escape cancels the gesture, Shift disables snap, Alt for finer grid; clipboard in a JSON that
  can move between projects. [doc]
  https://docs.wokwi.com/guides/diagram-editor

### Figma, Linear, Excalidraw, tldraw
- Zoom-to-fit all = Shift+1, zoom-to-selection = Shift+2 (Figma, Excalidraw, tldraw v1). Layers on
  the left, properties on the right, both follow selection; Figma "Actions" opens on Cmd+K. [forum
  and third-party pages, medium confidence]
  https://www.raycast.com/arturdz/figma-shortcuts ,
  https://deepwiki.com/tldraw/tldraw-v1/10-user-interactions
- tldraw puts a zoom menu bottom-left showing the percentage; useful for a drawing with a fixed
  scale. https://tldraw.dev/examples/ui/custom-zoom-menu [doc]
- Linear: "views are different ways of looking at the same underlying work", the idea behind
  saved filters. The Cmd+K and keyboard-first claims are **[unverified]** (the page did not cover
  them). https://linear.app/docs/conceptual-model
- Minimap and inspector in these canvases: not verified.

### Chrome DevTools and React DevTools
- Selection sync in three places: hover a node in the DOM tree and the element highlights in the
  page; Ctrl/Cmd+Shift+C picks from the page; the selected node drives the Styles and Computed
  panes. Computed shows only what applies, "Show All" opts in. [doc]
  https://developer.chrome.com/docs/devtools/css/reference
- Command Menu modelled on VS Code's: Ctrl+Shift+P runs commands (`>`), delete the prefix to open
  files, `?` lists more, Ctrl+P opens a file directly. [doc]
  https://developer.chrome.com/docs/devtools/command-menu
- React DevTools: inspect-element picker, component search, props/state panel, highlight updates.
  [unverified; the page fetched did not describe them]

## 3. Patterns that matter

"netsmith today" is from a read of the source (file names in brackets) unless marked.

| Pattern | Best at | netsmith today | Impact | Effort |
|---|---|---|---|---|
| Source line <-> drawing, both ways | netsmith itself; KiCad (PCB<->sch) | Yes: click part opens line, cursor line marks parts, Back (Esc, Alt+Left, browser Back) [store.ts, Schematic.tsx] | H | done |
| One selection shared by all views | Altium Cross Select; JLCEDA; KiCad | No. `selected` holds a designator; hover net is local state in Schematic and in Pcb; Power/System keep their own | H | M |
| Net highlight everywhere, with dim | KiCad (`` ` ``, `~`); Altium Mask | Hover only, per view; no sticky net, no dim of the rest in the PCB (it lights one) | H | S-M |
| Net navigator (members as list, Tab to step) | KiCad | Schematic inspector lists a hovered net's pins as chips; no sticky list, no Tab stepping | M | S |
| Command palette | VS Code; Chrome DevTools; Figma | None | H | M |
| Quick open / go to part, net, module | VS Code Ctrl+P/Ctrl+Shift+O; KiCad Search panel (Ctrl+G) | Tree filter in the sidebar only [Sidebar.tsx]; F12 go to definition in the editor | H | M |
| Problems list always reachable, F8 | VS Code; KiCad ERC | Panel only inside Source section; status bar counts jump to it; click goes to source; no filter, no next-problem key, not shown beside a view [Chrome.tsx] | H | S-M |
| Problem waiver with a reason | KiCad "exclude with comment" | None (language-level; see open questions) | M | L |
| Severity as configuration | Altium matrix; KiCad severity editor | Errors / warnings / notes fixed by the compiler | L | L |
| Inspector / properties dock | Altium Properties; Flux; Figma; Chrome Styles | Separate inspector in Schematic; PartInspector in PCB; none in Power/System/BOM [Schematic.tsx, pcb/Panel.tsx] | H | M |
| Hover card / peek in the source | VS Code Peek, Ctrl+hover | None (CodeMirror tooltip styled, unused) | H | M |
| Completion of names | VS Code IntelliSense | None; ADR 0019 lists it as a limit | H | L |
| Breadcrumbs / outline / sticky scope | VS Code; Altium Navigator hierarchy; KiCad Hierarchy Navigator | File tree with definitions per file; module outline inside Schematic; no breadcrumbs, no board > module > part path | H | S |
| Navigation history (back and forward) | Altium Navigator; VS Code Alt+Left/Right | One-step Back after a click-through | M | S |
| Part/net table with group, sort, scope | KiCad Symbol Fields Table; Net Inspector | BOM table (grouped as `bom.csv`), part list in schematic inspector; no net table | M | S-M |
| Selection filter | KiCad; Altium | Not in PCB (selection of parts, keepouts) | M | S |
| Zoom to fit all / to selection, shared keys | Figma/Excalidraw Shift+1/2; Wokwi F | `F` fits, `+ -` zoom [PanZoom.tsx]; no fit-selection; PCB has `focus` on part list | M | S |
| Minimap | VS Code (code); netsmith itself (power, system) | Component exists, used in Power and System, not in Schematic [MiniMap.tsx] | M | S |
| Level of detail on zoom | Maps; no EDA tool verified | Schematic is drawn by module past 16 parts (ADR 0019); no semantic zoom within a module box | M | M |
| Keyboard model discoverable | VS Code (shortcut in palette, rebinding, `when`) | Keys exist (1-7, Ctrl+\, F, R, L, K, `[`/`]`, arrows) with title tooltips; no cheat sheet, no rebinding | M | S |
| Status / progress | VS Code (left/right rule, progress notifications) | Status bar: counts, job + clock, unsaved, cursor, board, live/offline [Chrome.tsx]; cancel for the router only | M | S |
| Empty, loading, error states | VS Code welcome/walkthroughs | One-line `.empty` texts; last good drawing dimmed under a banner on error (good) | M | S |
| Theming light/dark/high contrast | VS Code | Dark only; CSS variables exist, `prefers-color-scheme` not used [style.css] | M | M |
| Accessibility | VS Code | ~94 `aria`/`role` uses, `focus-visible` on tree, divider; drawings are SVG with no text alternative that I found; not audited | M | M |
| Undo / redo | all | Source: CodeMirror history survives file switch; PCB edits: Ctrl+Z / Ctrl+Shift+Z in the PCB view [Canvas.tsx]; nothing for build/fab | M | done |
| Diff / history (git-aware) | none verified; Flux version history unverified | None | H | L |
| Split and docking | VS Code groups; netsmith | Two panes, drag a tab to a half, Ctrl+\, saved layout; no bottom dock | M | S for a bottom dock |
| Onboarding | VS Code walkthroughs; atopile "Open Example" | `netsmith init` writes a working board; the page has hint text only | M | S |
| Saved views / filters | Linear views; KiCad layer presets | Layout and per-board sheet remembered in localStorage | L | M |

## 4. Steal this: top 15, ranked

Ranking = impact for a code-first engineer over effort, then dependence (1 enables 2 to 5).

1. **One selection, everywhere.** Replace `selected` (a designator) with a selection `{kind: part |
   net | module | pin, id}` in the store. Selecting in any view (click, hover pinned with click)
   updates Source (cursor moves to the declaration, as today), Schematic (mark), PCB (select/light),
   BOM (row), Power/System (card). A visible chip in the status bar shows it and Esc clears it.
   Source: Altium Cross Select, JLCEDA Cross Select, KiCad cross-probe. Shape: a store module plus
   a `data-ref`/`data-net` lookup already present in the SVGs. Nets have no declaration line
   (ADR 0019), so a net selection highlights and lists, it does not jump.
2. **Sticky net highlight with dim.** `` ` `` (or `N`) pins the net under the pointer in whichever
   view has focus; every view dims what is not on it and lights it in all views at once; Esc or `~`
   clears. Show a floating "Net VCC: 7 pins, 3 parts" strip with Tab / Shift+Tab to step through
   its pins (KiCad Net Navigator). Where: PanZoom hosts in Schematic and PCB, plus the Power card
   for a rail. Replaces the per-view hover state.
3. **Command palette (Ctrl+K and Ctrl+Shift+P).** One input; prefix `>` commands, none = go to
   anything, `@` module/part, `#` net, `:` line, `?` help (VS Code, Chrome DevTools). Each row
   shows its key. First commands: show section, switch board, build, fab, route, cancel, split,
   toggle theme, next problem, open file, fit, copy a net name. Where: overlay in `App.tsx`,
   command registry in a new `commands.ts` so keys and menus read from one table.
4. **Go to anything (Ctrl+P / Ctrl+Shift+O / Ctrl+G).** Search over files, definitions (already
   in the project tree response), parts, nets, modules of the current board, with wildcard and
   `/regex/` (KiCad Search). Result row shows kind, path and file:line; Enter selects (rule 1) and
   opens the source. Same index feeds the palette. Needs `BoardDetail` (parts, nets) plus the
   tree; no server change.
5. **Problems dock that follows you.** Move the Problems list out of the Source section into a
   bottom dock under every view (VS Code Panel). Add: severity toggles with counts (KiCad), filter
   by file/board, grouping by file, F8 / Shift+F8 next/previous problem from any view (VS Code),
   and a click that also selects the offending part/net in the visible view (fixes the Fusion
   failure). Notes stay closed, as today. The status bar count opens/focuses it.
6. **Right-hand inspector, shared.** One dock (Altium Properties, Figma) that follows the
   selection and replaces the Schematic aside and PCB `PartInspector`: part (kind, value with its
   solved range, package, LCSC, pins and nets, declaration link), net (members, class, length and
   vias on the PCB), module (instances, parts, rails it draws from), finding. Collapsible
   sections, remembered. Read-only first; editing writes `.nsm` and is a language-design matter
   (see questions).
7. **Hover cards and peek in the source.** CodeMirror `hoverTooltip` on a name: part card (value,
   package, picked LCSC, price, pin-to-net list), net card (members), module card. Alt+F12 peek
   of the module definition inline; `Ctrl+hover` underline for F12 (exists). Backed by the
   existing `definition` endpoint plus `BoardDetail`. Highest "this is an IDE" signal per day of
   work.
8. **Breadcrumbs + a navigation stack.** Strip above every view: `project > board > module >
   part` (and `file > definition` in the source), each segment opens its siblings (VS Code). It
   shows where the pane is looking and is the control for module level zoom on big boards.
   Extend today's one-step Back into a history with Alt+Left / Alt+Right (Altium Navigator
   history, VS Code). A KiCad-style Hierarchy Navigator is the same data as a tree in the sidebar.
9. **Net and part tables.** A "Nets" table next to BOM: name, pin count, parts, class, rail
   voltage, and on the PCB tracks length, vias, layers (KiCad Net Inspector); group by net class
   or regex; double-click highlights (rule 2). Give BOM Group By and a scope switch (this
   module / board) like the KiCad Symbol Fields Table.
10. **Light, dark and high-contrast themes.** Tokens already live in `:root`; add a light set,
    `prefers-color-scheme`, a manual toggle in the palette, and per-theme drawing variables for
    the schematic and PCB (the drawing uses its own `--sheet`/`--wire` set). A high-contrast set
    doubles as an accessibility floor (VS Code). Check contrast of net colours in both.
11. **Zoom to selection, minimap in the schematic, semantic zoom.** `Shift+1` fit all, `Shift+2`
    fit selection, `F` kept (Figma/Excalidraw/tldraw/Wokwi). Reuse `MiniMap` for the all-modules
    drawing. Semantic zoom: below a threshold a module box shows only its title, rails and
    port nets; above it shows parts (the sheets code already cuts big modules). Zoom percentage
    readout bottom-left (tldraw).
12. **Status bar by the VS Code rule.** Left = project scope (problems, job, build freshness "built
    12 s ago / stale"); right = active-pane scope (file position, board, selection, live/offline).
    Long jobs become a progress entry with elapsed time and Cancel (today only the router can
    cancel); finishing leaves a clickable result ("fab: 2 notes"). Keep the `routing` clock.
13. **Compare to a git ref.** A "Compare" control (`since HEAD`, branch, tag) that asks the server
    for the board built at that ref and shows: parts added/removed/changed (value, package, pick),
    nets added/removed/changed pins, power tree demand/capacity delta, BOM price delta; source
    gutter marks for changed lines (CodeMirror merge extension). Text-as-truth makes this tractable
    where schematic-first tools struggle. Effort L (server builds at a ref); the gutter marks alone
    are S.
14. **Selection filter and isolate for PCB and schematic.** KiCad's lower-left filter (parts,
    tracks, vias, keepouts, locked) with right-click "only this"; plus `I` to isolate the
    selection and dim everything else (Altium Mask). The PCB already has marquee, lock and
    keepout tools, so the filter is a predicate in `Canvas.tsx` hit-testing.
15. **First run, empty and error states.** A walkthrough card on an empty project (VS Code
    walkthrough, atopile "Open Example"): open the example, change a value, see the schematic
    update, run fab. Every `.empty` says what to do and offers the button. A `?` overlay lists
    the keys, grouped by view (Figma, Wokwi, VS Code reference sheet), generated from the command
    registry in 3.

## 5. Do not copy

- **Altium's panel count and its blank-until-compiled Navigator.** The audience has one source
  and a live check; extra panels cost more than they give. Add docks only along the VS Code
  rule (left = navigate, right = inspect, bottom = problems).
- **A modal ERC/DRC dialog (KiCad, Altium).** It blocks the drawing you are fixing. netsmith's
  check is continuous; keep a docked list.
- **Mouse-driven editing of a drawing as the truth (Wokwi, EasyEDA, Flux, Fusion).** ADR 0003 and
  0019: the language stays text. Pointer edits stay in the layout file (PCB), never in a second
  copy of the circuit.
- **Severity matrices (Altium Connection Matrix).** Two tabs of squares that nobody reads. If
  checks need tuning, that is a language/manifest decision (see questions), not a dialog.
- **KiCad's separate-process cross-probe.** It exists because schematic and PCB are two programs.
  Here they are one page; do not copy the "cross-probe action" settings. One selection, always on.
- **AI chat as the main surface (Flux Copilot, the atopile fork's chat).** Not a UI pattern to
  copy now; right-click "Ask" is cheap but out of scope for this benchmark.
- **Code minimap in the source.** `.nsm` files are short by design (a file past one screen
  becomes a module); a minimap there is noise. Keep minimaps for the drawings.
- **Anything needing the network or an account** (cloud comments, multiplayer, marketplace
  extensions, telemetry). ADR 0019: nothing leaves the machine.
- **A full rebinding editor now.** Ship the registry and the cheat sheet first; rebinding can
  wait until there are enough commands to conflict.
- **A 3D viewer first.** KiCad and tscircuit have one; for this audience the power tree, net
  highlight and compare view pay back sooner. (Later, with the Tauri shell.)

## 6. Open questions for the owner

1. **Palette key.** Ctrl+K (Figma, Linear style) or Ctrl+Shift+P (VS Code)? Ctrl+K conflicts with
   CodeMirror's default delete-line on some keymaps; the browser also takes Ctrl+L, Ctrl+T. Both
   is the usual answer; the choice is what the docs print.
2. **"Sheet" in the UI.** The glossary says avoid sheet/block/circuit for Module, but the
   schematic strip, `sheets.ts`, `SheetBar` and the feature docs use "sheet". Rename the UI words to
   module (and "All modules"), or keep "sheet" for the drawn page?
3. **Waivers.** KiCad's "exclude with comment" is the best-liked ERC feature I found. Whether a
   warning can be accepted with a reason, and where it is written (`.nsm`, `netsmith.toml`, a
   sidecar), is a language/manifest design choice and yours. I would not add it to the editor
   before then.
4. **Editing from the inspector.** Should the inspector ever write `.nsm` (change a value, pick a
   different part)? ADR 0003 allows the editor to write `.nsm`; the question is whether any
   such edit is a feature or stays in the text.
5. **Net identity.** Nets are named only on the board (ADR 0005) and have no declaration span, so
   "select a net" cannot jump to a line. Accept "net selection highlights and lists members, with a
   link to each pin's declaration", or add a span to the model?
6. **Git in the page.** Compare-to-ref needs the server to read git and build at a ref. That adds
   a process dependency (`git`) to a binary that today needs none. Acceptable? Gutter marks only
   as a first step?
7. **Light theme priority.** Is a light theme wanted for printing, screenshots in docs, or users?
   The schematic and PCB colour sets are a larger piece of work than the chrome.
8. **Bottom dock vs. section.** Problems as a dock under every view changes the "seven sections,
   keys 1-7" layout; is Problems a section (key 8) or a dock? Both appear in the benchmark.
9. **Accessibility target.** WCAG AA for the chrome and keyboard reach for the drawings? The
   drawings are SVG with no text alternative; an equivalent list (parts, nets) is the realistic
   route, and the Go to anything list in rule 4 largely is it.
10. **Desktop shell.** If Tauri is coming (ADR 0019 "later"), shortcuts that browsers take
    (Ctrl+T, Ctrl+W, Ctrl+L) become available; do the key choices now assume a browser?

## Appendix: what I could not verify

- atopile's upstream extension views (schematic, layout, pinout, BOM, part picker) and their link
  to `.ato` code; only a third-party fork's description and release notes were readable.
- Flux version history, comments, hierarchical blocks; OrCAD X (nothing reachable); JLCEDA's
  editor overview page; Altium's Messages-panel columns and Draftsman; KiCad's interactive router
  status feedback and 3D viewer UI; React DevTools details; Figma/Excalidraw minimap and inspector;
  Linear's command menu.
- Whether the VS Code palette shows shortcuts is true in use but not stated on the pages read.
- netsmith claims come from reading source and the feature docs, not from running the app; the
  accessibility count (about 94 `aria-`/`role=` occurrences) is a grep, not an audit.
