## Projects and netsmith.toml

A project is a directory with a `netsmith.toml` at its root (ADR 0011). `netsmith project init` makes one in the current directory. It writes `netsmith.toml`, `src/main.nsm` (a working LED board), `.gitignore` (the output folders, and `/.netsmith/`, netsmith's cache of the router's copper, ADR 0138) and `README.md`, and for the agents that work in the project an `AGENTS.md`, a `CLAUDE.md` of the one line `@AGENTS.md` (Claude Code reads `CLAUDE.md`), the skills in `.agents/skills/` (with `.claude/skills` linking to them) and this guide as `docs/netsmith/LANGUAGE.md` (ADR 0015). It never overwrites a file; an `AGENTS.md` or `CLAUDE.md` that exists is left alone and `init` says what to add. It also writes `.agents/netsmith-files.sha256`, the SHA-256 of each agent file it wrote. After a netsmith upgrade, `netsmith project refresh` (in the project or a folder of it) brings those files up to date: it writes the ones that are missing, rewrites the ones that still have a text netsmith wrote (by the record, or by a list of every earlier version for a project made before the record), and leaves the others alone, listing each with a diff in which `-` is your file and `+` is what netsmith writes now; delete such a file and refresh again to take netsmith's text. It deletes nothing: a chapter or skill that a newer netsmith dropped is named and left. A project's `.gitignore`, `README.md` and sources are never touched.

The manifest's `netsmith` key names the release the project is written for, and `init` writes the running one. Another netsmith may give other results for the same project (_The release a project is written for_, below).

```toml
[project]
name = "blink"
top_level = ["src/main.nsm"]
out = "out"
fab = "fab"
kicad = 9
catalog_dirs = ["catalogs"]
```

Only `name` is required.

