---
name: netsmith-dev-verify-editor
description: Launch the netsmith editor, drive it in a real headless browser, take screenshots and look at them. Use when you change the editor (crates/netsmith-editor, crates/netsmith/src/editor) or any view it shows, and before you report that a UI change works.
---

# Verify the editor in a browser

Tests and types do not tell you the page looks right or that a click does what it should. Run the editor, drive it, and **look at the screenshots** (open the PNG files with whatever image viewer your tool has). Do not report a UI change as working until you have.

Entry point: `AGENTS.md`. The tool is `scripts/editor-verify` (a bash wrapper over `scripts/editor-verify-js/`, which holds the Playwright driver and the recipes). It keeps all state in `$NETSMITH_VERIFY_DIR` (default `/tmp/netsmith-verify`), never in the repo.

## Lifecycle

1. **Build**: `cargo build -p netsmith` (embeds the page: the committed `web-dist` bundle, or a fresh build when `web/` changed, which needs Node 20+; the browser tool itself needs Node 20+ too). Set `CARGO_TARGET_DIR` as usual; the tool finds the binary there, or in `$NETSMITH_BIN`.
2. **Doctor**: `scripts/editor-verify doctor`. It checks Node 20+, the binary, the built page, that Playwright imports and that Chromium launches. `doctor --no-browser` skips the last two. Fix what it says first.
3. **Launch**: `scripts/editor-verify launch [--fixture two_boards|led|regulator|four_layer|outline_and_pours|carrier_and_cards|led_panel|catalog_refresh|constrained|big] [--file x.nsm] [--git]`. `--git` (with `two_boards`) makes the project a throwaway git repository with an earlier design committed, for the `compare` recipe (`features/compare.md`). It makes a scratch project (`netsmith init`, your fixture as `src/main.nsm`, one module; `big` is generated, with module files) and starts `netsmith editor` on a free port. It wipes the last session's files, evidence included, but refuses while a session is running: `cleanup` first (a `drive` against the wrong fixture fails every recipe).
4. **Drive**: `scripts/editor-verify drive <recipe>... | all`. Each recipe opens the page in headless Chromium, performs steps, asserts, and saves screenshots in `$NETSMITH_VERIFY_DIR/evidence/`. Console errors and page errors fail a recipe. Exit status is 0 if all passed.
5. **Evidence**: `scripts/editor-verify evidence` lists screenshots and the server log. Read the screenshots and check layout, truncation, colour and what is missing, not only that the assertions passed.
6. **Cleanup**: `scripts/editor-verify cleanup [--keep-evidence]` stops the server and removes the scratch project.

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
