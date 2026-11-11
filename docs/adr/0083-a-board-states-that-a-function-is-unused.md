---
status: accepted
---

# A board states that a function is unused

The owner chose, on 2026-11-08, that a board states a function is unused. An agent wrote the word, its place, what counts as a use and the reading of test pads, each with the alternatives it rejected. Accepted as built by the owner, 2026-11-11. Tests 1 and 3. Cut from `git show 8023ee9:docs/adr/0083-a-board-states-that-a-function-is-unused.md`.

## The problem

A part states limits for a function whether or not a design uses it: a microcontroller's datasheet limits VBUS on its USB port to 4.35 V to 5.5 V. The part cannot leave the limit out (libraries state only what is always true), and a board with no USB port cannot make the limit true, because nothing gives the pin a voltage. Issue 188 (second point) met it: the solver listed `not evaluated: processor.usb.vbus.voltage >= 4.35V` and the same for 5.5 V, and the power view said `processor.usb.vbus.pos has no source: nothing supplies it`, on a board that does not use USB. The same showed for a modem module's USB pads, which the board names and brings out to unpopulated test pads. A limit that applies only when a function is used could not be written.

## Decision

A board states that an interface is not used with a statement of its own:

```nsm-fragment
unused .processor.usb
```

The word `unused` starts the statement and one path follows: an interface of a part or a module (`.processor.usb`, `.modem.usb`), the thing a datasheet calls a function when it is a port. It is a board's statement, written anywhere in the board, also inside the body of the declaration of the part (`.processor: Processor { unused .usb }`). A part, a module, a system, a panel and a physical file refuse it, with a message that says so. The path must name an interface; a pin, a value, a whole part and a whole module are errors, and naming an interface twice is one.

What it does, for the interface and everything inside it, after the checker has read every statement (order never matters):

- The relations that read only values of the interface (`.usb.vbus.voltage >= 4.35V`) are taken out of the model, as ADR 0074 takes out the relations of an unstated optional member: not checked, not listed as not evaluated, nothing asked of a catalog. A relation that reads a value outside the interface as well is kept.
- The currents of the rails inside it are taken out of the balances and the track-width carriers, so the power view has no net for them and no `has no source` note, and a pin of it that its table says must be connected is not asked to be.
- The level checks (`crates/netsmith-solve/src/levels.rs`) skip its pins: they are not drivers or receivers, and their absolute maximum is not read against a net.
- The model keeps the list (`Model::unused`, `Model::unused_by`), so a later stage asks the model and not the syntax.

An interface that the board states unused and also uses is an error. The two statements say opposite things, and the message shows both lines: the connection (the error points at it) and the `unused` line (`... is stated unused at main.nsm:15`). A use is one of:

- a pin of the interface that shares a net with a pin of another part that is not `passive` (nets named alike count; a pin joined to another pin inside its own part does not, and a connection made inside the module that holds the interface is the module's own wiring);
- the same through series passives: a net at the far end of a `Resistor`, a `Fuse` or a `FerriteBead` that leads from the interface, through however many of them, to a net that has such a pin (`docs/LANGUAGE.md`, "A function the board does not use"; `series_use` in `crates/netsmith-check/src/unused.rs`);
- an interface joined to another with `~`, which equates their values (a rail of the board that supplies the port is one);
- a function the board chooses on a pin of it (`.processor.usb.dm.function &= "USB_DM"`).

The second item came with pull request #323 (issue #293, item 5), on the owner's decision of 2026-11-09; what it left is under "Left open".

## What `unused` means for test pads

A board names the nets of a module's USB pads and brings them out to unpopulated test pads, and the pads have no driver. The conservative reading is that `unused` says **nothing on the board uses the interface**, not that nothing is connected to it. A part with passive pins only (a test pad, a header) brings a net out and uses nothing, so it is not a use, and nor is a return to ground, the `neg` pin of a rail, because every ground is one net and a connection to it proves nothing. The power view then has no `has no source` note for a net that goes only to test pads. A board that wants the pads to count as a use (a probe it drives) does not write the statement. The alternative, that any connection is a use, would make the statement useless for the modem's pads, which are connected to something by name; the alternative that the pads need an `unpopulated` marker was rejected as a second concept.

## Rejected

- **A guard on the relation, `when .usb is used`.** It puts the condition in every part that has a limit, restates it in each relation, and needs a notion of "used" that the part cannot know: the board decides. It is also a conditional in the language, which ADR 0074 rejected for the same reason.
- **`optional` on the limit's member** (ADR 0074). The limit is on `usb.vbus.voltage`, a member of `Rail` in `std`, which every use of `Rail` would then carry; and the relation is skipped when nothing states the member, while the board has a different fact to state (the interface is unused), which also takes the source, the levels and the connection rule out.
- **`.processor.usb: unused`**, the owner's example. A declaration says `.name: Type`, so `unused` would be read as a type and the statement as a second declaration of a member that exists, with a lowercase type that the checker would have to treat as a word. The keyword form is the same as `tie`, `bridge` and `peripheral`: a word first, then what it is about, and it needs no change to the declaration grammar. The word and the keyword form are accepted as built.
- **`.processor.usb.function &= "NONE"`**, the form a pin takes for no function. It names a pin and a function of a table that has no `NONE`, and it cannot say that an interface of several pins and a rail is unused.
- **A `no_connect` kind** on the interface's pins. It says the pin must stay open for ever, in the part, which is the datasheet's word for a pin that must not be connected, and the USB pins are not that.

## Left open

Not built, and not planned until a board needs it:

- A part stating a function unused by default (a processor in a package without USB). That is a statement about the part and not the board, and a package with no USB pads already has no pins to connect.
- The DC analysis (`crates/netsmith-solve/src/dc`) still reads the rails of an unused interface. No finding of it shows for the test boards of this change; a board where one does is a case to add.
- A module or a part forwarding an `unused` to an interface inside it. A board names only what a module's interface shows (ADR 0004), so it cannot reach an interface inside a module.

From pull request #323, the series parts of a use (`series_use` in `crates/netsmith-check/src/unused.rs`):

- Not built: a series `Inductor`, `Capacitor`, `Diode`/`Schottky` or `Switch` is not followed. A diode ORing VBUS into a rail feeds the board as much as a fuse does, and whether one is a use is not decided.
- Built, on the owner's decisions of 2026-11-10: a series part from the interface to the return `neg`, or to the supply that nothing on the board sources (a pull-down, a bleed resistor, a pull-up), is a pull and no use; a path to a rail that the board sources is a use.
- Built: a series part whose far end reaches another pin of the part that holds the interface (a processor fed from its own VBUS through a bead) is not a use, the same as a direct join inside the part.