| Key | Meaning |
|---|---|
| `name` | The project name. |
| `netsmith` | Optional. The netsmith release the project is written for: `"0.9.0"`, or `"0.9"`, which any patch of 0.9 matches; `project init` writes the running one, with its patch. A command that builds or checks the project with another minor release warns of it and refuses nothing (_The release a project is written for_, below). A value that is not two or three whole numbers is refused. netsmith 0.9.0 and earlier read only the three-number form. |
| `revision` | Optional. The revision of the project's boards: text on one line, up to 32 characters (`"A"`, `"1.2"`, a tag), as `netsmith` is a key of this table (issue #1134). The title block of every page of the schematic PDF shows `Rev A` beside the sheet's name (with "Drawn by netsmith ..." still in it), the first line of each board's `fab-notes.txt` is `fab-notes for <board>, revision A; places in mm from the outline's top left corner, y down`, and an order's `order.txt` begins `<project>, revision A`. It is one revision for the project, not one for each board: the boards of a project all say it. No file name carries it (the Gerber zip, `bom.csv` and `cpl.csv` keep theirs), `project init` does not write it, and a manifest without it gives the output it gave before. A value that is empty, over 32 characters or not on one line is refused with the manifest's line. |
| `top_level` | The files that hold boards or systems. `top_level = []` is a project of modules only: `build` and `fab` with no file then say `error: this project has no boards`. |
| `out` | Where `netsmith board build` writes. |
| `fab` | Where `netsmith board fab` writes. |
| `kicad` | The KiCad version of the files. Only 9 is accepted. |
| `catalog_dirs` | The folders whose `*.csv` files are the project's own catalogs, each named by its file name (`catalogs/acme_ldo.csv` is the catalog `acme_ldo`), in addition to the standard ones. Default `["catalogs"]`; `init` writes it out. See Catalog files. |
| `catalogs` | Optional. Single catalog files to add that are not in a `catalog_dirs` folder, each named by its catalog. See Catalog files. |
| `offer_dirs` | The folders whose `*.csv` files are the project's offers, each named for its supplier (`offers/lcsc.csv`, `offers/digikey.csv`, `offers/mouser.csv`), read after the standard offers (ADR 0143). Default `["offers"]`, which is not written out; `[]` reads no folder. A folder may not also be a `catalog_dirs` folder. See Offers files. |
| `assembly` | Optional. Who assembles a part nothing else speaks for: `"fab"`, `"hand"`, `"none"` or `"consigned"`. See Who assembles a part. |
| `environment` | Optional. The environment every board has when it declares none: `"std::env::Environment"`, a definition of the project's own, `"src/vehicle.nsm::Automotive"`, or one of a dependency by its module, `"shared::env::Lab"` (_Dependencies_, below). See The environment of a board. |
| `guidelines` | Optional. What a design that goes against a guideline gets: `"warn"` (the default) reports it and builds, `"error"` refuses to build. `--strict` asks for `"error"` for one run. See Track width against current, and The voltage a power track drops (a guideline of `fab`, which then stops before it writes the package). |
| `lock` | Optional. Which source files get a pick lock (ADR 0100): `"all"`, the default, every file a command reads; `"top_level"` only the files of `top_level`, so a unit test of one part or a scratch file named on the command line has none; `"none"` no file. A source with no lock is not read for one, written one or held to one, and `project lock --update` and `project lock --check` skip it. See _The pick lock_ in the chapter on the shape of a board. |
| `place_seeds` | Optional. How many seeds the placer tries for a board before it gives up: the layout file's seed (0 if it states none) and the ones after it, 8 by default; 1 tries only the file's. It must be from 1 to 32 (issue #458). The seed that places the board is written to the layout file. |
| `place_budget` | Optional. How long the placer may look for a placement of a board before it gives the closest try: seconds (`"120s"`, `"2m"`), tries (`"5tries"`; a try is one run of the placer in a fixed outline, a seed's first run or one of its retries) or both (`"120s+5tries"`), whichever is spent first; no limit by default (issue #764). When it is spent the placer ends the run it is in, begins no other seed, and `fab` and `dev board3d` stop with `placed nothing in 120 s (...)` and what blocks the closest try. A budget of tries gives the same answer on every machine; one of time does not. `--place-budget` wins over it for one run, and `--place-seeds` over `place_seeds`. |
| `track_margin` | Optional. How many times the process's least track a signal is drawn at where there is room: 1.5 by default, so JLCPCB's 0.1 mm least is drawn as 0.15 mm; 1 is the least, and it may be up to 4 (issue #431). The router tries the margin first, in one order of the lines and a quarter of the route time; a board it cannot finish at that width is routed again from the same placement at the least track, and `fab` says so. A net its current makes wider keeps that width. |
| `rip_rounds` | Optional. The rounds of rip-up and re-route of whole nets the router gives a try that leaves connections, 6 by default; 0 turns them off (issue #1088, chapter _The shape of a board_, _Connections left: rounds of rip-up_). `--rip-rounds` wins over it for one run. |
| `length_weight`, `via_weight`, `layer_change_weight` | Optional. The weights of the router's quality objective for the signal nets: what a millimetre of track costs as a share of the router's own (`length_weight`, 1, from 0.1 to 10), what a via costs in mm of track (`via_weight`, 15, from 0 to 1000) and what a change of layer costs beyond its via (`layer_change_weight`, 0, from 0 to 1000). Any of them turns on the router's polish pass, and a weight not stated is the router's own; with none stated the router routes as it always has (issue #1517, issue #1536, chapter _The shape of a board_, _The router's quality objective_). `board fab`, `project order`, `routes route` and the editor's routing all use them, so an order and the editor route the board as `fab` does; the flags `--length-weight`, `--via-weight` and `--layer-change-weight` of `board fab` and `routes route` win over their key for one run. A weight out of its range is an error of `netsmith.toml`. |
| `routing_objective` | Optional. The weight of the placer's objective for routing, from 0 to 10 (issue 1544, chapter _The shape of a board_, _Placement for routing_): the input power chain placed short and in the order its current flows, a part kept beside the pins of a bus it joins, and nothing under a fine-pitch part's escape. 0, the default, leaves it out, so every board places as it always has; 1 is the weight it was measured at. `board fab`, `project order`, `layout` and the other commands that place, and the editor all use it; `--routing-objective` wins over it for one run. A weight out of its range is an error of `netsmith.toml`. |
| `step` | Optional. Whether `fab` writes the board as STEP, `<board>.step`, into the fab package, for mechanical CAD: `true`, the default, writes it; `step = false` leaves it out, and removes the one of an earlier run. `board fab --no-step` leaves it out for one run. See `docs/BOARD3D.md`, "In the fab package". |
| `glb` | Optional. Whether `fab` writes the board's 3D scene as `<board>.glb` into the fab package: `true`, the default, writes it; `glb = false` leaves it out, and removes the one of an earlier run. `board fab --no-glb` leaves it out for one run. See `docs/BOARD3D.md`, "In the fab package". |
| `pdf` | Optional. Whether `fab` writes the board's schematic as `<board>-schematic.pdf` into the fab package, a page for each sheet: `true`, the default, writes it; `pdf = false` leaves it out, and removes the one of an earlier run. `board fab --no-pdf` leaves it out for one run. See chapter 27, "The schematic as a document". |

Beside `[project]`, the file may have `[order.<name>.boards]` tables, with `via_in_pad` in an order's own `[order.<name>]` table (_Orders_, below), a `[supply]` table (the project's supply channels and its supply provider, and in `[supply.parts]` the parts that name a channel of their own, keyed as the pick lock is: _Supply channels_ in the chapter on catalogs), a `[dependencies]` table (_Dependencies_, below), a `[lint]` table (the rules of `netsmith project lint` turned off for the project, `off = ["naming"]`, and `findings = "warn"` or `"error"`, which `--strict` asks for one run: _The linter_, in the chapter on commands), and a model table: a `[models]` table with `folder`, and `[[model]]` rows with `footprint`, `file`, `offset` and `rotation`, the STEP model a part is drawn from when it states none (_A model table_ in the chapter on the shape of a board). Any other table is refused.

### The release a project is written for

Another minor release of netsmith may pick other parts, find other design rules findings on the same copper and write other output, and may change how a board spells something (one spelling for each thing, with no release in which both work, ADR 0001), so a project that built on an earlier netsmith can stop with many errors. So that results of two releases are not mixed without notice:

- Every command that builds or checks a project warns, once in a run, when the running netsmith's major or minor release is not the one the `netsmith` key names: the `board`, `layout`, `routes` and `schematic` commands; `view 3d`, `view pcb`, `view schematic` and `view editor`; `project lock`, `lint`, `fmt` (of the whole project), `order` and `show`; `catalog check`, `dev board3d` and `dev parity`. Nothing is refused, and the exit code is what it would be without the warning.
- A patch release is silent (`"0.9.0"` with netsmith 0.9.3), a value with no patch (`"0.9"`) matches any patch, and a project with no `netsmith` key gets no warning.
- In text it is a `warning:` line on stderr as the command ends, after the errors it may explain. With `--format json`, `board check`, `build`, `explain`, `fab` and `drc` and `project lint`, `lock` and `order` have it as the first of their `findings`, with `severity` `note` and `code` `release`, and say nothing of it on stderr; every other command says it on stderr and leaves its document as it is.
- `netsmith view editor` says it in the terminal as it opens the project, and the page lists it first among its problems, as a warning. `netsmith mcp` says it on its stderr and in the server's instructions as it starts in the project's folder, and the commands it runs do not repeat it, unless the key has changed since.
- No file netsmith writes carries it: `fab-notes.txt`, the design files and the rest of the output are the same with the warning or without.
- A manifest that cannot be read (a key or a table this release does not know, a value it refuses) stops the command with that error and no warning. When its `netsmith` key names another minor release, the error ends with a line that says which release the manifest is written for and which netsmith is running. With no key, with this minor release, or in a file that is not TOML, the error says nothing of the release.

The warning of a project written for an older release points to `docs/MIGRATING.md` in netsmith's repository, which lists every change that makes source an error, release by release, with the old and the new spelling (ADR 0134). To move a project, check it with the new release and, when it builds, set the key to that release. The warning of a project written for a newer one asks for that release. A project with no `netsmith` key whose `board check`, `build` or `fab` stops because a board failed its check ends with a note that it was made before the key, which points to the same file.

#### Running the project's release

Releases install side by side, each in a folder of its own, and a project is run with the one it is written for by that program's path. From the source of a tag, which needs Rust (and Node 20 or newer for the editor's page):

```sh
cargo install --git https://github.com/noahdgrant/netsmith --tag v0.9.0 --root ~/netsmith-0.9.0 netsmith-cli --locked
~/netsmith-0.9.0/bin/netsmith --version
~/netsmith-0.9.0/bin/netsmith board check
```

`--root` puts the program in `bin/` below that folder and leaves the netsmith already on your `PATH` as it is. Or take the archive of the release from `https://github.com/noahdgrant/netsmith/releases/tag/v0.9.0`, with no Rust: `netsmith-0.9.0-x86_64-unknown-linux-gnu.tar.gz`, `netsmith-0.9.0-aarch64-apple-darwin.tar.gz` or `netsmith-0.9.0-x86_64-pc-windows-msvc.zip`, each with its `.sha256` file (v0.1.0 and v0.3.0 have no archives). Check it and unpack it; the program is in the folder the archive holds:

