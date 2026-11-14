# two_buck

An invented 12 V board with two buck stages (5 V and 3.3 V), a linear post-regulator (3.3 V from
the 5 V rail), a reverse-polarity Schottky diode and a TVS diode on the input, and the parts
around them: bulk and decoupling capacitors, a bootstrap capacitor and an enable divider for each
buck (the IC's own `support` section, ADR 0115), a power-good pull-up and test point for each buck
(joined to the IC's `PG`, which `BuckIc` does not declare, through `.stage.ic.pg`, ADR 0120), a
power light for every rail, test points, and a header for the input and
one for each output. It has 59 parts in the bill of materials (the two stages' capacitor banks
count: each input takes two capacitors). It is not a design to build. It exists to time the
solver on a board with regulators in it: `docs/solver-timings.md` has the numbers.

It builds with no KiCad (`NETSMITH_NO_KICAD=1 netsmith build`), solves with every generic part
picked and the DC analysis passing, and has a pick lock (`two_buck.lock.toml`, which
`scripts/check-example-locks.sh` checks). It has no physical file, so no board outline and no
`fab` run yet.

```
cd examples/two_buck
NETSMITH_TIMINGS=1 netsmith build --out out    # one line per stage on stderr
```

## What is invented, and why

The buck IC, `BenchBuck` (an SOIC-8, 4.5 V to 60 V, 3 A), is invented, and so is its footprint
mapping in `kicad.toml` (a footprint a generator answers, no symbol: the IC is drawn from its
pin table). Each stage carries 1.5 A at up to 15.8 V. Of the standard library's buck ICs, the
`Lt8641` has no row in its catalog (no stock above zero is recorded) and no standard inductor has
the DC resistance its datasheet asks for (0.04 ohm, the rows have 0.085 ohm and more); the
`Lmr16006y` (0.6 A) and `Rt8059` (5.5 V) do not take the stage at all; and the `Tps54202` now
does, with the standard 1210 inductors (the 3.3 V stage solves with it), so the stages could move
to a standard IC. They have not, yet: the stages join the IC's power-good pin, which needs the IC
named (ADR 0120) and which the TPS54202 does not have, and the board's picks and timings stay as
they were. The move is a follow-up. `examples/buck_converter.nsm` picks a real IC through
`BuckConverter`. `BenchBuck` states
the same kind of numbers (it provides `BuckIc`, writes one relation from its output to its input
current and `dc switching_regulator`, and carries its bootstrap capacitor and enable divider in
its `support` section) and asks for what the standard rows can give. Every other part is a
standard generic picked from the standard catalogs, with no catalog row added.

What the standard parts could not give, so what this board leaves out: a fuse (the one standard
fuse is rated for 6 V), mounting holes (a hole is physical intent and belongs in a physical file,
ADR 0048), a 1.8 V rail (no standard LED lights below 1.8 V) and a post-regulator for any
voltage but 3.3 V (the one linear regulator row is the XC6206 at 3.3 V, 100 mA).

## What the build says

The DC analysis cannot work out the current of the reverse-polarity diode, which joins two rails
that a voltage is stated on (`VIN_RAW` and `VIN`): the board states a current for it
(`.current &= 0A..3A`), so its rating is checked and the analysis takes that current, with no
note. The stage's capacitor ratings against the peak of its rails are not
evaluated, because no rail states a `.peak`. The four feedback-divider resistors are sized by
`Buck`'s default bound on each divider's current, 10 uA to 1 mA (the invented IC states no figure
of its own): 5.1 kohm over 1 kohm (about 0.8 mA) at 5 V and 6.8 kohm over 2.2 kohm (about
0.37 mA) at 3.3 V, the first rows that fit. A board that wants less narrows it,
`.buck_5v.stage.divider.current &= 10uA..100uA`.
