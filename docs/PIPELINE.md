# The pipeline: from source to a board

What netsmith does with a design, in order. Nothing is kept between runs but two files: `build`,
`fab` and the editor each start from the source text and run the stages again. The files a
person writes beside a board (layout files, the physical file) are inputs. The files netsmith
writes are outputs, and no stage reads one back, except the pick lock (`<stem>.lock.toml`,
ADR 0100, proposed): `build`, `fab` and `fab --order` write it when it is absent, `netsmith lock --update` rewrites it, and the solve stage then holds
each part to the row it names; and the pin file (`<stem>.pins.toml`, ADR 0101, proposed, below).
`NETSMITH_TIMINGS=1` prints the seconds of each stage; `docs/solver-timings.md` has them for a board with two buck stages.

Written against `main` at `b504854`. Where a name in the code differs from the name a person
would expect, the diagram says so. The code is the authority: `crates/netsmith-pipeline/src/lib.rs`
(`outcome_observed`) for stages 1 to 4, `crates/netsmith-fabricate/src/fab.rs` (`fab_board`) for stage 5,
and `crates/netsmith-solve/src/solve.rs` (`solve_model`) for the solver. The pick files, the
generic parts the solver leaves to the catalogs and the supply providers are in `crates/netsmith-catalog`.

## The stages

```mermaid
flowchart TD
    src[/"Source: .nsm files, netsmith.toml"/]
    cat[/"Catalogs: data/jlcpcb/*.csv, project rows"/]
    side[/"Side files a person writes:<br/>layout, schematic layout, physical"/]

    src --> check
    subgraph S1["1. Check (netsmith-check)"]
        check["Parse, resolve imports, elaborate each board.<br/>Pins, buses, connections, unused, ERC-like rules"]
    end
    check -- "any error" --> stop1(["Stop: errors with source lines"])

    check --> solve
    cat --> solve
    subgraph S2["2. Solve (netsmith-solve)"]
        solve["Propagate, pick real parts, DC analysis, verify<br/>(its steps are in the next diagram)"]
        prov{"Failed, or a choice<br/>left open?"}
        solve --> prov
        prov -- "yes: try the next provider (ADR 0068)" --> solve
    end
    prov -- "no way out" --> stop2(["Stop: failed relations with numbers"])

    prov -- "solved" --> design
    subgraph S3["3. Design (netsmith-design)<br/>builds the Design"]
        design["Solved model to the Design: the parts, nets,<br/>track widths from solved currents, sourcing, guidelines"]
    end
    design -- "power issue; guideline with --strict" --> stop3(["Stop: refused"])

    design --> export
    subgraph S4["4. Export"]
        export["Map parts to symbols and footprints (netsmith's own,<br/>KiCad's libraries only for a project's KiCad id),<br/>check pin table against them, write the schematic"]
    end
    export --> sch[("out/: .kicad_sch, .kicad_pro<br/>netsmith build ends here")]
    side --> export

    export --> resolve
    subgraph S5["5. Fab (netsmith fab; runs 1 to 4 first)"]
        resolve["Generic parts become catalog rows"]
        sup["Every assembled part has a supplier number"]
        pre["Precheck the physical constraints"]
        place["Place (netsmith-placer)"]
        route["Route (netsmith-router)"]
        drc["Design rules check (netsmith's own)"]
        pack["Write board, Gerbers, drill, bom.csv, cpl.csv, fab-notes.txt,<br/>3D scene (.glb), board as STEP (.step),<br/>pin map for firmware (.pins.h, .pins.json) when there are requests"]
        resolve --> sup --> pre --> place --> route --> drc --> pack
    end
    side --> place
    pack --> fabout[("fab/: the package for the board house")]
    pack -.-> xc["Optional KiCad cross-check"]
    route -- "connections left" --> inc(["Written as INCOMPLETE, run fails"])
```

Stage 3 was called "lower" before 2026-11-09. Code, ADRs written before that date and old issues still
use the old word.

Pin requests (`.status: .u.GPIO`, ADR 0101) are resolved at the end of stage 1, after the board is
elaborated and before its nets are made, so that every later stage sees the pins as if the source had
named them. Each request keeps the pin the pin file `<stem>.pins.toml` gives it while that holds, and
gets the first fitting pin in byte order otherwise. `build` and `fab` write the file when a run picks
a pin, `--locked` makes a run that would change it stop, and the next run reads it back: the one file
a stage reads that netsmith wrote. The editor assigns in memory and never writes it.

