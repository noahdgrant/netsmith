---
status: accepted (user decisions 2026-11-02)
---

# Boards joined by connectors, and panels

**User decisions (2026-11-02):** a panel is a `panel` definition in `.nsm` (cells, spacing, tabs and rails, with per-cell placement in the layout file); an order may name a panel and its quantity counts panels; the first step is a V-score grid of identical boards. When a system joins two connectors, the signals the mate carries are derived, and an explicit port join for those same signals is refused (one correct way). The other options in this ADR are the recommended ones unless a later decision says otherwise.

Extends ADR 0010 (a system joins ports; "does not model connectors or pinouts"), and answers the panel questions of ADR 0020 item 6 and ADR 0035.

## The goal

Each board of a kit is a stand-alone PCB. A carrier has sockets; plug-in boards have matching pin headers and touch the carrier only through them. Every board has its own ground net (five boards, five `GND` nets: ADR 0005 keeps them apart). netsmith should (A) check that the connectors really mate, (B) check that the grounds are really joined, and with enough current rating, and (C) panelise the boards for one fab order, sometimes ordering only one or two of the plug-ins (D).

Snippets marked `nsm` were run against today's tools (`netsmith-lang --example parse`, `netsmith build`) and the exact error is quoted. Snippets marked `nsm-new` need a construct that does not exist and do not parse. Ratings in the snippets are illustrative; a library part carries a datasheet number only with its source (`data/jlcpcb/README.md`).

## A. Connector mating in a system

### Today

A board already wires a port to its own connector (`.output.pos ~ .power_out.p1`, `examples/two_boards.nsm`), so the pins that carry a port are known inside the board. The system cannot see them. Reaching the connector is refused:

```text
error: `slot_a` is inside board `Carrier`; a system can reach only the ports a board declares directly, such as its rails
   --> mating.nsm:90:14
90 |     .carrier.slot_a.mating ~ .sensor.j1.mating
```

(That run is of a file whose `Mating` interface and connector parts are valid today, so everything but the last line checks.) Nothing compares a socket with a header: a 1x8 header "joined" to a 1x10 socket by two port joins is accepted.

### What a connector declares

A connector part gets one member, `.mating`, of a new std interface (`nsm`: the interface and parts below check today):

```nsm
interface Mating {
    .kind: String          // "pin_header", "edge_connector" (v1); "ffc", "wire_to_board" later
    .gender: String        // "header" | "socket"; "edge" | "edge_socket"
    .pitch: Length
    .rows: Number
    .positions: Number     // contacts; equals the part's pin count
    .pin_current: Current  // rated current per contact (a catalog column, below)
}

part Socket1x10 {
    pins { .p1 passive "P1" "1"  /* ... p10 */ }
    .mating: Mating {
        .kind &= "pin_header"
        .gender &= "socket"
        .pitch &= 2.54mm
        .rows &= 1
        .positions &= 10
    }
    .package: Package
}
```

`Header1x10` is the same with `.gender &= "header"`. Pitch, gender and count are always true of a part, so the library may state them (ADR 0002). The pin current is not: it depends on the manufacturer, so it is a catalog column (section B). Contact N of a connector is its pin `pN`; "pin 1" is the one the part's doc comment says, and a header and its socket number so that contact N mates with contact N. This is a convention of the library (the part doc says so, as `Socket1x10` already does for "mate"), not a field.

### The join

```nsm
system Kit {
    .carrier: Carrier
    .sensor: Sensor
    .carrier.slot_a.mating ~ .sensor.j1.mating     // nsm-new: accepted; today refused (above)
}
```

`~` between two `Mating` interfaces says "these two connectors are mated, contact N to contact N". No new token. The system does not name pins or nets: the contact pairs come from the connectors' numbering and the nets behind them are read from each board's own netlist.

**Reach rule (decision A1).** Option R2, recommended: besides a board's ports, a system reaches the `.mating` member of a connector part the board declares directly (never its pins, and never any other part). It is the connector's declared interface, ADR 0004's rule applied to a part. The error above becomes "...can reach only the ports a board declares directly, and the `.mating` of a connector". Option R1: the board declares a port `.deck: Mating` and ties it to the connector; this changes no rule but costs two lines per connector and a second description of the connector that can disagree with the part. Rejected.

