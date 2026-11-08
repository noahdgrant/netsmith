---
status: provisional, awaiting the owner's review (issue 243; the owner asked for "a way to mark 'I will ship this to the board house'"; the spelling, the cell contents and the other choices below are the agent's, each the conservative option of the issue, with the alternatives under "Choices for the owner")
---

# A part can be consigned: the owner ships it to the board house

## The problem

`AssemblyMode` had three values (ADR 0024, ADR 0054): `Fab` (the board house places a part it buys, so the part needs a supplier part number, an `lcsc`), `Hand` (soldered by hand: out of `bom.csv` and `cpl.csv`) and `None` (not fitted). A real 12 V automotive board has a processor that JLCPCB can place but does not stock. The owner buys it and ships it to JLCPCB (JLCPCB's consigned parts service). None of the three says that: `Fab` refuses the part for its missing number, and `Hand` leaves it out of the placement file, so the board house would not place it. The part's stock does not matter, and a stock of 0 in its catalog row (a note since #202) is no news.

## The decision

A fourth variant, `AssemblyMode::Consigned`. It is stated in every way `Hand` and `None` are, with one name, and the rules of precedence of ADR 0028 are untouched:

- on a part: `.assembly &= AssemblyMode::Consigned`;
- by a module for its parts: `.assembly_mode: AssemblyMode` tied with `.r.assembly &= .assembly_mode`, and the board writes `.assembly_mode &= AssemblyMode::Consigned` (ADR 0042);
- in a catalog row: the `assembly` column's word `consigned` (a catalog file writes the lower-case word, as for `hand`);
- as the project's default: `assembly = "consigned"` under `[project]` in `netsmith.toml`. It is allowed because the manifest accepts every word the column does, not because it is useful: it would send every part of every board to the board house as the owner's own.

`Fab` is "the board house buys it". `Consigned` is "the board house places it, and the owner supplies it". The two share everything but the supplier:

| | `Fab` | `Consigned` |
|---|---|---|
| in `cpl.csv` | yes | yes |
| in `bom.csv` | yes, with its `lcsc` | yes, with its manufacturer's number as the comment and the LCSC cell empty (or its `lcsc`, if the row has one) |
| needs an `lcsc` | yes; `fab` refuses without | no |
| `fab-notes.txt` | nothing | a `consigned:` line with each part's manufacturer's number and quantity |
| `build`, BOM column | the LCSC number and library | `consigned` |
| stock note (#202) | when a row states stock 0 | never |
| unresolved generic (#208, `fab`) | when no row can be ordered | never: no row stands for a part the owner supplies |
| a part over protruding pins (ADR 0045 section 15) | placed by the board house | placed by the board house |

A consigned part with no catalog row (a generic one) stays generic: `resolve_generics` leaves it alone, so no LCSC number is invented for a part nobody will order from LCSC.

## What the board house reads (the source of the format)

Read on 2026-11-08, as pages, by an agent (netsmith itself asks no supplier, ADR 0039):

- JLCPCB's help article "How to consign parts to JLCPCB" (`jlcpcb.com/help/article/How-to-consign-parts-to-JLCPCB`): the owner registers the components on JLCPCB's site (a quantity, a consignment list), ships them, and "the component seller must place PCBA orders to use the registered components" within the free storage period. The article says nothing of the BOM or the CPL.
- "Common BOM and CPL matching issues and explanations" and "Component matching guidelines for PCBA orders": consigned components, like pre-ordered and globally sourced ones, are chosen by the customer in the order page's component matching interface ("after you have completed the confirmation by selecting them in the component matching interface"); JLCPCB proceeds only once all of them are in its warehouse. A row it cannot match is "No Part Selected" and is matched by hand by the manufacturer's number or a C-code. The same part cannot be mixed between JLC library stock and consigned stock.
- JLCPCB's own BOM guide lists the columns (designator, footprint, comment, LCSC part number) and no column or word that marks a row as consigned. I found no JLCPCB page that gives a marker; a different assembler (PCBasic) asks for the BOM row to be marked. That is no JLCPCB rule.

So the file format has no marker, and the choice was between an empty LCSC cell and a made-up column or word. An unknown column is at best ignored and at worst fails the upload; a made-up word in the LCSC cell fails the `C` plus digits check of the column. The conservative option is the one the issue lists first: the row keeps designator, footprint and comment, the comment is the manufacturer's number (what the owner shipped, which the order page matches by), and the LCSC cell is empty unless the catalog row has a number, which the matching interface can use. Where the row has an `lcsc`, the rows of a consigned part and of a part from stock are never one line, even with equal value, footprint and number, because JLCPCB does not mix the two in one line.

The marking is therefore in `fab-notes.txt` (a `consigned:` line that says the owner ships the parts and that JLCPCB cannot go on without them) and in `order.txt` of an order, not in `bom.csv`. What this was not tested against: JLCPCB's order page. Nobody has uploaded a `bom.csv` with an empty cell for a registered consigned part from netsmith; the notes tell the person ordering to look at the matching interface (the `prepare-fab-order` step).

## What changed

`std/prelude.nsm`: the variant. `netsmith-lower`: `Assembly::Consigned`, its two words, `Sourced::assembled()` (true for `Fab` and `Consigned`: the board house places it) and `Sourced::needs_part_number()` (`Fab` only). `netsmith-solve`: the catalog column and its message. `netsmith`: the manifest word, the `build` report, the stock note (skips consigned), the resolver (skips consigned), the refusal for a missing number (`Fab` only), `fab`'s BOM and fit, the order's summary, and the rule for parts over protruding pins (only `Hand` and `None` are fitted after the pins are cut). `netsmith-fab`: `Fit::Consigned`, `Plan::consigned` and `Plan::bom_rows()`, the `consigned:` note. The language guide ("Who assembles a part"), the glossary and the tests of each stage are in the same change.

## Left open

- The editor's bill of materials shows a consigned part as an ordinary line with no number and no price: its API has a `hand_solder` flag and no consigned one, and a new field changes the generated types and the committed bundle. It is a follow-up.
- `bom.csv` and the order page are not tested against JLCPCB (above).
- A quantity (spares, attrition) for the consignment is the owner's to register on JLCPCB's site; netsmith states what the board needs, not what to ship.

## Choices for the owner

1. **The name.** `Consigned` was taken from JLCPCB's own term and the issue's example. Rejected: `Supplied`, `OwnerSupplied`, `Customer` (each names who and not what the board house does), and `Fab` plus a flag on the part (a second spelling for the same thing, against "one correct way", and it would not carry through `.assembly_mode`).
2. **One variant for every consignment.** A part whose catalog row has an `lcsc` can also be consigned (the owner registers the C-number). It is the same variant: the difference is only whether the cell is filled. A separate `ConsignedByNumber` was rejected.
3. **The mark in `bom.csv`.** Options: (a) an empty LCSC cell and the manufacturer's number as the comment, built here; (b) a fifth column such as `Consigned` (unknown to JLCPCB's uploader, risk of a failed upload); (c) a word in the LCSC cell (fails JLCPCB's own part number check); (d) leave consigned parts out of `bom.csv` and `cpl.csv` and list them in the notes only (the board house would not place them, which is the purpose). If JLCPCB documents a marker, (a) changes in one function (`bom_entries`).
4. **The comment is the manufacturer's number** and not the value, for a consigned part that has one. The value stays when the row has none. Alternative: the value always, with the number in a note only.
5. **A consigned part in the project default.** Allowed, because the manifest and the column accept the same words. Alternative: refuse it in the manifest.
6. **A consigned generic stays generic** (no LCSC number is chosen). Alternative: resolve it like any generic, which would put a C-number in the BOM that the owner never meant to buy.
7. **The notes line** is `consigned: <parts> (<number>, <quantity>)` under the same heading style as `hand-solder:`. A unit count of spares is left out.
