# netsmith

## Working in the repo

- Build and test with `cargo test`; lint with `cargo clippy --all-targets`.
- Start a project in a directory with `netsmith init` (ADR 0011, ADR 0015); with a `netsmith.toml`, `build`, `fab` and `editor` need no file.
- Build a board with `cargo run -p netsmith -- build examples/led.nsm --out out`; make the files for a board house with `... -- fab examples/led.nsm --out fab`; edit and see it in a browser with `... -- editor examples/led.nsm` (ADR 0019; the page is committed as a built bundle in `crates/netsmith-editor/web-dist`, so only a change to its sources in `crates/netsmith-editor/web` needs Node 20 or newer and `scripts/web-bundle update`).
- A part number goes into a catalog only with a recorded source (`data/jlcpcb/README.md`).
- Look at one stage with the example programs in `netsmith-lang`, `netsmith-check` and `netsmith-solve`.
- Check that two designs are the same circuit, for example a port against its KiCad original, with `cargo run -p netsmith -- compare <a> <b>` (ADR 0017).
- Check a generated schematic against Windows KiCad from WSL with `scripts/check-kicad.sh`.
- Check catalog rows against LCSC's record with `netsmith catalog refresh --dry-run` (opt-in; ADR 0039; it verifies the manufacturer's number and package and writes only a `checked` date, never a price, stock or library, which are JLCPCB's numbers entered by hand; JLCPCB data is not fetched). The standard catalogs ship no price, stock or date (`data/jlcpcb/README.md`); `netsmith catalog age` lists the rows whose price and stock (`date`) are older than 90 days or have no date.
- Choose boards and quantities for a fab order with a named order in `netsmith.toml` and `netsmith fab --order <name>` (ADR 0035); a key may name a `panel` of identical boards on a V-cut grid (ADR 0040, `examples/led_panel.nsm`).
- Take the outline, cutouts and mounting holes from an enclosure's ASCII DXF with `netsmith dxf import case.dxf --layer EDGE --holes-layer HOLES` (prints source to paste; `--write board.nsm` puts the places in the layout file and prints a diff; ADR 0045).
- Where the project stands and what comes next is in `docs/ROADMAP.md`.
- Parallel work goes in git worktrees under `../netsmith-worktrees/<branch>` (not `/tmp`, which is wiped when the machine restarts), with a build cache in `/tmp/target-<branch>`. Commit at least every 30 minutes: only committed work survives a shutdown.
- On the WSL machine at most 3 agents build at once, with `CARGO_BUILD_JOBS=4` and `nice`; more has crashed WSL.
- Stage files by explicit path when committing, so other people's in-progress files are left out.

## Writing `.nsm`

`docs/LANGUAGE.md` is the language guide: a quick start, one section for each construct with its syntax, a runnable example and the real error you get when you misuse it, and a catalogue of the standard library. A test (`crates/netsmith/tests/guide.rs`) runs its examples, so change the guide with the language.

## Vocabulary and decisions

The vocabulary is in `GLOSSARY.md`; use those terms and avoid the ones listed under _Avoid_. Decisions that are hard to reverse are in `docs/adr/`. The language's design rules are in `README.md`. Language-design choices (syntax, semantics) are the user's: present options and ask.

## Skills

Agent skills are in `.agents/skills/<name>/SKILL.md` (the Agent Skills format: `name` and `description` in the frontmatter). `.claude/skills` is a symlink to it. Agents that scan `.agents/skills` (Codex, Cursor and others) find them there, and Claude Code finds them through the link. Each agent loads a skill's name and description at the start and the body when the description matches the task.

Working on netsmith:

- `netsmith-dev-verify`: the gates, the KiCad checks, worktrees and target directories.
- `netsmith-dev-decision`: when to write an ADR, glossary upkeep, and which choices are the user's.
- `netsmith-dev-add-part`: adding a part, a bias curve or a catalog column with evidence.
- `netsmith-dev-extend-language`: changing the language across the crates, with tests and docs.
- `netsmith-dev-verify-editor`: launch the editor, drive it in a headless browser, and look at the screenshots.

Using netsmith in a project (these assume only the `netsmith` command; `netsmith init` copies them into a new project, ADR 0015):

- `netsmith-design-a-board`: requirements to a fab package.
- `netsmith-write-nsm`: a cheat sheet for `.nsm`.
- `netsmith-write-a-module`: splitting a circuit into modules and interfaces.
- `netsmith-read-errors`: what each checker, solver and fab message means.
- `netsmith-use-constraints`: where parts must be (a case, a carrier) and the findings when they are not.
- `netsmith-prepare-fab-order`: `fab-notes.txt` and the order preview checklist.
- `netsmith-port-a-kicad-board`: porting a KiCad design a sheet at a time.