### What the checker does at a mate

For each mated pair of connectors it builds, in the system's scope, one join group of contact pairs, as it already builds a `Balance` for joined ports. Nothing is put on a net (ADR 0005, ADR 0010): the group exists for checking, as `Model::scoped_to` already lets each board be read alone. For each contact it reads a **role** from the board: ground (the net is the `neg` of a rail), power (the `pos` of a rail, with its voltage and current), signal (the pin kinds of the other pins on the net: outputs, inputs, bidirectional), or open (nothing else on the net). Then:

1. **Same kind, pitch, rows and positions**, and **mating genders** (header with socket, edge with edge socket; two headers or two sockets do not mate). A 1x8 header in a 1x10 socket is refused in v1 (it mates physically, but which eight contacts is a statement the board must make; see questions).
2. **Pin for pin, with the rules the board already has on a net**, applied to the union of the two nets: two outputs are refused; an input with no driver on either side is refused; ground with power, or either with a signal, is a short; two power contacts must have voltage ranges that overlap and are then equated, as `~` of rails does; currents are balanced as in ADR 0010; open facing an input is refused, open facing open or a signal output is allowed. Direction is by pin kind only: output to input, bidirectional to anything but a second output.
3. **Orientation and keying.** Reversed insertion is found when the two sides' roles differ from contact N to contact `positions+1-N`; the mate is checked at N-to-N, which is the right insertion. If the pattern is also valid reversed (a palindrome such as GND, SIG, ..., SIG, GND) and the connector is not keyed, the checker warns, naming the two pins that would swap: a symmetric pinout is insertable the wrong way round and only a keyed connector (a field `.keyed`, "none" by default, "shrouded" or "polarised" for such parts) prevents it. The checker cannot see where pin 1 sits on the PCB; a footprint drawn with pin 1 at the wrong end is the layout's check (M2).
4. **Edge connectors**: the plug-in's `process.stackup.thickness` must be within the socket's `.card_thickness` (a field of an `edge_socket`). This is the one cross-board check that needs no geometry.
5. **Ports realised by a mate are joined by it.** Where a board's port rail sits on mated contacts, the join of those rails is derived from the mate (positive to positive, negative to negative) with `~`'s equating and the ADR 0010 balance. An explicit `.power.output ~ .led.input` for such a port is then a second way to say it and is refused, with the pins named (`already joined by the mate of .power.power_out and .led.power_in at pins 1 and 2`); it stays the way to join ports that no connector carries (a soldered lead, a pad). `examples/two_boards.nsm` becomes one `~` between the two `power_*` connectors.

