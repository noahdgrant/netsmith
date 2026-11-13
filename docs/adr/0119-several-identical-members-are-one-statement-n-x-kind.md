---
status: accepted
---

# Several identical members are one statement, `N x Kind`

The syntax is the owner's decision of 2026-11-13 (issue #623). The implementing agent suggested the rules below that the syntax did not settle, and the owner accepted them the same day, on pull request #682.

A board with four identical decoupling capacitors wrote four declarations and four chains (orbit, `examples/orbit/src/main.nsm`; rp2350 had eleven). A count before the type says it once, as a named member or inside a chain:

```nsm-fragment
.decoupling: 4 x Capacitor { .capacitance &= 100nF +/- 10% }
.v.pos ~> .decoupling ~> .v.neg
.v.pos ~> 2 x Capacitor { .capacitance &= 10uF +/- 20% } ~> .v.neg
```

The group is its members: `.decoupling[0]` to `.decoupling[3]`, each declared with the group's block, as if written one by one. `crates/netsmith-lang/src/group.rs` expands the syntax tree before the checker, so the checker, the solver, the design stage, the layout file, the physical file, the pick lock and the exporters see plain members whose last segment ends in an index, and need nothing new. The parser keeps the tree as written, so it prints back to its source. The index is part of the segment's name (`decoupling[2]`), which a path in the source, a physical file and the TOML files all write the same way.

What a statement applies to. The group's block holds for every member. One member is narrowed by its index (`.decoupling[1].package &= "0603"`), which intersects with the block as any `&=` does, so a member may be narrower than the group and never wider. A path to the whole group outside its block (`.decoupling.package`) is an error that names the members, and so is an index past the last; the checker says so instead of "not a member" (`Checker::group_hint`). A statement on the whole group outside its block is refused, not spread over the members, because a relation such as `.decoupling.capacitance == .x` has no single meaning.

Chains. A chain with a group alone between its two ends, every operator `~>`, is one chain for each member, so each stands in parallel between the ends and the current balance (ADR 0014) sees what it saw for the hand-written chains. A group anywhere else in a chain, or with `~`, is refused with the members' paths, because a group in series with another part would either repeat that part's connection once per member or need a junction the chain does not say.

A group written in a chain has no name, but its members need paths for the layout file and the lock. It is named from its kind and the two ends, `capacitor_v_pos_v_neg[0]`, so the name does not depend on statement order; a second group of the same kind between the same ends would have the same name and is refused with a message that says to name the groups.

Designators. The members are declared where the group is, in index order, so the numbering by declaration order (issue #166) gives them consecutive numbers in index order; the examples' designators did not move. Where parts are ordered by path (`natural_path_order`, and #634, which numbers by sorted path), an index compares as a number: `decoupling[2]` before `decoupling[10]`.

The count is a whole number from 2 to 1000, written `4 x` with spaces (`4x` lexes as a number with a unit and is refused with the spelling); `1 x Capacitor` is refused because it is a plain declaration written another way (ADR 0001). A group is declared by one name, not a path into a member. Any kind may be a group, a module too. The group's doc comment goes with its first member only, so the schematic has one note for it, as the hand-written examples had.

The bank (ADR 0052). A bank is one member whose `.count` the solver may choose from a range, with identical capacitors and paths (`bulk#2`) a board cannot narrow or place one by one. A group's count is the board's, fixed before the checker, and its members are members. So `N x Kind` does not subsume the bank: `BuckStage` gives `.output_cap.count ?= 1..8`, which a group cannot say, and a module's `.output_count` is a number the board passes through the module's interface. The overlap is a whole number written on a board's own capacitor (`.c: Capacitor { .count &= 3 }`), two spellings of three capacitors in parallel. The owner decided to keep the bank for a count the solver chooses and for a count passed into a module, to refuse a whole-number `.count` stated directly on a board's own capacitor with a message that points to `3 x Capacitor`, and to rename a bank's later paths from `bulk#2` to `bulk[1]`, so the two read alike. Both changes are issue #685, not this decision's change.

Rejected. `[4] Capacitor` and `Capacitor[4]` read as array types, which the language does not have; a member is not a value with a length. A `repeat` or `for` block is a loop, which the language has none of and which a board does not need for identical parts. Expanding in the checker rather than on the syntax tree would touch every place that walks a definition's items. Indices from 1 would not match the owner's `.decoupling[0]`. A doc comment on every member put one note per member on the schematic.

Cost. Names with brackets reach every file that holds a path: TOML keys need quotes (`[B.parts."decoupling[2]"]`), which the writers already add. A tool that reads the syntax tree itself and not the checker's model sees the group as written: the editor's outline lists it once, which is right for an outline, and its go-to-definition (`crates/netsmith-workspace/src/definition.rs`) does not yet find `decoupling[2]`, because it compares names as written. The power-manifold example declares its capacitors in the reference board's order, with docs per capacitor, and was left alone.

Left open. The two bank changes, in issue #685. A count from a value (`.n x Capacitor`), which needs the count before the checker runs. A group inside a module that a board reaches by index from outside (ADR 0004 governs that, as for any member).
