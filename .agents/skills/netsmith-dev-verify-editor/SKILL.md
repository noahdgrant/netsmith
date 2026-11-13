---
name: netsmith-dev-verify-editor
description: Launch the netsmith editor, drive it in a real headless browser, take screenshots and look at them. Use when you change the editor (crates/netsmith-editor, crates/netsmith/src/editor) or any view it shows, and before you report that a UI change works.
---

# Verify the editor in a browser

Tests and types do not tell you the page looks right or that a click does what it should. Run the editor, drive it, and **look at the screenshots** (open the PNG files with whatever image viewer your tool has). Do not report a UI change as working until you have.

Entry point: `AGENTS.md`. The tool is `scripts/editor-verify` (a bash wrapper over `scripts/editor-verify-js/`, which holds the Playwright driver and the recipes). It keeps all state in `$NETSMITH_VERIFY_DIR` (default `/tmp/netsmith-verify`), never in the repo.

## Lifecycle

1. **Build**: `cargo build -p netsmith-cli` (embeds the page: `build.rs` builds it from `web/` with Node 20+, and without Node the binary has only a placeholder page that `launch` refuses to drive, ADR 0114; the browser tool itself needs Node 20+ too). Set `CARGO_TARGET_DIR` as usual; the tool finds the binary there, or in `$NETSMITH_BIN`.
2. **Doctor**: `scripts/editor-verify doctor`. It checks Node 20+, the binary, that Playwright imports and that Chromium launches. `doctor --no-browser` skips the last two. Fix what it says first.
3. **Launch**: `scripts/editor-verify launch [--fixture two_boards|led|regulator|four_layer|outline_and_pours|carrier_and_cards|led_panel|catalog_refresh|constrained|warn_bench|empty|tri_state|pin_map|model_3d|big] [--file x.nsm] [--git]`. `--git` (with `two_boards`) makes the project a throwaway git repository with an earlier design committed, for the `compare` recipe (`features/compare.md`). It makes a scratch project (`netsmith init`, your fixture as `src/main.nsm`, one module; `big` is generated, with module files) and starts `netsmith editor` on a free port. It wipes the last session's files, evidence included, but refuses while a session is running: `cleanup` first (a `drive` against the wrong fixture fails every recipe).
4. **Drive**: `scripts/editor-verify drive <recipe>... | all`. Each recipe opens the page in headless Chromium, performs steps, asserts, and saves screenshots in `$NETSMITH_VERIFY_DIR/evidence/`. Console errors and page errors fail a recipe. Exit status is 0 if all passed.
   While changing `web/`, run the page through Vite instead of rebuilding the binary: start `node node_modules/vite/bin/vite.js --port 5199` in `crates/netsmith-editor/web` with `NETSMITH_EDITOR` set to the launched editor's URL, then `NETSMITH_PAGE_URL=http://localhost:5199/ scripts/editor-verify drive <recipe>`. A recipe that checks the page the binary serves (`settings`) fails there on purpose; run every recipe against the real binary before you report.
5. **Evidence**: `scripts/editor-verify evidence` lists screenshots and the server log. Read the screenshots and check layout, truncation, colour and what is missing, not only that the assertions passed.
6. **Sweep**: `scripts/editor-verify-all [recipe...]` runs every recipe (or those named) in as many sessions as they need: for each, `cleanup`, `launch` with the right `--fixture` (and `--git`), `drive`, copy the evidence, `cleanup`. It prints each recipe's result and a summary, and exits 1 if any failed. The sessions come from the recipes themselves (`scripts/editor-verify groups` lists them): a recipe's `export const fixture` names its fixture and `export const needs = 'git'` asks for `--git`, so a new recipe needs no change to the script, but it must declare its `fixture`. Each session's screenshots and server log are kept in `$NETSMITH_VERIFY_OUT` (default `/tmp/netsmith-verify-all`), as `launch` wipes the evidence of the one before. CI runs this in the `recipes` job, which installs KiCad 9, as three slices: each drives the recipes `scripts/ci-recipe-slices 3 K` gives it (balanced by the seconds in `scripts/editor-verify-weights`; a new recipe needs no entry, it weighs the table's fallback), after `scripts/ci-recipe-slices 3` has proved that every recipe is in exactly one slice (it runs without a browser, so run it locally too). A slice uploads its folder as the `recipes-evidence-K` artifact when it fails. To rebalance after recipes are added or got slower, regenerate the table from the logs of one run's recipes jobs (the one-liner is in the table's header: `scripts/ci-recipe-slices weights`). No recipe is run twice: a recipe that fails now and then has a race, in the recipe (it asserts before the page has settled: wait on the condition, not a fixed time) or in the page, and is fixed there.
7. **Cleanup**: `scripts/editor-verify cleanup [--keep-evidence]` stops the server and removes the scratch project.

## Driving the 3D view from the command line

With a session running, `netsmith view` (the command an agent uses, `docs/BOARD3D.md`, "Driving a running editor") drives the page that `drive` or a person has open: `$NETSMITH_BIN view --port <port from scripts/editor-verify's state> --view top --highlight <designator> --picture out.png` changes the 3D view and saves the canvas, and `view` alone prints the state. It needs a page open (a recipe has one; from a plain shell nothing is open: it exits 2 with "no editor page is open"). The recipe `pcb-3d-view-cli` runs it against the page; look at the PNG it saves.

## Features

`features/README.md` indexes one page per feature: how to get to it, how to drive it with expected results, and the gotchas. Recipes are in `scripts/editor-verify-js/recipes/<name>.mjs`; to cover a new feature, add a recipe and a page, and run the recipe until it passes and the screenshot is right.

## Rules

- Node: the default `node` here may be 18. Use `source ~/.nvm/nvm.sh && nvm use 22`; the wrapper does it for you if it finds `~/.nvm`.
- Playwright and its Chromium are installed under `~/.cache/ms-playwright`; if `doctor` cannot launch it, run `npx playwright install chromium` in `scripts/editor-verify-js`.
- A recipe that types into the editor changes only the scratch project.
- Do not use `pkill -f` with a pattern that also matches your own command line; use `cleanup`.
- Recipes never hard-code a designator (`C1`, `U7`, `J1`): they find a part by its path in the source and read its designator from the scene, so a change to the letters or numbers does not break them.
- Fixtures other than `two_boards` need `launch --fixture ...` first; recipes `single-board`, `four-layer` and the `big-*` ones say which.
- For a real project, copy it to `/tmp`, run `netsmith editor --port N` in the copy and write a throwaway script that screenshots it: look, never write in the original, never copy anything of it into the repo.
- A fixture's layout file (`x.pcb.toml` beside `x.nsm`) is copied into the scratch project as `src/main.pcb.toml`. The PCB recipes change it, so each starts with `fresh()` from `scripts/editor-verify-js/pcb-helpers.mjs`, which puts it back. That file is a helper, not a recipe: only `recipes/` holds recipes.
- Keep screenshots out of the repository.