```sh
sha256sum -c netsmith-0.9.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf netsmith-0.9.0-x86_64-unknown-linux-gnu.tar.gz
./netsmith-0.9.0-x86_64-unknown-linux-gnu/netsmith --version
```

(`shasum -a 256 -c` on macOS.) netsmith has no switch that chooses a release for you. An agent host takes that release's program as its server (`claude mcp add netsmith -- ~/netsmith-0.9.0/bin/netsmith mcp`), and `project refresh` writes the agent files of the netsmith that runs it. A release earlier than the one that added the warning says nothing of the key on a run that succeeds, and a release earlier than the one that added the line after a manifest's error says nothing of it there.

### Orders

An order says which boards go into one fabrication order and how many of each (ADR 0035). It is a table `[order.<name>.boards]` beside `[project]`: each key is a board as its output folder is named (`Board`, or `System/member`), or a system's name for all its members, and the value is the quantity.

```toml
[project]
name = "product"
top_level = ["src/devkit/devkit.nsm", "src/unit/unit.nsm"]

[order.devkit.boards]
"DevKit/carrier" = 2
"DevKit/power_card" = 1
Unit = 10

[order.spares.boards]
DevKit = 5
```

`netsmith project order` lists the orders. `netsmith project order devkit` makes only those boards, in `fab/orders/devkit/<board>/`, and writes `order.txt` (each board's layers, size, thickness, hand-solder parts and notes, the combined bill of materials, the parts you buy by supply channel, the unverified items) and `order-bom.csv` (each LCSC number the board house buys, with copies x parts per board, no spares). The parts you buy yourself, consigned or soldered by hand (_Supply channels_ in the chapter on catalogs), are listed under _parts you buy, by supply channel_: under each channel, each sku with the quantity of the whole order, what the part is, who places it and the boards it is on. With a `[supply]` provider (_Catalog files_), `order.txt` also prices each line, the board house's and yours, from the price ladder of the channel that buys it at the quantity of the **whole order** (where the provider quotes the part in no such channel, the stock it answers first, which the line names): a part used on several boards, or on a panel (all its cells, times the panels ordered), gets the price break the total reaches, not the break of one board. A quantity below the provider's first break is shown at the first break's price with "the price is for N pieces or more"; a part the provider has no quote for is "no known price" and left out of the total, which says how many lines it leaves out. With no provider `order.txt` says nothing of price, and `order-bom.csv` never has one. A plain `netsmith board fab` still makes every board. See `docs/PROJECT-LAYOUT.md`.

- An order name is letters, digits, `-` and `_`. A quantity is a whole number of 1 or more.
- A board name that is not in the project, or a board named twice (`DevKit` and `"DevKit/carrier"` in one order), is an error with the manifest line, listing the boards that exist.
- An order takes the package of a board from `fab/<board>/` instead of making the board again when a plain `netsmith board fab` made it from the same inputs and no file of it has changed since (issue #841). A plain `fab` writes a hidden `.fab-record.json` into each board's folder with a fingerprint of everything the package is made from (the netsmith executable, the built design, the parts as sourced, the footprints, the layout and routes files, the STEP and case models, the pin map and the options that change what is written) and of every file of the package. The order reuses the package only when its own fingerprint is the same and the folder holds exactly those files: a changed source, catalog, layout or option, a rebuilt netsmith, or a file edited, added or removed by hand makes the board again, and `fab` says which on a line of its progress. `netsmith project order` ends with a line such as `reused 2 of 3 boards from a plain fab; 11 stages run`: the stages are the resolve, place, route and package steps that were run, and a reused board runs none. A board of a system and a panel are always made again, and a board whose pins `--pins auto` moved leaves no record.
- A board with vias in pads stops the order (issue #1118). The board house fills and caps such a via, an option of its order form that costs extra, so the order needs a yes first: `via_in_pad = "accepted"` in the order's own table, or `netsmith project order devkit --accept-via-in-pad` for one run. Until then the order makes the packages, names each board with its count of vias in pads and the parts that carry them, writes no `order.txt` and ends in an error. With the cost accepted, `order.txt` says under each board `3 vias in pads: the board house's filled and capped option is needed (extra cost): U1, U4`. A board that forbids vias in pads (`.process.via_in_pad &= 0mm`, chapter 21) has none.

```toml
[order.devkit]
via_in_pad = "accepted"

[order.devkit.boards]
Unit = 10
```

- A key may be a panel (ADR 0040), and its quantity then counts panels: `LedPanel = 3` orders three panels, so 18 boards if the panel holds six. `order.txt` says so, and `order-bom.csv` multiplies the panel's parts by the quantity. A board may be ordered loose and in a panel; the two are summed in the bill of materials.

Rules.

- The commands look for `netsmith.toml` in the current directory and then in each parent.
- With no file argument, `netsmith board build`, `board fab` and `view editor` act on `top_level`. With a file they act on that file and still see the project's modules.
- Every other file under `src/` is a module: `src/parts/status.nsm` is `src::parts::status`.
- Paths in the manifest stay inside the project, and a catalog file is named for its catalog. An unknown key is refused with its line.
- A file name that cannot be written in an import is refused, naming the file.
- A `.csv` file in a `catalog_dirs` folder that is not named like a catalog is refused, naming the file.
- A `.csv` file in an `offer_dirs` folder that is not named for a supplier (`lcsc.csv`, `digikey.csv`, `mouser.csv`) is refused, naming the file; a `jlcpcb.csv` is refused with how to merge it into `lcsc.csv` (its classes are the `class` of the `lcsc` offers, ADR 0143).

Misuse.

```text
$ netsmith board build        # netsmith.toml has `topl = [...]`
/path/to/project: netsmith.toml: TOML parse error at line 3, column 1
  |
3 | topl = ["src/main.nsm"]
  | ^^^^
unknown field `topl`, expected one of `name`, `top_level`, `out`, `fab`, `kicad`, `catalog_dirs`, `catalogs`
```

```text
$ netsmith project order devkit        # "Unitt" is not a board of the project
netsmith.toml:8: order `devkit` names `Unitt`, but the project has no such board, panel or system; the boards and panels are: DevKit/carrier, DevKit/power_card, Unit
```

### Dependencies

A project can use the modules and catalogs of another project: circuits that several products share, or the catalogs of a team (issue #68, ADR 0136, proposed). The other project is named in a `[dependencies]` table beside `[project]`, by its folder, the one that holds its `netsmith.toml`, or by a git repository (_Dependencies by git_, below):

```toml
[project]
name = "unit"
top_level = ["src/unit.nsm"]

[dependencies]
shared = { path = "../shared" }
```

```text
work/
├── shared/                 a project of modules only (top_level = [])
│   ├── netsmith.toml
│   ├── catalogs/acme_ldo.csv
│   └── src/
│       ├── power.nsm       module LdoStage, which imports src::parts::ldo
│       └── parts/ldo.nsm
└── unit/
    ├── netsmith.toml       [dependencies] shared = { path = "../shared" }
    └── src/unit.nsm        import shared::power::LdoStage
```

The key of the entry is the first step of an import of the other project's modules, as `src` is of the project's own: `../shared/src/power.nsm` is the module `shared::power`, and `import shared::power::LdoStage` reads it. A file of the dependency that imports `src::parts::ldo` means its own `src/parts/ldo.nsm`, so the shared project builds the same alone and from `unit`. The dependency's catalog files (its `catalog_dirs` folders and its `catalogs` list) are read with the project's own, and a message names them from the project's root, `../shared/catalogs/acme_ldo.csv`. So is its `kicad.toml`: its `[[part]]` rows and its `[[library]]` symbol and footprint libraries (paths relative to the dependency's folder) are added to the project's. The editor lists the dependency's modules, read-only, in a _Dependencies_ section of the file tree, with their outline, and builds again when a dependency's manifest, modules or catalogs change.

- The key is letters, digits and `_`, not starting with a digit, and not `src` or `std`. It need not be the other project's `name`.
- `path` is relative to the project's root and may go up (`../shared`); it is not absolute, and the folder must hold a `netsmith.toml`: `` netsmith.toml: [dependencies] shared: ../shared is not a project: it has no netsmith.toml ``.
- A dependency's own dependencies are read too, under their keys, and every key names one project: two folders under one key are refused, naming both, and a project that depends on itself through others is refused with the chain (`app -> shared -> app`).
- A catalog that the project and a dependency both hold is refused naming both files, as two files of one catalog in one project are. A (part, package) row or a library name that the `kicad.toml` of the project and of a dependency both have is refused naming both files, as one file that has it twice is.
- A dependency gives its modules, its catalogs and its `kicad.toml`. Its boards are not built, and its `preferred.csv`, orders and `[supply]` are not read.
- The manifest's `environment` may name a definition of a dependency by its module path, `environment = "shared::env::Lab"`: the key of a direct dependency, then the module and the definition.

#### Dependencies by git

A dependency may be a git repository whose top folder is the project, at a `rev` (a branch, a tag or a commit) or at the newest tag that a `version` requirement admits (issue #1012, ADR 0137, proposed):

```toml
[project]
name = "unit"
top_level = ["src/unit.nsm"]

[dependencies]
shared = { git = "https://example.com/acme/shared.git", rev = "main" }
radio = { git = "git@example.com:acme/radio.git", version = "1.2" }
```

- `git` is a URL (`https://...`, `ssh://...`, `git@host:path`, `file:///...`) or an absolute path; a folder given by a relative path is a `path` dependency. netsmith runs the `git` command to fetch it, so git must be installed.
- `rev` or `version`, one of them. A `version` is a requirement in cargo's syntax: `"1.2"` admits 1.2.0 up to, not including, 2.0.0, `"=1.2.3"` that one only, `">=2"` any from 2.0.0. The tags read as versions are `v1.2.3` and `1.2.3`; there is no registry.
- The repository is fetched into a cache shared by every project of the machine: `git/` in the folder `NETSMITH_CACHE_DIR` names, else `$XDG_CACHE_HOME/netsmith`, else `~/.cache/netsmith`. A build fetches only what the cache cannot answer, so a branch read once stays where it was until `netsmith project lock --update`. With `--offline`, a dependency the cache does not hold is an error that says so.
- A message names a file of a dependency by git by its key and commit: `shared@0123456789ab/src/power.nsm`.
- A dependency by git may have dependencies of its own, by git, or by a `path` inside its repository.

The commit each dependency by git was read at is kept in `dependencies.lock.toml`, beside `netsmith.toml`, to commit with the project:

```toml
# The commit of each dependency by git (issue #1012). `netsmith project lock --update` rewrites it.
version = 1

[[dependency]]
name = "shared"
git = "https://example.com/acme/shared.git"
rev = "main"
commit = "0123456789abcdef0123456789abcdef01234567"
```

`build` and `fab` write it when it does not exist, with every dependency by git of the project and of its dependencies, and never rewrite it. While it holds an entry for what the manifest asks (the same `git` and the same `rev` or `version`), a build reads that commit, from the cache with no fetch, so a moved branch or a new tag changes nothing. `netsmith project lock --update` fetches each dependency by git again, takes the commit its `rev` or `version` names now, rewrites the lock and prints each entry that changed. `netsmith project lock --check` fails when the lock lacks an entry for a dependency by git or holds one no manifest asks for; it fetches nothing, so it runs in CI with no network.

### Preferred parts

A project says which rows it takes first for a generic part in `preferred.csv`, beside `netsmith.toml` (ADR 0047, issue #802). Each line names a part by its `mpn`, with its `manufacturer` when two parts share the number (ADR 0143); `note` is free text, for why. The file is optional, and a project without one resolves as before.

```csv file=preferred.csv
mpn,manufacturer,note
0402WGF1002TCE,,the 10k 0402 reel on our shelf
0603WAF1002T5E,,
```

When the bill of materials or `fab` resolves a generic part to a row of its catalog (_Generic parts_ in the chapter on catalogs), the rows that fit and that the file names come first, in the order of the file; the other rows that fit follow as without it, in the order of the project's supply channels, each channel's by its own rule (_Supply channels_ in the chapter on catalogs: JLCPCB Basic before Preferred before Extended in `jlcpcb-assembly`), then the order of the catalog file. A listed row that a supply provider says has no stock is passed over, as any row is. A line names a row as a part's identity does: by its `mpn`, and its `manufacturer` when the line and the row both name one. A part is listed once: two lines of one `mpn` from two manufacturers are two parts, and a line that names a manufacturer must have an `mpn`.

- The file changes only resolution. The solver picks from a catalog's rows in the order of the file as before, so a design's picks and its pick lock are the same with or without it.
- A part the file names that no catalog has is not an error: it never fits, so it changes nothing.
- A column other than `mpn`, `manufacturer` and `note`, a header with no `mpn`, a line that names no part and a part named twice are refused, with the file and line: `` preferred.csv:3: `0402WGF1002TCE` is listed twice; a part is listed once ``.
- An `lcsc` column, which named a part by a supplier's number before the number was an offer, is refused at the header, and the message gives the `mpn` of each number an offer has: `` preferred.csv:1: the column `lcsc` has gone from a preferred-parts file (ADR 0143): ...; line 2: `C25744` is `0402WGF1002TCE` (data/offers/lcsc.csv:234) ``.
- The editor builds again when the file is written or changed.

### A part of your own, with no KiCad installed

A part you define needs a footprint, and `kicad.toml` says which (`Library:Name` of a KiCad library, or one of your own). netsmith needs no KiCad install to build and fab a board (ADR 0097), so a part of your own can have a footprint with none either, in two ways. The part and the board are the same in both:

```nsm-fragment
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

part Sensor {
    pins {
        .vdd passive "VDD" "1"
        .out passive "OUT" "2"
        .gnd passive "GND" "3"
    }
}

board Probe {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .s: Sensor {
        .assembly &= AssemblyMode::Hand
    }
    .j: Header1x2
    .s.vdd ~ .j.p1
    .s.gnd ~ .j.p2
}
```

The part names no symbol in either row below, so it is drawn from its pin table, as an IC of the standard library is.

**Your own footprint file.** Draw the footprint in a `.kicad_mod` file, keep it in a `.pretty` folder of the project and list the folder as a `[[library]]`; the row names the footprint as `Library:Name`. This needs the file and nothing else:

```text
(footprint "Sensor" (layer "F.Cu")
  (attr smd)
  (pad "1" smd rect (at -1.5 -0.95) (size 1.2 0.7) (layers "F.Cu" "F.Mask" "F.Paste"))
  (pad "2" smd rect (at -1.5 0.95) (size 1.2 0.7) (layers "F.Cu" "F.Mask" "F.Paste"))
  (pad "3" smd rect (at 1.5 0) (size 1.2 0.7) (layers "F.Cu" "F.Mask" "F.Paste"))
  (fp_rect (start -2.5 -1.6) (end 2.5 1.6) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd"))
)
```

```toml
[[library]]
name = "own"
footprints = "libs/own.pretty"

[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "own:Sensor"
```

The file is `libs/own.pretty/Sensor.kicad_mod`. `build` and `fab` write the library name into the `fp-lib-table` beside the board, so KiCad finds the footprint. The board file names each footprint's `Reference` and `Value` whether the library file has those properties or not (a keep-out zone of the footprint belongs to its part by the designator).

**One part with a footprint of its own.** A `[[part]]` row maps every part of a kind in a package. A `[[member]]` row gives one part, named by its member path on its board (as the layout file names it), another footprint, in either way above. A port uses it to draw a part with the original's land pattern where that is not the standard row's: a project may not map a kind and package that `std/kicad.toml` maps, and an original may draw two parts of one kind and package with two land patterns. The symbol and the pin names stay those of the part's row, and the footprint is checked against the pin table as any other is. Here the sensor `.s` is drawn with the generator's SOT-23 instead of `own:Sensor`:

```toml
[[member]]
path = "s"
footprint = "gen:SOT23(pins=3)"
```

The board file, the bill of materials and the placement file name the member's footprint. A path given twice is refused. The rows are the project's, and its files may hold several boards, so a row whose path is no part of the board being built is not used there: check the path when you rename a member (a port's test compares every footprint with the original's, which finds a stale row).

**A footprint checked against the datasheet's table.** A footprint drawn by hand, copied from a vendor's file or taken from a library can be the wrong one: a 0.65 mm pitch where the package has 0.635 mm, a pad too few, a body of the next size. A row (or a `[[member]]` row) states the numbers of the datasheet's package drawing with `table`, and `build` and `fab` measure its footprint against them and stop on a disagreement, as they do on one with the pin table (issue #1281):

```toml
[[part]]
kind = "Driver"
package = "SOIC-8"
default = true
footprint = "own:Driver"
table = "pads=8, pitch=1.27mm, body=3.9x4.9mm"
```

The text is written as the parameters of a `gen:` footprint are, and has three columns, each optional, at least one given:

| Column | What the footprint must have |
|---|---|
| `pads` | That many pads, counted by number: the pieces of an exposed pad that share a number are one, and an unnumbered pad (a paste opening) is none. A 48-pin QFN with an exposed pad has `pads=49`. |
| `pitch` | The least distance between the centres of two pads of different numbers in one row or one column (the same y, or the same x), to 0.005 mm, so 0.635 mm and 0.65 mm are told apart. A SOT-23-3, whose two pads on one side are 1.9 mm apart, has `pitch=1.9mm`. |
| `body` | The body the footprint draws on its fabrication layer (`F.Fab`), across (x in the footprint's frame) and then along (y): `3.9x4.9mm`, matched to 0.05 mm each way, or a size each, `6.9-7.1x6.9-7.1mm`, matched as it stands. A footprint that draws nothing on `F.Fab` has no body to measure, and a `body` column is refused for it. |

Each disagreement names the part, the footprint, the row's table and line, and both numbers:

```text
error: B: U1 (Driver, package "SOIC-8"): the footprint `own:Driver` has its pads 1.25mm apart, but the table `pads=8, pitch=1.27mm, body=3.9x4.9mm` of kicad.toml line 5 says a pitch of 1.27mm; the pitch is the least distance between the centres of two pads in one row or column
```

A `[[member]]` row's footprint is checked against the member row's own `table`, never the (kind, package) row's. A column that is not one of the three, or a value that is not a count, a length or two sizes, is refused when the file is read, with the row and the line.

**A footprint from a vendor's file.** `netsmith import footprint FILE` writes the land pattern of a vendor's reference board or of a distributor's KiCad export into a footprint library of the project, as a `.kicad_mod` file (issue #1424). FILE is a KiCad board (`.kicad_pcb`), whose part `--ref REF` names, a footprint file (`.kicad_mod`), a `.pretty` folder, an Eagle board (`.brd`), whose part `--ref REF` names, or an Eagle library (`.lbr`). A folder or a library of several footprints takes `--footprint NAME`, the one to import (a folder's `.kicad_mod` file without its extension, a library's package by its name). The footprint keeps what it draws (its pads, courtyard, silkscreen, fabrication layer and 3D model) and loses what the board adds: its place and angle on the board are taken off its pads and texts, a footprint on the bottom face is turned back to the top (_A footprint on the bottom face_, below), and the nets of the pads, the ids, the links to the schematic and the symbol's own fields are left out. The reference becomes `REF**` and the value the footprint's name. `--name LIB:NAME` gives the library and the footprint's name, `--name NAME` the name alone, in the one footprint library `kicad.toml` lists; with no `--name` the footprint keeps the name the file gives it. A library that `kicad.toml` does not list is the folder `libs/LIB.pretty`, and the command prints its `[[library]]` stanza. It also prints a draft row, whose `table` is to be filled from the datasheet's package drawing, never from the footprint itself, since the point of the table is to catch a footprint that does not match the datasheet:

```text
$ netsmith import footprint vendor/eval.kicad_pcb --ref U3 --name own:Driver
wrote libs/own.pretty/Driver.kicad_mod (8 pads, turned from the bottom face to the top)
the file names it `SOIC-8_3.9x4.9mm_P1.27mm`; it is `Driver` here

add to kicad.toml:

[[library]]
name = "own"
footprints = "libs/own.pretty"

[[part]]
kind = "TODO"
package = "TODO"
default = true
footprint = "own:Driver"
table = "pads=8, pitch=TODO, body=TODO"
```

A footprint file that is already there is not written over, and a footprint netsmith cannot read back is not written at all.

An Eagle file (issue #1430) gives a package as its library draws it: a board keeps each part's package that way whatever its place, angle or face, so nothing is turned back. Eagle's y grows up and KiCad's down, so every y is negated. Its `smd` pads are surface pads (`rect`, or `roundrect` with a `roundness`), its `pad`s through-hole pads (`square` is `rect`, `round` is `circle`, `long` and `offset` are `oval`, `octagon` is written `circle`), a pad with no `diameter` gets Eagle's default ring, and a `hole` is a hole with no copper. The drawings on tPlace become the silkscreen, on tDocu the fabrication layer (whose box `table`'s `body` measures), on tStop and tCream the mask and paste; Eagle has no courtyard layer, so a layer the file names with `courtyard` or `crtyd` in it is the courtyard (a footprint with none gets the box round its pads, as any footprint does). `>NAME` and `>VALUE` become the reference and the value. A drawing on another layer (a keepout, a user layer) is left out, and the command says how many and on which layer, as it does for an octagonal pad and a polygon with curved edges:

```text
$ netsmith import footprint vendor/sensor.lbr --name own:Sensor
wrote libs/own.pretty/Sensor.kicad_mod (3 pads)
the file names it `SOT-3-SENSOR`; it is `Sensor` here
note: left out 1 drawing on Eagle's layer tKeepout, which a KiCad footprint has no layer for
```

A library of several packages (an Altium library's footprints too), and a `.pretty` folder of several footprints, are refused with their names until `--footprint NAME` says which one to import (issue 1438); `--ref` stays for the part of a board, and `--footprint` is refused for a board and for a single `.kicad_mod` file. A name that is not there is refused with the names that are:

```text
$ netsmith import footprint vendor/parts.lbr --name own:Sensor
error: vendor/parts.lbr: the library has 2 packages (SOT-3-SENSOR, SOT-3-ALT): name the one to import with `--footprint NAME`
$ netsmith import footprint vendor/parts.lbr --footprint SOT-3-SENSOR --name own:Sensor
wrote libs/own.pretty/Sensor.kicad_mod (3 pads)
the file names it `SOT-3-SENSOR`; it is `Sensor` here
```

An Altium board (`.PcbDoc`), whose part `--ref REF` names, or an Altium library (`.PcbLib`) that holds one footprint is read the same way (issue #1431); the file must be saved as binary, not as ASCII. A library of several footprints takes `--footprint NAME`, as an Eagle library does (above): the library has no footprint of the name, or holds several, and the refusal lists the names it has. From an Altium file the pads are taken (number, shape, size, angle, hole or slot, plated or not, and the face: a pad on the multi-layer is a through-hole pad), with the tracks, arcs, fills and regions of the overlay (silkscreen), paste, mask and copper layers. Altium has no courtyard layer, so the courtyard and the fabrication drawing are read from mechanical layers 15 and 13, the convention of Altium's IPC footprint wizard. The texts (the designator and comment strings), the 3D bodies, the keep-out and the other mechanical layers are left out, and the command prints a `note:` line for each kind it left out, and one when there is no courtyard. A pad's own mask and paste expansion is left to the board's rules, and a square hole is written as a round one of its width. A part of a board loses its place, its angle and, on the bottom face, its mirror, as from a KiCad board. A footprint drawn on other mechanical layers, or with a courtyard of lines on another one, needs its courtyard drawn on `F.CrtYd` after the import.

**A footprint from a reference board's Gerber files.** When a vendor gives no KiCad, Eagle or Altium file, FILE can be a folder or a zip of the reference board's Gerber, drill and placement files, and `--ref REF` names the part (issue #1432). This is a last resort: a Gerber file is a picture of the copper, not a footprint, so the footprint is rebuilt from it, and part of it is a guess. The part's place, angle and face come from the placement file, a CSV whose columns are found by name (an assembler's `Designator,Mid X,Mid Y,Layer,Rotation`, KiCad's position file, `Center-X(mm)` and the like; a length may carry `mm` or `mil`). The placement file and the Gerber files are taken to share their origin, as KiCad and netsmith write them. The pads are the flashes of the outer copper of the part's face, each with its aperture's shape and size (a circle, a rectangle, an obround, KiCad's rounded and turned rectangles, a polygon of a macro as a `custom` pad) at its place, the placement's centre and angle taken off and a part on the bottom mirrored to be seen from the top. A plated hole of the drill file under a pad makes it a through-hole pad, and an unplated hole nearest the part and within 2 mm of its pads is a non-plated hole. A surface pad has the mask and the paste where the mask and paste files open over it. When the copper file has the X2 attributes of a part's pins (`%TO.P`), those name the part's pads and their numbers. When it has not, each flash goes to the part whose centre is nearest, vias (a flash on a plated hole with no mask opening) left out, and the pads are numbered counter-clockwise from the top left, the pads at the middle last. The courtyard is a rectangle 0.25 mm round the pads, and the body, the silkscreen and a 3D model are not drawn, so the draft row's `table` has no `body`. With no `--name`, the placement file's package column names the footprint. The command says what it rebuilt and what it guessed, to check against the datasheet:

```text
$ netsmith import footprint vendor/gerbers --ref U3 --name own:Sensor
wrote libs/own.pretty/Sensor.kicad_mod (3 pads)

rebuilt from the Gerber files (a last resort: a Gerber file is a picture, not a footprint):
  - the place of U3 from cpl.csv: (20, 30) mm, 90°, top
  - 3 pads with their apertures' shapes and sizes
  - the mask openings from the mask file
  - the paste from the paste file
  - not rebuilt: the body on the fabrication layer, the silkscreen and a 3D model (the files do not say them)

guessed, to check against the datasheet:
  - the pads of U3: the flashes of ref.GTL nearer its centre than any other part's of cpl.csv (the copper has no X2 attributes naming them); a big part's pad may have gone to a neighbour, or a part the placement file leaves out given its pads to U3; 1 copper flash on a plated hole with no mask opening taken for via
  - the pad numbers, counted counter-clockwise from the top left with the pads at the middle last: check pin 1 against the datasheet
  - the footprint's angle: the placement file's 90° is taken off; an assembler's file may add its own offset, so check that pin 1 is where the datasheet draws it
  - the courtyard: a rectangle 0.25 mm round the pads (the files have no courtyard)
```

**A footprint on the bottom face.** Draw every footprint as seen from the top, as KiCad's libraries do. A part on the bottom (`.place.side &= "bottom"`, or `side = "bottom"` in the layout file) is the footprint seen in a mirror: every x of the footprint's frame (pads, drawings, courtyard) negated, then turned by `.place.rotation`, with every layer on the other face (`F.Cu` becomes `B.Cu`, `F.CrtYd` becomes `B.CrtYd`). The board file `fab` writes stores it as KiCad's flip does, which mirrors in y: every y of the footprint's frame negated and the footprint turned by `.place.rotation` plus 180, which puts the pads, the drawings and KiCad's 3D model in the same place. A board file from KiCad stores a footprint on the back the same way, and so does a port's `placements.toml`, which is read from it: to draw a footprint from those figures, negate each y and each pad's own angle (its angle less the footprint's) in the footprint, and put each layer on the other face; the part's `.place.rotation` is the file's rotation less 180. A footprint drawn from them as they stand comes out flipped twice: its pads mirrored and on the wrong face.

**A courtyard of several shapes.** The placer and the placement check keep other parts out of a part's courtyard. Each drawing on the footprint's `F.CrtYd` layer (or `B.CrtYd` where it draws no front one) is a shape, and drawings whose boxes overlap or touch are one shape (the four sides of a rectangle drawn as lines are one). A footprint that draws two courtyard rectangles, one for each strip of sockets of a plug-in module, has two shapes, and another part may stand in the space between them as long as its own courtyard touches neither, as KiCad's own check treats each `F.CrtYd` drawing apart. A shape is the box round its drawing: a courtyard drawn as an L is its box. A footprint that draws one shape, or none, is as it was: its courtyard is one box that takes in every hole that reaches past it and every pad that does with 0.15 mm round it (the pad's reach, `PAD_COURTYARD_MARGIN`; KiCad's courtyard clearance for chip parts smaller than 0603, KLC F5.3). With two shapes or more, a hole or a pad that lies outside every shape is a shape of its own (the pad's box with the same 0.15 mm, the hole's reach), so it is kept clear as copper where it is and does not widen the courtyard over the space between the shapes; a part cannot stand on it. The box round all the shapes is still what the outline, a requirement on the body (where no fabrication outline is drawn), the reference text and the editor's selection use: a part is inside the outline when that box is. The overlap with other parts, a keepout that forbids parts and a cutout are judged shape by shape, and the proofs before placing (_Where a part goes_) leave a part with several shapes to the check after placing. `netsmith dev board3d --format json` lists the shapes in `parts[].courtyards` and the editor's scene in each part's `courtyards`, beside the box.

**A footprint netsmith generates.** When one of netsmith's generators makes the package, the row asks for it by the generator's name and parameters, in place of `Library:Name` (ADR 0104). There is no file and no library:

```toml
[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "gen:SOT23(pins=3)"
```

The generators are `QFN`, `DFN`, `QFP`, `SO` (SOIC, SSOP, TSSOP and MSOP), `SOT23`, `PinHeader`, `PinSocket`, `BGA`, `LGA`, and `MountingHole` and `PlatedHole` (one parameter, `drill=2.375mm`, which a `.diameter` on the part asks for without a row). A parameter is a column of the generator's table of packages, so the pads of `gen:` are those of the table row with the same numbers: `gen:SOT23(pins=5)` has the pads of `Package_TO_SOT_SMD:SOT-23-5`. A length carries its unit (`pitch=0.5mm`), a size is one length or the least and the greatest (`lead_width=0.2-0.3mm`), a pair is `7x7mm` (across, then along) and a count pair `3x3`. The datasheet's dimensions have no default, so a QFN is:

```toml
[[part]]
kind = "Driver"
package = "QFN-48"
default = true
footprint = "gen:QFN(pins=48, pitch=0.5mm, body=7x7mm, ep=5.2x5.2mm, lead_width=0.2-0.3mm, lead_length=0.35-0.45mm)"
```

| Generator | Parameters (all required unless it says) |
|---|---|
| `QFN` | `pins` (a multiple of 4), `pitch`, `body` (`7x7mm`, or a size each: `6.9-7.1x6.9-7.1mm`), `ep` (the exposed pad), `lead_width`, `lead_length`, `paste` (optional: the paste openings over the exposed pad, `4x4`; by default no opening is wider than 1.5 mm), `heel` (optional: the heel fillet taken off, `0.05mm`) |
| `DFN` | the same, with `pins` even: leads on two edges |
| `QFP` | `pins` (a multiple of 4), `body` (one length: the body is square), `pitch`, `lead_width` (optional at 0.4, 0.5, 0.65 and 0.8 mm pitch, where MS-026 gives it), `ep` and `paste` (optional) |
| `SO` | `pins` (even), `pitch`, `body` (across the columns, then along: `3.9x4.9mm`), `span` (tip to tip of the leads of the two columns), `lead_width`, `lead_length` |
| `SOT23` | `pins` (3, 5 or 6); `span`, `lead_length`, `lead_width` and `body` are optional, and are the MO-178 outline's when left out |
| `PinHeader`, `PinSocket` | `rows` (1 or 2), `pins` (in a row, 1 to 40), `pitch` (`2.54mm` or `2mm`) |
| `BGA` | `layout` (balls across and down: `11x11`, a full grid), `pitch`, `body`, `pad` (the diameter of a ball's pad) |
| `LGA` | `edge` (the pads on each top and bottom edge, then on each left and right edge: `3x4`; `0x4` for two rows, left and right), `pitch` (of the ring), `pad` (a ring pad's width along its edge, then its length towards the middle: `0.35x0.625mm`), `body`, `span` (optional: the distance between the centres of the left and right columns, then of the top and bottom rows; by default the pads' outer ends are on the body's edge), `grid` (optional: the pads inside the ring, across and down, `3x3`), `grid_pitch` (with `grid`: one length, or across and down, `1x1.2mm`) and `grid_pad` (with `grid`: a grid pad across and along) |

An `LGA` is a land grid array whose pads are a ring round the body, with a grid inside it or none (issue #1423). The pads are the land pattern of the package drawing as it stands (no fillet is added: an LGA has no leads), each a rectangle with rounded corners. The ring is numbered as a QFN's leads are, from the top of the left edge counterclockwise; the grid follows, row by row from the top left. A sensor of 3 by 3 mm with five pads on each side, three on the top and bottom, and a grid of four pads in the middle, checked against its drawing's table:

```toml
[[part]]
kind = "Sensor"
package = "LGA-20"
default = true
footprint = "gen:LGA(edge=3x5, pitch=0.5mm, pad=0.35x0.55mm, span=2.45x2.45mm, body=3x3mm, grid=2x2, grid_pitch=0.8mm, grid_pad=0.4x0.4mm)"
table = "pads=20, pitch=0.5mm, body=3x3mm"
```

The same numbers with no `grid` are KiCad's `Package_LGA:LGA-16_3x3mm_P0.5mm_LayoutBorder3x5y`, which netsmith also makes by that name, with five more of KiCad's `LayoutBorder` LGAs. A grid that reaches past the inner edges of the ring is refused, as are ring pads as wide as the pitch and corner pads that meet.

The footprint is named from the generator and the parameters in alphabetical order, `gen:SOT23_pins3`, so one set of numbers is one footprint however the row spells it. A generator refuses what is not a land pattern. The message names the part, its package, the file and the line:

```toml
[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "gen:QFN(pins=16, pitch=0.5mm)"
```

```text
kicad.toml:5: part `Sensor` in package `SOT-23`: footprint `gen:QFN(pins=16, pitch=0.5mm)`: `body` is missing: the body across and along (`gen:QFN(..., body=...)`)
```

A part whose pin table has more or fewer pads than the footprint is refused when the board is built, by the check of the pin table against the footprint (see _Pads_): the message names the row of `kicad.toml` and its line, and how many pads the footprint makes. Use a `.kicad_mod` file for a package no generator makes (a connector, a crystal, a package of one maker's drawing); `kicad.toml` may mix both ways, and a part may still map to a footprint or a symbol of KiCad's libraries. That is the one thing that needs KiCad installed, and only for an id that netsmith does not make: a row that names no `symbol` is drawn from the pin table, a `gen:` footprint and every id of the standard library are netsmith's own, and a `[[library]]` of the project is read from the project. When a row names a KiCad id that none of these answers and no installed library has it, `build` and `fab` stop and say which part, which id, which line, where they looked (only if they looked) and the three ways out. When an install was found and only lacks the id (a typo, or a name that KiCad 9 does not have), the message says that KiCad is installed, where, and that it has no such id, and it does not advise installing KiCad:

```text
error: B: rf: part `Coax` in package `Jack35` needs KiCad's footprint `Connector_Audio:Jack_3.5mm_CUI_SJ1-3513N_Horizontal` (kicad.toml line 5), and no installed KiCad library has it.
footprint `Connector_Audio:Jack_3.5mm_CUI_SJ1-3513N_Horizontal` not found: no KiCad footprint libraries found (a folder with *.pretty folders); looked in:
  ...
Three ways out: install KiCad 9, so that netsmith reads its libraries; supply the file through a `[[library]]` of kicad.toml; or give that row a `gen:` footprint, for a package a generator makes (`footprint = "gen:SOT23(pins=3)"`).
```
