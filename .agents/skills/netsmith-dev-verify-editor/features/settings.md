# Settings, the help menu, About, the tab title, the favicon, the splash

## Get to it

- **Settings**: the gear in the top bar (`Ctrl+,`, or `Settings` in the palette). A panel with the theme (the five of `themes.md`), **Density** (Comfortable, Compact), **Size** (90, 100, 110, 125 %) and **Motion** (System, Reduce), and a button back to the defaults. Each choice is also a command: `Density: Compact`, `Size: 125 %`, `Motion: Reduce`, `Settings: back to the defaults`.
- **Help**: the question mark: Keyboard shortcuts, Command palette, Go to anything, `Report a problem: copy the diagnostics`, About netsmith. Below 1000 px the theme and shortcuts buttons give way (the panel and the menu hold both); the gear and the question mark stay.
- Kept in `localStorage` under `netsmith.settings` as one JSON object `{ density, scale, motion }`; `index.html`'s `#settings-boot` applies it before the first paint (`<html data-density data-motion style="--ui-scale">`), and `src/settings.test.ts` runs that script against `parseSettings` and `rootState` for every value and for junk. Storage that is missing or throws gives the defaults, and the toast says the choice is not remembered.

## How it is built

- `settings.ts` is the logic (parse, read, write, `reducedMotion()`), `settingsUi.ts` the choosing with a toast, `settingsCommands.ts` the commands (kept apart from `commandset.ts`), `components/TopMenus.tsx` the two menus (`menus.ts` holds which is open, so the palette can open them), `components/About.tsx` the dialog, `diagnostics.ts` the block that is copied (pure, tested) and `diagnosticsState.ts` what the page gathers for it.
- **Size** is the root font size: `html { font-size: calc(100% * var(--ui-scale)) }`. Every length of `style.css`, `views.css` and the shared scale of `themes.css` is rem or a token; `scripts/editor-verify-js/css-rem.mjs` converts a stylesheet that has gained px (lengths up to 2 px, borders, shadows, radii, the media queries and the rules of the drawings and of the graph cards stay px). A drawing and the cards of the power tree and the system view have a zoom of their own and a size computed in script, so `.panzoom .canvas` sets the scale tokens back to px (`responsive.css`); an icon scales with `--ui-scale` except inside a canvas (`--icon-k`). The source editor's text size is its own (CodeMirror), not this one.
- **Density** is `[data-density="compact"]` in `settings.css`: `--ctl` 24, `--ctl-lg` 28, `--h-*`, `--row`, `--row-lg`, `--row-sm`, `--bar`, `--tab`, `--status`, `--gap` a step smaller. A rule that had a hard-coded height of a control or a row now reads the token, so a new rule must too.
- **Motion** `reduce` is `[data-motion="reduce"]`: the same rule as `prefers-reduced-motion` (every animation and transition 0.001 ms), and `reducedMotion()` for what script decides (the toast's exit, the splash's fade).
- **About** shows what `/api/project` and `/api/analysis` carry: the project's title, the boards, the files, whether KiCad was found, the analysis and its time, the connection, where the editor listens, the page's bundle (the file name of its script, `development` under Vite). The server does **not** report the version of netsmith, the project's folder or where KiCad is: the dialog and the copied block say so (`NOT_REPORTED` in `diagnostics.ts`). When the server sends them, those lines fill in.
- **Report a problem** copies `netsmith editor: diagnostics` with the page, the server, the analysis, the first problems and the last eight errors the page showed (`noteError`: a failure toast, `window.onerror`, an unhandled rejection). It has no path of the project and no file text.
- The tab title is `netsmith - <board>` (the project's title when no board, `netsmith` alone before the project), with a `●` in front while a file has unsaved changes (`title.ts`).
- The favicon is `src/assets/favicon.svg`: a bolder net-anvil whose orange follows the tab strip (`prefers-color-scheme` inside the SVG), with the PNGs of `index.html` as fallbacks. The bundle serves it by its path.
- The splash (`components/Splash.tsx`, `splash.css`) is the mark and `Reading the project…` (then the analysis's own words and clock) over the whole window until `/api/project` has answered; `index.html` holds the same markup inside `#app` so it shows before any script has run. It does not stay when the server never answers.

## Drive

`scripts/editor-verify drive settings` (`two_boards`): the defaults, the panel and Escape, Compact (sizes smaller, kept, a toast), every view in compact and at 125 % (no overflow, no clipped card), 125 % and 90 % (root 20 and 14.4 px), applied before the first paint (`readystatechange`), Reduce (no animation over 1 ms), the palette commands, `Ctrl+,`, the help menu with the arrow keys, About, the diagnostics copy (clipboard), the title and its dot, the favicon, the splash (the project request delayed) and blocked storage.
`drive empty-project` (`launch --fixture empty`): a project with no board says `No boards yet` and names `src/main.nsm`; the button opens it.

## Gotchas

- Anything new that is a size of a control or a row should be a token (`--ctl`, `--row`, ...), and a length in rem, or compact and 125 % will not reach it. Run `css-rem.mjs` on a stylesheet you touched.
- A test that measures pixels reads them after the settings are set, and a rem is `getComputedStyle(documentElement).fontSize / 16` px (`remScale()` in `util.ts`).
- The first-run splash is only visible for as long as `/api/project` takes; a recipe that wants to see it delays the route.
