---
status: accepted (user review 2026-11-02)
---

# Track width against current

Builds on ADR 0020 (net classes: `Rail.min_track`), ADR 0007, ADR 0014 and ADR 0027 (the solved currents) and ADR 0030 (manufacturing limits block `fab`, guidelines warn).

## Problem

A rail's `.min_track` sets the width of its nets, and the solver knows the worst-case current the rail draws. Nothing compared them: a 2 A rail drawn at the process's 0.1 mm built and went to the board house, and the track would have been a fuse. A rail that states no `.min_track` is drawn at the process's least, so the commonest mistake is the silent default.

## Decision

**The check.** After the currents are solved, lowering checks each net with at least two terminals. The net's current is the largest magnitude, taking the top of each range, of the things on it: the rails (any interface with a `current` that draws) with a pin on it, and the parts with a `current` whose `bridge` ends on it. The second is how the nets inside a series chain, which belong to no rail, take the chain's current (ADR 0014 equates neighbouring parts of a chain). The net's track width is the widest `.min_track` bottom of the rails on it, and never less than the process's least track; a net no rail states a width for is drawn at the process's least, so a 2 A rail with no `.min_track` is a finding. That is the width the router uses (`Board::track_width`).

The width needed is IPC-2221's formula solved for the width: `I = k * dT^0.44 * A^0.725`, `I` in amperes, `dT` in kelvin, `A` in square mils (width times copper thickness), `k` = 0.048 for a track on an outer layer and 0.024 on an inner layer. The router draws on the outer layers only, so the check uses 0.048 and the outer copper, `stackup.copper_weight` ounces of 0.035 mm (the same unit conversion the board file writer uses). The inner-layer constant is in the code (`Layer::Internal`) and is tested, for the day tracks go on inner layers; nothing uses it now. The relation is nonlinear in the current and the width, so it is a dedicated check and not a relation of the solver. A net that is an inner plane or an outer pour is not checked: it is an area, not a track.

A finding names the net, the current range and what carries it, the track width and where it comes from (a rail's `.min_track`, or the process's least because no rail states one), the copper weight and thickness, the rise and whether it is the default, the current that width carries, and the width needed rounded up to 0.01 mm so that writing the number the message asks for passes.

**The temperature rise.** `.process.max_temperature_rise`, a new `Temperature` field (a new built-in quantity type, kelvin) of `TwoLayer` and `FourLayer`, stated by neither. It is an upper bound like `via_in_pad`: the tool uses the top of the range (`&= 20K`). When the board states none, the check uses **10 K**. That is a stated default of netsmith's, chosen because it is a conservative, commonly used design figure; it is not a number a fabricator publishes, so the process does not carry it, the message says "(the default)", and `std/mfg/jlcpcb.nsm` says so where the field is declared. Copper weight needed no new field: `.stackup.copper_weight` was already in both processes.

**Guidelines warn, with a mechanism.** ADR 0030 accepted that manufacturing-rule violations block `fab` and guideline violations warn, with a key in `netsmith.toml` and a flag for one run. IPC-2221 is a design guide, not a limit of JLCPCB, so it is a guideline. This ADR builds the general mechanism: `[project] guidelines = "warn" | "error"` (default `"warn"`), and `--strict` on `build` and `fab` for one run. A check produces a `Guideline` (rule, message, the instance it is about) and the pipeline decides. Under `"warn"` the finding is a `warning:` line from `build`, a line in `fab-notes.txt`, and a warning in the editor's problems panel at the rail or part (through the existing diagnostics: `Built.guidelines`, no editor change beyond one function); under `"error"` the build stops with one error per finding at the same line. The policy travels in `Catalogs` (`with_strict_guidelines`), as the default assembly of ADR 0028 does, so every command that builds applies it. The editor takes the manifest key and has no flag. The later layout constraints of ADR 0030 (datasheet "put the capacitor next to pin 3") are the next guidelines and use the same mechanism.

## Why IPC-2221 and not IPC-2152

IPC-2221 is the formula in every track width calculator and is easy to state and to test by hand. It is a conservative guide: its curves were fitted to old measurements of tracks on thin boards and ignore the board's thickness, the planes near the track and the copper around it. IPC-2152 is more accurate (it accounts for those) and can allow a narrower track, but it is a family of charts and corrections, not one formula. We implement IPC-2221 only. A track that passes is not thereby safe at the stated rise, and one that fails may be fine; the finding is a prompt to decide, which is why it warns.

## Limits

- Only the largest single current on a net is used. A ground net shared by several rails carries their sum where they join; the rail with the largest current stands for it, and the board input's rail usually is the sum.
- A net is checked at one width, the net's. The router may draw shorter runs narrower where there is no room (a neck-down between pads, a stub at a pad's width), and the pad's own copper carries the last millimetres. Vias are not checked.
- The router's masks reach as far as the width of the track needs: a track wider than a via once reached further than the masks looked, which put 0.6 and 0.9 mm rails too close to other copper (shorts included). Every line the router returns has also been measured as drawn, against the other nets' pads, tracks, vias, holes and the board edge; a track nearer than the rules allow is drawn at the process's least instead, and a line that still is too near is left unrouted.
- Nets that are planes or pours are not checked; a pour's real width at a bottleneck is not measured.
- No ambient temperature, no duty cycle: the worst-case current is taken as continuous.
- Inner tracks are not drawn by the router, so the inner-layer constant is unused.

## Consequences

Boards that were silently under-width now warn. None of the examples did. A project that wants the check to block puts `guidelines = "error"` in its manifest. `Temperature` is a new type name in the language (kelvin only; no temperature curves yet).