Stacking and side (plug-in above or below) is a layout fact, not a schematic one: it is read from the layout file (ADR 0032: each part's `side`) once both boards have one, and the system view shows `sensor above carrier`. A header on the plug-in's bottom face over a socket on the carrier's top face stacks; both on top faces is a right-angle mate and needs `.mount` ("vertical" or "right_angle") on both connectors; anything else is an error. Board-to-board height and clearance between the plug-in's parts and the carrier need part heights, which the library does not carry; deferred with `.mated_height` as the future field.

### How errors read

```text
error: contact 3 of the mated connectors is ground on one board and power on the other
   --> kit.nsm:90:5
90 |     .carrier.slot_a.mating ~ .sensor.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^
   = Carrier `.slot_a` pin 3 is on net `GND`, the negative of rail `.gnd`
   = Sensor `.j1` pin 3 is on net `3V3`, the positive of rail `.supply` (3.3 V +/- 5%)
   = a mate joins contact N to contact N; if the plug-in is meant to sit the other way round, its pin 3 would meet pin 8
error: `.carrier.slot_a` has 10 positions and `.sensor.j1` has 8
   = Header1x8 mates with Socket1x8
error: `.carrier.slot_a` and `.sensor.j1` are both sockets; a socket mates with a header
```

Voltage disagreement reads like the existing propagation message (`3.3 V +/- 5% (3.135 V to 3.465 V)` against `5 V +/- 5%`: the ranges do not overlap) with both connectors named.

### Options weighed (rule: one correct way; only a board names nets; encapsulation)

| | Option | Against the rules |
|---|---|---|
| **A1** | `~` of `.mating`; ports realised by it are derived | One way for a physical join, the port join kept for what no connector carries; no pins or nets in the system; the only cost is that a port join is refused where a mate says it (a breaking change of `two_boards.nsm`, acceptable under ADR 0001) |
| A2 | mate as well as the port `~`, checked to agree | Two statements for one fact; they can disagree, so the checker needs a third rule to say which wins |
| A3 | pin by pin in the system (`.carrier.slot_a.p3 ~ .sensor.j1.p3`) | A system would name pins, which is a net by another name (ADR 0005, 0010); ten lines per connector |
| A4 | a `harness`/`connection` definition with an explicit pin map | The general answer (adapters, FFC cables that swap order, wire-to-board). Heavier than the kit needs; A1 is its N-to-N special case, so A4 can come later without replacing A1 |

**Recommendation: A1 with R2.** Connector kinds, in order: `pin_header`/socket and `edge_connector` (v1); `ffc` (needs a cable part, because type A and type B cables reverse the contact order); `wire_to_board` (needs a harness definition, A4). Both of the last two are known limits until built.

## B. Grounds

Each board keeps its ground net. A system never merges them (ADR 0005); it reports a **ground group**: the nets joined through mated ground contacts, shown in the system view as `GND: carrier.GND + sensor_a.GND (3 contacts) + sensor_b.GND (2 contacts)`. Five boards give five nets and one group. Checks:

- **Reference.** A mate that carries a power or signal contact must also carry at least one ground contact; and a board in the system that mates with another and shares no ground group with it is reported (`Sensor` has no ground contact in the mate with `Carrier`). Two ground nets that are isolated on purpose are an open question.
- **Current.** The current that returns through a mate is the worst case of the balance of its joined rails (the sum of the largest currents of the power contacts' ports, ADR 0010 and 0038). The checker adds to both connectors the requirement `pin_current >= I / N` for each of the power contacts (N power contacts) and the ground contacts (N ground contacts):

```text
error: ground return of the mate `.carrier.slot_a` / `.sensor.j1` is short of contacts
   = `.sensor.supply` can draw up to 1.5 A, which returns through the 2 ground contacts (pins 2 and 3): 0.75 A each
   = the rated current per contact is 0.5 A (Header1x10, row C0000)  [illustrative]
   = fix: more ground contacts on both boards, or a header with a higher rating
```

- **Rating as a catalog column.** `pin_current` is read from a new `header` catalog (`data/jlcpcb/catalog/header.csv`, one row per series and pin count, each with its datasheet in `source`, ADR 0022), so the solver picks a row that satisfies the added requirement, as it picks a capacitor for its voltage. If no row does, the existing solver message names the requirement and the best row. Contacts of a multi-pin connector are rated by manufacturers with all contacts loaded, so N contacts carry `N x pin_current`; a derating column is added only if a datasheet gives one.
- Cost: the numbers are data work (each series read from a public datasheet), not code.

## C. Panels

### A `panel`, not a system feature and not an order table

ADR 0020 recommended a top-level `panel`; the case for it stands. A system is electrical (no copper of its own), an order is a list of packages (ADR 0035). A panel is one outline with geometry and rules. The choice is where it is written:

- **C1, in the order (`[order.x.panel]` in `netsmith.toml`).** No new definition, but plain numbers without units, no ranges, no check against a process, and ADR 0035 said the order table is replaced, not extended, if it needs more fields.
- **C2, a `panel` definition in `.nsm` (recommended).** Quantities have units, the board house's rules are a std interface like `TwoLayer` (sourced, ADR 0008), the checker can compare the panel with its members, and the manifest keeps only how many panels to order.

Geometry follows ADR 0032: what the source states (method, spacing, rail width, copies) is in `.nsm`; where each cell sits is derived (a grid, or rows) and a hand placement or rotation is in the layout file's table for the panel (`[KitPanel.cells.sensor_2]`, `at`, `rotation`).

```nsm-new
panel KitPanel {
    .fab: JlcpcbPanel {
        .method &= "vcut"             // "vcut" or "mousebite"
    }
    .carrier: Kit.carrier { .copies &= 1 }   // a board of a system, by path; or a plain board name
    .sensor:  Kit.sensor  { .copies &= 4 }
    .frame {
        .rail_width &= 5mm
        .columns &= 3                 // cells per row; rows are derived
    }
}
```

The member `Kit.carrier` is the board as the system narrowed it, not the bare board (ADR 0010); a panel never changes a board. A panel names no nets and adds no electrical content; its fiducials and tooling holes are bare, so they are on no net.

### What a panel needs (from the board house)

Published figures of JLCPCB, read on 2026-11-02 through a page-summarising tool. **Whoever builds P0 must re-read each from the page and record it in `data/jlcpcb/README.md` before it goes into `std/mfg/jlcpcb.nsm`**, and the two disagreements below are open until then.

| Rule | Figure | Source |
|---|---|---|
| V-cut | straight lines across the whole panel; boards with zero spacing; not for 0.4 mm boards; at most 25 lines per direction; lines at least 2 mm apart (3 mm recommended); groove angle 25 degrees; copper at least 0.4 mm from a V-cut edge | S1, S2 |
| V-cut panel size | at least 70 x 70 mm, at most 475 x 475 mm; not supported in economic assembly | S1, S2 |
| Spacing, routed | 1.6 or 2 mm between boards (S2 also says at least 2 mm; the blog's minimum is 1.2 mm: they disagree, use 1.6 or 2) | S2, S3 |
| Breakaway tab | at least 4 mm wide; 5 mm with mouse bites | S2 |
| Mouse bites | hole 0.5 to 0.8 mm (blog: 0.60 mm), 5 to 8 holes in a set, edge-to-edge pitch 0.2 to 0.3 mm (S2) against 0.35 to 0.4 mm with a 0.3 mm minimum (S3): disagree | S2, S3 |
| Copper to a routed edge | at least 0.2 mm (`min_edge_clearance`, already in the process) | S2 |
| Rails, tooling, fiducials | handling rails 5 to 8 mm; tooling edge at least 3 mm; for JLCPCB assembly 5 mm edges, 2 mm tooling holes, 1 mm fiducials centred 3.85 mm from the panel edges; added by the fab if it makes the panel, by the customer if given Gerbers | S2, S3 |
| Designs per panel | more than 5 different designs are charged as separate designs; at most 10 | S1 |
| Panel size | longest side preferably under 250 mm; the blog advises 150 to 200 mm and 2 to 6 boards | S2, S3 |

Sources: S1 `https://jlcpcb.com/help/article/pcb-panelization` (V-cut, rails, designs); S2 `https://jlcpcb.com/capabilities/pcb-capabilities` (routing, tabs, mouse bites, tooling, size); S3 `https://jlcpcb.com/blog/mouse-bite-panelization-guide` (mouse bites, spacing, assembly sets). Not found in any of them: the V-cut file format JLCPCB reads for a customer Gerber panel, designator rules for panel assembly, and the tab count per board. Those are unverified and must be asked of the fab, or learnt from one real quote, before P2.

### Checks

- **Per board:** unchanged (ADR 0037's DRC runs on each member alone, before it is placed).
- **Panel level**, all with the numbers in the message: members agree on layer count, thickness, copper weight and finish (a panel is one stackup); V-cut: every cell edge on a cut is straight and cells sit at zero spacing in rows or columns of equal height or width (so a cut runs across), corner radius is 0 where a V-cut meets it, the board's `min_edge_clearance` is at least 0.4 mm, the panel is 70 to 475 mm; routed: spacing is 1.6 or 2 mm, a tab is at least 4 mm (5 mm with bites), the board's edge clearance is at least 0.2 mm; fiducials and tooling holes are present when any member is assembled; the design count is shown against 5 and 10. Message form: `panel KitPanel: sensor's copper is 0.2 mm from its edge and a V-cut needs 0.4 mm (JLCPCB, S2)`.

### What the fab package of a panel contains

`fab/<Panel>/` beside the boards' own packages (a plain `fab` still makes those):

- One Gerber set for the panel: each board's layers drawn at its cell's place and rotation (netsmith writes the Gerbers itself, ADR 0034, so the geometry is transformed before it is written, not the text), the frame outline, the cut-outs of the routed method, and for the V-cut method the cell outlines touching, as JLCPCB's help describes (S1).
- One drill file: the boards' holes transformed, with the mouse-bite perforations and the frame's tooling holes. **The perforations, rails, fiducials and tooling holes are generated, never drawn by the user.** A tab is placed centred on an edge segment; a bite set is generated at the published hole size and pitch; the sidecar can move a tab.
- `bom.csv` for the panel: each part's quantity is copies in the panel times the parts on one board. `cpl.csv`: positions transformed by the cell's place, rotation added modulo 360 to the part's own (the fab's rotation correction ADR 0034 applies stays per footprint). Designators repeat across cells, so each cell prefixes its own (`C2:R1`); whether JLCPCB's assembly accepts that is unverified (above).
- `fab-notes.txt` with the method, the spacing, the design count against 5 and 10, the unverified items, and the line that V-cut is not available in economic assembly (S1).
- `panel.svg`, the drawing the editor shows.

### The order

An order key may be a panel (decision D). `[order.devkit.boards]` already maps a key to a count; a panel key is a count of panels, so the cell counts are written once, in the panel (`.copies`), and never twice:

```toml
[order.devkit.boards]
"DevKit/carrier" = 2        # two loose carriers, as today
KitPanel = 3                # three panels: 3 carriers and 12 sensors
```

`order.txt` and `order-bom.csv` report the boards that result (3 x 1 carriers, 3 x 4 sensors) and the panel's own line. Ordering a board both loose and in a panel is allowed and summed. To order only one or two plug-ins, the order names those boards loose (nothing new), or a second panel for the subset: a panel has no optional members, because a missing cell changes the outline and the design count and so is a different panel.

### Staged plan

- **P0 (smallest useful step):** identical copies of one board in a grid, V-cut method only, rails and fiducials generated, one merged Gerber and drill, a panel BOM and CPL, the panel-level checks above, a package in `fab/<Panel>/`, and an order key that names a panel. Needs sourcing of the V-cut and rail numbers into the process data. Syntax: `.cell: Board { .copies }` with `.frame.columns`.
- **P1:** members of different boards, in rows of one height (a V-cut must run across), then rotation by 90 degrees from the layout file.
- **P2:** routed tabs with mouse bites, tab placement, option of the fab adding the rails (`Panel by JLCPCB`), mixed spacing (V-cut one way, routed the other, S2). Needs a quote to settle the assembly unknowns.
- **P3:** the editor's panel view: cells, cut lines, tabs, bites, rails and findings drawn on one canvas; drag and rotate write the layout file (source "hand"); spacing and method stay in the source.

### Effort per crate

| Crate | A (mating, B grounds) | C (panels) |
|---|---|---|
| `netsmith-lang` | none for A1 (it parses today); `panel` and its members: small | a new definition kind, `Kit.carrier` path as a type: medium |
| `std` | `Mating` and `.mating` on the header and socket parts, `header` catalog column: small, plus sourcing | `JlcpcbPanel`, `Frame`: small, plus sourcing |
| `netsmith-check` | the reach rule, the role reader, the mate group, derived port joins: medium | `panel` scope, member resolution, panel checks: medium |
| `netsmith-solve` | the added `pin_current` requirement: small | none |
| `netsmith-pcb`, `netsmith-fab` | none | geometry transform of a finished board, panel outline, drills, BOM/CPL: large (the bulk) |
| `netsmith` (CLI, manifest) | none | an order key naming a panel, `fab <Panel>`: small |
| `netsmith-view`, editor | system view shows mates and ground groups: small | panel view: large (P3) |

## E. Questions for the user, ranked

1. **Is a port join refused where a mate says it (A1 point 5)?** Recommend yes: otherwise a fact has two spellings. Consequence: `examples/two_boards.nsm` and any user system with connector-borne ports changes by one line; ports no connector carries keep `~`.
2. **R2 or R1: may a system reach a connector's `.mating`?** Recommend R2. R1 keeps ADR 0004 to the letter at two lines per connector.
3. **Is a panel a `.nsm` definition (C2), with cell places in the layout file?** Recommend yes. The alternative (C1) is the order table with unitless numbers.
4. **Does P0 write a merged panel Gerber, or only an order note telling the board house to panelise (`4 x 2, V-cut, 5 mm rails`, all of S1's choices)?** Recommend the Gerber, because it is what you asked for, and the note is a by-product of `order.txt` that costs nothing. Consequence of the note alone: nothing to verify against a fab, but the panel is not checked or previewed.
5. **Subset orders: a second panel or loose boards (D)?** Recommend loose boards or a named second panel; no optional cells. A panel that can lose a cell has no single outline to check.
6. **Mixed boards on V-cut:** V-cut needs full-width lines, so mixed boards of different sizes need equal-height rows or the routed method (P2). Recommend rows in P1; the checker names the cell that breaks a cut.
7. **Panel assembly with JLCPCB:** the designator scheme, V-cut Gerber format and tab rules are unpublished in what was read. Recommend one real quote with a P0 panel before P2; until then the package states them as unverified.
8. **Partial mates** (a 1x8 header in a 1x10 socket) and **isolated grounds** (a barrier that must not join grounds): refused in v1. Recommend a later `.contacts` subset on the mate and a `.isolated` mark on a rail, each with its own check, if a design needs them.
9. **Pin ratings:** sourcing is by series from public datasheets; recommend starting with the one series the kit uses and refusing the rating check (listed "not evaluated", as ADR 0036 does) for a connector with no row.
10. **Where the panel's fab rules live:** `std/mfg/jlcpcb.nsm` with the process, or a file of their own. Recommend the same file, since one board house's numbers belong together.

## Known limits if this is built as proposed

Wire-to-board and FFC are not modelled until a harness exists. Orientation of pin 1 on the PCB and stack height are not checked until the layout and 3D data exist. A panel is one board house's rules at a time. Parts A and B (connector mating and grounds) and part C as far as P0 are built (the two sections at the end). The rest of part C (P1 onward) and part D are not: issues #16 and #62 say so.

## Built: connector mating and grounds (A and B)

Parts A and B are built, with the recommended options and the user's decision (one correct way: the signals a mate carries are derived, and a port join for them is refused). Panels (part C, P0) are built in the next section.

**The interface.** `std::parts::connector::Mating` (kind, gender, pitch, rows, positions, `pin_current`, `keyed`) and a `.mating: Mating` member on every header and socket (`pin_header`, 2.54 mm, with its rows and positions; `.gender` `"header"` or `"socket"`). Contact N is pin `pN`; a header and its socket number alike, so contact N meets contact N. `pin_current` is a column of a new `header` catalog (`data/jlcpcb/catalog/header.csv`: 1x2, 1x4, 1x8 and 1x10 headers and sockets, 3 A a contact from JXTCONN and Hanxia datasheets read on 2026-11-02, sources recorded in `data/jlcpcb/README.md`). The two-row headers (`Header2x3`, `Header2x5`, `Header2x10`) have no source for a rating, so they have no catalog and are unrated: the check of the current through them is listed as not evaluated. A catalog row's label is the KiCad symbol's name, so the value field of a schematic reads as before, and headers and sockets now carry an LCSC number although they are hand-soldered (`Header*` and `Socket*` get the reference letter `J`).

**The reach rule (R2).** A system reaches a connector's `.mating` and nothing else of a board's connector. Any other member of the connector, or any other part of a board, is refused with the message that now ends "and the `.mating` of a connector".

**The mate.** `~` between two `.mating` records the mate; after the nets are known the checker reads, for each contact N, what each board has on the net of pin `pN` and gives it a role: ground (the net holds the `neg` of a rail), power (the `pos`), signal (a part's `input`, `output` or `bidirectional` pin; the connector's own pins are passive and ignored) or open. The checks, each reported at the `~` with the nets and pins of both sides, are in `crates/netsmith-check/src/mate.rs`: kind (only `pin_header` is modelled), gender, positions, rows, pitch; per contact, ground against power or a signal (a short), two outputs, an input no board drives (an open or passive contact facing an input is refused too); a mate that carries power or a signal with no contact that is ground on both boards. A connector is in one mate; the two connectors of a mate are on different boards. The orientation check is a guideline (below); edge connectors and their `card_thickness` check are not built (only `pin_header` mates are accepted).

**Derived port joins.** A port of one board (an interface it declares directly) whose pins are all on contacts that a port of the same kind on the other board has its pins on is joined to it as if `~` had been written: its values are equated and its currents are added to one `Balance` in the system, so the voltage ranges, the sums of currents and the envelope rules of ADR 0010 apply unchanged (the existing machinery, `Checker::connect_interfaces` and `balance_joins`, is what the mate calls). A port join written for ports a mate carries is refused: "`.a` and `.b` are already joined by the mate of ... at contacts ...; remove this one". The rule is "any pin of the two ports is carried by a mate", so a port with only its ground on a mate is refused too; a port no connector carries is still joined with `~`. Two ports of one board on the same contacts (a supply input and an output on one net) are ambiguous and are refused with the message to give the board one port for that net (the example puts a fuse between its input and the rail it gives to the cards). Power contacts whose rails are not ports of a board (a part's rail on a connector) have their voltages equated all the same. A signal port such as `Spi` is carried when its pins are on the same contacts on both sides; a `Uart` that crosses `tx` and `rx` is not, which is right: the board joins the pins.

**Grounds.** Each board keeps its ground net. `Model::ground_groups` lists the nets joined by contacts that are ground on both sides (union-find over board and net name), with how many contacts each mate gives; the editor's System view shows them as `carrier.GND + sensor.GND + serial.GND`, and the mates with their contacts coloured by role. A mate with power or signals and no ground contact is an error. Isolated grounds cannot be said (open question 8).

**The current in the contacts.** For each rail pair a mate joins, and each connector with a `pin_current`, the checker adds two relations (the current and its negative): `contacts * pin_current >= current`, where `contacts` is the number of contacts that carry the rail's `pos`, and for the ground the sum of the currents of every rail that returns through those contacts against the ground contacts. The current is that of the port joined to fewer others, so a card's own draw is checked and not the whole of a carrier's supply that its other cards share. A connector with a catalog row is picked so that it satisfies them (an unrated catalog has none), and the solver's message names the relation: `no header candidate satisfies carrier.slot: ... (1 * carrier.slot.mating.pin_current) must always be at least carrier.cards.current, but 3A is not always above 0A..4A`. A connector with no `pin_current` leaves the relations unevaluated, which `build` lists as notes.

**Turned end for end.** The pairing of contact N with contact `positions + 1 - N` is checked the same way (with the voltages the boards state); when it also works, neither connector is keyed (`.mating.keyed`, unset is `"none"`) and some pair of nets would swap places, the mate has a `reversible` text and the board of the first connector gets a guideline finding (`mating`), a `warning:` line, an error with `--strict`. A pinout that reads the same turned round (the same nets swap places) is not reported, because nothing would change.

**Changes of behaviour.** `examples/two_boards.nsm` mates a header with a socket (the LED board's connector is now a `Socket1x2`) and no longer joins the ports. A `Socket1x2` has a sample symbol and footprint in `netsmith_kicad::testlib` (the sample libraries now also have the 1x4 pair). The editor's System view lists the mates and ground groups (`SystemView.mates` and `.groundGroups`, typed in `api.rs` like the rest). `compare` and `fab` take nothing new; headers and sockets are in the BOM as hand-soldered parts with an LCSC number, as they were as hand-soldered parts without one.

**Not built, and listed in issues #16 and #62.** Edge connectors and `card_thickness`; flat cables and wire to board (need a cable or harness definition, A4); a partial mate and isolated grounds (questions 8); stacking, side, mount and `.mated_height` (need the layout file and part heights); pin 1 on the PCB (a layout check); a rating for the two-row headers; derating of several loaded contacts. Messages are one line each, with the nets and pins of both sides (the multi-line form of the examples above would need a notes field on diagnostics).

## Built: P0

Stage P0 of part C is built: identical copies of one board in a V-cut grid. What follows is what was decided where the plan left room, and what is still open.

**The language.** A new definition kind, `panel`. It holds the board it repeats and the board house's rules, and nothing else (it joins nothing, names no nets, configures nothing inside its board):

```nsm-fragment
panel LedPanel {
    .cell: LedTile
    .fab: VCutPanel {            // std::mfg::jlcpcb::VCutPanel
        .columns &= 3
        .rows &= 2
        .rail &= 5mm             // 0mm, the default, is no rail
    }
}
```

The ADR's `.cell: Board { .copies }` and `.frame.columns` became `.columns`, `.rows` and `.rail` of the `.fab` member, because the board house's limits are fields of that same member (ADR 0008's way for a process). Spacing is not a field: V-cut boards have none. A board in a panel is the stand-alone board of the same file; a board a system places, or one defined in another file, is refused with that said (P1 lifts both). The cell is not narrowed or joined: the panel never changes a board.

**The rules and their sources** are `VCutPanel` in `std/mfg/jlcpcb.nsm` and the table in `data/jlcpcb/README.md`, read from JLCPCB's pages on 2026-11-02. They settled the ADR's open items: the V-cut lines go on the same layer as the board outline (`Edge.Cuts`) and are marked "V-CUT" (the blog and the ordering instructions); copper is at least 0.4 mm from a V-cut edge (1.0 mm is advice); an assembled panel is at most 250 x 250 mm; rails are 5 to 8 mm. Where pages differ the stricter is stated.

**What `fab` writes** for a panel is `fab/<Panel>/`: one Gerber set, the drill files, `bom.csv`, `cpl.csv`, `fab-notes.txt`, `<Panel>.kicad_pcb`, `<Panel>.kicad_pro` and `panel.svg`, and the board's own package in `fab/<Panel>/cell/<Board>/`. A plain `fab` makes panels with the boards; `--order` makes the panels it names. The panel is made by writing it as a `.kicad_pcb` (each cell the board's items moved by the cell's offset, its nets renamed `cell3/GND` and its designators `R1_3`, the boards' outlines replaced by one, the cut lines and their marks on `Edge.Cuts`, the rails' tooling holes and fiducials as footprints) and running the same writers on that file, so the Gerbers are the boards' own, moved, and KiCad can plot the panel for the cross-check. The silkscreen keeps the board's own reference text (the unique designator is hidden on the panel board), so the panel's silk is the boards' silk. The ADR's `C2:R1` designator is `R1_3` here, a suffix, so that a panel designator does not read as a capacitor's. The marks are tooling holes in the four corners (centred in the rail) and three fiducials 3.85 mm from the left and right edges, on the front, and on the back if a part is assembled there.

**The checks** are in `netsmith_fab::panel::check`, each message with the numbers and the page: rail width, panel size (and the assembled size), lines per side, line spacing, thickness, the board's edge clearance (the board's own DRC ran at its `min_edge_clearance`, so the panel needs that to be 0.4 mm or more: `.process.min_edge_clearance &= 0.4mm..1m`), tooling holes and fiducials when anything is assembled, cells that overlap (by construction they cannot), and a rectangle with square corners. Then netsmith's DRC runs on the panel board itself.

**The cross-check** (`--cross-check`) plots the panel board with KiCad and compares as for a board; it agrees on `examples/led_panel.nsm` (copper, mask, paste, outline, drills, placements, and no design rules finding either way). Two things are special to a panel: the text "V-CUT" on `Edge.Cuts` is left out of the comparison on both sides (the fonts differ, as for silk), and KiCad's `invalid_outline` finding on each cut line is expected, not compared, because KiCad calls a V-cut line on `Edge.Cuts` a malformed outline. netsmith's own outline reading takes a straight line with both ends on the outline as a cut line.

**The order.** A key of `[order.x.boards]` may be a panel; the quantity counts panels. `order.txt` counts boards and panels apart and says what the panel holds; `order-bom.csv` multiplies the panel's parts by the quantity (the panel's bill already holds one part for each cell). A board may be ordered loose and in a panel.

**The editor** lists panels in the tree (badge `N`) and draws one in the PCB view, over the board it repeats: the boards outlined and named, the cut lines, rails and marks, and the panel checks for the board as it is placed now. It is read only.

**Still unverified, and why it matters.** JLCPCB has not accepted a netsmith panel. Their pages do not say which designators a panel made by the customer may have in its BOM and CPL, how their viewer reads V-cut lines on `Edge.Cuts` (a forum user saw a V-cut on a user layer in the outline layer), or how many tabs a routed panel needs. `fab-notes.txt` says so in a line that starts "UNVERIFIED", and keeps the single board's `bom.csv` and `cpl.csv` beside the panel for the other path (order the single board and let JLCPCB make the panel, which adds its own rails and fiducials). One real quote with a P0 panel should come before P2.

**Not built, for P1 onward:** different boards on one panel (rows of one height), a board of a system as the cell, a cell in another file, rotation, per-cell placement in the layout file, routed tabs and mouse bites, the fab adding rails, and moving cells in the editor.
