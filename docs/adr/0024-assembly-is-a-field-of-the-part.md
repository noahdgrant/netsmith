---
status: accepted
---

# Who assembles a part is the field `.assembly` of the part

Status notes: Accepted on the user's review, 2026-11-02.

Amended by ADR 0028: the manifest may give a project-wide default, between the built-in rule and the kind's own statement. Amended by ADR 0042: a module may offer the choice for its parts.

ADR 0020, item 4, option A proposed a string field `.assembly` with the values `fab`, `hand` and `none`, and left decision D5 to the user: where the default comes from, and whether a through-hole part is the fab's or is soldered by hand. The user has not decided. This ADR records the minimal version that was built so that boards with holes, fiducials and programming pads can be packaged. Every choice below is provisional.

## What it is

Every `part` has a text field `.assembly`, as every pin has `.net`. A part does not declare it; declaring it again is the error "`assembly` is already declared in this block". The values are:

| Value | Meaning | BOM and placement file | Fab notes |
|---|---|---|---|
| `fab` | The board house places and solders it. | listed | no line |
| `hand` | Soldered by hand after the boards come back. | not listed | `hand-solder:` |
| `none` | Not fitted by anyone: a mounting hole, a fiducial, a pad that a cable presses on. | not listed | `not fitted (no BOM line, no placement):` |

The solver and the balance still count a part marked `hand` or `none`, as ADR 0020 said.

## Where the value comes from

In this order:

1. The design's narrowing: `.j1: Header1x2 { .assembly &= "hand" }`. A board names the parts it owns; a module's parts are not reachable from outside it (ADR 0004), so a module that wants to offer the choice declares a text value of its own and ties the part to it (ADR 0042).
2. The catalog row. A catalog file may have an optional fixed column `assembly` (like `library`), with `fab`, `hand` or `none`; an empty cell says nothing. The pick carries it. No row of the standard catalogs has one.
3. The default. A kind that is always through hole (`Header1x2`) and a part whose footprint the KiCad library marks through hole are `hand`, as before; everything else is `fab`. A `fab` part with no LCSC number is still refused by `fab`, as before.

A word that is not one of the three is a lowering error naming the part and the three words.

## The standard library

`MountingHole`, `PlatedHole`, `Fiducial`, `TestPoint` and `ProgrammingPads6` state `.assembly &= "none"` in their definitions, because it is always true of them. A text field narrows to one value, so a board cannot widen it again: a design that wants a fiducial or a test point on the BOM cannot say so yet. That needs a default that a board may replace, which is a language choice for the user (option 4A could not express "default" at all).

## How `fab` honours it

`netsmith fab` passes the packager each part's fitting (`Options::fit`), which overrides what the footprint's attributes in the `.kicad_pcb` say. The BOM lists only `fab` parts; the placement file is made from the BOM, so it has the same parts; the notes list the `hand` parts and the `none` parts by designator, including a part whose footprint KiCad's library leaves out of the board's part list (a stock mounting hole or fiducial). The editor's bill of materials omits `none` parts and shows `hand` parts apart, as it showed hand-soldered ones.

## Not done

- A default that a board may widen (a fiducial listed on the BOM).
- A catalog row with an `assembly` in the standard JLCPCB catalogs; a row says so only with a source (`data/jlcpcb/README.md`).
- Assembly variants (a part present in one build and not another).