The pipeline has one back edge, from stage 5 to the pin file, taken only when the router leaves
connections of a board that has pin requests (the pin optimizer, `crates/netsmith-fabricate/src/pin_swap.rs`,
the loop in `fab_board_pinned`). The edge goes through the file, not through memory: a round
searches the placed board for a better pin map, then stages 1 to 5 run again from the source and
that map, so no stage runs out of order. At most three rounds, which together may take as long as
the route timeout. `--pins locked` takes no edge: it writes the proposed map to `pins.proposed.toml`
in the output folder and the run fails as before. The schematic of `build` is drawn from the pin
file, so it follows the map `fab` ended with.

```mermaid
flowchart TD
    src[/"Source + pin file"/] --> s14["1 to 4: check, assign pins, solve, design, export"]
    s14 --> place["5: place, route"]
    place -- "complete" --> pack[("fab/ package")]
    place -- "connections left" --> opt{"--pins auto?<br/>rounds left (3)?<br/>a swap lowers crossings<br/>or ratsnest?"}
    opt -- "yes: the new map" --> pf[/"pin file (written when it ends)"/]
    pf -- "back edge: stages 1 to 5 again" --> src
    opt -- "no, or --pins locked" --> inc(["INCOMPLETE as before,<br/>proposal in out/pins.proposed.toml"])
```

`build` runs stages 1 to 4. The editor runs the same stages on each edit. None of `build`, `fab` and
the editor needs a KiCad install: netsmith answers the ids of its own mapping itself, and reads an
installed KiCad library only for a KiCad id of a project's `kicad.toml` that it does not make (ADR
0097). `fab` repeats stages 1 to 4 in the same process, then goes on
(ADR 0076 gives about 1.25 s for a 60-part board, optimised build; the router may take up to 300 s).

## Inside the solver

The solver is where the analysis happens. There is no separate simulation stage. It is the DC
pass, after the parts are picked.

```mermaid
flowchart TD
    a["Propagate ranges through equations,<br/>and the currents at each net (KCL)"]
    b["Start circular values from the supply's range"]
    c["Pick real parts from the catalogs:<br/>depth-first, back to the part that caused a failure"]
    d["Propagate again, check windows"]
    e["DC analysis of the picked network:<br/>every corner and switch state, island by island"]
    f["Propagate: write what the analysis found<br/>where nothing states a value"]
    g["Verify every relation in the worst case"]
    h["Ohm's-law bounds, current balances, supply budgets,<br/>logic levels, pin faults, absolute maximums"]
    i["Solution: values, picks, findings,<br/>and every check that could not be made"]

    a --> b --> c --> d --> e --> f --> g --> h --> i
    g -. "a value is unknown" .-> n["Noted as: not evaluated"]
    e -. "no operating point, singular, skipped" .-> q["Today: no note (issue #289)"]
```

Three points to hold on to:

- The DC analysis is worst case and static. It does not model time: no startup, inrush, ripple,
  AC or heating. ADR 0076 says why it does not run a circuit simulator.
- A check that cannot be made is never a pass. It becomes a note that says what is missing.
- Parts are picked before the analysis, and a few parts that read the analysis are picked after
  it (ADR 0076, amendment 7). The two overlap a little. The order above is the main line.

## One run as states

```mermaid
stateDiagram-v2
    [*] --> Source
    Source --> Checked: check passes
    Source --> Failed: syntax or check error
    Checked --> Solved: parts picked, relations hold
    Checked --> Failed: a relation fails
    Solved --> Designed: design
    Solved --> Failed: power issue
    Designed --> Schematic: export (build ends)
    Designed --> Failed: guideline with --strict
    Schematic --> Resolved: fab resolves generics
    Resolved --> Placed
    Placed --> Routed
    Routed --> Packaged: design rules pass
    Routed --> Incomplete: connections left
    Packaged --> [*]
    Incomplete --> [*]
    Failed --> [*]
```

`Failed` ends the run with the errors. `Incomplete` writes the board and the Gerbers anyway, marked
INCOMPLETE, so a person can open the routed part, and the run still fails.

## What is decided late

- **The orderable part.** The solver judges a generic resistor or capacitor by its value. The
  catalog row that stands for it is chosen in stage 5 (ADR 0047). A part no row stands for stops
  `fab` by name.
- **Price and stock.** No stage knows them yet (issues #8 and #30).
- **Where parts go.** Only stage 5 places and routes. The schematic never depends on the layout.
