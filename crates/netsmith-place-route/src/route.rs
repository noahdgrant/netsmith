//! A small router: A* over a grid of two copper layers, with vias.
//!
//! On a four-layer board the router draws on the two outer layers only. The inner layers are
//! planes, so a pad on a plane's net is joined to it by a via dropped beside the pad and a
//! short track from the pad to the via; that is all such a pad needs.
//!
//! The grid is 0.1 mm. A cell is open to a track of a net only if the track's copper, and
//! the clearance round it, stays off every pad, track and via of other nets and off the
//! board edge. Every size and space comes from the board's [`Rules`].
//!
//! No track bends by more than 45 degrees (issue #415). The first pass of each try, which gives
//! most lines the copper they keep, searches in eight directions: a state remembers the
//! direction it came in, a turn of 45 degrees costs [`TURN`] (or [`Costs::turn`] of the options)
//! and a right angle [`TURN_RIGHT`],
//! dearer than two bends, and an acute angle or a step back is no move. A diagonal step costs
//! [`DIAGONAL`] against [`STEP`] (14 against 10, the length), passes between the two cells
//! beside it, which must both be open, and the heuristic is the octile distance. The repairs
//! after it (rip-ups, negotiation, the chain) make four fifths of the searches on a dense board
//! and search in four directions, as the router did before. The exact pass over the copper
//! ([`Router::settled`]) then cuts every right angle into two bends of 45 degrees, a diagonal
//! from 0.3 mm down to 0.05 mm from the corner ([`CHAMFERS`]), the farthest that keeps the
//! rules' space; where none does, the line is searched for again in eight directions with
//! right angles that are cut, and last in eight with no right angle at all, and a line that
//! still has none is left, never drawn with the corner. Two places turn more sharply, where
//! the copper shows no corner: inside a pad of the line's own net (the square a round pad of
//! that size holds; also where corners are not cut), and the short stub from a pad's centre to
//! its cell. A via is a state of its own, from which the path may not take a via back at once,
//! so two vias at one place cannot hide a corner. The search queue breaks ties of cost toward
//! the state that has come farthest. The design rules check's `track_angle` (netsmith's own)
//! refuses a corner of more than 45 degrees outside a pad or a via of the net.
//!
//! Measured (2026-11-11): eight directions in every search left RP2350's 61 thousand searches
//! popping 3 billion states, against 853 million in four, and hit the route timeout; with eight
//! in the first pass only, RP2350 routes in 92 s, Orbit in 11 s and the Easyduino in 5 s, all
//! completely, with no right angle (they had 270, 171 and 154), and 1.49, 1.28 and 1.15 times
//! their spanning length against 1.55, 1.41 and 1.45 before.
//!
//! Lines are routed shortest first, except that a line that ends on a pad deep in a
//! fine-pitch grid of pads, which has the fewest ways out, comes before the others (see
//! [`pad_depth`]). A line that cannot be routed is then tried again by ripping up the nets
//! that stand in its way, routing it, and routing those again; lines still left are negotiated
//! for, and then each is given a bounded chain of rip-ups of single lines, each of which may
//! rip up the lines in its own way ([`Router::rip_up_chain`]). If lines are still left, the
//! whole board is routed again with the lines in another order (shortest first, longest
//! first, by net, in reverse net order), at most [`MOST_TRIES`] times in all; on a board with a
//! fine-pitch part, the second try is the first order again with **escape lanes** reserved
//! softly round the part's pads, and every later try has them too (see "Escape lanes"). From the
//! second try on, every order also keeps the lines' classes in turn (see "Classes of lines").
//! The first complete result is kept, or failing that the one that leaves fewest lines. There is no
//! push-and-shove, so a board can still end partly routed, and what is left is returned.
//!
//! The work is bounded. A line is searched for in a window round its pads first, and in a
//! larger one only if that finds nothing. Before a search, a cheap flood from both ends finds
//! out whether the ends are joined at all, so a pad that its neighbours wall in is given up on
//! at once instead of after a search of the whole board. The router makes a fixed number of
//! tries and rip-up passes, and stops at the time limit in [`RouteOptions`], returning what is
//! left.
//!
//! # Why the router is shaped so
//!
//! This is ADR 0031, accepted after the owner's review of 2026-11-04. The parts that were
//! provisional (the rip-up chain, below) are accepted as built by the owner, 2026-11-11.
//!
//! A real board has a module whose pads are 0.6 mm squares on a 1.1 mm pitch, and parts on both
//! faces. The first router left the pads inside the module unrouted: it kept half a cell and a
//! margin beyond the rules from every obstacle, which closed the 0.5 mm between two pads, and a
//! track with its two spaces needs 0.3 mm. A line is routed in these steps, each paid for only
//! by the lines the step before left:
//!
//! 1. It is searched for with the old room (the rules, [`MARGIN`] of 0.05 mm and half a 0.1 mm
//!    cell).
//! 2. A line left is tried again with 0.02 mm beyond the rules ([`ROOM_TIGHT`]), which a track
//!    between two open cells and a stub from a pad's centre to the nearest cell stay inside
//!    of. A board that routes at the old room routes as it did.
//! 3. A pad still walled in is reached by a **via in the pad**, if the process allows it
//!    (`.via_in_pad`, the largest via the fabricator fills with epoxy and caps with plating).
//!    The via is at the pad's centre, no wider than the pad's smaller side, and is checked at
//!    that exact place against all other copper and holes. A path that starts or ends on a
//!    pad's far layer pays 60 mm of track for it, so it is the last resort. A pad on a plane
//!    net that cannot get out gets the via and no track.
//! 4. Where the board draws a track wider than the process's least, the tight try also looks
//!    for cells where a track of the least width fits, and a track through them is **necked
//!    down** to the least width there and full width elsewhere.
//! 5. Lines still left are **negotiated** for (PathFinder in a small form): each is routed
//!    where it would go if no other net were in the way, the nets it comes near are ripped up,
//!    the cells they fought over cost more from then on, and what was ripped up is routed
//!    again. Six rounds, and the best state is kept. The earlier
//!    rip-up put the nets back if the line did not fit; this one lets lines fail for a round so
//!    the others can move.
//!
//! There is no fan-out stage that puts a via beside each pad of a dense footprint before
//! routing: the search already puts a via on any free cell next to a pad, and a pad with no
//! free cell beside it is the case a via in the pad is for. A fan-out stage was tried on a
//! branch (PR #85, not merged; it was `RouteOptions::fan_out`, off by default):
//! each pad of a fine-pitch part (at least 8 surface-mount pads, its
//! box narrower than a via with the process's space on each side, another pad nearer than
//! that) got a fixed track to the nearest cell where a via may stand, deepest pads first. On
//! the invented tight board below it was worse, so it was not kept: 89 lines left over 11
//! placed seeds against 41 with no stage, and 53 for the inner ring only.
//! Each fixed escape is a choice the router no longer makes: the nearest cell for a via is not
//! the end that suits the line. A reservation the negotiation may take back is the escape lanes
//! below; push and shove is not tried.
//!
//! What came after, each measured on invented boards (nothing here is a real design):
//!
//! - **The nets least in the way.** The search for the nets to rip up ([`Router::contested`])
//!   crosses other nets' copper at a price ([`SOFT`], 100 a cell, a via 300) and not at none,
//!   so only the nets on the path with the fewest crossings are ripped up. Before, the
//!   negotiation ripped up every net its ideal path came near, 12 to 20 nets for a line
//!   crossing the module's neighbourhood.
//! - **Fixed copper counted once** (`Base`). The masks stamped every pad, edge and keepout for
//!   each of 30 000 to 40 000 searches of a hard board; the fixed obstacles are counted once
//!   for each room and track width and a net's own pads are taken away again for that net.
//! - **Lines that end deep in a fine-pitch grid go first** ([`pad_depth`]: at least eight
//!   surface-mount pads, a gap under a via and two spaces, counted in pitches from the outside
//!   of the part's pads). Such a pad has few ways out and a short line of a part beside the
//!   grid can take the gap it needs. A board with no such grid has the old first order.
//! - **A last chain of rip-ups of single lines** ([`Router::rip_up_chain`]), after the
//!   negotiation of each try, for each line still left. A net's other lines stay where they
//!   are, and lines routed for a line above are locked below it (a lock, not a reservation:
//!   this ends a cycle of two lines). Bounded by [`CHAIN_SEARCHES`] 600 for a line and
//!   [`CHAIN_TOTAL`] 3000 for a try of the board, depth 4, whatever the time limit. Nothing in
//!   it is random, ties go by line number, and the same board gives the same copper.
//! - **`.kicad_pro` gives the Default net class a `priority`.** Without it KiCad 9 ignored the
//!   Default class's clearance and checked every board against its own 0.2 mm (measured: a
//!   0.1 mm project reported "clearance 0.2000 mm"). One added line in every `.kicad_pro`.
//! - **`fab` with connections left** writes the board file, the project file, the Gerbers, the
//!   bill of materials, the placement file and `fab-notes.txt` with an `ERROR: INCOMPLETE`
//!   line at its head and one `ERROR: not routed` line for each connection, and
//!   `routing-failure.txt` (each connection with its net, its two pads and their places, and
//!   the parts most of them end on). The exit status is an error as before.
//!
//! # Escape lanes (issue #431, step 3)
//!
//! RP2350 (`examples/rp2350_minimal`) leaves lines only at U1, a 0.4 mm QFN-60 ringed by
//! decoupling capacitors, whose lines dropped vias against its pins and closed each other's
//! way out; most of the board is empty. Main routed it completely at its own turn cost, 25, and
//! one try left 2, 0, 2 and 2 lines at 24, 25, 26 and 27: completion was luck.
//!
//! **What is built** ([`Lanes`]). Each pad of a part with at least 16 surface-mount pads, two
//! of them 0.55 mm apart or nearer, facing at least three sides (not a connector's row), has a
//! lane: the cells straight out from the pad's edge, away from the part, on the pad's layer,
//! as wide as the pad and [`LANE_LENGTH`] 1.5 mm long, past the ring of capacitors. A track of
//! another net pays [`LANE_TRACK`] 10 a step in it (a step is 10); a via of any net, the part's
//! own included, pays [`LANE_VIA`] 300 (two vias) where its copper and space would close a lane
//! (within half a via, the clearance and half the least track of it), except the lane's own
//! net at the lane's end, where its line leaves the ring. The two nets of a diff pair use each
//! other's lanes as their own. A cost, not a wall: a line with no other way crosses a lane.
//! **When**: not on the first try, whose copper is that of main byte for byte, but on the second,
//! which routes the first order again with the lanes, and on every try after it (so a board
//! with lanes may have six tries). A board that routes on its first try is unchanged.
//!
//! **Measured** (2026-11-12, main at ec7d165's placements, `netsmith route --margin 1 --tries N
//! --route-timeout 0 --turn T`; lines left at T = 24 / 25 / 26 / 27):
//!
//! | | RP2350, one try | RP2350, two tries | Orbit (one try) | EasyDuino (one try) |
//! |---|---|---|---|---|
//! | main | 2 / 0 / 2 / 2 | 0 / 0 / 2 / 2 | 1.25x, 87 vias | 1.18x, 118 vias, pair 79 % |
//! | lanes on every try, the part's own vias pay | 0 / 1 / 0 / 0 | | 0/0/0/0, 1.27-1.31x, 90-96 vias | 1.26x, 119 vias, pair 38 % |
//! | the same, own vias free | 1 / 2 / 2 / 2 | | 1.25x, 87 vias | 1.18x, 118 vias, pair 79 % |
//! | own vias pay 50, 100, 150 | | | 1.27-1.33x | 1.19-1.21x, 125 vias, pair 10 % |
//! | the same, lanes 1.0 mm | 1 / 1 / 1 / 2 | | 1.27x | 1.17x, 119 vias, pair 22-70 % |
//! | lanes on every part (a connector's row, a 1.27 mm module) | 1 / 3 / 3 / 3 | | 1.24x | 1.24x, 135 vias |
//! | lanes from the second try, orders as before | = main | 1 / 0 / 1 / 0 | | |
//! | **the first order again with lanes (built)** | = main | **0 / 0 / 0 / 0** | | |
//!
//! Orbit leaves 1 line at 26 and 27 after one try; with two tries main still leaves it and the
//! built form routes it (1.31x, 96 vias). Orbit and EasyDuino route on their first try at the
//! default turn cost, so their scorecard and their robustness are main's. The parts' own vias are
//! what RP2350 needs charged (free, it leaves more than main) and what EasyDuino's CP2102N
//! needs free (any cost moves its USB pair off its partner); no cost of one try served both,
//! which is why the lanes are the second try's. A try of RP2350 takes 4 to 10 minutes, so under
//! `fab`'s 300 s the second try runs only on a smaller or faster board: the lanes make the
//! board's later tries finish, not its first.
//!
//! # A coarse plan (issue #431, S10)
//!
//! [`RouteOptions::plan`] can have the router make a global route of every line over tiles of
//! 1 mm first, negotiated for the tiles' capacities, and the detailed search then pays up to
//! [`OFF_PLAN`] for each step outside its line's corridor on a tile the plan crowds. It is off
//! by default: on the three ported boards it makes every board route at every turn cost, but
//! costs Orbit 10 to 18 % more vias. The design, the numbers and what to try next are in the
//! header of `plan.rs`.
//!
//! # What a try costs (issue #431)
//!
//! A try of RP2350 (`examples/rp2350_minimal`) took 150 to 250 s on a dense two-layer board
//! with a QFN-60, against `fab`'s route timeout of 300 s, so `fab` got one try after the
//! margin trial, and whether the board routed was luck. Exact speed-ups of a single search
//! had been measured to gain nothing (S3, 2026-11-11), so this is about how many searches a
//! try makes and how much of each is wasted. Counted per stage (2026-11-12), at a turn cost of
//! 25: the negotiation re-routed 9 900 lines for 118 negotiated, and never left fewer lines
//! than it started with; at 24, 26 and 27 the chain spent two thirds of its states in searches
//! with no right angle that had no path, each running out of states in each window in turn.
//! Each change below leaves the copper of a board as it was, except the last, which leaves it
//! on the boards measured:
//!
//! - **The neck boxes are kept** ([`Router::necks`]): asking [`Board::neck_box`] at every cell
//!   made the margin trial (signals at 0.15 mm) spend 58 s on masks and checks.
//! - **Stale entries of the search queue are skipped**: a state queued again at a lower cost
//!   comes off the queue first, so its dearer entries relax nothing (12 to 17 % of states).
//! - **A flood before a search with no right angle** ([`reachable`]), over the same states and
//!   moves ([`moves`]): it says that there is no path for much less than a search that runs
//!   out of states.
//! - **A memo of searches** ([`Router::memo`], [`Router::print`]): a search in a window whose
//!   print (the other nets' copper within reach, hard or soft, the net's own vias, the
//!   negotiation's history of the cells) is unchanged finds what it found before. The joins
//!   with the net's own tracks are checked each time. A quarter of the repairs' searches.
//! - **Not kept: the negotiation stopping after a round that does not improve.** On RP2350 at
//!   0.15 mm no round ever did, so it saved time there with the same copper. But at the least
//!   track (0.1 mm), Orbit's first try routed completely only through a round that improved
//!   after one that did not: with the early stop it left a line after six tries. All six rounds
//!   are run.
//!
//! Measured on the same machine, back to back with main at fda6cd4f, the copper byte-identical:
//! RP2350's `fab` takes 136 and 133 s against main's 138 and 166 s (a loaded machine, load
//! average 6 to 12), and Orbit routed at the least track from a saved placement
//! (`netsmith route --margin 1 --route-timeout 0`) takes 18.5 s against 32.6 s. With the
//! negotiation's early stop as well (not kept, above), one RP2350 try took a third to two
//! fifths of main's time: most of a try at 0.15 mm was negotiation rounds that changed nothing.
//!
//! Not done: a cache keyed on the whole board (searches repeat only where little changed),
//! smaller first windows (a line found in 4 mm is cheap already), fewer soft searches (one a
//! failed line a pass already), and incremental masks (12 % at most, S3).
//!
//! # Diff pairs side by side (issue #431, S9)
//!
//! A diff pair is two nets whose names differ only in a suffix pair of the scorecard's
//! `PAIR_SUFFIXES` (`dp`/`dm`, `D+`/`D-`, `_P`/`_N`, `.p`/`.n` and the like), so the fields
//! of a std interface such as `Usb2.dp` and `.dm` make one with no marking (decided by the agent
//! under the owner's 2026-11-11 delegation: a marking would be a language choice, and the names
//! already say it).
//!
//! **What is built** ([`Router::side_by_side`]). When a try has routed every line, the lines of
//! each net of a pair, the second net and then the first, are routed again against all the other
//! copper, where a step within [`PAIR_NEAR`] (the scorecard's coupled 0.5 mm) of the partner's
//! track, and no nearer than the rules allow, costs [`FOLLOW`] (half) of its length, and the
//! search's estimate is cut to match, so the search goes out of its way for it. The new copper
//! is kept only if more of the net runs coupled and it is no more than [`PAIR_SLACK`] 0.2 mm
//! longer, with no more vias ([`keeps_pair`]). A line is never left and no other net moves, so
//! a board routes exactly as completely as before; the pass takes 0.1 to 0.3 s.
//!
//! **Measured** (2026-11-12, `netsmith fab`; coupled, skew; length over the spanning length,
//! vias). On main at 1852adb8 (the escape lanes and #550's cheaper try):
//!
//! | | RP2350 `mcu.usb` / `usb` | Orbit `usb` | EasyDuino `bridge` |
//! |---|---|---|---|
//! | main | 64 %, 15.07 mm / 0 %, 0.86 mm; 1.40x, 137 vias | 0 %, 0.77 mm; 1.28x, 87 vias | 23 %, 0.75 mm; 1.22x, 105 vias |
//! | **the pass (built)** | **69 %, 13.50 mm** / 0 %; 1.40x, 137 | = main | **54 %**, 0.85 mm; 1.22x, 105 |
//! | the pass, skew kept from growing | = built | = main | = main (nothing kept) |
//!
//! Orbit's pair would run 35 % and 75 % coupled, but only with 2 vias more, which the pass
//! does not take. On main before #548 moved the examples' parts (at e9e558b7), where the forms
//! were compared:
//!
//! | | RP2350 `mcu.usb` / `usb` | Orbit `usb` | EasyDuino `bridge` |
//! |---|---|---|---|
//! | main | 7 %, 1.59 mm / 0 %, 0.60 mm; 1.49x, 160 vias | 4 %, 1.74 mm; 1.25x, 87 vias | 79 %, 1.19 mm; 1.18x, 118 vias |
//! | the discount in every search of every try, estimate kept | 35 %, 0.12 mm; 1.52x, 174 vias | 21 %, 1.41 mm | **48 %**, 1.72 mm, 123 vias |
//! | from the second try (the lanes' way) | = main: every board routes on its first try | = main | = main |
//! | the pass (built) | 7 % / **18 %**, 0.69 mm; 1.49x, 160 | **27 %, 0.73 mm**; 1.24x, 87 | **85 %, 0.93 mm**; 1.18x, 118 |
//! | the pass, the estimate kept | | 26 %, 0.73 mm | 79 % (nothing kept) |
//! | the pass, a slack of 0.05 mm | 7 % / 0 % (nothing kept) | 27 % | 85 % |
//!
//! Robustness (one try at turn costs 24 to 27) is main's: RP2350 0 / 0 / 0 / 1, Orbit and
//! EasyDuino 0 / 0 / 0 / 0 lines left, and at the least track (`netsmith route --margin 1`) every
//! board routes completely, as on main: the pass runs only on a try that routed every line and
//! never leaves one. In every search, earlier (before the lanes), the discount with the
//! estimate cut to match left RP2350 with 2 connections at 300 s; in the pass the same pull
//! costs nothing it does not gain.
//!
//! # Track off other nets' pours (issue #431, S7)
//!
//! A pour on one outer layer of a two-layer board (GND on B.Cu on RP2350 and Orbit) is the
//! return path of the signals over it, and every other net's track on it cuts it.
//!
//! **What is built** ([`Router::off_the_pours`]). When a try has routed every line, after the
//! pairs' pass, each line with track on an outer layer another net pours, while the other outer
//! layer is free of other nets' pours, is routed again, the most such track first, against all
//! the other copper, at the usual room and failing that at the least. In that search a step on
//! the pour costs [`POUR`] more (twice its length) and a via [`POUR_VIA`] more (60 mm of track
//! with [`VIA`]), and the new copper is kept only if it costs less so counted. A line is never
//! left, so a board routes exactly as completely as before; a board with both outer layers
//! poured (EasyDuino) or neither is unchanged. The nets of a diff pair are left alone: moved
//! by this pass, Orbit's `usb.dp` came out 7.7 mm shorter than `.dm` (skew 0.77 to 6.87 mm).
//! It runs after the pairs' pass for the same reason: before it, the other nets it moved let
//! the pairs' pass shorten `usb.dp` the same way. One sweep ([`POUR_SWEEPS`]).
//!
//! **Measured** (2026-11-12, main at d07d6e8f with S9, `netsmith fab`, the same machine back to
//! back; other nets' track on the pour, length over the spanning length, vias, `fab` time):
//!
//! | | RP2350 | Orbit | EasyDuino |
//! |---|---|---|---|
//! | main | 646 mm, 1.40x, 137 vias, 122 s | 466 mm, 1.28x, 87 vias, 14.5 s | 504 + 352 mm, 1.22x, 105, 6.8 s |
//! | **the pass (built)** | **423 mm**, 1.29x, 129 vias, 117 s | **304 mm**, 1.22x, 87 vias, 15.6 s | = main, 7.0 s |
//! | the pass before the pairs' | 423 mm, 1.29x, 129 | 294 mm, 1.21x, 87, pair skew 6.87 mm | = main |
//! | three sweeps | 407 mm, 1.26x, 123 vias | 286 mm, 1.21x, 86 vias | = main |
//!
//! The same board gives the same copper on every run. A second `fab` of each, at a load of 6 to
//! 10 against 2 to 3, took 130 / 26 / 9 s on main and 154 / 32 / 12 s with the pass. The pass
//! itself, from one try of each (`netsmith route --tries 1 --route-timeout 0`, six at once):
//! 6 to 10 s of RP2350's 170 to 185 s, 4 to 12 s of Orbit's 17 to 48 s, nothing on EasyDuino.
//! Three sweeps took 5 s more on RP2350 and 19 s more on Orbit (doubling its try) for a sixth of
//! the first sweep's gain, so one sweep is kept. Robustness (one try at turn costs 24 / 25 / 26 /
//! 27, at the placed margin and at `--margin 1`) is main's: RP2350 0 / 0 / 0 / 1 lines left at
//! both, Orbit and EasyDuino 0 / 0 / 0 / 0. Every pair's coupling and skew are main's on every
//! run.
//!
//! Earlier forms, on main before #548 and #550 (782 and 458 mm on main then): the pour's cost in
//! every search of every try left RP2350 with 9 connections at 300 s (Orbit 152 mm, 131 vias);
//! from the second try only it changed nothing, as every board routes on its first; vias at no
//! extra cost in the pass moved more of Orbit (210 mm) for 28 vias more; and a line that cannot
//! move ripping up the nets in its way moved no line. The pass does not reach the epic's target
//! of 300 mm on RP2350: what is left is round U1, whose front is full, and the soft search
//! for the nets in a line's way finds none whose rip-up would free it.
//!
//! # Classes of lines (issue #416, S5 of #431)
//!
//! Every line has a [`Class`], derived from the board with nothing in the source, in a fixed
//! order: escape (to a plane, or a pad deep in a fine-pitch grid, [`pad_depth`]), decoupling (a
//! capacitor's pad to the pin it decouples, [`crate::quality`]), pair (a net of a diff pair),
//! clock (a crystal's net, or one named `clk`, `xin`, `xout`, `xtal`, but no plane's or pour's),
//! power (at least [`POWER_CURRENT`]) and the rest.
//!
//! **What is built** ([`RouteOptions::classes`], [`LineClasses::FromSecondTry`] by default).
//! The first try is the router's own, with no classes, so a board that routes on it has the
//! copper of main byte for byte. From the second try on, which routes the first order again (as
//! the escape lanes do), every order keeps the classes in turn and each class in its own order
//! ([`Router::classed_order`]), and the copper of a decoupling, pair or clock line is never
//! ripped up for a line of a later class: in the soft searches for the lines in a line's way it is
//! hard, and its net is never named ([`Router::protected`], [`Class::protected`]); the pass off
//! the pours leaves those lines too. Escape and power lines keep their place in the order only.
//! **Decided** by the agent under the owner's delegation of 2026-11-13 (option 2 of the S5
//! comment): protecting every class left RP2350 unrouted or cost Orbit a line at every turn cost,
//! and the classes on the first try made RP2350's one try less robust.
//!
//! **Measured** (2026-11-13, main at 0e05e4d9 with S7, the placed boards of `fab`; lines left at
//! turn cost 24 / 25 / 26 / 27, `netsmith route --route-timeout 0 --tries N --turn T`, at the
//! default margin and at `--margin 1`; `every` is [`LineClasses::EveryTry`]):
//!
//! | | RP2350 one try, default / margin 1 | RP2350 two tries, default / margin 1 | Orbit, EasyDuino |
//! |---|---|---|---|
//! | main | 0/0/0/1, 0/0/0/1 | 0/0/0/0, 0/0/0/0 | 0/0/0/0 everywhere |
//! | **from the second try (built)** | = main, the same copper | 0/0/0/0, 0/0/0/**1** | = main, the same copper |
//! | every try | **1/1**/0/0, **1/1**/0/1 | 0/**1**/0/0, 0/**1**/0/**1** | 0/0/0/0; EasyDuino 108 vias against 105 |
//!
//! `fab` writes the same board as main for all three (each routes on its first try), and
//! `netsmith route --margin 1` at `fab`'s 300 s routes all three; with classes on every try it
//! leaves RP2350 a line at the timeout. Where the second try ran (RP2350, default margin, two
//! tries, T = 26 and 27), it routed the board at its track margin (signals at 0.15 mm, where main
//! falls back to 0.1 mm), with the longest decoupling track 22.1 mm against 36.1 and 35.3 mm (mean
//! 7.3 against 10.4 and 9.2), 1.28x the spanning length against 1.33x and 1.30x, `mcu.usb` 75 %
//! coupled against 80 and 86 % (skew 12.2 mm against 13.9 and 12.7), and 143 vias against 141 and
//! 130. At `--margin 1` and T = 27 the second try leaves a line that main's (lanes only) routes;
//! with three tries it routes (129 vias against main's 130).
//!
//! # What the router says about a board it leaves (issue #431, S3b)
//!
//! An agent cannot fix what it cannot see, so [`Routing`] says why each line is left and where
//! the board was fought over. None of it changes the routing: the same board gives the same
//! copper with or without it.
//!
//! - **Why** ([`Routing::why`], a [`LineReport`] for each line the best state leaves). The stage
//!   it last failed in ([`Stage`]) is recorded wherever a search for copper, not a soft one,
//!   fails for a line ([`Router::try_line_locked`]), on the try that gave the result; a search
//!   the time limit stops, and a line never tried before it, is out of time. A line ripped up
//!   and not routed again in a rip-up that is put back also counts: it was tried and failed
//!   there. The nets in its way are those of [`Router::contested`] on the final state at
//!   [`ROOM_TIGHT`], one soft search for each line left and nothing more; after the time limit
//!   that search stops at once and names none.
//! - **Crowding** ([`Routing::crowding`], [`Crowding`]): squares of 1 mm (ten cells), counting
//!   each soft path (every search for the nets in a line's way: rip-up, negotiation, whose
//!   `learn` is of the same path and is not counted again, and the chain) once in each square it
//!   passes through, over all tries of a run. A counter a path cell; the searches for `why`
//!   are not counted.
//! - **Snapshots** ([`RouteOptions::snapshots`], off by default): the copper and the lines left
//!   after the first pass, the rip-ups, the negotiation and at the end of the try that gave the
//!   result, for pictures of each stage. A clone of the copper per stage, so only on request.
//! - **Costs** ([`RouteOptions::costs`], [`Costs`]): the turn cost the search uses, the
//!   router's own by default, so that a tool can route with it moved by one unit and see how
//!   much a result depends on luck.
//!
//! The boards. `tight_board` (`tests/common/mod.rs`): 99 parts (80 0402 resistors and
//! capacitors, 18 SOT-23 regulators, half of each on the back) round one module of 80 pads,
//! 0.6 mm square on a 1.1 mm pitch in two rings round a hollow centre, four layers, 106 nets,
//! squeezed by the placer into 27 x 25 mm; at 50 x 42 mm it routes completely in under a
//! second. `mixed_board` (`tests/mixed.rs`): 101 parts on 35 x 60 mm, a QFN of 48 pads and one
//! of 32 at 0.5 mm pitch with exposed pads, parts at most 4 mm high, both faces. Both are
//! measured by `tight::measured` and `mixed::measured` (`--ignored --nocapture`, with
//! `NETSMITH_AREA` and `NETSMITH_SEEDS`). Lines left, seeds 0 to 11, release build:
//!
//! | | tight, 27 x 25 mm | mixed, 35 x 60 mm |
//! |---|---|---|
//! | before ADR 0031 | 2 of 11 seeds done, 46 lines left | 0 of 12, 96 left |
//! | the nets least in the way | 2 of 11, 41 left | |
//! | the placer keeps the room two busy parts need (`anneal.rs`) | unchanged | 1 of 12, 28 left |
//! | the chain of single-line rip-ups | 8 of 12, 8 left | 12 of 12, 0 left (24 seeds, 12 not used to choose anything) |
//!
//! On the invented board a change of one or two lines on one seed is no signal (another order
//! of lines changes a seed by plus or minus two or three); 89 against 41 over eleven seeds is.
//! Every board passes the design rules check on what it routed.
//!
//! **Tried and not kept.** Starting the next try with the lines the last left: no seed
//! changed, the time doubled. A finer grid (0.05 mm cells, margin 0.02 mm): 3 seeds left 0, 2,
//! 2 against 1, 5, 2 for the coarse grid at twice the time, and it changes every existing
//! routed board. In the placer, a soft cost for a part on the other face under a fine-pitch
//! grid: 65 lines left against 46. More negotiation rounds (50 against 6) help by chance, not
//! by design, and the cost of a history cell (10 to 400) made no difference; forty rounds,
//! another restart radius (2.5, 5 mm), ripping up only the lines whose copper the cheapest
//! path comes near: no gain. Constants of the search (via 60 or 300 against 150, turn 8 or 60
//! against 25, soft crossing 40 or 300 against 100): none is a signal. A soft cost in front of
//! fine-pitch pads: 71 to 66 and 64 lines, since the cause is the room between the parts. A
//! first form of the chain that ripped up everything within 0.35 mm of the path, layer-blind,
//! left 3 lines; depth 5 and 1000 searches left 7 of a different form: the set-by-set order
//! (crossed lines, then within 0.1 mm, then 0.25 mm, then each alone) is not a measured optimum
//! but the form that did not depend on one threshold.
//!
//! **What is in the way** (read from cell maps, not proved). Every line left can be routed if
//! all other nets' copper is removed, and one or two nets near its pad are enough to free it,
//! but those nets then cannot be routed again (cycles of two or three nets). The gaps between
//! pads take one track each and no via fits in a gap, so a pad escapes by a track in a gap or
//! a via in the pad to the other face, where the space under the module is full. A pad of the
//! outer ring next to the board edge can be sealed in a pocket by two other nets' vias, one on
//! each side of the pad.
//!
//! **Choices** (the conservative one is what is built, accepted as built by the owner,
//! 2026-11-11):
//!
//! - Rip up single lines, not nets. A net's other lines stay where they are. The alternative,
//!   nets as before, keeps the connection of a net's pads together but ripped up 3 to 20 lines
//!   at a time here.
//! - Depth 4, 600 searches a line, 3000 a try. The numbers are tuned on the two invented boards
//!   over 12 seeds, and 12 more seeds of the first that were not used for tuning came out the
//!   same. A deeper chain finishes more of the tight board's last lines at more time; the
//!   bound exists so that time stays bounded whatever the board, and a lower one gives back
//!   some of the gain.
//! - A line of this chain is a lock, not a reservation. A push and shove router would move
//!   their copper a little and keep them routed: more code (a track moved must keep its
//!   clearance to everything beside it), and not needed on the board that measured it.
//!
//! **Not done** (issue #3 is open): the invented tight board in 27 x 25 mm still leaves 1
//! to 3 lines on the module on 4 of 12 seeds. A router that can move a track (push and shove),
//! or reserve a pad's way out before the others are routed without fixing where it goes, and
//! the placer weighing the capacity of the strip between a module and the edge, are not done.
//! A board of this density with a fine-pitch grid next to an edge is the case none of this
//! reaches.

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use netsmith_pcb::board::{Board, CopperLayer, Track, Via};
use netsmith_pcb::geometry::{Pt, Rect};
use netsmith_pcb::ratsnest::{Ratline, Ratsnest};

/// The distance between the cells' centres, in mm.
const PITCH: f64 = 0.1;
/// How much nearer than the rules an exact measurement of finished copper may come, which is
/// only the rounding of the numbers.
const EXACT_SLACK: f64 = 1e-6;
/// Space kept over the rules, which covers the short stub from a pad's centre to the
/// nearest cell.
const MARGIN: f64 = 0.05;
/// How many times the nets in the way of a line are ripped up for it.
const RIP_UP_PASSES: usize = 3;
/// How much room a track keeps beyond the rules, as a mask is made: half a cell, which covers
/// a track running between two cells that are both open, and the margin on every obstacle.
const ROOM_LOOSE: f64 = PITCH / 2.0;
/// The least room that is still safe: a track between two open cells can come nearer an
/// obstacle than either cell by a few thousandths of a mm, and a stub from a pad's centre to
/// the nearest cell is inside the pad. A line that cannot be routed with the room above is
/// tried again with this, which opens the gaps between fine-pitch pads (a 0.6 mm pad on a
/// 1.1 mm pitch leaves 0.5 mm, and a track with its spaces needs 0.3 mm).
const ROOM_TIGHT: f64 = 0.02 - MARGIN;

/// How far round the ends of a line, in mm, its first searches are confined to; then the
/// whole board.
const WINDOW_MARGINS: [f64; 2] = [4.0, 16.0];
/// How often progress is reported while routing, at most.
const REPORT_EVERY: Duration = Duration::from_secs(2);

/// How many rounds of negotiation the lines left over get, and what a cell the lines fought
/// over costs for each time they did, and the most it counts.
const NEGOTIATION_ROUNDS: usize = 6;

const HISTORY_COST: i64 = 40;
const HISTORY_MOST: u8 = 25;

/// The last resort for a line that is still left ([`Router::rip_up_chain`]): how many levels of
/// lines ripped up for lines ripped up it goes down, how many searches it may spend on one
/// line and on all lines of one try of the board, and how far from the path (mm beyond the
/// least the masks keep) the copper of a line may be for the line to be ripped up: first the
/// lines the path crosses, then those within 0.1 mm of it, then within 0.25 mm.
const CHAIN_DEPTH: usize = 4;
const CHAIN_SEARCHES: usize = 600;
const CHAIN_TOTAL: usize = 3000;
const CHAIN_NEAR: [f64; 3] = [0.0, 0.1, 0.25];

/// The most times the whole board is routed, each with the lines in another order: the lines
/// of a fine-pitch grid first, shortest first, longest first, by net and in reverse net order.
/// A board that routes on the first order is routed once; the fifth order (reverse net order)
/// was dropped while there were four, and it finishes the invented mixed board on seed 2 since
/// the via became 0.2 in 0.45 mm (issue #431), so it costs time only on a board that the other
/// four leave unfinished. A board with escape lanes has one try more ([`route_with`]).
const MOST_TRIES: usize = 5;

/// What a move costs, in tenths of a millimetre of track, straight and diagonal.
const STEP: i64 = 10;
const DIAGONAL: i64 = 14;
/// The share of its length a step of a pair's net costs on a cell beside its routed partner, as
/// a fraction, in [`Router::side_by_side`]: half (issue #431, S9). The search's estimate is cut
/// as much, so the search goes out of its way to run beside the partner; the pass keeps the
/// new copper only if it is no longer than the old.
const FOLLOW: (i64, i64) = (1, 2);
/// How far from the centre line of a pair's partner a cell counts as beside it, in mm: the
/// scorecard's coupled distance.
const PAIR_NEAR: f64 = netsmith_fab::score::COUPLED;
/// What a bend of 45 degrees costs by default ([`Costs::turn`]).
const TURN: i64 = 25;
/// What a right-angle turn costs, which the router cuts into two bends of 45 degrees after the
/// search: dearer than two of them, so that a path bends by 45 degrees where it can.
const TURN_RIGHT: i64 = 60;
const VIA: i64 = 150;
/// A via standing in a pad costs a process step the fabricator charges for, so a path uses
/// one only when no path without is shorter than this much more: 60 mm of track.
const VIA_IN_PAD: i64 = 600;
/// What it costs to put a track, or a via, through a cell another net's copper keeps it
/// from, when the router looks for the nets least in the way of a line: a track crosses a
/// few cells of another track, so crossing one costs several times this.
const SOFT: i64 = 100;
const SOFT_VIA: i64 = 300;
/// What a step of a track, and a via, of another net costs beyond its length in a pad's
/// escape lane ([`Lanes`]): enough that a track goes round another pin's lane where the way
/// round is short, and a via stands off it, but not a wall.
const LANE_TRACK: i64 = 10;
const LANE_VIA: i64 = 300;
/// How far a pad's escape lane runs out from the pad's edge, in mm: past the ring of
/// decoupling capacitors round a fine-pitch part.
const LANE_LENGTH: f64 = 1.5;
/// What a track pays a step, beyond the step, outside its corridor of the coarse plan, on a tile
/// the plan fills on that layer ([`crate::plan::crowding_cost`]): two steps.
const OFF_PLAN: i64 = 20;
/// What a step costs beyond its length, in the pass that takes track off other nets' pours
/// ([`Router::off_the_pours`]), on an outer layer another net is poured on while the other outer
/// layer is not: the same again, so that a track cuts the pour (the return path) only where the
/// way round is more than twice as long (issue #431, S7).
const POUR: i64 = 10;
/// What a via costs beyond [`VIA`] in that pass: a line moved off the pour by a via pair pays
/// 60 mm of track for them, so the pass does not trade a short cut of the pour for vias.
const POUR_VIA: i64 = 450;
/// How many times that pass goes over the lines with track on another net's pour.
const POUR_SWEEPS: usize = 1;
/// A step through a cell where the track has to be narrower costs this much more: a necked
/// down track carries less and is harder to make, so it is kept short.
const NECK: i64 = 30;
/// How far from the pads of a part that one of a line's ends is on, in mm (from the box round
/// the centres of the part's pads), a track may be necked down to the process's least width
/// whatever its current: the escape from a fine-pitch part, whose pads are no wider than the
/// neck and carry the same current into it. Elsewhere a track is no narrower than its net's
/// current needs (issue #431).
pub const NECK_REACH: f64 = 1.0;

const FRONT: usize = 0;
const BACK: usize = 1;
/// The four directions a flood ([`link`]) spreads in.
const MOVES: [(isize, isize); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
/// The eight directions a path moves in, in turn round the compass (issue #431, #415): a step
/// from one to the next is a bend of 45 degrees, the only bend a path makes. Then "none", for
/// the first step of a path, and "through a via", for the first step on the other layer, from
/// which a path may not take a via back.
const MOVES8: [(isize, isize); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
const NO_DIRECTION: usize = 8;
const AFTER_VIA: usize = 9;
/// The directions a search state remembers.
const DIRECTIONS: usize = 10;

fn layer_of(index: usize) -> CopperLayer {
    if index == FRONT {
        CopperLayer::Front
    } else {
        CopperLayer::Back
    }
}

/// Something that copper must keep clear of.
#[derive(Clone, Copy)]
enum Form {
    Rect(Rect),
    /// A track: its two ends, and half its width.
    Segment(Pt, Pt, f64),
    Circle(Pt, f64),
}

impl Form {
    /// How far a point is from the form's edge, zero inside it.
    fn distance(&self, p: Pt) -> f64 {
        match self {
            Form::Rect(r) => {
                let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0.0);
                let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0.0);
                dx.hypot(dy)
            }
            Form::Segment(a, b, half) => {
                let ab = *b - *a;
                let len2 = ab.x * ab.x + ab.y * ab.y;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
                };
                let nearest = Pt::new(a.x + ab.x * t, a.y + ab.y * t);
                (p.distance(nearest) - half).max(0.0)
            }
            Form::Circle(c, radius) => (p.distance(*c) - radius).max(0.0),
        }
    }

    /// How far a track's centre line from `a` to `b` is from the form's edge, zero if it
    /// touches or crosses the form. Exact, not on the grid.
    fn distance_to_line(&self, a: Pt, b: Pt) -> f64 {
        match self {
            Form::Rect(r) => {
                let inside =
                    |p: Pt| r.min.x <= p.x && p.x <= r.max.x && r.min.y <= p.y && p.y <= r.max.y;
                if inside(a) || inside(b) {
                    return 0.0;
                }
                let corners = [
                    r.min,
                    Pt::new(r.max.x, r.min.y),
                    r.max,
                    Pt::new(r.min.x, r.max.y),
                ];
                (0..4)
                    .map(|k| line_gap(a, b, corners[k], corners[(k + 1) % 4]))
                    .fold(f64::MAX, f64::min)
            }
            Form::Segment(c, d, half) => (line_gap(a, b, *c, *d) - half).max(0.0),
            Form::Circle(c, radius) => (point_to_line(*c, a, b) - radius).max(0.0),
        }
    }

    fn bounds(&self) -> Rect {
        match self {
            Form::Rect(r) => *r,
            Form::Segment(a, b, half) => Rect::new(
                Pt::new(a.x.min(b.x), a.y.min(b.y)),
                Pt::new(a.x.max(b.x), a.y.max(b.y)),
            )
            .expanded(*half),
            Form::Circle(c, r) => Rect::new(*c, *c).expanded(*r),
        }
    }
}

/// How far a point is from the segment from `a` to `b`.
fn point_to_line(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
    };
    p.distance(Pt::new(a.x + ab.x * t, a.y + ab.y * t))
}

/// The least distance between the segments `a`-`b` and `c`-`d`, zero if they cross.
fn line_gap(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    let side = |p: Pt, q: Pt, r: Pt| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    [
        point_to_line(a, c, d),
        point_to_line(b, c, d),
        point_to_line(c, a, b),
        point_to_line(d, a, b),
    ]
    .into_iter()
    .fold(f64::MAX, f64::min)
}

struct Obstacle {
    /// The net it carries, as an index into the board's nets.
    net: Option<usize>,
    layers: [bool; 2],
    form: Form,
    clearance: f64,
}

struct Grid {
    origin: Pt,
    nx: usize,
    ny: usize,
}

impl Grid {
    fn over(outline: &Rect) -> Grid {
        Grid {
            origin: outline.min,
            nx: (outline.width() / PITCH).floor() as usize + 1,
            ny: (outline.height() / PITCH).floor() as usize + 1,
        }
    }

    fn centre(&self, i: usize, j: usize) -> Pt {
        Pt::new(
            self.origin.x + i as f64 * PITCH,
            self.origin.y + j as f64 * PITCH,
        )
    }

    /// The cell whose centre is nearest.
    fn cell(&self, p: Pt) -> (usize, usize) {
        let i = ((p.x - self.origin.x) / PITCH).round().max(0.0) as usize;
        let j = ((p.y - self.origin.y) / PITCH).round().max(0.0) as usize;
        (i.min(self.nx - 1), j.min(self.ny - 1))
    }

    /// The cells of `window` within `reach` of the box.
    fn cells_near(
        &self,
        window: &Window,
        bounds: &Rect,
        reach: f64,
    ) -> impl Iterator<Item = (usize, usize)> + use<> {
        let (i0, j0) = self.cell(add_scalar(bounds.min, -reach));
        let (i1, j1) = self.cell(add_scalar(bounds.max, reach));
        let (i0, j0) = (i0.max(window.i0), j0.max(window.j0));
        let (i1, j1) = (
            i1.min(window.i0 + window.nx - 1),
            j1.min(window.j0 + window.ny - 1),
        );
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
    }
}

/// A rectangle of cells: the part of the grid a search is confined to. Most lines join pads
/// that are near each other, so most are routed in a small window; one that cannot be is
/// tried again in a larger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Window {
    i0: usize,
    j0: usize,
    nx: usize,
    ny: usize,
}

impl Window {
    fn full(grid: &Grid) -> Window {
        Window {
            i0: 0,
            j0: 0,
            nx: grid.nx,
            ny: grid.ny,
        }
    }

    /// The cells within `margin` mm of the box that spans these spots.
    fn around(grid: &Grid, spots: &[Spot], margin: f64) -> Window {
        let cells = (margin / PITCH).ceil() as usize;
        let (mut i0, mut j0, mut i1, mut j1) = (usize::MAX, usize::MAX, 0, 0);
        for &(i, j, _) in spots {
            (i0, j0, i1, j1) = (i0.min(i), j0.min(j), i1.max(i), j1.max(j));
        }
        let (i0, j0) = (i0.saturating_sub(cells), j0.saturating_sub(cells));
        let (i1, j1) = ((i1 + cells).min(grid.nx - 1), (j1 + cells).min(grid.ny - 1));
        Window {
            i0,
            j0,
            nx: i1 - i0 + 1,
            ny: j1 - j0 + 1,
        }
    }

    fn cells(&self) -> usize {
        self.nx * self.ny
    }

    fn contains(&self, i: usize, j: usize) -> bool {
        (self.i0..self.i0 + self.nx).contains(&i) && (self.j0..self.j0 + self.ny).contains(&j)
    }

    /// The cell's place in a vector over the window.
    fn index(&self, i: usize, j: usize) -> usize {
        (j - self.j0) * self.nx + (i - self.i0)
    }

    fn covers(&self, grid: &Grid) -> bool {
        *self == Window::full(grid)
    }
}

/// `p` moved by `by` along both axes.
fn add_scalar(p: Pt, by: f64) -> Pt {
    Pt::new(p.x + by, p.y + by)
}

/// Which cells of a window a track, or a via, of one net may not use.
struct Masks {
    window: Window,
    track: [Vec<bool>; 2],
    via: Vec<bool>,
    /// The cells a track at its full width may not use, when `track` is for the narrowest
    /// track the process makes: a track through a cell of this is necked down there. `None`
    /// when `track` is for the full width.
    wide: Option<[Vec<bool>; 2]>,
    /// Of the necked cells, those near the line's own pads where only the process's least track
    /// fits: elsewhere a necked track is the width its current needs. `None` when the two are
    /// one width.
    pinch: Option<[Vec<bool>; 2]>,
    /// Where a via could stand before [`Router::room_for_vias`] kept only the cells with room
    /// for a cluster: where the others of a cluster stand. `None` when it did not run.
    via_alone: Option<Vec<bool>>,
    /// The cells inside a pad of the line's own net, by layer, where a path may turn by more
    /// than 45 degrees: the corner is inside the pad's copper (issue #415). Empty when unset.
    free_turn: [Vec<bool>; 2],
    /// How often lines have fought over each cell while the router negotiated, which makes
    /// a track through it dearer. Empty when it is not negotiating.
    history: [Vec<u8>; 2],
    /// Cells that other nets' routed copper keeps tracks and vias from, which a search for
    /// the nets least in the way may cross at a price.
    soft: Option<Soft>,
    /// What a bend of 45 degrees costs ([`Costs::turn`]).
    turn: i64,
    /// The cells of other nets' escape lanes, which a track or a via of this line pays
    /// [`LANE_TRACK`] or [`LANE_VIA`] to use. `None` when there are none.
    lane: Option<LaneCost>,
    /// What a track pays, beyond a step, on each cell by layer for the coarse plan
    /// ([`crate::plan`]): nothing in the line's corridor, and outside it the crowding of the
    /// cell's tile on that layer. `None` when the try follows no plan or nothing costs.
    off_plan: Option<[Vec<i64>; 2]>,
    /// For a net of a diff pair whose partner is routed, the cells beside the partner's tracks
    /// (within [`PAIR_NEAR`] of their centre lines), by layer: a step onto one costs [`FOLLOW`]
    /// of its length, so the net runs beside its partner (issue #431, S9). Empty when unset.
    follow: [Vec<bool>; 2],
    /// The share of the octile distance the search's estimate is, as a fraction: whole, or
    /// [`FOLLOW`] where a step may cost that little.
    estimate: (i64, i64),
    /// What a step costs beyond its length on each layer ([`POUR`] on another net's pour), and
    /// a via beyond [`VIA`] ([`POUR_VIA`]), in the pass off the pours; zero elsewhere.
    pour: ([i64; 2], i64),
}

struct Soft {
    track: [Vec<bool>; 2],
    via: Vec<bool>,
}

impl Masks {
    /// Nothing in the way anywhere in the window.
    fn open(window: Window) -> Masks {
        Masks {
            window,
            track: [vec![false; window.cells()], vec![false; window.cells()]],
            via: vec![false; window.cells()],
            wide: None,
            pinch: None,
            via_alone: None,
            free_turn: [Vec::new(), Vec::new()],
            history: [Vec::new(), Vec::new()],
            soft: None,
            turn: TURN,
            lane: None,
            off_plan: None,
            follow: [Vec::new(), Vec::new()],
            estimate: (1, 1),
            pour: ([0, 0], 0),
        }
    }

    fn at(&self, i: usize, j: usize) -> usize {
        self.window.index(i, j)
    }

    /// Whether a path may turn by more than 45 degrees on the cell: it is inside a pad of the
    /// line's net.
    fn turns_freely(&self, layer: usize, i: usize, j: usize) -> bool {
        self.free_turn[layer]
            .get(self.at(i, j))
            .copied()
            .unwrap_or(false)
    }

    /// Whether a track on the cell has to be necked down.
    fn necked(&self, layer: usize, i: usize, j: usize) -> bool {
        self.wide.as_ref().is_some_and(|w| w[layer][self.at(i, j)])
    }

    /// How narrow a track on the cell is: 0 at full width, 1 necked to the width its current
    /// needs, 2 pinched to the process's least.
    fn neck(&self, layer: usize, i: usize, j: usize) -> u8 {
        if self.pinch.as_ref().is_some_and(|p| p[layer][self.at(i, j)]) {
            2
        } else {
            u8::from(self.necked(layer, i, j))
        }
    }

    /// What it costs, beyond a step, to put a track on a cell that other nets' routed copper
    /// keeps tracks from.
    fn soft_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        match &self.soft {
            Some(soft) if soft.track[layer][self.at(i, j)] => SOFT,
            _ => 0,
        }
    }

    /// The same for a via.
    fn soft_via_cost(&self, i: usize, j: usize) -> i64 {
        match &self.soft {
            Some(soft) if soft.via[self.at(i, j)] => SOFT_VIA,
            _ => 0,
        }
    }

    /// What a step of `length` costs onto a cell: less by [`FOLLOW`] on a cell beside a
    /// pair's partner.
    fn follow_cost(&self, layer: usize, i: usize, j: usize, length: i64) -> i64 {
        match self.follow[layer].get(self.at(i, j)) {
            Some(true) => length * FOLLOW.0 / FOLLOW.1,
            _ => length,
        }
    }

    /// What it costs, beyond a step, to put a track on a cell of another net's escape lane.
    fn lane_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        match &self.lane {
            Some(lane) if lane.track[layer][self.at(i, j)] => LANE_TRACK,
            _ => 0,
        }
    }

    /// The same for a via.
    fn lane_via_cost(&self, i: usize, j: usize) -> i64 {
        match &self.lane {
            Some(lane) if lane.via[self.at(i, j)] => LANE_VIA,
            _ => 0,
        }
    }

    /// What it costs, beyond a step, to put a track on a cell for the coarse plan.
    fn off_plan_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        self.off_plan
            .as_ref()
            .map_or(0, |cost| cost[layer][self.at(i, j)])
    }

    /// What it costs, beyond a step, to put a track on the cell.
    fn history_cost(&self, layer: usize, i: usize, j: usize) -> i64 {
        self.history[layer]
            .get(self.at(i, j))
            .map_or(0, |&h| i64::from(h) * HISTORY_COST)
    }
}

/// How far from an obstacle a mask keeps a track and a via: the room beyond the rules, and
/// the width the track is drawn at.
#[derive(Clone, Copy)]
struct Reach {
    room: f64,
    width: f64,
}

/// Everything copper must keep clear of, apart from what has been routed.
struct Obstacles {
    list: Vec<Obstacle>,
    /// Where no via may go whatever its net, and how far from the box: a via does not sit
    /// on a pad, and keeps further from the holes of through-hole pads.
    via_keepouts: Vec<(Form, f64)>,
}

/// How many obstacles keep each cell of a window from tracks on each layer, and from vias.
/// A cell is open when its count is zero. Counting is what lets one net's own pads be taken
/// away again from counts made for all of them.
struct Counts {
    window: Window,
    track: [Vec<u16>; 2],
    via: Vec<u16>,
}

impl Counts {
    fn new(window: Window) -> Counts {
        Counts {
            window,
            track: [vec![0; window.cells()], vec![0; window.cells()]],
            via: vec![0; window.cells()],
        }
    }

    /// Add one (`up`) or take away one obstacle's reach: the cells within the rules' space
    /// of it, plus the room, for a track of `width` and for a via.
    fn stamp(
        &mut self,
        grid: &Grid,
        rules: &netsmith_pcb::rules::Rules,
        o: &Obstacle,
        reach: Reach,
        up: bool,
    ) {
        let Reach { room, width } = reach;
        let window = self.window;
        let track_reach = o.clearance + width / 2.0 + room;
        let via_reach = o.clearance + rules.via_diameter / 2.0 + room;
        let bounds = o.form.bounds();
        let far = track_reach.max(via_reach);
        // Most obstacles are nowhere near a small window.
        let corner = |i: usize, j: usize| grid.centre(i, j);
        let seen = Rect::new(
            corner(window.i0, window.j0),
            corner(window.i0 + window.nx - 1, window.j0 + window.ny - 1),
        );
        if !seen.expanded(PITCH).overlaps(&bounds.expanded(far)) {
            return;
        }
        // Look as far as either goes: a track wider than a via reaches further than it.
        for (i, j) in grid.cells_near(&window, &bounds, far) {
            let d = o.form.distance(grid.centre(i, j));
            let at = window.index(i, j);
            let bump = |c: &mut u16| {
                if up {
                    *c += 1;
                } else {
                    *c -= 1;
                }
            };
            for layer in [FRONT, BACK] {
                if o.layers[layer] && d < track_reach {
                    bump(&mut self.track[layer][at]);
                }
            }
            if d < via_reach {
                bump(&mut self.via[at]);
            }
        }
    }

    fn stamp_keepout(&mut self, grid: &Grid, form: &Form, reach: f64) {
        for (i, j) in grid.cells_near(&self.window, &form.bounds(), reach) {
            if form.distance(grid.centre(i, j)) < reach {
                let at = self.window.index(i, j);
                self.via[at] += 1;
            }
        }
    }
}

/// What the copper that never moves keeps cells from, over the whole board, for one room
/// and track width: the board's edge, the pads, the holes and the keepouts.
struct Base {
    counts: Counts,
}

impl Base {
    fn new(board: &Board, grid: &Grid, fixed: &Obstacles, reach: Reach) -> Base {
        let mut counts = Counts::new(Window::full(grid));
        let (outline, rules) = (&board.outline, &board.rules);
        for j in 0..grid.ny {
            for i in 0..grid.nx {
                let p = grid.centre(i, j);
                if !outline.contains(&Rect::new(p, p)) {
                    let at = counts.window.index(i, j);
                    counts.track[FRONT][at] += 1;
                    counts.track[BACK][at] += 1;
                    counts.via[at] += 1;
                }
            }
        }
        for o in &fixed.list {
            counts.stamp(grid, rules, o, reach, true);
        }
        for (form, keep) in &fixed.via_keepouts {
            counts.stamp_keepout(grid, form, *keep);
        }
        Base { counts }
    }
}

/// The cells of `window` that a track, or a via, of `net` may not use: what never moves
/// (`base`, less the net's own pads, `own`), the copper routed so far of other nets
/// (`routed`), and the keep-outs of the holes of routed vias (`keepouts`).
#[allow(clippy::too_many_arguments)]
fn masks_for(
    grid: &Grid,
    window: Window,
    board: &Board,
    base: &Base,
    own: &[&Obstacle],
    routed: &[&Obstacle],
    keepouts: &[(Form, f64)],
    net: usize,
    reach: Reach,
) -> Masks {
    let rules = &board.rules;
    let mut counts = Counts::new(window);
    let from = &base.counts;
    for layer in [FRONT, BACK] {
        copy_window(&from.track[layer], &mut counts.track[layer], grid, window);
    }
    copy_window(&from.via, &mut counts.via, grid, window);
    for o in own {
        counts.stamp(grid, rules, o, reach, false);
    }
    for o in routed.iter().filter(|o| o.net != Some(net)) {
        counts.stamp(grid, rules, o, reach, true);
    }
    for (form, keep) in keepouts {
        counts.stamp_keepout(grid, form, *keep);
    }
    let mut m = Masks::open(window);
    for layer in [FRONT, BACK] {
        m.track[layer] = counts.track[layer].iter().map(|&c| c > 0).collect();
    }
    m.via = counts.via.iter().map(|&c| c > 0).collect();
    m
}

/// The rows of a window out of counts over the whole grid.
fn copy_window(whole: &[u16], out: &mut [u16], grid: &Grid, window: Window) {
    for j in 0..window.ny {
        let from = (window.j0 + j) * grid.nx + window.i0;
        out[j * window.nx..(j + 1) * window.nx].copy_from_slice(&whole[from..from + window.nx]);
    }
}

/// A cell on a layer: where a path starts or ends, and so is open whatever the masks say.
type Spot = (usize, usize, usize);

/// Cells a path may also start or end on, by way of a via in the pad: the cell of the pad's
/// other layer. Reaching one costs [`VIA_IN_PAD`] more.
#[derive(Clone, Copy, Default)]
struct Extras<'a> {
    from: &'a [Spot],
    to: &'a [Spot],
    /// Whether the path may turn by a right angle, at [`TURN_RIGHT`], for the clean-up of
    /// [`Router::settled`] to cut into two bends of 45 degrees (issue #415).
    right_angles: bool,
    /// Four directions and free right angles, as a search that only finds which nets are in a
    /// line's way does: its path is never drawn, and eight directions cost it three times the
    /// work.
    coarse: bool,
}

/// Where a path is to end.
#[derive(Clone, Copy)]
enum Goal<'a> {
    /// Any of these cells.
    Spots(&'a [Spot]),
    /// The nearest cell where a via may go: the path is to a plane, which a via reaches.
    Via,
}

/// How a search ended without a path.
#[derive(Debug, PartialEq, Eq)]
enum Miss {
    /// Every cell the search could reach was tried.
    Blocked,
    /// The search was stopped, at the time limit.
    Stopped,
}

/// Whether a path can exist at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    /// The ends are joined by free cells, so a search will find a path.
    Joined,
    /// One end is walled in: the cells it can reach, which touch no edge of the window, do
    /// not reach the other end or a place for a via, and a larger window would not change
    /// that.
    Closed,
    /// The cells one end can reach run out, but only at the window's edge.
    Cramped,
}

/// Find out, much more cheaply than a search can, whether the ends of a line are joined by
/// cells a track may use. A search that fails has looked at everything it could reach, which
/// on a big board with one end walled in by fine-pitch pads is most of the board; this
/// looks at the walled-in end only, because it spreads from both ends a cell at a time and
/// stops when either runs out.
///
/// A cell is passable if the masks leave it open, or it is an end of the line. A move
/// between passable cells is possible both ways, so it does not matter which end is which.
fn link(masks: &Masks, from: &[Spot], goal: Goal, extras: Extras) -> Link {
    let window = masks.window;
    let cells = window.cells();
    let from_all: Vec<Spot> = from.iter().chain(extras.from).copied().collect();
    let to_all: Vec<Spot> = match goal {
        Goal::Spots(to) => to.iter().chain(extras.to).copied().collect(),
        Goal::Via => Vec::new(),
    };
    let from = &from_all[..];
    let goal = match goal {
        Goal::Spots(_) => Goal::Spots(&to_all),
        Goal::Via => Goal::Via,
    };
    let ends: Vec<Spot> = match goal {
        Goal::Spots(to) => from.iter().chain(to).copied().collect(),
        Goal::Via => from.to_vec(),
    };
    let passable =
        |(i, j, layer): Spot| !masks.track[layer][masks.at(i, j)] || ends.contains(&(i, j, layer));
    let on_edge = |(i, j, _): Spot| {
        i == window.i0
            || j == window.j0
            || i + 1 == window.i0 + window.nx
            || j + 1 == window.j0 + window.ny
    };
    // Each side: the cells it has reached (by whom: 1 for the first side, 2 for the second),
    // its queue, and whether it has reached an edge of the window.
    let mut seen = vec![0u8; cells * 2];
    let mark = |(i, j, layer): Spot| layer * cells + window.index(i, j);
    let sides: &[&[Spot]] = match goal {
        Goal::Spots(to) => &[from, to],
        Goal::Via => &[from],
    };
    let mut queues: Vec<std::collections::VecDeque<Spot>> = Vec::new();
    let mut edge = vec![false; sides.len()];
    for (side, spots) in sides.iter().enumerate() {
        let mut queue = std::collections::VecDeque::new();
        for &spot in spots.iter().filter(|s| window.contains(s.0, s.1)) {
            seen[mark(spot)] = side as u8 + 1;
            queue.push_back(spot);
        }
        queues.push(queue);
    }
    loop {
        for side in 0..sides.len() {
            let Some(spot) = queues[side].pop_front() else {
                return if edge[side] {
                    Link::Cramped
                } else {
                    Link::Closed
                };
            };
            let (i, j, layer) = spot;
            edge[side] |= on_edge(spot);
            if matches!(goal, Goal::Via) && !masks.via[masks.at(i, j)] {
                return Link::Joined;
            }
            let mut next = |n: Spot, queues: &mut Vec<std::collections::VecDeque<Spot>>| {
                let m = mark(n);
                if seen[m] == 0 {
                    seen[m] = side as u8 + 1;
                    queues[side].push_back(n);
                    false
                } else {
                    seen[m] != side as u8 + 1
                }
            };
            for (di, dj) in MOVES {
                let (ni, nj) = (i as isize + di, j as isize + dj);
                if ni < 0 || nj < 0 || !window.contains(ni as usize, nj as usize) {
                    continue;
                }
                let n = (ni as usize, nj as usize, layer);
                if passable(n) && next(n, &mut queues) {
                    return Link::Joined;
                }
            }
            let other = (i, j, 1 - layer);
            if !masks.via[masks.at(i, j)] && passable(other) && next(other, &mut queues) {
                return Link::Joined;
            }
        }
    }
}

/// How often a search looks at the clock, in cells taken from the queue.
const CLOCK_EVERY: u32 = 1 << 14;

/// The cheapest path from any of `from` to the goal, as cells with their layers, within the
/// masks' window. `stop` is asked now and then whether to give up.
fn search(
    masks: &Masks,
    from: &[Spot],
    goal: Goal,
    extras: Extras,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Spot>, Miss> {
    let window = masks.window;
    let states = window.cells() * 2 * DIRECTIONS;
    let state = |(i, j, layer): Spot, dir: usize| {
        (layer * window.cells() + window.index(i, j)) * DIRECTIONS + dir
    };
    let unpack = |s: usize| -> (Spot, usize) {
        let dir = s % DIRECTIONS;
        let cell = s / DIRECTIONS;
        let (layer, rest) = (cell / window.cells(), cell % window.cells());
        (
            (
                window.i0 + rest % window.nx,
                window.j0 + rest / window.nx,
                layer,
            ),
            dir,
        )
    };
    let open = |spot: Spot| {
        from.contains(&spot)
            || extras.from.contains(&spot)
            || extras.to.contains(&spot)
            || matches!(goal, Goal::Spots(to) if to.contains(&spot))
    };
    let reached = |spot: Spot| match goal {
        Goal::Spots(to) => to.contains(&spot),
        Goal::Via => !masks.via[masks.at(spot.0, spot.1)],
    };
    let heuristic = |(i, j, _): Spot| -> i64 {
        match goal {
            // The octile distance: as far diagonally as both axes go, then straight.
            Goal::Spots(to) => to
                .iter()
                .map(|&(ti, tj, _)| {
                    let (a, b) = (i.abs_diff(ti) as i64, j.abs_diff(tj) as i64);
                    let d = if extras.coarse {
                        (a + b) * STEP
                    } else {
                        a.min(b) * DIAGONAL + (a - b).abs() * STEP
                    };
                    d * masks.estimate.0 / masks.estimate.1
                })
                .min()
                .unwrap_or(0),
            Goal::Via => 0,
        }
    };

    let mut cost = vec![i64::MAX; states];
    let mut came_from = vec![usize::MAX; states];
    // The queue orders states by their estimate, then by how far they have come, the farthest
    // first, then by when they were queued: among the many paths of one cost that eight
    // directions give, the search follows one to its end before it widens, instead of spreading
    // over all of them; the path found is still a cheapest one, and the same every time.
    let mut queue = BinaryHeap::new();
    let mut sequence = 0u64;
    for (&spot, start) in from
        .iter()
        .map(|s| (s, 0))
        .chain(extras.from.iter().map(|s| (s, VIA_IN_PAD)))
        .filter(|(s, _)| window.contains(s.0, s.1))
    {
        let s = state(spot, NO_DIRECTION);
        cost[s] = start;
        queue.push(Reverse((start + heuristic(spot), -start, sequence, s)));
        sequence += 1;
    }
    // A state that stands on a cell reached through a via in a pad is not the end of the
    // path itself; it becomes so, at the via's cost, when it comes off the queue. Those
    // ends are queued as a state number past the real ones.
    let mut finished = vec![false; states];
    let end_of = |s: usize| s - states;
    let mut taken = 0u32;
    while let Some(Reverse((_, g, _, s))) = queue.pop() {
        taken += 1;
        if taken.is_multiple_of(CLOCK_EVERY) && stop() {
            return Err(Miss::Stopped);
        }
        // A state queued again when a cheaper way to it was found is taken from the queue first
        // at that cost (its estimate is the same, the cost lower); the dearer entries left are
        // stale, and taking them again would relax nothing.
        if s < states && -g > cost[s] {
            continue;
        }
        if s >= states {
            let mut path = Vec::new();
            let mut at = end_of(s);
            path.push(unpack(at).0);
            while came_from[at] != usize::MAX {
                at = came_from[at];
                path.push(unpack(at).0);
            }
            path.reverse();
            return Ok(path);
        }
        let (spot, dir) = unpack(s);
        if matches!(goal, Goal::Spots(_)) && extras.to.contains(&spot) && !finished[s] {
            finished[s] = true;
            queue.push(Reverse((
                cost[s] + VIA_IN_PAD,
                -(cost[s] + VIA_IN_PAD),
                sequence,
                states + s,
            )));
            sequence += 1;
        }
        if reached(spot) {
            let mut path = vec![spot];
            let mut at = s;
            while came_from[at] != usize::MAX {
                at = came_from[at];
                path.push(unpack(at).0);
            }
            path.reverse();
            return Ok(path);
        }
        let here = cost[s];
        moves(masks, &open, extras, spot, dir, |next, next_dir, step| {
            let n = state(next, next_dir);
            if here + step < cost[n] {
                cost[n] = here + step;
                came_from[n] = s;
                queue.push(Reverse((cost[n] + heuristic(next), -cost[n], sequence, n)));
                sequence += 1;
            }
        });
    }
    Err(Miss::Blocked)
}

/// The moves of the search from a state, in the order the search queues them: each legal next
/// state of `spot`, come in by direction `dir`, with what the move costs, given to `relax`.
/// [`search`] and [`reachable`] share it, so they agree on which states can be reached.
fn moves(
    masks: &Masks,
    open: &impl Fn(Spot) -> bool,
    extras: Extras,
    spot: Spot,
    dir: usize,
    mut relax: impl FnMut(Spot, usize, i64),
) {
    let window = masks.window;
    let (i, j, layer) = spot;
    let passable = |(ci, cj): (isize, isize)| {
        ci >= 0
            && cj >= 0
            && window.contains(ci as usize, cj as usize)
            && (!masks.track[layer][masks.at(ci as usize, cj as usize)]
                || open((ci as usize, cj as usize, layer)))
    };
    for (d, (di, dj)) in MOVES8.iter().enumerate() {
        if extras.coarse && d % 2 == 1 {
            continue;
        }
        // No bend sharper than 45 degrees: a right angle, an acute one, or back.
        let bend = if dir < 8 {
            let k = (d + 8 - dir) % 8;
            k.min(8 - k)
        } else {
            0
        };
        // Inside a pad of the net the corner is in the pad's copper, and any turn but back
        // is free.
        let free = bend < 4 && masks.turns_freely(layer, i, j);
        if bend > 1 && !free && !(bend == 2 && (extras.right_angles || extras.coarse)) {
            continue;
        }
        let (ni, nj) = (i as isize + di, j as isize + dj);
        if !passable((ni, nj)) {
            continue;
        }
        // A diagonal step passes between the two cells beside it, which must be open too.
        let diagonal = *di != 0 && *dj != 0;
        if diagonal
            && !(passable((i as isize + di, j as isize)) && passable((i as isize, j as isize + dj)))
        {
            continue;
        }
        let next = (ni as usize, nj as usize, layer);
        let turn = match bend {
            0 => 0,
            _ if extras.coarse || free => masks.turn,
            1 => masks.turn,
            _ => TURN_RIGHT,
        };
        let neck = if masks.necked(layer, next.0, next.1) {
            NECK
        } else {
            0
        };
        relax(
            next,
            d,
            masks.follow_cost(
                layer,
                next.0,
                next.1,
                if diagonal { DIAGONAL } else { STEP },
            ) + turn
                + neck
                + masks.history_cost(layer, next.0, next.1)
                + masks.soft_cost(layer, next.0, next.1)
                + masks.lane_cost(layer, next.0, next.1)
                + masks.off_plan_cost(layer, next.0, next.1)
                + masks.pour.0[layer],
        );
    }
    let other = 1 - layer;
    let at = masks.at(i, j);
    if dir != AFTER_VIA && !masks.via[at] && (!masks.track[other][at] || open((i, j, other))) {
        relax(
            (i, j, other),
            AFTER_VIA,
            VIA + masks.pour.1 + masks.soft_via_cost(i, j) + masks.lane_via_cost(i, j),
        );
    }
}

/// Whether a search with these masks, ends and moves can reach the goal at all: a flood over
/// the search's states (cell, layer and the direction come in by) with [`moves`], which costs
/// far less than a search that runs out of states, and says the same.
fn reachable(
    masks: &Masks,
    from: &[Spot],
    goal: Goal,
    extras: Extras,
    stop: &dyn Fn() -> bool,
) -> Result<bool, Miss> {
    let window = masks.window;
    let cells = window.cells();
    let state =
        |(i, j, layer): Spot, dir: usize| (layer * cells + window.index(i, j)) * DIRECTIONS + dir;
    let open = |spot: Spot| {
        from.contains(&spot)
            || extras.from.contains(&spot)
            || extras.to.contains(&spot)
            || matches!(goal, Goal::Spots(to) if to.contains(&spot))
    };
    let done = |spot: Spot| match goal {
        Goal::Spots(to) => to.contains(&spot) || extras.to.contains(&spot),
        Goal::Via => !masks.via[masks.at(spot.0, spot.1)],
    };
    let mut seen = vec![false; cells * 2 * DIRECTIONS];
    let mut stack: Vec<(Spot, usize)> = Vec::new();
    for &spot in from
        .iter()
        .chain(extras.from)
        .filter(|s| window.contains(s.0, s.1))
    {
        let s = state(spot, NO_DIRECTION);
        if !seen[s] {
            seen[s] = true;
            stack.push((spot, NO_DIRECTION));
        }
    }
    let mut taken = 0u32;
    while let Some((spot, dir)) = stack.pop() {
        taken += 1;
        if taken.is_multiple_of(CLOCK_EVERY) && stop() {
            return Err(Miss::Stopped);
        }
        if done(spot) {
            return Ok(true);
        }
        moves(masks, &open, extras, spot, dir, |next, next_dir, _| {
            let n = state(next, next_dir);
            if !seen[n] {
                seen[n] = true;
                stack.push((next, next_dir));
            }
        });
    }
    Ok(false)
}

/// Straight runs through a list of cells on one layer; a single cell has none.
#[cfg(test)]
fn runs(grid: &Grid, cells: &[(usize, usize)]) -> Vec<(Pt, Pt)> {
    let none = vec![0u8; cells.len()];
    runs_with(grid, cells, &none)
        .into_iter()
        .map(|(a, b, _)| (a, b))
        .collect()
}

/// [`runs`], where a step to or from a cell of `neck` above 0 is a run of its own, marked with
/// the narrower of its two cells, so that the track is drawn narrow there and wide elsewhere.
fn runs_with(grid: &Grid, cells: &[(usize, usize)], neck: &[u8]) -> Vec<(Pt, Pt, u8)> {
    let mut out = Vec::new();
    let mut start = 0;
    let narrow = |a: usize, b: usize| neck[a].max(neck[b]);
    for k in 1..cells.len() {
        let step = |a: usize, b: usize| {
            (
                cells[b].0 as isize - cells[a].0 as isize,
                cells[b].1 as isize - cells[a].1 as isize,
                narrow(a, b),
            )
        };
        let ends = k + 1 == cells.len() || step(k - 1, k) != step(k, k + 1);
        if ends {
            out.push((
                grid.centre(cells[start].0, cells[start].1),
                grid.centre(cells[k].0, cells[k].1),
                narrow(k - 1, k),
            ));
            start = k;
        }
    }
    out
}

/// The pad a ratsnest line ends on, and the part it is of.
fn pad_of<'a>(
    board: &'a Board,
    pad: &'a netsmith_pcb::board::PadRef,
    at: Pt,
) -> (
    &'a netsmith_pcb::board::Placed,
    &'a netsmith_pcb::footprint::Pad,
) {
    let part = board
        .part(&pad.refdes)
        .expect("a ratsnest line names a part");
    let found = part
        .footprint
        .pads_numbered(&pad.number)
        .find(|p| part.pad_centre(p) == at)
        .expect("a ratsnest line ends on a pad centre");
    (part, found)
}

/// The copper layers a pad of this footprint has, and its centre.
fn pad_spots(board: &Board, pad: &netsmith_pcb::board::PadRef, at: Pt, grid: &Grid) -> Vec<Spot> {
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    let (i, j) = grid.cell(at);
    [(front, FRONT), (back, BACK)]
        .iter()
        .filter(|(has, _)| *has)
        .map(|&(_, layer)| (i, j, layer))
        .collect()
}

/// A pad a via may stand in: a surface-mount pad of a plain shape that is wide enough for
/// the via's copper, when the process fills and caps vias that size.
#[derive(Clone, Copy, Debug)]
struct InPad {
    /// Where the via goes: the pad's centre.
    at: Pt,
    /// The pad's box, which the via stays off all the keep-outs of other pads.
    bounds: Rect,
    /// The cell of the pad's other layer, where a path through the via goes on.
    spot: Spot,
}

fn in_pad(board: &Board, pad: &netsmith_pcb::board::PadRef, at: Pt, grid: &Grid) -> Option<InPad> {
    let rules = &board.rules;
    if !rules.via_in_pad {
        return None;
    }
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    if front == back || found.extent.is_some() || found.drill.is_some() {
        return None;
    }
    if found.width.min(found.height) < rules.via_diameter {
        return None;
    }
    let (i, j) = grid.cell(at);
    Some(InPad {
        at,
        bounds: part.pad_bounds(found),
        spot: (i, j, if front { BACK } else { FRONT }),
    })
}

/// How deep a pad stands in a fine-pitch grid of pads, in pitches from the outside of the
/// part's pads: 0 for a pad on the outer ring, and for every pad of a part that is not a
/// fine-pitch grid (fewer than eight surface-mount pads, or none within the room a via and the
/// rules' spaces need between pads). Such a pad can leave only through the gaps between its
/// neighbours, and the deeper it is the fewer ways out it has.
fn pad_depth(board: &Board, pad: &netsmith_pcb::board::PadRef, at: Pt) -> f64 {
    let rules = &board.rules;
    let (part, found) = pad_of(board, pad, at);
    let (front, back) = part.pad_layers(found);
    let on_face = |p: &netsmith_pcb::footprint::Pad| {
        let (f, b) = part.pad_layers(p);
        (front && f) || (back && b)
    };
    let pads: Vec<&netsmith_pcb::footprint::Pad> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none() && on_face(p))
        .collect();
    if pads.len() < 8 || found.drill.is_some() {
        return 0.0;
    }
    let here = part.pad_bounds(found);
    let gap = |a: &Rect, b: &Rect| {
        let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
        let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
        dx.hypot(dy)
    };
    let nearest = pads
        .iter()
        .filter(|p| part.pad_centre(p) != at)
        .map(|p| {
            (
                gap(&here, &part.pad_bounds(p)),
                part.pad_centre(p).distance(at),
            )
        })
        .fold((f64::MAX, f64::MAX), |m, g| (m.0.min(g.0), m.1.min(g.1)));
    if nearest.0 >= rules.via_diameter + 2.0 * rules.clearance {
        return 0.0;
    }
    let mut span = here;
    for p in &pads {
        let b = part.pad_bounds(p);
        span = Rect::new(
            Pt::new(span.min.x.min(b.min.x), span.min.y.min(b.min.y)),
            Pt::new(span.max.x.max(b.max.x), span.max.y.max(b.max.y)),
        );
    }
    let inside = (at.x - span.min.x)
        .min(span.max.x - at.x)
        .min(at.y - span.min.y)
        .min(span.max.y - at.y);
    (inside / nearest.1).round().max(0.0)
}

/// The escape lanes of the pads of fine-pitch parts (issue #431, see the module's header): for
/// each pad on the outside of such a part, the cells straight out from its edge, away from the
/// part, on the pad's layer, [`LANE_LENGTH`] long and as wide as the pad. A track of another
/// net pays [`LANE_TRACK`] a step to use one. A via of any net pays [`LANE_VIA`] to stand where
/// its copper would close a lane, except a via of the lane's own net at the lane's end, where
/// its line leaves the part's ring. A net uses the lanes of the other net of its diff pair as
/// its own. It is a cost, not a wall: a line that has no other way crosses a lane, and the
/// negotiation and rip-ups can take one back.
struct Lanes {
    nx: usize,
    /// For each layer and cell of the whole grid, the nets whose lanes take it, as a net's
    /// index plus one, 0 for none. A cell counts [`LANE_SLOTS`] nets at most; another's lane
    /// does not take it.
    track: [Vec<[u32; LANE_SLOTS]>; 2],
    /// The cells where a via would close a lane.
    via_block: Vec<bool>,
    /// The cells at the end of a lane where its own net's via is free, by net as `track`.
    via_end: Vec<[u32; LANE_SLOTS]>,
    /// The other net of each net's diff pair ([`diff_pair_partners`]).
    partner: Vec<Option<usize>>,
}

/// How many nets' lanes one cell counts.
const LANE_SLOTS: usize = 4;

/// The cells of a window that a line's track, or via, pays to use: [`Lanes::charged`].
struct LaneCost {
    track: [Vec<bool>; 2],
    via: Vec<bool>,
}

impl Lanes {
    fn over(grid: &Grid, nets: usize) -> Lanes {
        let cells = grid.nx * grid.ny;
        Lanes {
            nx: grid.nx,
            track: [vec![[0; LANE_SLOTS]; cells], vec![[0; LANE_SLOTS]; cells]],
            via_block: vec![false; cells],
            via_end: vec![[0; LANE_SLOTS]; cells],
            partner: vec![None; nets],
        }
    }

    fn take(slot: &mut [u32; LANE_SLOTS], net: usize) {
        let mark = net as u32 + 1;
        if slot.contains(&mark) {
            return;
        }
        if let Some(free) = slot.iter_mut().find(|s| **s == 0) {
            *free = mark;
        }
    }

    /// The lane of a pad of `net`: the cells whose centres are in `track`, on the layers in
    /// `layers`; for a via, the cells within `via_block` of it, and the cells within `via_end`
    /// of `end`, where its own net's via is free.
    #[allow(clippy::too_many_arguments)]
    fn claim(
        &mut self,
        grid: &Grid,
        net: usize,
        layers: [bool; 2],
        track: &Rect,
        end: Pt,
        via_block: f64,
        via_end: f64,
    ) {
        let whole = Window::full(grid);
        for (i, j) in grid.cells_near(&whole, track, via_block) {
            let p = grid.centre(i, j);
            let at = j * self.nx + i;
            if track.contains(&Rect::new(p, p)) {
                for layer in [FRONT, BACK] {
                    if layers[layer] {
                        Self::take(&mut self.track[layer][at], net);
                    }
                }
            }
            if Form::Rect(*track).distance(p) <= via_block + 1e-9 {
                self.via_block[at] = true;
            }
        }
        for (i, j) in grid.cells_near(&whole, &Rect::new(end, end), via_end) {
            if grid.centre(i, j).distance(end) <= via_end + 1e-9 {
                Self::take(&mut self.via_end[j * self.nx + i], net);
            }
        }
    }

    /// The cells of `window` that a line of `net` pays to use. `None` when the window has none.
    fn charged(&self, window: Window, net: usize) -> Option<LaneCost> {
        let marks = [Some(net), self.partner.get(net).copied().flatten()];
        let own = |slot: &[u32; LANE_SLOTS]| {
            marks
                .iter()
                .flatten()
                .any(|&n| slot.contains(&(n as u32 + 1)))
        };
        let mut any = false;
        let mut track = [Vec::new(), Vec::new()];
        let mut via = Vec::with_capacity(window.cells());
        for j in window.j0..window.j0 + window.ny {
            for i in window.i0..window.i0 + window.nx {
                let at = j * self.nx + i;
                for layer in [FRONT, BACK] {
                    let slot = &self.track[layer][at];
                    let charged = slot[0] != 0 && !own(slot);
                    any |= charged;
                    track[layer].push(charged);
                }
                let end = &self.via_end[at];
                let charged = !own(end) && (self.via_block[at] || end[0] != 0);
                any |= charged;
                via.push(charged);
            }
        }
        any.then_some(LaneCost { track, via })
    }
}

/// Whether a part gets escape lanes ([`Lanes`]): at least 16 surface-mount pads, two of them
/// 0.55 mm apart or nearer (pads at one centre, the pieces of an exposed pad, are not counted
/// apart), facing at least three sides. A connector's single row is not ringed by other parts.
fn fine_pitch(part: &netsmith_pcb::board::Placed, faces: usize) -> bool {
    let pads: Vec<&netsmith_pcb::footprint::Pad> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.drill.is_none())
        .collect();
    pads.len() >= 16
        && faces >= 3
        && pads.iter().enumerate().any(|(a, p)| {
            pads.iter().skip(a + 1).any(|q| {
                let d = part.pad_centre(p).distance(part.pad_centre(q));
                d > 1e-6 && d <= 0.55 + 1e-9
            })
        })
}

/// The escape lanes of every fine-pitch part of the board ([`Lanes`]), `None` if it has none.
fn escape_lanes(board: &Board, grid: &Grid) -> Option<Lanes> {
    let net_of: HashMap<(&str, &str), usize> = board
        .nets
        .iter()
        .enumerate()
        .flat_map(|(n, net)| {
            net.pads
                .iter()
                .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n))
        })
        .collect();
    let rules = &board.rules;
    let mut lanes = Lanes::over(grid, board.nets.len());
    lanes.partner = diff_pair_partners(board);
    // A via closes a lane where its copper and the rules' space come within half the least
    // track of the lane's middle.
    let via_block = rules.via_diameter / 2.0 + rules.clearance + rules.min_track_width / 2.0;
    let reach = LANE_LENGTH;
    let mut any = false;
    for part in &board.parts {
        let pads: Vec<&netsmith_pcb::footprint::Pad> = part
            .footprint
            .pads
            .iter()
            .filter(|p| p.drill.is_none())
            .collect();
        let Some(span) = pads.iter().map(|p| part.pad_bounds(p)).reduce(|a, b| {
            Rect::new(
                Pt::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                Pt::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
            )
        }) else {
            continue;
        };
        let mut claims = Vec::new();
        for pad in pads {
            let Some(&net) = net_of.get(&(part.refdes.as_str(), pad.number.as_str())) else {
                continue;
            };
            let b = part.pad_bounds(pad);
            let c = b.centre();
            // The sides of the part's pads that this pad is on; a pad on none (an exposed pad,
            // an inner row) has no lane. Of two, the one that points away from the part's
            // origin.
            let sides = [
                ((-1.0, 0.0), b.min.x - span.min.x),
                ((1.0, 0.0), span.max.x - b.max.x),
                ((0.0, -1.0), b.min.y - span.min.y),
                ((0.0, 1.0), span.max.y - b.max.y),
            ];
            let away = |(dx, dy): (f64, f64)| dx * (c.x - part.at.x) + dy * (c.y - part.at.y);
            let Some(&((dx, dy), _)) = sides
                .iter()
                .filter(|(_, gap)| *gap <= 1e-6)
                .max_by(|a, b| away(a.0).total_cmp(&away(b.0)))
            else {
                continue;
            };
            let (track, end) = if dx < 0.0 {
                let r = Rect::new(Pt::new(b.min.x - reach, b.min.y), Pt::new(b.min.x, b.max.y));
                (r, Pt::new(r.min.x, c.y))
            } else if dx > 0.0 {
                let r = Rect::new(Pt::new(b.max.x, b.min.y), Pt::new(b.max.x + reach, b.max.y));
                (r, Pt::new(r.max.x, c.y))
            } else if dy < 0.0 {
                let r = Rect::new(Pt::new(b.min.x, b.min.y - reach), Pt::new(b.max.x, b.min.y));
                (r, Pt::new(c.x, r.min.y))
            } else {
                let r = Rect::new(Pt::new(b.min.x, b.max.y), Pt::new(b.max.x, b.max.y + reach));
                (r, Pt::new(c.x, r.max.y))
            };
            let (front, back) = part.pad_layers(pad);
            claims.push(((dx as i8, dy as i8), net, [front, back], track, end));
        }
        let mut faces: Vec<(i8, i8)> = claims.iter().map(|c| c.0).collect();
        faces.sort_unstable();
        faces.dedup();
        if !fine_pitch(part, faces.len()) {
            continue;
        }
        for (_, net, layers, track, end) in claims {
            let via_end = rules.via_diameter / 2.0;
            lanes.claim(grid, net, layers, &track, end, via_block, via_end);
            any = true;
        }
    }
    any.then_some(lanes)
}

/// For each net, the other net of its diff pair, if it has one: two nets whose names differ only
/// in a suffix pair the route scorecard knows (`netsmith_fab::score::PAIR_SUFFIXES`).
fn diff_pair_partners(board: &Board) -> Vec<Option<usize>> {
    let index: HashMap<&str, usize> = board
        .nets
        .iter()
        .enumerate()
        .map(|(n, net)| (net.name.as_str(), n))
        .collect();
    let mut partner = vec![None; board.nets.len()];
    for (n, net) in board.nets.iter().enumerate() {
        for (p, m) in netsmith_fab::score::PAIR_SUFFIXES {
            if let Some(stem) = net.name.strip_suffix(p)
                && let Some(&other) = index.get(format!("{stem}{m}").as_str())
            {
                partner[n] = Some(other);
                partner[other] = Some(n);
            }
        }
    }
    partner
}

/// How important a line is (issue #416), most important first: the order the classes are
/// routed in from the second try on ([`LineClasses`]). Every class is derived from what the
/// board knows, with nothing written in the source. The copper of a decoupling, pair or clock
/// line is not ripped up for a line of a later class ([`Class::protected`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Class {
    /// A pad that must reach its net's plane, or a pad deep in a fine-pitch grid of pads: the
    /// fewest ways out.
    Escape,
    /// A decoupling capacitor's pad to the power pin it decouples ([`crate::quality`]).
    Decoupling,
    /// A net of a diff pair ([`diff_pair_partners`]).
    Pair,
    /// A net of a crystal (a part of the kind, or that provides the generic, `Crystal`), or
    /// whose name says it is a clock (`clk`, `xin`, `xout`, `xtal`).
    Clock,
    /// A net that carries at least [`POWER_CURRENT`].
    Power,
    Rest,
}

impl Class {
    /// Whether a line of this class keeps its copper against a line of a later class: a
    /// decoupling, pair or clock line does. An escape or power line keeps its place in the order
    /// only (decided by the agent under the owner's delegation of 2026-11-13): protecting the
    /// escapes of a QFN and every power net left RP2350 unrouted, and a power line ripped up is
    /// routed again at its full width.
    fn protected(self) -> bool {
        matches!(self, Class::Decoupling | Class::Pair | Class::Clock)
    }
}

/// The least current of a net that is routed as power, in amperes.
const POWER_CURRENT: f64 = 0.1;

/// The class of each line ([`Class`]); `depth` is each line's [`pad_depth`].
fn classes(board: &Board, lines: &[Ratline], depth: &[f64]) -> Vec<Class> {
    let partners = diff_pair_partners(board);
    let index: HashMap<&str, usize> = board
        .nets
        .iter()
        .enumerate()
        .map(|(n, net)| (net.name.as_str(), n))
        .collect();
    let pair = |net: &str| index.get(net).is_some_and(|&n| partners[n].is_some());
    let crystal = |part: &netsmith_pcb::board::Placed| {
        part.kind.starts_with("Crystal") || part.provides.iter().any(|g| g == "Crystal")
    };
    let clock_nets: Vec<&str> = board
        .nets
        .iter()
        .filter(|n| {
            let lower = n.name.to_lowercase();
            let named = ["clk", "xin", "xout", "xtal"]
                .iter()
                .any(|w| lower.contains(w));
            let on_crystal = n
                .pads
                .iter()
                .any(|p| board.part(&p.refdes).is_some_and(crystal));
            // A crystal's ground pins are not its clock.
            let plane = board.planes.iter().any(|pl| pl.net == n.name)
                || board.pours.iter().any(|po| po.net == n.name);
            !plane && (named || on_crystal)
        })
        .map(|n| n.name.as_str())
        .collect();
    let power = |net: &str| {
        board
            .net_currents
            .iter()
            .any(|c| c.net == net && c.current >= POWER_CURRENT)
    };
    let decoupling = crate::quality::decoupling_pairs(&board.parts, &board.nets);
    let decouples = |l: &Ratline| {
        decoupling.iter().any(|pair| {
            let cap = &board.parts[pair.cap].refdes;
            pair.nets.iter().any(|n| {
                let ends = |a: &netsmith_pcb::board::PadRef, b: &netsmith_pcb::board::PadRef| {
                    &a.refdes == cap
                        && a.number == n.cap_pad
                        && n.pins.iter().any(|(part, pad)| {
                            board.parts[*part].refdes == b.refdes && *pad == b.number
                        })
                };
                ends(&l.from, &l.to) || ends(&l.to, &l.from)
            })
        })
    };
    lines
        .iter()
        .zip(depth)
        .map(|(l, &d)| {
            if l.to_plane || d > 0.0 {
                Class::Escape
            } else if decouples(l) {
                Class::Decoupling
            } else if pair(&l.net) {
                Class::Pair
            } else if clock_nets.contains(&l.net.as_str()) {
                Class::Clock
            } else if power(&l.net) {
                Class::Power
            } else {
                Class::Rest
            }
        })
        .collect()
}

fn initial_obstacles(board: &Board) -> Obstacles {
    let rules = &board.rules;
    let net_of: HashMap<(&str, &str), usize> = board
        .nets
        .iter()
        .enumerate()
        .flat_map(|(n, net)| {
            net.pads
                .iter()
                .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n))
        })
        .collect();
    let mut list = Vec::new();
    let mut via_keepouts = Vec::new();
    for part in &board.parts {
        for pad in &part.footprint.pads {
            let (front, back) = part.pad_layers(pad);
            if !front && !back {
                // A hole with no copper (a mounting hole, a pogo-pin pad) still keeps
                // copper and other holes away from its edge.
                if let Some((a, b, radius)) = part.hole(pad) {
                    // The page's "NPTH to track" (issue #431).
                    list.push(Obstacle {
                        net: None,
                        layers: [true, true],
                        form: Form::Segment(a, b, radius),
                        clearance: rules.clearance.max(rules.hole_clearance) + MARGIN,
                    });
                    // A via's hole keeps `hole_to_hole` from this hole's edge.
                    via_keepouts.push((
                        Form::Segment(a, b, radius),
                        rules.hole_to_hole + rules.via_drill / 2.0 + MARGIN,
                    ));
                }
                // Its opening in the solder mask, which may be wider than the hole, exposes
                // copper: a track keeps the process's space from it (issue #431).
                let mask = mask_sides(part, pad);
                if mask != [false, false] && pad.width > 0.0 && pad.height > 0.0 {
                    let form = match pad.shape {
                        netsmith_pcb::footprint::PadShape::Circle => {
                            Form::Circle(part.pad_centre(pad), pad.width.min(pad.height) / 2.0)
                        }
                        _ => Form::Rect(part.pad_bounds(pad)),
                    };
                    list.push(Obstacle {
                        net: None,
                        layers: mask,
                        form,
                        clearance: rules.mask_to_track + MARGIN,
                    });
                }
                continue;
            }
            let bounds = part.pad_bounds(pad);
            let pad_net = net_of
                .get(&(part.refdes.as_str(), pad.number.as_str()))
                .copied();
            // A track keeps the page's "PTH to track" from a plated pad's hole, and "NPTH to
            // track" from a hole with copper round it that is not plated (issue #431); the pad's
            // copper and its clearance usually keep more.
            if let Some((a, b, radius)) = part.hole(pad) {
                let keep = if pad.kind == netsmith_pcb::footprint::PadKind::NpThruHole {
                    rules.hole_clearance
                } else {
                    rules.plated_hole_clearance
                };
                list.push(Obstacle {
                    net: pad_net,
                    layers: [true, true],
                    form: Form::Segment(a, b, radius),
                    clearance: keep + MARGIN,
                });
            }
            list.push(Obstacle {
                net: pad_net,
                layers: [front, back],
                form: Form::Rect(bounds),
                // The larger of the board's clearance and the pad's own, as the design
                // rules check applies it.
                clearance: rules.clearance.max(pad.clearance.unwrap_or(0.0)) + MARGIN,
            });
            // A via keeps off every pad, and keeps its hole clear of a pad's hole.
            let reach = match pad.drill {
                Some(drill) => {
                    // The via's hole keeps `hole_to_hole` from the pad's hole, measured from
                    // the pad's centre, which is at least half the shorter side from its edge.
                    let apart = drill.width.max(drill.height) / 2.0
                        + rules.hole_to_hole
                        + rules.via_drill / 2.0;
                    let inset = bounds.width().min(bounds.height()) / 2.0;
                    (apart - inset).max(rules.via_diameter / 2.0) + MARGIN
                }
                None => rules.via_diameter / 2.0 + PITCH / 2.0,
            };
            via_keepouts.push((Form::Rect(bounds), reach));
        }
    }
    // An opening the footprint draws in the solder mask exposes any copper in it, so a via
    // (whose own hole the mask may not cover) keeps the mask's least web from it.
    for part in &board.parts {
        for opening in part.mask_openings() {
            via_keepouts.push((
                Form::Rect(opening),
                rules.via_diameter / 2.0 + rules.mask_bridge + MARGIN,
            ));
        }
    }
    // Keepouts: tracks keep out of the areas that forbid them on a layer (and, because a
    // track's cells also bound vias here, vias keep out of them too), and vias of the
    // areas that forbid them.
    for k in board.keepout_areas() {
        let layers = [k.on(CopperLayer::Front), k.on(CopperLayer::Back)];
        if k.tracks && layers != [false, false] {
            list.push(Obstacle {
                net: None,
                layers,
                form: Form::Rect(k.rect),
                clearance: MARGIN,
            });
        }
        if k.vias {
            via_keepouts.push((Form::Rect(k.rect), rules.via_diameter / 2.0 + PITCH / 2.0));
        }
    }
    // The board edge: the outline, with its rounded corners, and the edge of each cutout.
    let edges = std::iter::once(board.outline_shape())
        .chain(board.cutout_shapes().into_iter().map(|(_, shape)| shape));
    for (from, to) in edges.flat_map(|shape| shape.segments()) {
        list.push(Obstacle {
            net: None,
            layers: [true, true],
            form: Form::Segment(from, to, 0.0),
            clearance: rules.copper_to_edge + MARGIN,
        });
    }
    Obstacles { list, via_keepouts }
}

/// Why a line's copper was not kept: it comes too near other copper, or a right angle has no
/// cut into two bends of 45 degrees that keeps the rules' space.
#[derive(Clone, Copy)]
enum Unsettled {
    Clearance,
    Corner,
}

/// How far from a right angle, in mm, the clean-up tries to cut it, the farthest first.
const CHAMFERS: [f64; 4] = [0.3, 0.2, 0.1, 0.05];

/// How much longer, in mm, a net of a diff pair may become when [`Router::side_by_side`] runs
/// it beside its partner: what a grid path of the same length measures differently once drawn.
const PAIR_SLACK: f64 = 0.2;

/// Whether [`Router::side_by_side`] keeps a net's new copper, from its length in mm, its vias
/// and the share of it that runs coupled, before and after: more of it coupled, by a point at
/// least, no longer than [`PAIR_SLACK`] more, and no more vias.
fn keeps_pair(before: (f64, usize, f64), after: (f64, usize, f64)) -> bool {
    after.2 > before.2 + 0.01 && after.0 <= before.0 + PAIR_SLACK + 1e-6 && after.1 <= before.1
}

/// The faces whose solder mask a pad opens, on the board: front first.
fn mask_sides(part: &netsmith_pcb::board::Placed, pad: &netsmith_pcb::footprint::Pad) -> [bool; 2] {
    let has = |name: &str| pad.layers.iter().any(|l| l == name || l == "*.Mask");
    let (front, back) = (has("F.Mask"), has("B.Mask"));
    if part.mirrored() {
        [back, front]
    } else {
        [front, back]
    }
}

/// The coarse plan of a board and the corridor of each line ([`crate::plan`]).
struct Planned {
    tiles: crate::plan::Tiles,
    /// For each line, the tiles of its corridor, row by row; `None` for a line the plan does
    /// not steer.
    corridors: Vec<Option<Vec<bool>>>,
    /// How full the plan leaves each tile on each layer ([`crate::plan::Plan::fill`]).
    fill: [Vec<f64>; 2],
}

/// What one ratsnest line became.
#[derive(Clone)]
struct Routed {
    line: usize,
    tracks: Vec<Track>,
    vias: Vec<Via>,
}

/// A step of a search for a line in a window, and the print of what it depends on
/// ([`Router::print`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MemoKey {
    line: usize,
    room: u64,
    soft: bool,
    fine: bool,
    /// Whether the search pays for the escape lanes ([`Lanes`]), which never move.
    escaping: bool,
    /// Whether the search pays for leaving its corridor of the coarse plan, which never moves.
    planned: bool,
    /// Whether the search runs a pair's net beside its partner ([`Router::side_by_side`]):
    /// the partner's tracks are in the print.
    pairing: bool,
    /// Whether the search pays for other nets' pours ([`Router::off_the_pours`]).
    pouring: bool,
    window: Window,
    print: u128,
    /// 0 for the flood ([`link`]), and 1 on for the searches in turn.
    step: u8,
}

/// What a step of a search found: whether the ends are joined, or the copper of the path (as
/// [`Router::settled`] left it, before the joins with the net's own tracks are checked) and
/// the path, `None` if there is no path.
#[derive(Clone)]
enum Memo {
    Link(Link),
    Search(Option<Result<(Routed, Vec<Spot>), Unsettled>>),
}

/// How many steps of searches the memo holds before it is emptied.
const MEMO_MOST: usize = 200_000;

/// Which lines are routed and how, and which are not.
#[derive(Clone, Default)]
struct State {
    routed: Vec<Routed>,
    failed: Vec<usize>,
}

struct Router<'a> {
    board: &'a Board,
    grid: Grid,
    fixed: Obstacles,
    /// The obstacles of `fixed` that each net's pads make, as indices into its list.
    fixed_by_net: Vec<Vec<usize>>,
    /// What the fixed obstacles keep cells from, for each room and track width asked for.
    bases: RefCell<HashMap<(u64, u64), std::rc::Rc<Base>>>,
    lines: Vec<Ratline>,
    /// The net of each line, as an index into the board's nets.
    nets: Vec<usize>,
    /// The cells each line starts and ends on.
    ends: Vec<(Vec<Spot>, Vec<Spot>)>,
    /// The pads at each end of a line that a via may stand in.
    in_pads: Vec<(Option<InPad>, Option<InPad>)>,
    /// The boxes round the pads of the parts at each end of a line, that a track may be necked
    /// down near ([`Board::neck_box`]), the second `None` for a line to a plane. Asked for at
    /// every cell of a window, so kept.
    necks: Vec<[Option<Rect>; 2]>,
    /// How deep in a fine-pitch grid of pads the deeper end of each line is ([`pad_depth`]).
    depth: Vec<f64>,
    /// How important each line is ([`Class`], issue #416).
    class: Vec<Class>,
    /// Which tries keep the classes in turn and their protection ([`LineClasses`]).
    class_use: LineClasses,
    /// Whether this try keeps the classes' protection.
    classed: Cell<bool>,
    /// Whether lines left over are negotiated for and given a chain of rip-ups, and what the
    /// negotiation has learnt.
    negotiate: bool,
    history: RefCell<[Vec<u8>; 2]>,
    /// Whether the router is in the first pass of a try, which searches in eight directions.
    fine: Cell<bool>,
    /// When to stop, if there is a limit.
    deadline: Option<Instant>,
    /// Whether the deadline has passed; once it has, every search gives up at once.
    stopped: Cell<bool>,
    progress: Option<fn(&str)>,
    cancel: Option<&'static AtomicBool>,
    last_report: Cell<Instant>,
    /// What the search's moves cost.
    costs: Costs,
    /// The stage the router is in, and for each line the stage of this try it last failed in.
    stage: Cell<Stage>,
    failed_in: RefCell<Vec<Stage>>,
    /// How many soft paths have passed through each cell of the crowding map, over the whole
    /// run, and whether they are being counted (not while the report is made).
    crowding: RefCell<Crowding>,
    counting: Cell<bool>,
    /// What each stage of this try routed, if it is asked for.
    snapshots: Option<RefCell<Vec<Snapshot>>>,
    /// The escape lanes of the fine-pitch parts' pads ([`Lanes`]), if the board has any.
    lanes: Option<Lanes>,
    /// Whether this try's searches pay for the lanes: from the second try on.
    escaping: Cell<bool>,
    /// The coarse plan ([`crate::plan`]) and each line's corridor, if one is made.
    plan: Option<Planned>,
    /// Whether this try's searches pay for leaving their corridors.
    planned: Cell<bool>,
    /// Whether the searches pay [`POUR`] for a step on another net's pour: in
    /// [`Router::off_the_pours`] only.
    pouring: Cell<bool>,
    /// Which tries follow the plan, and what a step outside a corridor costs.
    plan_use: CoarsePlan,
    plan_cost: i64,
    /// What searches found, by line, window and the print of the copper near the window
    /// ([`Router::print`]): a search whose print is unchanged finds what it found before.
    memo: RefCell<HashMap<MemoKey, Memo>>,
    /// How far beyond a window copper is in its print: farther than any mask, check or via
    /// keeps from it.
    print_reach: f64,
    /// The other net of each net's diff pair ([`diff_pair_partners`]).
    partners: Vec<Option<usize>>,
    /// Whether this try's searches run a pair's net beside its partner ([`Masks::follow`]).
    pairing: Cell<bool>,
}

impl<'a> Router<'a> {
    fn new(board: &'a Board, options: &RouteOptions) -> Self {
        let grid = Grid::over(&board.outline);
        let net_index: HashMap<&str, usize> = board
            .nets
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.as_str(), i))
            .collect();
        let lines = Ratsnest::of(board).lines;
        let nets = lines.iter().map(|l| net_index[l.net.as_str()]).collect();
        let ends = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    Vec::new()
                } else {
                    pad_spots(board, &l.to, l.to_at, &grid)
                };
                (pad_spots(board, &l.from, l.from_at, &grid), to)
            })
            .collect();
        let in_pads = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    None
                } else {
                    in_pad(board, &l.to, l.to_at, &grid)
                };
                (in_pad(board, &l.from, l.from_at, &grid), to)
            })
            .collect();
        let necks = lines
            .iter()
            .map(|l| {
                [
                    board.neck_box(&l.from.refdes),
                    (!l.to_plane)
                        .then(|| board.neck_box(&l.to.refdes))
                        .flatten(),
                ]
            })
            .collect();
        let depth: Vec<f64> = lines
            .iter()
            .map(|l| {
                let to = if l.to_plane {
                    0.0
                } else {
                    pad_depth(board, &l.to, l.to_at)
                };
                pad_depth(board, &l.from, l.from_at).max(to)
            })
            .collect();
        let class = classes(board, &lines, &depth);
        let fixed = initial_obstacles(board);
        let mut fixed_by_net = vec![Vec::new(); board.nets.len()];
        for (k, o) in fixed.list.iter().enumerate() {
            if let Some(n) = o.net {
                fixed_by_net[n].push(k);
            }
        }
        let crowding = Crowding::over(&grid);
        let lanes = escape_lanes(board, &grid);
        // The farthest any copper reaches into a mask or a check: half the widest track or
        // a via, its space and the room, or a via's hole and its space, with a millimetre over.
        let rules = &board.rules;
        let widest = board
            .nets
            .iter()
            .map(|n| board.track_width(&n.name))
            .fold(rules.via_diameter, f64::max);
        let print_reach = 1.0
            + widest
            + rules.clearance
            + MARGIN
            + (rules.via_hole_to_hole + rules.via_drill).max(rules.hole_clearance);
        Router {
            board,
            fixed,
            fixed_by_net,
            bases: RefCell::new(HashMap::new()),
            grid,
            lines,
            nets,
            ends,
            in_pads,
            necks,
            depth,
            class,
            class_use: options.classes,
            classed: Cell::new(false),
            negotiate: options.negotiate,
            history: RefCell::new([Vec::new(), Vec::new()]),
            fine: Cell::new(true),
            deadline: options.time_limit.map(|t| Instant::now() + t),
            stopped: Cell::new(false),
            progress: options.progress,
            cancel: options.cancel,
            last_report: Cell::new(Instant::now()),
            costs: options.costs,
            stage: Cell::new(Stage::FirstPass),
            failed_in: RefCell::new(Vec::new()),
            crowding: RefCell::new(crowding),
            counting: Cell::new(true),
            snapshots: options.snapshots.then(|| RefCell::new(Vec::new())),
            lanes,
            escaping: Cell::new(false),
            plan: None,
            planned: Cell::new(false),
            pouring: Cell::new(false),
            plan_use: options.plan,
            plan_cost: options.plan_cost.unwrap_or(OFF_PLAN),
            memo: RefCell::new(HashMap::new()),
            print_reach,
            partners: diff_pair_partners(board),
            pairing: Cell::new(false),
        }
    }

    /// The masks of a window for a net: see [`masks_for`].
    fn masks(
        &self,
        window: Window,
        net: usize,
        (routed, keepouts): (&[&Obstacle], &[(Form, f64)]),
        (soft, soft_keepouts): (&[&Obstacle], &[(Form, f64)]),
        reach: Reach,
    ) -> Masks {
        let key = (reach.room.to_bits(), reach.width.to_bits());
        let base = self
            .bases
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                std::rc::Rc::new(Base::new(self.board, &self.grid, &self.fixed, reach))
            })
            .clone();
        let own: Vec<&Obstacle> = self.fixed_by_net[net]
            .iter()
            .map(|&k| &self.fixed.list[k])
            .collect();
        let mut m = masks_for(
            &self.grid, window, self.board, &base, &own, routed, keepouts, net, reach,
        );
        m.turn = self.costs.turn;
        if !soft.is_empty() || !soft_keepouts.is_empty() {
            // The cells only the soft copper keeps tracks and vias from.
            let mut counts = Counts::new(window);
            for o in soft.iter().filter(|o| o.net != Some(net)) {
                counts.stamp(&self.grid, &self.board.rules, o, reach, true);
            }
            for (form, keep) in soft_keepouts {
                counts.stamp_keepout(&self.grid, form, *keep);
            }
            m.soft = Some(Soft {
                track: [FRONT, BACK].map(|l| {
                    (0..window.cells())
                        .map(|c| counts.track[l][c] > 0 && !m.track[l][c])
                        .collect()
                }),
                via: (0..window.cells())
                    .map(|c| counts.via[c] > 0 && !m.via[c])
                    .collect(),
            });
        }
        m
    }

    /// The width a line's tracks are drawn at: its net's (ADR 0020).
    fn width_of(&self, line: usize) -> f64 {
        self.board.track_width(&self.lines[line].net)
    }

    /// The narrowest a line's track may be necked down to away from its pads: the width its
    /// net's current needs (issue #431), and no wider than the track itself.
    fn neck_width(&self, line: usize) -> f64 {
        self.board
            .least_width(&self.lines[line].net)
            .min(self.width_of(line))
    }

    /// For a net whose current one via does not carry, leave a cell open for a via only where
    /// the others fit beside it (issue #431, [`crate::parallel`]): enough cells a cluster's step
    /// away, on the four straight or the four diagonal directions, open for a via too.
    fn room_for_vias(&self, line: usize, masks: &mut Masks) {
        if self.vias_needed(line) <= 1 {
            return;
        }
        let w = masks.window;
        let keep: Vec<bool> = (0..w.cells())
            .map(|c| {
                masks.via[c]
                    || self
                        .cluster_cells(line, masks, w.i0 + c % w.nx, w.j0 + c / w.nx)
                        .is_none()
            })
            .collect();
        masks.via_alone = Some(std::mem::replace(&mut masks.via, keep));
    }

    /// The cells inside a pad of the net, on each layer the pad has copper on.
    fn mark_own_pads(&self, net: usize, masks: &mut Masks) {
        let w = masks.window;
        let mut free = [vec![false; w.cells()], vec![false; w.cells()]];
        // The box round a pad of any shape is more than its copper: a round pad fills only the
        // square inside it, so only that is counted, for every pad.
        for (r, layers) in self.own_pad_squares(net) {
            for (i, j) in self.grid.cells_near(&w, &r, 0.0) {
                let p = self.grid.centre(i, j);
                if r.contains(&Rect::new(p, p)) {
                    for layer in [FRONT, BACK] {
                        if layers[layer] {
                            free[layer][w.index(i, j)] = true;
                        }
                    }
                }
            }
        }
        masks.free_turn = free;
    }

    /// [`Masks::follow`] for a line of `net`: the cells from the pair's spacing (half of each
    /// width and the clearance) out to [`PAIR_NEAR`] from the centre line of the partner's
    /// routed tracks, on their layers; empty when the net has no routed partner.
    fn follow_cells(
        &self,
        net: usize,
        routed: &[&Routed],
        w: Window,
        width: f64,
    ) -> [Vec<bool>; 2] {
        let Some(partner) = self.partners[net] else {
            return [Vec::new(), Vec::new()];
        };
        let tracks: Vec<&Track> = routed
            .iter()
            .filter(|r| self.nets[r.line] == partner)
            .flat_map(|r| r.tracks.iter())
            .collect();
        if tracks.is_empty() {
            return [Vec::new(), Vec::new()];
        }
        let mut cells = [vec![false; w.cells()], vec![false; w.cells()]];
        for t in tracks {
            let spacing = t.width / 2.0 + width / 2.0 + self.board.rules.clearance;
            let layer = usize::from(t.layer == CopperLayer::Back);
            let form = Form::Segment(t.from, t.to, 0.0);
            for (i, j) in self.grid.cells_near(&w, &form.bounds(), PAIR_NEAR) {
                let d = point_to_line(self.grid.centre(i, j), t.from, t.to);
                if d >= spacing - PITCH * 0.75 && d <= PAIR_NEAR {
                    cells[layer][w.index(i, j)] = true;
                }
            }
        }
        cells
    }

    /// How many vias side by side the line's net needs where it changes layer.
    fn vias_needed(&self, line: usize) -> usize {
        let net = &self.lines[line].net;
        self.board
            .net_currents
            .iter()
            .find(|c| &c.net == net)
            .map_or(1, |c| c.vias)
    }

    /// Where the other vias of the line's cluster stand round a via at cell (i, j): a cluster's
    /// step from its centre (their copper overlaps, their holes keep the via-to-via space), on
    /// the four straight directions or else the four diagonal ones, the first whose nearest
    /// cells the masks leave open for a via, in that order. `None` if too few are open.
    fn cluster_cells(&self, line: usize, masks: &Masks, i: usize, j: usize) -> Option<Vec<Pt>> {
        let want = self.vias_needed(line).saturating_sub(1).min(4);
        let rules = &self.board.rules;
        let step = rules.via_hole_to_hole + rules.via_drill + crate::parallel::MARGIN;
        let w = masks.window;
        let centre = self.grid.centre(i, j);
        let open = |p: Pt| {
            let (a, b) = (
                ((p.x - self.grid.origin.x) / PITCH).round(),
                ((p.y - self.grid.origin.y) / PITCH).round(),
            );
            if a >= 0.0 && b >= 0.0 && w.contains(a as usize, b as usize) {
                let alone = masks.via_alone.as_ref().unwrap_or(&masks.via);
                !alone[w.index(a as usize, b as usize)]
            } else {
                false
            }
        };
        let r = std::f64::consts::FRAC_1_SQRT_2;
        for set in [
            [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)],
            [(r, r), (-r, r), (-r, -r), (r, -r)],
        ] {
            let found: Vec<Pt> = set
                .iter()
                .map(|(a, b)| Pt::new(centre.x + a * step, centre.y + b * step))
                .filter(|&p| open(p))
                .take(want)
                .collect();
            if found.len() >= want {
                return Some(found);
            }
        }
        None
    }

    /// Whether a point is within `reach` of the pads of a part an end of the line is on.
    fn near_end_by(&self, line: usize, p: Pt, reach: f64) -> bool {
        self.necks[line]
            .iter()
            .flatten()
            .any(|b| b.expanded(reach).contains(&Rect::new(p, p)))
    }

    /// Whether a point is within [`NECK_REACH`] of the pads of a part an end of the line is on.
    fn near_end(&self, line: usize, p: Pt) -> bool {
        self.near_end_by(line, p, NECK_REACH)
    }

    /// For each cell of the window, whether it is within [`NECK_REACH`] of the line's pads.
    fn near_ends(&self, line: usize, window: Window) -> Vec<bool> {
        (0..window.cells())
            .map(|c| {
                let (i, j) = (window.i0 + c % window.nx, window.j0 + c / window.nx);
                // A cell a step inside the reach, so that the step out of the last pinched cell,
                // which is drawn at the pinched width, stays within it.
                self.near_end_by(line, self.grid.centre(i, j), NECK_REACH - PITCH * 1.5)
            })
            .collect()
    }

    /// Whether the time limit has passed, or the caller asked for the router to stop.
    fn out_of_time(&self) -> bool {
        // A job that was cancelled (`fab`) ends here; the flag below stops the router and
        // keeps what it routed, which is what the PCB view's "stop" asks for.
        netsmith_core::job::checkpoint();
        if !self.stopped.get()
            && (self.deadline.is_some_and(|d| Instant::now() >= d)
                || self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)))
        {
            self.stopped.set(true);
        }
        self.stopped.get()
    }

    /// Tell the progress function something, if there is one. Messages with `now` set are
    /// always sent, the others at most once a second.
    fn report(&self, now: bool, message: impl FnOnce() -> String) {
        let Some(progress) = self.progress else {
            return;
        };
        if now || self.last_report.get().elapsed() >= REPORT_EVERY {
            self.last_report.set(Instant::now());
            progress(&message());
        }
    }

    /// The obstacles that routed copper makes, and the places it keeps vias from.
    fn copper_of(&self, routed: &[&Routed]) -> (Vec<Obstacle>, Vec<(Form, f64)>) {
        let rules = &self.board.rules;
        let mut list = Vec::new();
        let mut keepouts = Vec::new();
        for r in routed {
            let net = Some(self.nets[r.line]);
            for t in &r.tracks {
                list.push(Obstacle {
                    net,
                    layers: [t.layer == CopperLayer::Front, t.layer == CopperLayer::Back],
                    form: Form::Segment(t.from, t.to, t.width / 2.0),
                    clearance: rules.clearance + MARGIN,
                });
            }
            for v in &r.vias {
                list.push(Obstacle {
                    net,
                    layers: [true, true],
                    form: Form::Circle(v.at, v.diameter / 2.0),
                    clearance: rules.clearance + MARGIN,
                });
                // Another via's hole keeps its distance, whatever the net.
                keepouts.push((
                    Form::Circle(v.at, 0.0),
                    rules.via_hole_to_hole + rules.via_drill + MARGIN,
                ));
            }
        }
        (list, keepouts)
    }

    /// Try to route one line, given the lines routed so far, but leaving out the copper of
    /// the nets in `ignore`, which is as if they were ripped up.
    ///
    /// The line is searched for in a window round its ends first, which is cheap, and in a
    /// larger one only if that finds nothing.
    fn try_line(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
    ) -> Option<(Routed, Vec<Spot>)> {
        self.try_line_locked(line, state, ignore, room, soft, &[])
    }

    /// [`Self::try_line`], where the lines in `locked` keep their copper hard even in a soft
    /// search: they are not to be ripped up. A line that a search for copper (not a soft one)
    /// fails to route is marked as failed in the stage the router is in, or out of time.
    fn try_line_locked(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
        locked: &[usize],
    ) -> Option<(Routed, Vec<Spot>)> {
        let found = self.search_line(line, state, ignore, room, soft, locked);
        if found.is_none() && !soft {
            let stage = if self.stopped.get() {
                Stage::OutOfTime
            } else {
                self.stage.get()
            };
            if let Some(s) = self.failed_in.borrow_mut().get_mut(line) {
                *s = stage;
            }
        }
        found
    }

    /// The search of [`Self::try_line_locked`].
    fn search_line(
        &self,
        line: usize,
        state: &State,
        ignore: &[usize],
        room: f64,
        soft: bool,
        locked: &[usize],
    ) -> Option<(Routed, Vec<Spot>)> {
        let others: Vec<&Routed> = state
            .routed
            .iter()
            .filter(|r| !ignore.contains(&self.nets[r.line]))
            .collect();
        // In a soft search the locked lines' copper stays hard; the rest is crossed at a price.
        let (fixed_others, free_others): (Vec<&Routed>, Vec<&Routed>) = if soft {
            others.iter().partition(|r| locked.contains(&r.line))
        } else {
            (others.clone(), Vec::new())
        };
        let (list, routed_keepouts) = self.copper_of(&fixed_others);
        let (soft_copper, soft_copper_keepouts) = self.copper_of(&free_others);
        // A soft search goes through the copper of other nets at a price, to find the nets
        // that are least in the way; what it finds is not copper that can be drawn.
        let obstacles: Vec<&Obstacle> = self.fixed.list.iter().chain(list.iter()).collect();
        let routed: Vec<&Obstacle> = list.iter().collect();
        let (hard, hard_keepouts): (&[&Obstacle], &[(Form, f64)]) = (&routed, &routed_keepouts);
        let soft_routed: Vec<&Obstacle> = soft_copper.iter().collect();
        let (soft_list, soft_keepouts): (&[&Obstacle], &[(Form, f64)]) =
            (&soft_routed, &soft_copper_keepouts);
        let mut keepouts = hard_keepouts.to_vec();
        keepouts.extend(self.fixed.via_keepouts.iter().copied());
        let (from, to) = &self.ends[line];
        let goal = if self.lines[line].to_plane {
            Goal::Via
        } else {
            Goal::Spots(to)
        };
        // The pads a via may stand in at the ends of the line, where the via would keep the
        // rules' space from the copper there is now.
        let net = self.nets[line];
        let usable =
            |pad: Option<InPad>| pad.filter(|p| self.via_fits(p, net, &obstacles, &keepouts));
        let (in_from, in_to) = (usable(self.in_pads[line].0), usable(self.in_pads[line].1));
        let from_extra: Vec<Spot> = in_from.iter().map(|p| p.spot).collect();
        let to_extra: Vec<Spot> = in_to.iter().map(|p| p.spot).collect();
        let extras = Extras {
            from: &from_extra,
            to: &to_extra,
            right_angles: true,
            coarse: false,
        };
        let ends: Vec<Spot> = from.iter().chain(to).copied().collect();
        let mut tried: Option<Window> = None;
        for margin in WINDOW_MARGINS.iter().map(|&m| Some(m)).chain([None]) {
            let window = match margin {
                Some(m) => Window::around(&self.grid, &ends, m),
                None => Window::full(&self.grid),
            };
            if tried == Some(window) {
                continue;
            }
            tried = Some(window);
            // What a search in this window found before, if nothing it depends on has changed
            // since ([`Router::memo`]): the masks are made only when something is searched.
            let print = self.print(window, net, &others, soft, locked);
            let key = |step: u8| MemoKey {
                line,
                room: room.to_bits(),
                soft,
                fine: self.fine.get(),
                escaping: self.escaping.get(),
                planned: self.planned.get(),
                pairing: self.pairing.get(),
                pouring: self.pouring.get(),
                window,
                print,
                step,
            };
            let mut built: Option<Masks> = None;
            let make = |built: &mut Option<Masks>| {
                if built.is_none() {
                    *built = Some(self.window_masks(
                        line,
                        window,
                        room,
                        (hard, hard_keepouts),
                        (soft_list, soft_keepouts),
                    ));
                    if self.pairing.get()
                        && let Some(masks) = built.as_mut()
                    {
                        masks.follow = self.follow_cells(net, &others, window, self.width_of(line));
                        // The estimate is cut to match, so that the search does go out of its
                        // way to run beside the partner; the pass keeps the new copper only if
                        // it is no longer.
                        if !masks.follow[FRONT].is_empty() {
                            masks.estimate = FOLLOW;
                        }
                    }
                }
            };
            let own: Vec<&Routed> = others
                .iter()
                .copied()
                .filter(|r| r.line != line && self.nets[r.line] == net)
                .collect();
            let linked = match self.recall(&key(0)) {
                Some(Memo::Link(linked)) => linked,
                _ => {
                    make(&mut built);
                    let linked = link(built.as_ref().expect("made"), from, goal, extras);
                    self.remember(key(0), Memo::Link(linked));
                    linked
                }
            };
            match linked {
                Link::Joined => {}
                Link::Closed => break,
                Link::Cramped if window.covers(&self.grid) => break,
                Link::Cramped => continue,
            }
            // First with right angles, which the clean-up cuts into two bends of 45 degrees;
            // where a cut would come too near other copper, the line is searched for again with
            // no right angle at all (issue #415).
            let mut give_up = false;
            // A repair searches first in four directions, then (where a cut does not fit) in
            // eight, both with right angles that are cut; and last in eight with none. The first
            // pass starts at eight.
            let modes: &[(bool, bool)] = if soft {
                &[(true, true)]
            } else if self.fine.get() {
                &[(true, false), (false, false)]
            } else {
                &[(true, true), (true, false), (false, false)]
            };
            for (n, &(right_angles, coarse)) in modes.iter().enumerate() {
                let last = n + 1 == modes.len();
                let extras = Extras {
                    right_angles,
                    coarse,
                    ..extras
                };
                let found = match self.recall(&key(n as u8 + 1)) {
                    Some(Memo::Search(found)) => found,
                    _ => {
                        make(&mut built);
                        let masks = built.as_ref().expect("made");
                        let stop = || self.out_of_time();
                        // Where no right angle is allowed, a line often has no path: a pad that
                        // is reached only round a corner. A flood says so for much less than
                        // a search that runs out of states.
                        let flood = (!extras.right_angles && !extras.coarse)
                            .then(|| reachable(masks, from, goal, extras, &stop));
                        let searched = match flood {
                            Some(Err(miss)) => Err(miss),
                            Some(Ok(false)) => Err(Miss::Blocked),
                            _ => search(masks, from, goal, extras, &stop),
                        };
                        let found = match searched {
                            Ok(path) => {
                                let start = in_from.filter(|p| path.first() == Some(&p.spot));
                                let end = in_to.filter(|p| path.last() == Some(&p.spot));
                                let routed = self.realize(line, &path, start, end, masks);
                                // What the grid let through is measured again on the copper
                                // itself: a line whose copper is nearer to another net's than
                                // the rules allow, even at the narrowest track, is not routed.
                                // A path that only finds the nets in the way is never drawn, so
                                // its corners are not cut.
                                Some(
                                    self.settled(routed, net, &obstacles, !soft)
                                        .map(|r| (r, path)),
                                )
                            }
                            Err(Miss::Stopped) => return None,
                            Err(Miss::Blocked) => None,
                        };
                        self.remember(key(n as u8 + 1), Memo::Search(found.clone()));
                        found
                    }
                };
                match found {
                    // The joins with the net's own tracks are checked on the copper there is
                    // now: the memo does not depend on them.
                    Some(settled) => match settled
                        .and_then(|(r, path)| Ok((self.joins_cleanly(r, &own, !soft)?, path)))
                    {
                        Ok(done) => return Some(done),
                        Err(Unsettled::Corner) if !last => continue,
                        Err(_) => {
                            give_up = true;
                            break;
                        }
                    },
                    None if window.covers(&self.grid) => {
                        give_up = true;
                        break;
                    }
                    None => break,
                }
            }
            if give_up {
                break;
            }
        }
        // A pad on a plane whose way out is blocked is joined by a via in the pad.
        if self.lines[line].to_plane
            && !self.out_of_time()
            && let Some(pad) = in_from
        {
            let rules = &self.board.rules;
            let via = Via {
                net: self.lines[line].net.clone(),
                at: pad.at,
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            };
            return Some((
                Routed {
                    line,
                    tracks: Vec::new(),
                    vias: vec![via],
                },
                Vec::new(),
            ));
        }
        None
    }

    /// The masks a line is searched for in, in a window: the copper of other nets, kept at
    /// `room`, with the cells where its track is necked down, the room its vias need, the
    /// cells inside its own pads and what the negotiation has learnt.
    fn window_masks(
        &self,
        line: usize,
        window: Window,
        room: f64,
        (hard, hard_keepouts): (&[&Obstacle], &[(Form, f64)]),
        (soft_list, soft_keepouts): (&[&Obstacle], &[(Form, f64)]),
    ) -> Masks {
        let net = self.nets[line];
        let rules = &self.board.rules;
        // The width this line's net is drawn at.
        let track_width = self.width_of(line);
        let mut masks = self.masks(
            window,
            net,
            (hard, hard_keepouts),
            (soft_list, soft_keepouts),
            Reach {
                room,
                width: track_width,
            },
        );
        // With the least room, a track that cannot pass between two pads at its full
        // width may pass necked down: to the width its current needs anywhere, and to the
        // narrowest the process makes within `NECK_REACH` of the line's own pads.
        let floor = self.neck_width(line);
        if room == ROOM_TIGHT && track_width > rules.min_track_width + 1e-9 {
            let reach = |width| Reach { room, width };
            let mut narrow = self
                .masks(
                    window,
                    net,
                    (hard, hard_keepouts),
                    (soft_list, soft_keepouts),
                    reach(floor),
                )
                .track;
            if floor > rules.min_track_width + 1e-9 {
                let thin = self
                    .masks(
                        window,
                        net,
                        (hard, hard_keepouts),
                        (soft_list, soft_keepouts),
                        reach(rules.min_track_width),
                    )
                    .track;
                let near = self.near_ends(line, window);
                let pinch = [FRONT, BACK].map(|l| {
                    (0..window.cells())
                        .map(|c| narrow[l][c] && !thin[l][c] && near[c])
                        .collect::<Vec<bool>>()
                });
                for l in [FRONT, BACK] {
                    for c in 0..window.cells() {
                        if pinch[l][c] {
                            narrow[l][c] = false;
                        }
                    }
                }
                masks.pinch = Some(pinch);
            }
            if floor < track_width - 1e-9 || masks.pinch.is_some() {
                masks.wide = Some(std::mem::replace(&mut masks.track, narrow));
            }
        }
        self.room_for_vias(line, &mut masks);
        self.mark_own_pads(net, &mut masks);
        if self.escaping.get() {
            masks.lane = self.lanes.as_ref().and_then(|l| l.charged(window, net));
        }
        if self.planned.get()
            && let Some(plan) = &self.plan
            && let Some(inside) = &plan.corridors[line]
        {
            let tiles = &plan.tiles;
            let cost = |layer: usize| -> Vec<i64> {
                (0..window.cells())
                    .map(|c| {
                        let (a, b) = tiles.of(window.i0 + c % window.nx, window.j0 + c / window.nx);
                        let t = b * tiles.nx + a;
                        if inside[t] {
                            0
                        } else {
                            crate::plan::crowding_cost(plan.fill[layer][t], self.plan_cost)
                        }
                    })
                    .collect()
            };
            let off = [cost(FRONT), cost(BACK)];
            if off.iter().flatten().any(|&c| c > 0) {
                masks.off_plan = Some(off);
            }
        }
        if self.pouring.get() {
            masks.pour = (self.pour_costs(net), POUR_VIA);
        }
        self.add_history(&mut masks);
        masks
    }

    /// A print of everything a search of a line of `net` in `window` depends on that changes
    /// while the router runs ([`Router::memo`]): the routed copper near the window, each piece
    /// marked hard or soft (in a soft search, the copper of the `locked` lines is hard), and
    /// what the negotiation has learnt about the window's cells. The net's own tracks are left
    /// out: the masks and the check of the copper do not see them, and the joins with them are
    /// checked on every search ([`Router::joins_cleanly`]). The net's own vias are in: their
    /// holes keep other vias away. The print does not depend on the order of the copper.
    fn print(
        &self,
        window: Window,
        net: usize,
        others: &[&Routed],
        soft: bool,
        locked: &[usize],
    ) -> u128 {
        use std::hash::{Hash, Hasher};
        let seen = Rect::new(
            self.grid.centre(window.i0, window.j0),
            self.grid
                .centre(window.i0 + window.nx - 1, window.j0 + window.ny - 1),
        )
        .expanded(self.print_reach);
        let one = |piece: &dyn Fn(&mut std::hash::DefaultHasher)| {
            let mut h = std::hash::DefaultHasher::new();
            piece(&mut h);
            h.finish()
        };
        let mut pieces: Vec<u64> = Vec::new();
        for r in others {
            let own = self.nets[r.line] == net;
            let hard = !soft || locked.contains(&r.line);
            if !own {
                for t in &r.tracks {
                    if seen.overlaps(&Form::Segment(t.from, t.to, t.width / 2.0).bounds()) {
                        pieces.push(one(&|h| {
                            (0u8, hard, t.layer == CopperLayer::Front).hash(h);
                            [t.from.x, t.from.y, t.to.x, t.to.y, t.width]
                                .map(f64::to_bits)
                                .hash(h);
                        }));
                    }
                }
            }
            for v in &r.vias {
                if seen.overlaps(&Form::Circle(v.at, v.diameter / 2.0).bounds()) {
                    pieces.push(one(&|h| {
                        (1u8, hard, own).hash(h);
                        [v.at.x, v.at.y, v.diameter].map(f64::to_bits).hash(h);
                    }));
                }
            }
        }
        pieces.sort_unstable();
        let history = self.history.borrow();
        let halves = [0u8, 1].map(|seed| {
            let mut h = std::hash::DefaultHasher::new();
            seed.hash(&mut h);
            pieces.hash(&mut h);
            if !history[FRONT].is_empty() {
                for layer in [FRONT, BACK] {
                    for j in window.j0..window.j0 + window.ny {
                        let row = j * self.grid.nx + window.i0;
                        history[layer][row..row + window.nx].hash(&mut h);
                    }
                }
            }
            h.finish()
        });
        (u128::from(halves[0]) << 64) | u128::from(halves[1])
    }

    /// What a search with this key found before, if it is remembered.
    fn recall(&self, key: &MemoKey) -> Option<Memo> {
        self.memo.borrow().get(key).cloned()
    }

    /// Remember what a search found. The memo is emptied when it grows past [`MEMO_MOST`],
    /// which only costs the searches again.
    fn remember(&self, key: MemoKey, found: Memo) {
        let mut memo = self.memo.borrow_mut();
        if memo.len() >= MEMO_MOST {
            memo.clear();
        }
        memo.insert(key, found);
    }

    /// The line's copper as it will be drawn, checked against the copper of other nets, the
    /// pads, the holes and the board edge as they are, not as cells: a track that is nearer
    /// than the rules allow is drawn at the narrowest width the process makes instead (a stub
    /// from a fine-pitch pad, or a run from a cell the path was given at its end, for a wide
    /// net); `None` if some copper is too near whatever its width. This is what makes
    /// "routed" mean that netsmith's own design rules check finds the copper clear.
    fn settled(
        &self,
        mut routed: Routed,
        net: usize,
        obstacles: &[&Obstacle],
        cut: bool,
    ) -> Result<Routed, Unsettled> {
        let rules = &self.board.rules;
        let floor = self.neck_width(routed.line);
        let line = routed.line;
        let others: Vec<&&Obstacle> = obstacles.iter().filter(|o| o.net != Some(net)).collect();
        // The space the rules ask for: an obstacle's clearance has the router's margin on it.
        let apart = |o: &Obstacle| o.clearance - MARGIN - EXACT_SLACK;
        for track in &mut routed.tracks {
            let layer = usize::from(track.layer == CopperLayer::Back);
            let clear = |width: f64| {
                let reach = Rect::new(
                    Pt::new(track.from.x.min(track.to.x), track.from.y.min(track.to.y)),
                    Pt::new(track.from.x.max(track.to.x), track.from.y.max(track.to.y)),
                );
                others.iter().filter(|o| o.layers[layer]).all(|o| {
                    let near = reach.expanded(width / 2.0 + o.clearance);
                    !near.overlaps(&o.form.bounds())
                        || o.form.distance_to_line(track.from, track.to) - width / 2.0 >= apart(o)
                })
            };
            if !clear(track.width) {
                // Narrower, but not below what the current needs, except near the line's pads.
                let steps = ((track.from.distance(track.to) / 0.05).ceil() as usize).max(1);
                let near = (0..=steps).all(|k| {
                    let t = k as f64 / steps as f64;
                    let p = Pt::new(
                        track.from.x + (track.to.x - track.from.x) * t,
                        track.from.y + (track.to.y - track.from.y) * t,
                    );
                    self.near_end(line, p)
                });
                let narrowest = if near { rules.min_track_width } else { floor };
                let width = [floor, narrowest]
                    .into_iter()
                    .filter(|&w| w < track.width - 1e-9)
                    .find(|&w| clear(w))
                    .ok_or(Unsettled::Clearance)?;
                track.width = width;
            }
        }
        for via in &routed.vias {
            let radius = via.diameter / 2.0;
            if !others
                .iter()
                .all(|o| o.form.distance(via.at) - radius >= apart(o))
            {
                return Err(Unsettled::Clearance);
            }
        }
        if cut {
            self.cut_corners(&mut routed, net, &others, &apart)?;
        }
        Ok(routed)
    }

    /// `routed` if every place where its tracks end meets the other tracks of its net there as
    /// `fab`'s `track_angle` check asks: two directions bend by 45 degrees at most, and three or
    /// more meet at right angles or wider, except inside a pad or a via of the net.
    /// `Err(Unsettled::Corner)` otherwise, for the line to be searched for again in the next way
    /// (issue #476): lines of one net may run over each other and cross at 45 degrees.
    fn joins_cleanly(
        &self,
        routed: Routed,
        own: &[&Routed],
        check: bool,
    ) -> Result<Routed, Unsettled> {
        if !check || own.is_empty() {
            return Ok(routed);
        }
        let net = self.nets[routed.line];
        let pads = self.own_pad_squares(net);
        let vias: Vec<&Via> = own
            .iter()
            .flat_map(|r| r.vias.iter())
            .chain(routed.vias.iter())
            .collect();
        let tracks: Vec<&Track> = own.iter().flat_map(|r| r.tracks.iter()).collect();
        let same = |p: Pt, q: Pt| p.distance(q) < 1e-3;
        for t in &routed.tracks {
            let layer = usize::from(t.layer == CopperLayer::Back);
            for at in [t.from, t.to] {
                if pads
                    .iter()
                    .any(|(r, layers)| layers[layer] && r.contains(&Rect::new(at, at)))
                    || vias.iter().any(|v| v.at.distance(at) <= v.diameter / 2.0)
                {
                    continue;
                }
                // The directions out of `at` of every track of the net that ends there.
                let mut dirs: Vec<(f64, f64)> = Vec::new();
                let mut met = false;
                for (s, theirs) in routed
                    .tracks
                    .iter()
                    .map(|s| (s, false))
                    .chain(tracks.iter().map(|s| (*s, true)))
                {
                    if s.layer != t.layer {
                        continue;
                    }
                    for (end, far) in [(s.from, s.to), (s.to, s.from)] {
                        let d = far - end;
                        let n = d.x.hypot(d.y);
                        if same(end, at) && n > 1e-6 {
                            met |= theirs;
                            let u = (d.x / n, d.y / n);
                            if !dirs
                                .iter()
                                .any(|v| (v.0 - u.0).abs() < 1e-3 && (v.1 - u.1).abs() < 1e-3)
                            {
                                dirs.push(u);
                            }
                        }
                    }
                }
                if !met || dirs.len() < 2 {
                    continue;
                }
                let least = (0..dirs.len())
                    .flat_map(|i| (i + 1..dirs.len()).map(move |j| (i, j)))
                    .map(|(i, j)| {
                        let dot = dirs[i].0 * dirs[j].0 + dirs[i].1 * dirs[j].1;
                        dot.clamp(-1.0, 1.0).acos().to_degrees()
                    })
                    .fold(180.0, f64::min);
                let limit = if dirs.len() == 2 { 134.0 } else { 89.0 };
                if least < limit {
                    return Err(Unsettled::Corner);
                }
            }
        }
        Ok(routed)
    }

    /// Cut each right angle between two tracks of the line on one layer into two bends of 45
    /// degrees (issue #415): the corner is replaced by a diagonal from a point on one track to a
    /// point on the other, as far from the corner as the shorter of them allows, up to
    /// [`CHAMFERS`]' first, checked exactly against the other nets' copper. A corner inside a
    /// pad of the net is the pad's copper and stays. `Err(Unsettled::Corner)` when a corner has
    /// no cut that keeps the rules' space, for the line to be searched for with no right angle.
    fn cut_corners(
        &self,
        routed: &mut Routed,
        net: usize,
        others: &[&&Obstacle],
        apart: &dyn Fn(&Obstacle) -> f64,
    ) -> Result<(), Unsettled> {
        let pads = self.own_pad_squares(net);
        let in_pad = |p: Pt, layer: usize| {
            pads.iter()
                .any(|(r, layers)| layers[layer] && r.contains(&Rect::new(p, p)))
        };
        let clear = |a: Pt, b: Pt, width: f64, layer: usize| {
            let reach = Rect::new(
                Pt::new(a.x.min(b.x), a.y.min(b.y)),
                Pt::new(a.x.max(b.x), a.y.max(b.y)),
            );
            others.iter().filter(|o| o.layers[layer]).all(|o| {
                let near = reach.expanded(width / 2.0 + o.clearance);
                !near.overlaps(&o.form.bounds())
                    || o.form.distance_to_line(a, b) - width / 2.0 >= apart(o)
            })
        };
        let same = |p: Pt, q: Pt| p.distance(q) < 1e-6;
        let mut k = 0;
        while k < routed.tracks.len() {
            let mut cut = false;
            for m in 0..routed.tracks.len() {
                if m == k || routed.tracks[m].layer != routed.tracks[k].layer {
                    continue;
                }
                let (a, b) = (&routed.tracks[k], &routed.tracks[m]);
                // The corner: an end of `a` that is an end of `b`.
                let Some((corner, a_far, b_far)) = [(a.to, a.from), (a.from, a.to)]
                    .into_iter()
                    .find_map(|(c, af)| {
                        if same(c, b.from) {
                            Some((c, af, b.to))
                        } else if same(c, b.to) {
                            Some((c, af, b.from))
                        } else {
                            None
                        }
                    })
                else {
                    continue;
                };
                let (u, v) = (a_far - corner, b_far - corner);
                let (la, lb) = (u.x.hypot(u.y), v.x.hypot(v.y));
                if la < 1e-6 || lb < 1e-6 || ((u.x * v.x + u.y * v.y) / (la * lb)).abs() > 1e-3 {
                    continue;
                }
                let layer = usize::from(a.layer == CopperLayer::Back);
                if in_pad(corner, layer) {
                    continue;
                }
                let width = a.width.min(b.width);
                let found = CHAMFERS.iter().copied().find_map(|c| {
                    if c >= la - 1e-6 || c >= lb - 1e-6 {
                        return None;
                    }
                    let p = Pt::new(corner.x + u.x / la * c, corner.y + u.y / la * c);
                    let q = Pt::new(corner.x + v.x / lb * c, corner.y + v.y / lb * c);
                    clear(p, q, width, layer).then_some((p, q))
                });
                let Some((p, q)) = found else {
                    return Err(Unsettled::Corner);
                };
                let net_name = routed.tracks[k].net.clone();
                let layer_of_track = routed.tracks[k].layer;
                for (index, to) in [(k, p), (m, q)] {
                    let t = &mut routed.tracks[index];
                    if same(t.from, corner) {
                        t.from = to;
                    } else {
                        t.to = to;
                    }
                }
                routed.tracks.push(Track {
                    net: net_name,
                    layer: layer_of_track,
                    from: p,
                    to: q,
                    width,
                });
                cut = true;
                break;
            }
            if !cut {
                k += 1;
            }
        }
        Ok(())
    }

    /// The pads of a net, each as the square a round pad of its size holds (where a corner is
    /// inside the pad's copper whatever its shape), with the layers it has copper on.
    fn own_pad_squares(&self, net: usize) -> Vec<(Rect, [bool; 2])> {
        self.fixed_by_net[net]
            .iter()
            .filter_map(|&k| {
                let o = &self.fixed.list[k];
                let Form::Rect(r) = o.form else { return None };
                let inset =
                    r.width().min(r.height()) / 2.0 * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
                let r = r.expanded(-inset);
                (r.width() > 0.0 && r.height() > 0.0).then_some((r, o.layers))
            })
            .collect()
    }

    /// Whether a via in the pad would keep the rules' space from all other copper, and its
    /// hole from other holes. The via is at a known place, so this is measured there and
    /// not on the grid.
    fn via_fits(
        &self,
        pad: &InPad,
        net: usize,
        obstacles: &[&Obstacle],
        keepouts: &[(Form, f64)],
    ) -> bool {
        let radius = self.board.rules.via_diameter / 2.0;
        obstacles
            .iter()
            .filter(|o| o.net != Some(net))
            .all(|o| o.form.distance(pad.at) >= o.clearance - MARGIN + radius + 1e-3)
            && keepouts.iter().all(|(form, reach)| match form {
                // The pad's own keep-out is what the via is allowed to stand in.
                Form::Rect(b) if *b == pad.bounds => true,
                _ => form.distance(pad.at) >= *reach,
            })
    }

    /// The tracks and vias that follow a path. A path to a plane ends in a via, which is the
    /// join to the plane.
    fn realize(
        &self,
        line: usize,
        path: &[Spot],
        in_from: Option<InPad>,
        in_to: Option<InPad>,
        masks: &Masks,
    ) -> Routed {
        let rules = &self.board.rules;
        // A track through a cell that is too tight for the full width is necked down: to the
        // width its current needs, or near its pads to the narrowest.
        let width_of = |neck: u8| match neck {
            0 => self.width_of(line),
            1 => self.neck_width(line),
            _ => rules.min_track_width,
        };
        let necked = |&(i, j, layer): &Spot| masks.neck(layer, i, j);
        let l = &self.lines[line];
        let mut tracks: Vec<Track> = Vec::new();
        let mut vias: Vec<Via> = Vec::new();
        let mut start = 0;
        for k in 1..=path.len() {
            if k < path.len() && path[k].2 == path[k - 1].2 {
                continue;
            }
            let layer = path[start].2;
            let cells: Vec<(usize, usize)> = path[start..k].iter().map(|s| (s.0, s.1)).collect();
            let neck: Vec<u8> = path[start..k].iter().map(necked).collect();
            for (a, b, narrow) in runs_with(&self.grid, &cells, &neck) {
                tracks.push(Track {
                    net: l.net.clone(),
                    layer: layer_of(layer),
                    from: a,
                    to: b,
                    width: width_of(narrow),
                });
            }
            if k < path.len() {
                vias.push(Via {
                    net: l.net.clone(),
                    at: self.grid.centre(path[k].0, path[k].1),
                    diameter: rules.via_diameter,
                    drill: rules.via_drill,
                });
                // The others of its cluster, where the masks left room (issue #431).
                for at in self
                    .cluster_cells(line, masks, path[k].0, path[k].1)
                    .unwrap_or_default()
                {
                    vias.push(Via {
                        net: l.net.clone(),
                        at,
                        diameter: rules.via_diameter,
                        drill: rules.via_drill,
                    });
                }
            }
            start = k;
        }
        let last = path[path.len() - 1];
        let mut pads = vec![(l.from_at, path[0])];
        if l.to_plane {
            vias.push(Via {
                net: l.net.clone(),
                at: self.grid.centre(last.0, last.1),
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            });
        } else {
            pads.push((l.to_at, last));
        }
        // A path that begins or ends on the far layer of a pad goes through a via in the pad.
        for pad in in_from.iter().chain(&in_to) {
            vias.push(Via {
                net: l.net.clone(),
                at: pad.at,
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            });
        }
        // A pad's centre is rarely on the grid, so join it to the first and last cell.
        for (pad_at, spot) in pads {
            let cell = self.grid.centre(spot.0, spot.1);
            if pad_at != cell {
                tracks.push(Track {
                    net: l.net.clone(),
                    layer: layer_of(spot.2),
                    from: pad_at,
                    to: cell,
                    width: width_of(necked(&spot)),
                });
            }
        }
        Routed { line, tracks, vias }
    }

    /// Route the lines in this order, then rip up for any that failed. `attempt` and
    /// `attempts` say which try of the board this is, for the progress messages. If the time
    /// runs out, the lines not yet tried count as failed.
    fn run(&self, order: &[usize], rip_up: bool, attempt: usize, attempts: usize) -> State {
        self.escaping.set(attempt > 1);
        self.classed.set(match self.class_use {
            LineClasses::Off => false,
            LineClasses::EveryTry => true,
            LineClasses::FromSecondTry => attempt > 1,
        });
        self.planned.set(
            self.plan.is_some()
                && match self.plan_use {
                    CoarsePlan::Off => false,
                    CoarsePlan::EveryTry => true,
                    CoarsePlan::FromSecondTry => attempt > 1,
                },
        );
        *self.failed_in.borrow_mut() = vec![Stage::FirstPass; self.lines.len()];
        if let Some(snapshots) = &self.snapshots {
            snapshots.borrow_mut().clear();
        }
        let mut state = self.run_stages(order, rip_up, attempt, attempts);
        if state.failed.is_empty() {
            // The pairs first: the pass off the pours leaves their nets alone, so it cannot undo
            // their coupling.
            self.side_by_side(&mut state);
            self.off_the_pours(&mut state, attempt, attempts);
        }
        self.snapshot(SnapshotStage::Final, &state);
        state
    }

    /// The track length of the lines of `net`, their vias, and the share of that length within
    /// [`PAIR_NEAR`] of a track of `partner` on the same layer.
    fn pair_measure(&self, state: &State, net: usize, partner: usize) -> (f64, usize, f64) {
        let theirs: Vec<&Track> = state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] == partner)
            .flat_map(|r| r.tracks.iter())
            .collect();
        let (mut length, mut near, mut vias) = (0.0, 0.0, 0);
        for r in state.routed.iter().filter(|r| self.nets[r.line] == net) {
            vias += r.vias.len();
            for t in &r.tracks {
                let l = t.from.distance(t.to);
                length += l;
                let n = (l / 0.05).ceil().max(1.0) as usize;
                for k in 0..n {
                    let f = (k as f64 + 0.5) / n as f64;
                    let p = Pt::new(
                        t.from.x + (t.to.x - t.from.x) * f,
                        t.from.y + (t.to.y - t.from.y) * f,
                    );
                    if theirs.iter().any(|o| {
                        o.layer == t.layer && point_to_line(p, o.from, o.to) <= PAIR_NEAR + 1e-9
                    }) {
                        near += l / n as f64;
                    }
                }
            }
        }
        (length, vias, if length > 0.0 { near / length } else { 0.0 })
    }

    /// Run each net of a diff pair beside its partner on a try that routed every line (issue
    /// #431, S9): the lines of one net are routed again against all the other copper, a step
    /// beside the partner's track costing [`FOLLOW`] of its length, and the new copper is kept
    /// only if the share that runs coupled grows and the net is no longer and has no more vias.
    /// A line is never left unrouted, and a board with no pair is unchanged.
    fn side_by_side(&self, state: &mut State) {
        let failed_in = self.failed_in.borrow().clone();
        self.pairing.set(true);
        self.fine.set(true);
        for n in 0..self.partners.len() {
            let Some(p) = self.partners[n] else { continue };
            if p < n {
                continue;
            }
            for (x, y) in [(p, n), (n, p)] {
                if self.out_of_time() {
                    break;
                }
                let (len0, vias0, c0) = self.pair_measure(state, x, y);
                if len0 == 0.0 || self.pair_measure(state, y, x).0 == 0.0 {
                    continue;
                }
                let mut trial = state.clone();
                let (gone, kept): (Vec<Routed>, Vec<Routed>) =
                    trial.routed.drain(..).partition(|r| self.nets[r.line] == x);
                trial.routed = kept;
                let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
                again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
                let mut ok = true;
                for l in again {
                    match self
                        .try_line(l, &trial, &[], ROOM_LOOSE, false)
                        .or_else(|| self.try_line(l, &trial, &[], ROOM_TIGHT, false))
                    {
                        Some((r, _)) => trial.routed.push(r),
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let (len1, vias1, c1) = self.pair_measure(&trial, x, y);
                if keeps_pair((len0, vias0, c0), (len1, vias1, c1)) {
                    *state = trial;
                }
            }
        }
        *self.failed_in.borrow_mut() = failed_in;
        self.fine.set(false);
        self.pairing.set(false);
    }

    /// What a step of a line of `net` costs beyond its length on each layer in
    /// [`Self::off_the_pours`]: [`POUR`] on an outer layer another net is poured on, where the
    /// other outer layer is free of other nets' pours (where both are poured, either way cuts
    /// one).
    fn pour_costs(&self, net: usize) -> [i64; 2] {
        let name = &self.board.nets[net].name;
        let poured = |layer: CopperLayer| {
            self.board
                .pours
                .iter()
                .any(|p| p.layer == layer && &p.net != name)
        };
        let (front, back) = (poured(CopperLayer::Front), poured(CopperLayer::Back));
        [
            if front && !back { POUR } else { 0 },
            if back && !front { POUR } else { 0 },
        ]
    }

    /// What a line's copper costs as the search counts it in [`Self::off_the_pours`]: its
    /// length, [`POUR`] more a step on another net's pour, and its vias at [`VIA`] and
    /// [`POUR_VIA`].
    fn pour_cost(&self, r: &Routed) -> i64 {
        let cost = self.pour_costs(self.nets[r.line]);
        let steps: f64 = r
            .tracks
            .iter()
            .map(|t| {
                let layer = usize::from(t.layer == CopperLayer::Back);
                t.from.distance(t.to) / PITCH * (STEP + cost[layer]) as f64
            })
            .sum();
        steps.round() as i64 + (VIA + POUR_VIA) * r.vias.len() as i64
    }

    /// The length of `r`'s track on another net's pour, in mm.
    fn on_pour(&self, r: &Routed) -> f64 {
        let cost = self.pour_costs(self.nets[r.line]);
        r.tracks
            .iter()
            .filter(|t| cost[usize::from(t.layer == CopperLayer::Back)] > 0)
            .map(|t| t.from.distance(t.to))
            .sum()
    }

    /// Take track off other nets' pours on a try that routed every line (issue #431, S7): each
    /// line with track on one, the most first, is searched for again against the copper of all
    /// the others with a step on the pour paying [`POUR`], at the usual room and failing that at
    /// the least, and its new copper is kept only where it costs less so counted
    /// ([`Self::pour_cost`]). A line is never left unrouted, and a board with no such pour is
    /// unchanged. The nets of a diff pair ([`diff_pair_partners`]) are left as they are: moving
    /// one alone pulls it off its partner.
    fn off_the_pours(&self, state: &mut State, attempt: usize, attempts: usize) {
        if (0..self.board.nets.len()).all(|n| self.pour_costs(n) == [0, 0]) {
            return;
        }
        let stage = self.stage.get();
        let failed_in = self.failed_in.borrow().clone();
        self.pouring.set(true);
        self.fine.set(true);
        for _ in 0..POUR_SWEEPS {
            let mut todo: Vec<(usize, f64)> = state
                .routed
                .iter()
                .filter(|r| self.partners[self.nets[r.line]].is_none())
                // On a try with classes, a decoupling or clock line keeps the copper its class
                // gave it, as a pair's does (issue #416).
                .filter(|r| !(self.classed.get() && self.class[r.line].protected()))
                .map(|r| (r.line, self.on_pour(r)))
                .filter(|&(_, mm)| mm > 0.0)
                .collect();
            todo.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let total = todo.len();
            for (k, (line, _)) in todo.into_iter().enumerate() {
                if self.out_of_time() {
                    break;
                }
                self.report(false, || {
                    format!(
                        "routing, try {attempt} of {attempts}: off the pours, {k} of {total} lines"
                    )
                });
                let Some(at) = state.routed.iter().position(|r| r.line == line) else {
                    continue;
                };
                let old = state.routed.remove(at);
                let kept = match self
                    .try_line(line, state, &[], ROOM_LOOSE, false)
                    .or_else(|| self.try_line(line, state, &[], ROOM_TIGHT, false))
                {
                    Some((new, _)) if self.pour_cost(&new) < self.pour_cost(&old) => new,
                    _ => old,
                };
                state.routed.insert(at, kept);
            }
        }
        self.fine.set(false);
        self.pouring.set(false);
        *self.failed_in.borrow_mut() = failed_in;
        self.stage.set(stage);
    }

    /// The stages of [`Self::run`].
    fn run_stages(&self, order: &[usize], rip_up: bool, attempt: usize, attempts: usize) -> State {
        let mut state = State::default();
        self.stage.set(Stage::FirstPass);
        // The first pass, which gives most lines the copper they keep, searches in eight
        // directions; the repairs after it search in four, with every right angle cut, which is
        // a fifth of the work (issue #415).
        self.fine.set(true);
        for (k, &line) in order.iter().enumerate() {
            if self.out_of_time() {
                state.failed.extend_from_slice(&order[k..]);
                let mut failed_in = self.failed_in.borrow_mut();
                for &line in &order[k..] {
                    failed_in[line] = Stage::OutOfTime;
                }
                return state;
            }
            self.report(false, || {
                format!(
                    "routing, try {attempt} of {attempts}: {k} of {} connections done, {} not routed",
                    order.len(),
                    state.failed.len()
                )
            });
            match self.try_line(line, &state, &[], ROOM_LOOSE, false) {
                Some((routed, _)) => state.routed.push(routed),
                None => state.failed.push(line),
            }
        }
        self.fine.set(false);
        self.snapshot(SnapshotStage::FirstPass, &state);
        if !rip_up {
            return state;
        }
        // First with the room the masks usually keep; what is still left is tried with the
        // least room that is safe, which opens the gaps between fine-pitch pads.
        for room in [ROOM_LOOSE, ROOM_TIGHT] {
            if room == ROOM_TIGHT {
                self.stage.set(Stage::LessRoom);
                for line in state.failed.clone() {
                    if self.out_of_time() {
                        return state;
                    }
                    self.report(false, || {
                        format!(
                            "routing, try {attempt} of {attempts}: {} again with less room, {} not routed",
                            self.lines[line],
                            state.failed.len()
                        )
                    });
                    if let Some((routed, _)) = self.try_line(line, &state, &[], room, false) {
                        state.routed.push(routed);
                        state.failed.retain(|&f| f != line);
                    }
                }
            }
            self.stage.set(Stage::RipUp);
            for pass in 1..=RIP_UP_PASSES {
                let before = state.failed.len();
                for line in state.failed.clone() {
                    if self.out_of_time() {
                        return state;
                    }
                    self.report(false, || {
                        format!(
                            "routing, try {attempt} of {attempts}: ripping up for {} (pass {pass} of {RIP_UP_PASSES}), {} not routed",
                            self.lines[line], state.failed.len()
                        )
                    });
                    self.rip_up_for(line, &mut state, room);
                }
                if state.failed.is_empty() || state.failed.len() == before {
                    break;
                }
            }
            if state.failed.is_empty() {
                break;
            }
        }
        self.snapshot(SnapshotStage::RipUp, &state);
        if self.negotiate && !state.failed.is_empty() {
            self.stage.set(Stage::Negotiation);
            self.negotiate(&mut state, ROOM_TIGHT, attempt, attempts);
            self.snapshot(SnapshotStage::Negotiation, &state);
            if !state.failed.is_empty() {
                self.stage.set(Stage::Chain);
                self.finish(&mut state, ROOM_TIGHT, attempt, attempts);
            }
        }
        state
    }

    /// Keep what a stage of this try routed, if snapshots are asked for.
    fn snapshot(&self, after: SnapshotStage, state: &State) {
        let Some(snapshots) = &self.snapshots else {
            return;
        };
        let mut routed: Vec<&Routed> = state.routed.iter().collect();
        routed.sort_by_key(|r| r.line);
        let mut failed = state.failed.clone();
        failed.sort_unstable();
        snapshots.borrow_mut().push(Snapshot {
            after,
            tracks: routed
                .iter()
                .flat_map(|r| r.tracks.iter().cloned())
                .collect(),
            vias: routed.iter().flat_map(|r| r.vias.iter().cloned()).collect(),
            left: failed.iter().map(|&l| self.lines[l].clone()).collect(),
        });
    }

    /// Count a soft path in the crowding map: once for each map cell it passes through.
    fn count_crowding(&self, path: &[Spot]) {
        if !self.counting.get() {
            return;
        }
        let mut crowding = self.crowding.borrow_mut();
        let mut cells: Vec<usize> = path
            .iter()
            .map(|&(i, j, _)| crowding.index_of(i, j))
            .collect();
        cells.sort_unstable();
        cells.dedup();
        for c in cells {
            crowding.counts[c] = crowding.counts[c].saturating_add(1);
        }
    }

    /// Why each line left in `state` is left: the stage it last failed in (`failed_in`, of the
    /// try that gave the state), and the nets whose copper stands in its way, from one soft
    /// search on the state at the least room. After the time limit that search gives up at once
    /// and names no nets.
    fn why(&self, state: &State, failed_in: &[Stage]) -> Vec<LineReport> {
        self.counting.set(false);
        let mut failed = state.failed.clone();
        failed.sort_unstable();
        failed.dedup();
        failed
            .into_iter()
            .map(|line| LineReport {
                line: self.lines[line].clone(),
                stage: failed_in.get(line).copied().unwrap_or(Stage::FirstPass),
                blockers: self
                    .blockers(line, state, ROOM_TIGHT)
                    .into_iter()
                    .map(|n| self.board.nets[n].name.clone())
                    .collect(),
            })
            .collect()
    }

    /// Where a line would go if the copper of other nets could be crossed, at a price per
    /// cell ([`SOFT`]), and the nets whose copper that path comes near: the nets least in the
    /// line's way, not all that lie between its ends. `None` if even that has no path.
    fn contested(&self, line: usize, state: &State, room: f64) -> Option<(Vec<Spot>, Vec<usize>)> {
        let net = self.nets[line];
        // A protected line's copper is no way through, and its net is not ripped up for this
        // line ([`Class::protected`]).
        let protected = self.protected(line, state);
        let (_, path) = self.try_line_locked(line, state, &[], room, true, &protected)?;
        self.count_crowding(&path);
        let rules = &self.board.rules;
        let reach = rules.clearance + MARGIN + self.width_of(line) + PITCH;
        let mut found = Vec::new();
        let protected_nets: Vec<usize> = protected.iter().map(|&l| self.nets[l]).collect();
        for r in state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] != net && !protected_nets.contains(&self.nets[r.line]))
        {
            let forms = r
                .tracks
                .iter()
                .map(|t| Form::Segment(t.from, t.to, t.width / 2.0))
                .chain(r.vias.iter().map(|v| Form::Circle(v.at, v.diameter / 2.0)));
            let near = forms.into_iter().any(|f| {
                path.iter()
                    .any(|&(i, j, _)| f.distance(self.grid.centre(i, j)) < reach)
            });
            let n = self.nets[r.line];
            if near && !found.contains(&n) {
                found.push(n);
            }
        }
        found.sort_unstable();
        Some((path, found))
    }

    /// [`Self::contested`] by line: where the line would go if the copper of other nets could
    /// be crossed, except the copper of the lines in `locked`, and for each other line how far
    /// its copper is from being crossed by that path, in mm (negative when the path crosses
    /// it, which is when the masks would keep the path from the cell), nearest first.
    fn contested_lines(
        &self,
        line: usize,
        state: &State,
        room: f64,
        locked: &[usize],
    ) -> Option<Vec<(usize, f64)>> {
        let net = self.nets[line];
        let mut locked = locked.to_vec();
        locked.extend(self.protected(line, state));
        let locked = &locked[..];
        let (_, path) = self.try_line_locked(line, state, &[], room, true, locked)?;
        self.count_crowding(&path);
        let rules = &self.board.rules;
        // The cells the masks keep a track, or a via, from, for copper of this width.
        let track_reach = rules.clearance + MARGIN + self.width_of(line) / 2.0 + room;
        let via_reach = rules.clearance + MARGIN + rules.via_diameter / 2.0 + room;
        // Where the path puts a via: the cell where it changes layer.
        let via_at: Vec<(usize, usize)> = path
            .windows(2)
            .filter(|w| w[0].2 != w[1].2)
            .map(|w| (w[0].0, w[0].1))
            .collect();
        let mut found: Vec<(usize, f64)> = Vec::new();
        for r in state
            .routed
            .iter()
            .filter(|r| self.nets[r.line] != net && !locked.contains(&r.line))
        {
            let tracks = r.tracks.iter().map(|t| {
                (
                    Form::Segment(t.from, t.to, t.width / 2.0),
                    [t.layer == CopperLayer::Front, t.layer == CopperLayer::Back],
                )
            });
            let vias = r
                .vias
                .iter()
                .map(|v| (Form::Circle(v.at, v.diameter / 2.0), [true, true]));
            let mut least = f64::MAX;
            for (f, layers) in tracks.chain(vias) {
                for &(i, j, layer) in &path {
                    if layers[layer] {
                        least = least.min(f.distance(self.grid.centre(i, j)) - track_reach);
                    }
                }
                for &(i, j) in &via_at {
                    least = least.min(f.distance(self.grid.centre(i, j)) - via_reach);
                }
            }
            if least < CHAIN_NEAR.last().copied().unwrap_or(0.0) {
                found.push((r.line, least));
            }
        }
        found.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        Some(found)
    }

    /// The last resort for a line that is still left: route it, ripping up the single lines
    /// (not whole nets) in its way, and route those again, each ripping up the lines in its
    /// way in turn, down to `CHAIN_DEPTH` levels, never touching a line that is routed for
    /// the one above. The whole attempt costs at most `CHAIN_SEARCHES` searches and changes
    /// `state` only if every line ends up routed. Nothing is random: the same board gives
    /// the same copper.
    fn rip_up_chain(
        &self,
        line: usize,
        state: &mut State,
        room: f64,
        depth: usize,
        locked: &[usize],
        budget: &mut usize,
    ) -> bool {
        if *budget < 2 || self.out_of_time() {
            return false;
        }
        *budget -= 1;
        if let Some((routed, _)) = self.try_line(line, state, &[], room, false) {
            state.routed.push(routed);
            return true;
        }
        if depth >= CHAIN_DEPTH {
            return false;
        }
        *budget -= 1;
        let Some(blockers) = self.contested_lines(line, state, room, locked) else {
            return false;
        };
        // First the lines the path crosses, then those it comes near, as one set each, and
        // then each line it crosses alone.
        let mut options: Vec<Vec<usize>> = Vec::new();
        for &near in &CHAIN_NEAR {
            let mut set: Vec<usize> = blockers
                .iter()
                .filter(|b| b.1 < near)
                .map(|b| b.0)
                .collect();
            set.sort_unstable();
            if !set.is_empty() && !options.contains(&set) {
                options.push(set);
            }
        }
        let crossed: Vec<usize> = blockers.iter().filter(|b| b.1 < 0.0).map(|b| b.0).collect();
        if crossed.len() > 1 {
            options.extend(crossed.iter().map(|&b| vec![b]));
        }
        for ripped in options.into_iter().filter(|o| !o.is_empty()) {
            if *budget == 0 || self.out_of_time() {
                return false;
            }
            let mut trial = state.clone();
            let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
                .routed
                .drain(..)
                .partition(|r| ripped.contains(&r.line));
            trial.routed = kept;
            let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
            again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
            *budget -= 1;
            let Some((first, _)) = self.try_line(line, &trial, &[], room, false) else {
                continue;
            };
            trial.routed.push(first);
            let mut lock = locked.to_vec();
            lock.push(line);
            let mut ok = true;
            for l in again {
                if self.rip_up_chain(l, &mut trial, room, depth + 1, &lock, budget) {
                    lock.push(l);
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                *state = trial;
                return true;
            }
        }
        false
    }

    /// [`Self::rip_up_chain`] for every line that is left, as far as the searches go.
    fn finish(&self, state: &mut State, room: f64, attempt: usize, attempts: usize) {
        let mut total = CHAIN_TOTAL;
        for line in state.failed.clone() {
            if self.out_of_time() || total < 2 {
                return;
            }
            self.report(false, || {
                format!(
                    "routing, try {attempt} of {attempts}: moving lines for {}, {} not routed",
                    self.lines[line],
                    state.failed.len()
                )
            });
            let mut trial = state.clone();
            let mut budget = CHAIN_SEARCHES.min(total);
            let start = budget;
            let done = self.rip_up_chain(line, &mut trial, room, 0, &[], &mut budget);
            total -= start - budget;
            if done {
                trial.failed.retain(|&f| f != line);
                *state = trial;
            }
        }
    }

    /// The nets whose copper stands in the way of a line.
    fn blockers(&self, line: usize, state: &State, room: f64) -> Vec<usize> {
        self.contested(line, state, room)
            .map_or_else(Vec::new, |(_, nets)| nets)
    }

    /// Copy what the negotiation has learnt about the cells of the window into the masks.
    fn add_history(&self, masks: &mut Masks) {
        let history = self.history.borrow();
        if history[FRONT].is_empty() {
            return;
        }
        let window = masks.window;
        for layer in [FRONT, BACK] {
            let mut cells = vec![0u8; window.cells()];
            for j in window.j0..window.j0 + window.ny {
                let row = j * self.grid.nx;
                for i in window.i0..window.i0 + window.nx {
                    cells[window.index(i, j)] = history[layer][row + i];
                }
            }
            masks.history[layer] = cells;
        }
    }

    /// Make the cells of a path dearer to every line that comes after.
    fn learn(&self, path: &[Spot]) {
        let mut history = self.history.borrow_mut();
        for &(i, j, layer) in path {
            let h = &mut history[layer][j * self.grid.nx + i];
            *h = (*h + 1).min(HISTORY_MOST);
        }
    }

    /// Lines the router could not place are negotiated for: each is routed where it would go
    /// if no other net were in the way, and the nets it then comes near are ripped up. The
    /// cells they fought over are made dearer for all, so the lines ripped up find another
    /// way round, and the ones that find none are the next to be negotiated for. This goes
    /// on for a few rounds, and the best state seen is kept.
    fn negotiate(&self, state: &mut State, room: f64, attempt: usize, attempts: usize) {
        let cells = self.grid.nx * self.grid.ny;
        *self.history.borrow_mut() = [vec![0; cells], vec![0; cells]];
        let mut best = state.clone();
        for round in 1..=NEGOTIATION_ROUNDS {
            if state.failed.is_empty() || self.out_of_time() {
                break;
            }
            for line in state.failed.clone() {
                if self.out_of_time() {
                    break;
                }
                self.report(false, || {
                    format!(
                        "routing, try {attempt} of {attempts}: negotiating for {} (round {round} of {NEGOTIATION_ROUNDS}), {} not routed",
                        self.lines[line],
                        state.failed.len()
                    )
                });
                let Some((path, blockers)) = self.contested(line, state, room) else {
                    continue;
                };
                self.learn(&path);
                let (gone, kept): (Vec<Routed>, Vec<Routed>) = state
                    .routed
                    .drain(..)
                    .partition(|r| blockers.contains(&self.nets[r.line]));
                state.routed = kept;
                let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
                again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
                // The line first, and then what was ripped up for it.
                match self.try_line(line, state, &[], room, false) {
                    Some((routed, _)) => {
                        state.routed.push(routed);
                        state.failed.retain(|&f| f != line);
                    }
                    None => {
                        // Nothing came of it: put back what was taken up.
                        state.routed.extend(gone);
                        continue;
                    }
                }
                for l in again {
                    match self.try_line(l, state, &[], room, false) {
                        Some((routed, _)) => state.routed.push(routed),
                        None => state.failed.push(l),
                    }
                }
            }
            if state.failed.len() < best.failed.len() {
                best = state.clone();
            }
        }
        *state = best;
        *self.history.borrow_mut() = [Vec::new(), Vec::new()];
    }

    /// Try to get a failed line routed by ripping up the nets in its way, routing it, and
    /// routing what was ripped up again. If that does not work out, nothing changes.
    fn rip_up_for(&self, line: usize, state: &mut State, room: f64) {
        let blockers = self.blockers(line, state, room);
        let mut attempts: Vec<Vec<usize>> = vec![blockers.clone()];
        if blockers.len() > 1 {
            attempts.extend(blockers.iter().map(|&n| vec![n]));
        }
        for ripped in attempts.into_iter().filter(|a| !a.is_empty()) {
            if self.out_of_time() {
                return;
            }
            let mut trial = state.clone();
            let (gone, kept): (Vec<Routed>, Vec<Routed>) = trial
                .routed
                .drain(..)
                .partition(|r| ripped.contains(&self.nets[r.line]));
            trial.routed = kept;
            let mut again: Vec<usize> = gone.iter().map(|r| r.line).collect();
            again.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));

            let Some((first, _)) = self.try_line(line, &trial, &[], room, false) else {
                continue;
            };
            trial.routed.push(first);
            let mut ok = true;
            for l in again {
                match self.try_line(l, &trial, &[], room, false) {
                    Some((r, _)) => trial.routed.push(r),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                trial.failed.retain(|&f| f != line);
                *state = trial;
                return;
            }
        }
    }

    /// The coarse plan of the board ([`crate::plan`]), with the lines in `order`: the tiles'
    /// capacities from what the fixed copper leaves open to the narrowest line's track at the
    /// room the first pass keeps, and each line's corridor.
    fn make_plan(&self, order: &[usize]) -> Planned {
        let tile_cells = crate::plan::TILE_CELLS;
        let rules = &self.board.rules;
        let width = (0..self.lines.len())
            .filter(|&l| !self.lines[l].to_plane)
            .map(|l| self.width_of(l))
            .fold(f64::INFINITY, f64::min);
        let width = if width.is_finite() {
            width
        } else {
            rules.min_track_width
        };
        let reach = Reach {
            room: ROOM_LOOSE,
            width,
        };
        let key = (reach.room.to_bits(), reach.width.to_bits());
        let base = self
            .bases
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                std::rc::Rc::new(Base::new(self.board, &self.grid, &self.fixed, reach))
            })
            .clone();
        let counts = &base.counts;
        let open = |c: &Vec<u16>| c.iter().map(|&n| n == 0).collect::<Vec<bool>>();
        let (mut front, mut back, via) = (
            open(&counts.track[FRONT]),
            open(&counts.track[BACK]),
            open(&counts.via),
        );
        // A pad is a way across a tile's edge for the line of its net that ends in it: the
        // cells of a pad's copper count as open for tracks.
        for o in self.fixed.list.iter().filter(|o| o.net.is_some()) {
            let whole = Window::full(&self.grid);
            for (i, j) in self.grid.cells_near(&whole, &o.form.bounds(), 0.0) {
                if o.form.distance(self.grid.centre(i, j)) <= 1e-9 {
                    let at = j * self.grid.nx + i;
                    if o.layers[FRONT] {
                        front[at] = true;
                    }
                    if o.layers[BACK] {
                        back[at] = true;
                    }
                }
            }
        }
        let via_pitch = (rules.via_diameter + rules.clearance)
            .max(rules.via_drill + rules.via_hole_to_hole)
            / PITCH;
        let tiles = crate::plan::Tiles::measure(
            tile_cells,
            (self.grid.nx, self.grid.ny),
            [&front, &back],
            &via,
            (width + rules.clearance) / PITCH,
            via_pitch,
        );
        let at = |spots: &[Spot]| {
            let mut nodes: Vec<crate::plan::Node> = spots
                .iter()
                .map(|&(i, j, layer)| {
                    let (a, b) = tiles.of(i, j);
                    (a, b, layer)
                })
                .collect();
            nodes.sort_unstable();
            nodes.dedup();
            nodes
        };
        let ends: Vec<Option<crate::plan::Ends>> = (0..self.lines.len())
            .map(|l| {
                let (from, to) = &self.ends[l];
                (!self.lines[l].to_plane && !from.is_empty() && !to.is_empty()).then(|| {
                    crate::plan::Ends {
                        from: at(from),
                        to: at(to),
                    }
                })
            })
            .collect();
        let plan = crate::plan::negotiate(&tiles, &ends, order);
        self.report(true, || {
            format!(
                "coarse plan: {} by {} tiles of {} mm, {} rounds, {} tracks over capacity",
                tiles.nx,
                tiles.ny,
                tile_cells as f64 * PITCH,
                plan.rounds,
                plan.overuse
            )
        });
        // The plan steers a line of the narrowest track only: the capacities are counted in it,
        // so a net its current widens has no corridor. Nor has a net of a pour, whose copper
        // the pour joins anyway, or a net of a diff pair, which runs beside its partner (S9)
        // and which any cost moves off it.
        let poured = |l: usize| self.board.pours.iter().any(|p| p.net == self.lines[l].net);
        let corridors = plan
            .paths
            .iter()
            .enumerate()
            .map(|(l, p)| {
                p.as_ref()
                    .filter(|_| self.width_of(l) <= width + 1e-9)
                    .filter(|_| !poured(l) && self.partners[self.nets[l]].is_none())
                    .map(|p| crate::plan::corridor(&tiles, p, crate::plan::CORRIDOR_GROW))
            })
            .collect();
        let fill = plan.fill(&tiles);
        Planned {
            tiles,
            corridors,
            fill,
        }
    }

    /// The orders to try the lines in.
    fn orders(&self, retry: bool) -> Vec<Vec<usize>> {
        let mut shortest: Vec<usize> = (0..self.lines.len()).collect();
        shortest.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
        // Lines that end deep in a fine-pitch grid of pads first, the deepest first, and the
        // others shortest first. It differs from `shortest` only on a board that has such a
        // grid.
        let mut constrained = shortest.clone();
        constrained.sort_by(|&a, &b| self.depth[b].total_cmp(&self.depth[a]));
        let mut orders = vec![constrained];
        if !orders.contains(&shortest) {
            orders.push(shortest.clone());
        }
        if retry {
            let mut longest = shortest.clone();
            longest.reverse();
            let mut by_net = shortest.clone();
            by_net.sort_by_key(|&l| self.nets[l]);
            let mut reversed_nets = by_net.clone();
            reversed_nets.reverse();
            for o in [longest, by_net, reversed_nets] {
                if !orders.contains(&o) {
                    orders.push(o);
                }
            }
        }
        orders.truncate(MOST_TRIES);
        orders
    }

    /// The order of each try: [`Self::orders`], with the first order again second on a board
    /// with escape lanes or a later-try plan or classes, and the classes in turn from the try
    /// [`RouteOptions::classes`] says ([`Self::classed_order`]); the first try is never changed
    /// by the classes when they come later.
    fn tries(&self, options: &RouteOptions) -> Vec<Vec<usize>> {
        let mut orders = self.orders(options.retry_orders);
        // On a board with escape lanes, the second try is the first order again with the lanes
        // (see the module's header), and every try after it has them too. A plan used from the
        // second try on is used there too.
        let second_again = self.lanes.is_some()
            || options.plan == CoarsePlan::FromSecondTry
            || options.classes == LineClasses::FromSecondTry;
        if second_again && options.retry_orders {
            orders.insert(1, orders[0].clone());
        }
        // The tries with classes route them in turn (issue #416), each in its own order.
        let first_classed = match options.classes {
            LineClasses::Off => orders.len(),
            LineClasses::EveryTry => 0,
            LineClasses::FromSecondTry => 1,
        };
        for order in orders.iter_mut().skip(first_classed) {
            *order = self.classed_order(order);
        }
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let mut k = 0;
        orders.retain(|o| {
            k += 1;
            // The first try, and the first order again on the second, are kept as they are.
            let keep = k == 1 || (k == 2 && second_again) || !seen.contains(o);
            seen.push(o.clone());
            keep
        });
        orders
    }

    /// `order` with the classes in turn, the most important first, and its own order inside
    /// each ([`Class`], issue #416).
    fn classed_order(&self, order: &[usize]) -> Vec<usize> {
        let mut order = order.to_vec();
        order.sort_by_key(|&l| self.class[l]);
        order
    }

    /// The routed lines whose copper is not ripped up for `line` on this try: those of a
    /// protected class before `line`'s ([`Class::protected`]). None on a try without classes.
    fn protected(&self, line: usize, state: &State) -> Vec<usize> {
        if !self.classed.get() {
            return Vec::new();
        }
        state
            .routed
            .iter()
            .map(|r| r.line)
            .filter(|&l| self.class[l] < self.class[line] && self.class[l].protected())
            .collect()
    }
}

/// How hard to try.
#[derive(Debug, Clone, Copy)]
pub struct RouteOptions {
    /// Route the board again with the lines in other orders if some are left.
    pub retry_orders: bool,
    /// Rip up the nets in the way of a line that failed, and route it first.
    pub rip_up: bool,
    /// Negotiate for lines that are still left after that: route each where it would go
    /// if nothing stood in its way, rip up what it comes near, make the cells they fought
    /// over dearer, and route what was ripped up again, for a few rounds; and then give each
    /// line still left a bounded chain of rip-ups of single lines ([`Router::rip_up_chain`]).
    /// Only used with `rip_up`.
    pub negotiate: bool,
    /// Give up after this long, leaving the lines not yet routed in the ratsnest. `None` is
    /// no limit.
    pub time_limit: Option<Duration>,
    /// Told what the router is doing, now and then, so that a long run is not silent.
    pub progress: Option<fn(&str)>,
    /// Stop as at the time limit when this is set, from another thread.
    pub cancel: Option<&'static AtomicBool>,
    /// What the search's moves cost. The default is the router's own; another is for measuring
    /// how much a result depends on them.
    pub costs: Costs,
    /// Keep the copper and the lines left after each stage of the try that gives the result
    /// ([`Routing::stages`]).
    pub snapshots: bool,
    /// Try at most this many orders of the lines (at least one); `None` is the router's own
    /// number. For measuring how a result depends on one try.
    pub most_tries: Option<usize>,
    /// Which tries follow a coarse plan of the board ([`CoarsePlan`]).
    pub plan: CoarsePlan,
    /// What a step outside a line's corridor costs, beyond the step, on a tile the plan fills,
    /// in tenths of a millimetre of track; `None` is the router's own.
    pub plan_cost: Option<i64>,
    /// Which tries route the lines' classes in turn, with their protection ([`LineClasses`]).
    pub classes: LineClasses,
}

/// Which tries of the router route the lines by class, the most important first, and keep the
/// copper of a decoupling, pair or clock line from being ripped up for a line of a later class
/// (issue #416, the module's header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClasses {
    /// No try: the orders and the rip-ups know no classes.
    Off,
    /// Every try.
    EveryTry,
    /// From the second try on, which routes the first order again by class: a board that
    /// routes on its first try is unchanged.
    FromSecondTry,
}

/// Which tries of the router follow the coarse plan (the header of `plan.rs`): the detailed
/// search pays for each step outside a line's corridor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoarsePlan {
    /// No plan is made.
    Off,
    /// Every try.
    EveryTry,
    /// From the second try on, which routes the first order again: a board that routes on its
    /// first try is unchanged.
    FromSecondTry,
}

impl Default for RouteOptions {
    fn default() -> Self {
        Self {
            retry_orders: true,
            rip_up: true,
            negotiate: true,
            time_limit: None,
            progress: None,
            cancel: None,
            costs: Costs::DEFAULT,
            snapshots: false,
            most_tries: None,
            plan: CoarsePlan::Off,
            plan_cost: None,
            classes: LineClasses::FromSecondTry,
        }
    }
}

/// What the router's search pays for a move, beyond its length, in tenths of a millimetre of
/// track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Costs {
    /// A bend of 45 degrees.
    pub turn: i64,
}

impl Costs {
    /// The costs the router routes with.
    pub const DEFAULT: Costs = Costs { turn: TURN };
}

impl Default for Costs {
    fn default() -> Self {
        Costs::DEFAULT
    }
}

/// A stage of a try of the board, the one a line last failed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// The first pass, in eight directions with the room the masks usually keep.
    FirstPass,
    /// The line alone again with the least room that is safe ([`ROOM_TIGHT`]).
    LessRoom,
    /// Ripping up the nets in its way ([`Router::rip_up_for`]).
    RipUp,
    /// The negotiation ([`Router::negotiate`]).
    Negotiation,
    /// The chain of rip-ups of single lines ([`Router::rip_up_chain`]).
    Chain,
    /// The time limit passed, or the router was told to stop, before or while it was tried.
    OutOfTime,
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Stage::FirstPass => "failed in the first pass",
            Stage::LessRoom => "failed in the try with less room",
            Stage::RipUp => "failed while ripping up the nets in its way",
            Stage::Negotiation => "failed in the negotiation",
            Stage::Chain => "failed in the chain of rip-ups",
            Stage::OutOfTime => "not finished, the time ran out",
        })
    }
}

/// Why a line is left: the ratsnest line as the router had it, the stage it last failed in,
/// and the nets whose copper stands in its way (those the cheapest path that may cross other
/// nets' copper comes near, [`Router::contested`]), by name. No nets when even that path does
/// not exist (the pads and the edge alone wall it in) or the time was up.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LineReport {
    pub line: Ratline,
    pub stage: Stage,
    pub blockers: Vec<String>,
}

/// The crowding map: how often, over a whole run of the router, the paths that may cross other
/// nets' copper (each search for the nets in a line's way, in rip-up, negotiation and the chain)
/// passed through each square of the board, a path counted once in a square.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Crowding {
    /// The board corner of the first square, in mm (the outline's least x and y).
    pub origin: Pt,
    /// The side of a square, in mm.
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    /// Row by row, x fastest: square (a, b) is `counts[b * nx + a]`.
    pub counts: Vec<u32>,
}

impl Crowding {
    /// The side of a square, in grid cells.
    const CELLS: usize = 10;

    fn over(grid: &Grid) -> Crowding {
        let (nx, ny) = (grid.nx.div_ceil(Self::CELLS), grid.ny.div_ceil(Self::CELLS));
        Crowding {
            origin: grid.origin,
            cell: PITCH * Self::CELLS as f64,
            nx,
            ny,
            counts: vec![0; nx * ny],
        }
    }

    fn index_of(&self, i: usize, j: usize) -> usize {
        let (a, b) = (
            (i / Self::CELLS).min(self.nx - 1),
            (j / Self::CELLS).min(self.ny - 1),
        );
        b * self.nx + a
    }

    /// How often paths passed through the square at a board point.
    pub fn at(&self, p: Pt) -> u32 {
        let a = ((p.x - self.origin.x) / self.cell).floor();
        let b = ((p.y - self.origin.y) / self.cell).floor();
        if a < 0.0 || b < 0.0 || a as usize >= self.nx || b as usize >= self.ny {
            return 0;
        }
        self.counts[b as usize * self.nx + a as usize]
    }
}

/// The stage a [`Snapshot`] is taken after.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotStage {
    FirstPass,
    /// The tries with less room and the rip-ups.
    RipUp,
    Negotiation,
    /// The end of the try, after the chain of rip-ups.
    Final,
}

/// What a try of the board had routed after a stage: its copper (before the vias beside vias
/// and the stitching of pours) and the lines left.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Snapshot {
    pub after: SnapshotStage,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    pub left: Vec<Ratline>,
}

/// What the router did.
#[derive(Debug, Clone, PartialEq)]
pub struct Routing {
    /// What is still unrouted, which is empty when the board is done.
    pub left: Ratsnest,
    /// The time limit passed before the router had tried everything it would have.
    pub stopped: bool,
    /// How many times the whole board was routed, each with the lines in another order.
    pub tries: usize,
    /// Why each line of the result the router left is left, in the router's order of lines.
    /// These are the router's own lines; `left` is the ratsnest of the finished board, which
    /// can join the same pads by other pairs.
    pub why: Vec<LineReport>,
    /// Where the board was fought over.
    pub crowding: Crowding,
    /// With [`RouteOptions::snapshots`], what the try that gave the result had routed after
    /// each stage it ran, in order, the last [`SnapshotStage::Final`]; empty otherwise.
    pub stages: Vec<Snapshot>,
}

/// Route as much of the ratsnest as the router can, adding tracks and vias to the board.
/// Returns what is still unrouted, which is empty when the board is done.
pub fn route(board: &mut Board) -> Ratsnest {
    route_with(board, RouteOptions::default()).left
}

/// [`route`], with a choice of how hard to try.
pub fn route_with(board: &mut Board, options: RouteOptions) -> Routing {
    let (best, stopped, tries, why, crowding, stages) = {
        let mut router = Router::new(board, &options);
        let mut orders = router.tries(&options);
        if options.plan != CoarsePlan::Off {
            router.plan = Some(router.make_plan(&orders[0]));
        }
        if let Some(most) = options.most_tries {
            orders.truncate(most.max(1));
        }
        router.report(true, || {
            format!(
                "routing {} connections on a {} by {} mm board",
                router.lines.len(),
                board.outline.width(),
                board.outline.height()
            )
        });
        // The best state, with the stage each line last failed in on its try and its snapshots.
        let mut best: Option<(State, Vec<Stage>, Vec<Snapshot>)> = None;
        let mut tries = 0;
        for order in &orders {
            tries += 1;
            let state = router.run(order, options.rip_up, tries, orders.len());
            let done = state.failed.is_empty();
            if best
                .as_ref()
                .is_none_or(|b| state.failed.len() < b.0.failed.len())
            {
                let failed_in = router.failed_in.take();
                let snapshots = router
                    .snapshots
                    .as_ref()
                    .map(|s| s.take())
                    .unwrap_or_default();
                best = Some((state, failed_in, snapshots));
            }
            if done || router.out_of_time() {
                break;
            }
        }
        let stopped = router.out_of_time();
        let (best, failed_in, stages) = best.unwrap_or_default();
        let why = router.why(&best, &failed_in);
        let crowding = router.crowding.into_inner();
        (best, stopped, tries, why, crowding, stages)
    };
    let mut routed = best.routed;
    routed.sort_by_key(|r| r.line);
    for r in routed {
        board.tracks.extend(r.tracks);
        board.vias.extend(r.vias);
    }
    // A via that does not carry its net's current gets others beside it (issue #431).
    crate::parallel::parallel_vias(board);
    // Pours are stitched to the other layers of their nets in what the routing left free.
    crate::stitch::stitch(board);
    Routing {
        left: Ratsnest::of(board),
        stopped,
        tries,
        why,
        crowding,
        stages,
    }
}

#[cfg(test)]
mod tests {
    use crate::testlib::{fixtures, four_layer_design};

    use super::*;

    /// A grid of 3 mm by 1 mm with nothing in the way.
    fn open_grid() -> (Grid, Masks) {
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
        let masks = Masks::open(Window::full(&grid));
        (grid, masks)
    }

    /// A search that is never stopped.
    fn find(masks: &Masks, from: &[Spot], goal: Goal) -> Option<Vec<Spot>> {
        search(masks, from, goal, Extras::default(), &|| false).ok()
    }

    /// A search for the nets in a line's way, as the router makes one: in four directions.
    fn find_soft(masks: &Masks, from: &[Spot], goal: Goal) -> Option<Vec<Spot>> {
        let extras = Extras {
            coarse: true,
            ..Extras::default()
        };
        search(masks, from, goal, extras, &|| false).ok()
    }

    #[test]
    fn a_straight_path_stays_on_its_layer() {
        let (grid, masks) = open_grid();
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert_eq!(path.len(), 31);
        assert!(path.iter().all(|&(_, j, layer)| j == 5 && layer == FRONT));
        let runs = runs(&grid, &path.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>());
        assert_eq!(runs.len(), 1, "one straight track");
        assert_eq!(runs[0], (Pt::new(0.0, 0.5), Pt::new(3.0, 0.5)));
    }

    #[test]
    fn a_wall_on_the_front_sends_the_path_through_the_back() {
        let (grid, mut masks) = open_grid();
        for j in 0..grid.ny {
            let at = masks.at(15, j);
            masks.track[FRONT][at] = true;
        }
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert!(path.iter().any(|s| s.2 == BACK));
        let changes = path.windows(2).filter(|w| w[0].2 != w[1].2).count();
        assert_eq!(changes, 2, "down to the back and up again");
    }

    /// Masks of the 3 by 1 mm grid with cells closed at random (a fixed sequence), about one
    /// in `one_in` for tracks on each layer and for vias.
    fn random_masks(seed: u64, one_in: u64) -> (Grid, Masks) {
        let (grid, mut masks) = open_grid();
        let mut x = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut next = || {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (x >> 33).is_multiple_of(one_in)
        };
        for c in 0..masks.window.cells() {
            masks.track[FRONT][c] = next();
            masks.track[BACK][c] = next();
            masks.via[c] = next();
        }
        (grid, masks)
    }

    #[test]
    fn the_flood_finds_a_way_exactly_when_the_search_finds_a_path() {
        let modes = [(true, true), (true, false), (false, false)];
        let (mut joined, mut apart) = (0, 0);
        for seed in 0..120 {
            let (_, masks) = random_masks(seed, 2 + seed % 3);
            let from = [(0, (seed % 10) as usize, FRONT)];
            let to = [(29, ((seed * 7) % 10) as usize, (seed % 2) as usize)];
            for goal in [Goal::Spots(&to), Goal::Via] {
                for (right_angles, coarse) in modes {
                    let extras = Extras {
                        right_angles,
                        coarse,
                        ..Extras::default()
                    };
                    let found = search(&masks, &from, goal, extras, &|| false).is_ok();
                    let flood = reachable(&masks, &from, goal, extras, &|| false);
                    assert_eq!(flood, Ok(found), "seed {seed}, {right_angles} {coarse}");
                    if found {
                        joined += 1;
                    } else {
                        apart += 1;
                    }
                }
            }
        }
        assert!(joined > 50 && apart > 50, "{joined} joined, {apart} apart");
    }

    #[test]
    fn a_pairs_net_goes_out_of_its_way_to_run_beside_its_partner() {
        // On a grid of 10 mm by 1.5 mm, from row 2 at the left to row 2 at the right. The
        // partner's cells are row 6, from column 10 to 90: a step there costs half, and with the
        // estimate cut to match, the path leaves its straight way to run on them.
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(10.0, 1.5)));
        let mut masks = Masks::open(Window::full(&grid));
        let mut beside = vec![false; masks.window.cells()];
        for i in 10..=90 {
            beside[masks.at(i, 6)] = true;
        }
        masks.follow = [beside, vec![false; masks.window.cells()]];
        masks.estimate = FOLLOW;
        let path = find(&masks, &[(0, 2, FRONT)], Goal::Spots(&[(100, 2, FRONT)])).unwrap();
        let on = path.iter().filter(|s| s.1 == 6).count();
        assert!(on >= 70, "{on} cells beside the partner");
        // With nothing to follow, it runs straight.
        let masks = Masks::open(Window::full(&grid));
        let path = find(&masks, &[(0, 2, FRONT)], Goal::Spots(&[(100, 2, FRONT)])).unwrap();
        assert!(path.iter().all(|s| s.1 == 2));
    }

    #[test]
    fn a_pairs_new_copper_is_kept_only_if_more_is_coupled_at_no_more_length_or_vias() {
        let before = (10.0, 2, 0.30);
        assert!(keeps_pair(before, (10.0, 2, 0.60)));
        assert!(keeps_pair(before, (10.0 + PAIR_SLACK, 1, 0.60)));
        assert!(!keeps_pair(before, (10.5, 2, 0.60)), "longer");
        assert!(!keeps_pair(before, (9.0, 3, 0.60)), "a via more");
        assert!(!keeps_pair(before, (9.0, 2, 0.305)), "no more coupled");
    }

    #[test]
    fn a_wall_on_both_layers_cannot_be_crossed() {
        let (grid, mut masks) = open_grid();
        for j in 0..grid.ny {
            let at = masks.at(15, j);
            masks.track[FRONT][at] = true;
            let at = masks.at(15, j);
            masks.track[BACK][at] = true;
        }
        assert_eq!(
            find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])),
            None
        );
    }

    #[test]
    fn a_via_is_not_put_where_the_mask_forbids_one() {
        let (grid, mut masks) = open_grid();
        for j in 0..grid.ny {
            let at = masks.at(15, j);
            masks.track[FRONT][at] = true;
        }
        for i in 0..grid.nx {
            for j in 0..grid.ny {
                let at = masks.at(i, j);
                masks.via[at] = true;
            }
        }
        assert_eq!(
            find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])),
            None
        );
    }

    #[test]
    fn a_run_through_cells_that_need_a_narrow_track_is_a_run_of_its_own() {
        let (grid, _) = open_grid();
        let cells: Vec<(usize, usize)> = (0..10).map(|i| (i, 5)).collect();
        let mut neck = vec![0u8; 10];
        for n in &mut neck[4..7] {
            *n = 1;
        }
        let runs = runs_with(&grid, &cells, &neck);
        // Wide up to the first necked cell, narrow through it, wide again after.
        let widths: Vec<u8> = runs.iter().map(|r| r.2).collect();
        assert_eq!(widths, [0, 1, 0], "{runs:?}");
        assert_eq!(runs[0].1, runs[1].0);
        assert_eq!(runs[1].1, runs[2].0);
        assert_eq!(runs[0].0, Pt::new(0.0, 0.5));
        assert_eq!(runs[2].1, Pt::new(0.9, 0.5));
    }

    #[test]
    fn turns_cost_more_than_straight_runs() {
        let (grid, masks) = open_grid();
        // Same cost in steps either way round, so the path with one corner wins.
        let path = find(&masks, &[(0, 0, FRONT)], Goal::Spots(&[(20, 8, FRONT)])).unwrap();
        let cells: Vec<_> = path.iter().map(|s| (s.0, s.1)).collect();
        assert_eq!(runs(&grid, &cells).len(), 2);
    }

    #[test]
    fn the_turn_cost_of_the_masks_is_what_a_bend_costs() {
        // A wall on the front with a gap at its top: round it takes four bends, through the
        // back two vias.
        let (_, mut masks) = open_grid();
        for j in 0..9 {
            let at = masks.at(15, j);
            masks.track[FRONT][at] = true;
        }
        let ends = ([(0, 4, FRONT)], [(30, 4, FRONT)]);
        let layers = |masks: &Masks| -> Vec<usize> {
            find(masks, &ends.0, Goal::Spots(&ends.1))
                .unwrap()
                .iter()
                .map(|s| s.2)
                .collect()
        };
        assert_eq!(masks.turn, TURN);
        assert!(
            !layers(&masks).contains(&BACK),
            "bends are cheaper than vias"
        );
        masks.turn = 1000;
        assert!(
            layers(&masks).contains(&BACK),
            "vias are cheaper than bends"
        );
    }

    /// A grid of 3 mm by 1 mm with one escape lane on the front, of net 7: the cells of row 5
    /// from column 10 to 20, and a via's place at its end.
    fn one_lane() -> (Grid, Lanes) {
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
        let mut lanes = Lanes::over(&grid, 8);
        let track = Rect::new(Pt::new(1.0, 0.5), Pt::new(2.0, 0.5));
        lanes.claim(&grid, 7, [true, false], &track, Pt::new(2.0, 0.5), 0.2, 0.2);
        (grid, lanes)
    }

    #[test]
    fn an_escape_lane_is_free_for_its_own_net_and_dear_for_another() {
        let (grid, lanes) = one_lane();
        let window = Window::full(&grid);
        let straight = |net: usize| {
            let mut masks = Masks::open(window);
            masks.lane = lanes.charged(window, net);
            let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
            path.iter().all(|&(_, j, layer)| j == 5 && layer == FRONT)
        };
        assert!(straight(7), "the lane's own net goes straight through it");
        assert!(!straight(3), "another net goes round the lane");
        // A via of another net at the lane's end costs more than one off the lane.
        let mut masks = Masks::open(window);
        masks.lane = lanes.charged(window, 3);
        assert!(masks.lane_via_cost(20, 5) > 0);
        assert!(masks.lane_cost(FRONT, 15, 5) > 0);
        assert_eq!(
            masks.lane_cost(BACK, 15, 5),
            0,
            "the lane is on the pad's layer only"
        );
        assert_eq!(masks.lane_via_cost(20, 9), 0);
        // The lane's own net puts its via at the lane's end for nothing, but pays as any other
        // for one that closes the lane before its end.
        masks.lane = lanes.charged(window, 7);
        assert_eq!(masks.lane_via_cost(20, 5), 0);
        assert!(masks.lane_via_cost(14, 5) > 0);
        assert!(
            masks.lane_via_cost(14, 7) > 0,
            "beside the lane, its copper would close it"
        );
    }

    #[test]
    fn the_other_net_of_a_diff_pair_uses_the_lane_as_its_own() {
        let (grid, mut lanes) = one_lane();
        lanes.partner[7] = Some(3);
        lanes.partner[3] = Some(7);
        let window = Window::full(&grid);
        let mut masks = Masks::open(window);
        masks.lane = lanes.charged(window, 3);
        assert_eq!(masks.lane_cost(FRONT, 15, 5), 0);
        masks.lane = lanes.charged(window, 2);
        assert!(masks.lane_cost(FRONT, 15, 5) > 0);
    }

    #[test]
    fn another_net_still_crosses_a_lane_when_nothing_else_is_free() {
        let (grid, lanes) = one_lane();
        let window = Window::full(&grid);
        let mut masks = Masks::open(window);
        masks.lane = lanes.charged(window, 3);
        // Everything but row 5 is walled, and no via may stand anywhere.
        for j in 0..grid.ny {
            for i in 0..grid.nx {
                let at = masks.at(i, j);
                masks.via[at] = true;
                masks.track[BACK][at] = true;
                if j != 5 {
                    masks.track[FRONT][at] = true;
                }
            }
        }
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)]));
        assert_eq!(path.map(|p| p.len()), Some(31), "a cost, not a wall");
    }

    #[test]
    fn a_path_to_a_plane_ends_at_the_nearest_cell_a_via_may_use() {
        let (grid, mut masks) = open_grid();
        // No via within three cells of the start, along either layer.
        for i in 0..3 {
            for j in 0..grid.ny {
                let at = masks.at(i, j);
                masks.via[at] = true;
            }
        }
        let path = find(&masks, &[(0, 5, FRONT)], Goal::Via).unwrap();
        assert_eq!(path.last(), Some(&(3, 5, FRONT)));
        // With a via nowhere, there is no way to the plane.
        masks.via.iter_mut().for_each(|v| *v = true);
        assert_eq!(find(&masks, &[(0, 5, FRONT)], Goal::Via), None);
    }

    #[test]
    fn a_window_round_the_ends_of_a_line_is_clipped_to_the_grid() {
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(10.0, 5.0)));
        let w = Window::around(&grid, &[(20, 25, FRONT), (30, 25, BACK)], 1.0);
        assert_eq!(
            w,
            Window {
                i0: 10,
                j0: 15,
                nx: 31,
                ny: 21
            }
        );
        assert!(w.contains(10, 15) && w.contains(40, 35) && !w.contains(41, 25));
        let edge = Window::around(&grid, &[(1, 1, FRONT)], 2.0);
        assert_eq!((edge.i0, edge.j0), (0, 0));
        let all = Window::around(&grid, &[(50, 25, FRONT)], 100.0);
        assert!(all.covers(&grid));
    }

    #[test]
    fn a_search_in_a_window_stays_inside_it() {
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
        let mut masks = Masks::open(Window {
            i0: 10,
            j0: 0,
            nx: 15,
            ny: grid.ny,
        });
        // A wall on both layers across the window, with a gap only outside it is no gap.
        for j in 0..grid.ny {
            let at = masks.at(20, j);
            masks.track[FRONT][at] = true;
            masks.track[BACK][at] = true;
        }
        assert_eq!(
            find(&masks, &[(10, 5, FRONT)], Goal::Spots(&[(24, 5, FRONT)])),
            None
        );
    }

    #[test]
    fn a_pad_walled_in_is_closed_and_one_hemmed_in_by_the_window_is_cramped() {
        let (_, mut masks) = open_grid();
        let ring = |masks: &mut Masks, (ci, cj): (usize, usize)| {
            for (i, j) in [(ci - 1, cj), (ci + 1, cj), (ci, cj - 1), (ci, cj + 1)] {
                let at = masks.at(i, j);
                masks.track[FRONT][at] = true;
                masks.track[BACK][at] = true;
            }
        };
        ring(&mut masks, (15, 5));
        let from = [(15, 5, FRONT)];
        let to = [(28, 5, FRONT)];
        assert_eq!(
            link(&masks, &from, Goal::Spots(&to), Extras::default()),
            Link::Closed
        );
        // From the other end the answer is the same.
        assert_eq!(
            link(&masks, &to, Goal::Spots(&from), Extras::default()),
            Link::Closed
        );
        // Open it again and the ends are joined.
        let (_, open) = open_grid();
        assert_eq!(
            link(&open, &from, Goal::Spots(&to), Extras::default()),
            Link::Joined
        );

        // A window with a wall in it, from edge to edge: each side is hemmed in by the wall
        // and the window, which a larger window might not.
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(3.0, 1.0)));
        let mut cut = Masks::open(Window {
            i0: 5,
            j0: 0,
            nx: 20,
            ny: grid.ny,
        });
        for j in 0..grid.ny {
            let at = cut.at(15, j);
            cut.track[FRONT][at] = true;
            cut.track[BACK][at] = true;
        }
        assert_eq!(
            link(
                &cut,
                &[(10, 5, FRONT)],
                Goal::Spots(&[(20, 5, FRONT)]),
                Extras::default()
            ),
            Link::Cramped
        );
    }

    #[test]
    fn a_link_to_a_plane_needs_a_place_for_a_via() {
        let (_, mut masks) = open_grid();
        assert_eq!(
            link(&masks, &[(0, 5, FRONT)], Goal::Via, Extras::default()),
            Link::Joined
        );
        masks.via.iter_mut().for_each(|v| *v = true);
        assert_ne!(
            link(&masks, &[(0, 5, FRONT)], Goal::Via, Extras::default()),
            Link::Joined
        );
    }

    /// The open grid with the cell `(ci, cj)` walled in on the front: its four neighbours
    /// are closed to tracks there, and no via may be put anywhere (the way out of a pad
    /// that has no room beside it).
    fn walled_front(ci: usize, cj: usize) -> Masks {
        let (_, mut masks) = open_grid();
        masks.via.iter_mut().for_each(|v| *v = true);
        for (i, j) in [(ci - 1, cj), (ci + 1, cj), (ci, cj - 1), (ci, cj + 1)] {
            let at = masks.at(i, j);
            masks.track[FRONT][at] = true;
        }
        masks
    }

    fn search_with(masks: &Masks, from: &[Spot], to: &[Spot], extras: Extras) -> Option<Vec<Spot>> {
        search(masks, from, Goal::Spots(to), extras, &|| false).ok()
    }

    #[test]
    fn a_pad_walled_in_on_its_layer_is_left_through_a_via_in_the_pad_if_it_may_have_one() {
        let masks = walled_front(10, 5);
        let from = [(10, 5, FRONT)];
        let to = [(28, 5, BACK)];
        assert_eq!(search_with(&masks, &from, &to, Extras::default()), None);
        let extras = Extras {
            from: &[(10, 5, BACK)],
            to: &[],
            ..Extras::default()
        };
        let path = search_with(&masks, &from, &to, extras).unwrap();
        // The path starts on the back, in the pad's cell, and comes up at the far end.
        assert_eq!(path.first(), Some(&(10, 5, BACK)));
        assert_eq!(path.last(), Some(&(28, 5, BACK)));
        assert_eq!(link(&masks, &from, Goal::Spots(&to), extras), Link::Joined);
        assert_eq!(
            link(&masks, &from, Goal::Spots(&to), Extras::default()),
            Link::Closed
        );
    }

    #[test]
    fn the_far_end_of_a_path_may_be_a_via_in_a_pad_too() {
        let masks = walled_front(20, 5);
        let from = [(2, 5, BACK)];
        let to = [(20, 5, FRONT)];
        assert_eq!(search_with(&masks, &from, &to, Extras::default()), None);
        let extras = Extras {
            from: &[],
            to: &[(20, 5, BACK)],
            ..Extras::default()
        };
        let path = search_with(&masks, &from, &to, extras).unwrap();
        assert_eq!(path.last(), Some(&(20, 5, BACK)));
        assert_eq!(path.first(), Some(&(2, 5, BACK)));
    }

    #[test]
    fn a_via_in_a_pad_is_used_only_when_nothing_cheaper_reaches_the_pad() {
        // Nothing in the way: the straight path on the front costs 180 and a via in the pad
        // costs far more, so the path is not through it.
        let (_, masks) = open_grid();
        let extras = Extras {
            from: &[(2, 5, BACK)],
            to: &[(28, 5, BACK)],
            ..Extras::default()
        };
        let path = search_with(&masks, &[(2, 5, FRONT)], &[(28, 5, FRONT)], extras).unwrap();
        assert!(path.iter().all(|s| s.2 == FRONT), "{path:?}");
    }

    #[test]
    fn a_search_that_is_told_to_stop_gives_up_and_says_so() {
        // No place for a via anywhere, and so much to look at that the search looks at the
        // clock before it has seen it all.
        let grid = Grid::over(&Rect::new(Pt::new(0.0, 0.0), Pt::new(40.0, 40.0)));
        let mut all_via = Masks::open(Window::full(&grid));
        all_via.via.iter_mut().for_each(|v| *v = true);
        assert_eq!(
            search(
                &all_via,
                &[(0, 0, FRONT)],
                Goal::Via,
                Extras::default(),
                &|| true
            ),
            Err(Miss::Stopped)
        );
        assert_eq!(
            search(
                &all_via,
                &[(0, 0, FRONT)],
                Goal::Via,
                Extras::default(),
                &|| false
            ),
            Err(Miss::Blocked)
        );
    }

    fn led_board() -> Board {
        crate::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap()
    }

    #[test]
    fn the_first_try_keeps_the_first_order_and_later_tries_keep_the_classes_in_turn() {
        let board = led_board();
        let options = RouteOptions::default();
        let mut router = Router::new(&board, &options);
        assert!(router.lines.len() >= 3);
        // The classes most important last in the plain order, so that putting them in turn
        // moves every line.
        let plain = router.orders(true);
        let n = router.lines.len();
        let mut class = vec![Class::Rest; n];
        let order = [
            Class::Escape,
            Class::Decoupling,
            Class::Pair,
            Class::Clock,
            Class::Power,
        ];
        for (k, &l) in plain[0].iter().rev().enumerate() {
            class[l] = order[k.min(order.len() - 1)];
        }
        router.class = class;
        let tries = router.tries(&options);
        assert_eq!(tries[0], plain[0], "the first try is the router's own");
        assert_eq!(
            tries[1],
            router.classed_order(&plain[0]),
            "then the first order by class"
        );
        assert_ne!(tries[1], tries[0]);
        for t in &tries[1..] {
            let classes: Vec<Class> = t.iter().map(|&l| router.class[l]).collect();
            assert!(classes.is_sorted(), "{classes:?}");
        }
        // Inside a class a line keeps the order it had.
        router.class = vec![Class::Rest; n];
        assert_eq!(router.classed_order(&plain[0]), plain[0]);
        // With no classes the tries are the plain ones; with classes on every try, the first
        // is by class too.
        let off = RouteOptions {
            classes: LineClasses::Off,
            ..RouteOptions::default()
        };
        assert_eq!(router.tries(&off)[0], plain[0]);
    }

    #[test]
    fn the_fixed_order_of_the_classes_is_escape_decoupling_pair_clock_power_rest() {
        let fixed = [
            Class::Escape,
            Class::Decoupling,
            Class::Pair,
            Class::Clock,
            Class::Power,
            Class::Rest,
        ];
        assert!(fixed.is_sorted());
        let protected: Vec<Class> = fixed.into_iter().filter(|c| c.protected()).collect();
        assert_eq!(protected, [Class::Decoupling, Class::Pair, Class::Clock]);
    }

    /// A state of the LED board where only line `a` is routed: a track of its net across the
    /// middle of line `b`, of another net.
    fn across(router: &Router, a: usize, b: usize) -> State {
        let (from, to) = (router.lines[b].from_at, router.lines[b].to_at);
        let mid = Pt::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let len = (dx * dx + dy * dy).sqrt();
        let (nx, ny) = (-dy / len, dx / len);
        // On both layers and right across the board, so that no way round is cheaper.
        let track = |layer| Track {
            net: router.lines[a].net.clone(),
            layer,
            from: Pt::new(mid.x - nx * 50.0, mid.y - ny * 50.0),
            to: Pt::new(mid.x + nx * 50.0, mid.y + ny * 50.0),
            width: 0.2,
        };
        State {
            routed: vec![Routed {
                line: a,
                tracks: vec![track(CopperLayer::Front), track(CopperLayer::Back)],
                vias: Vec::new(),
            }],
            ..State::default()
        }
    }

    #[test]
    fn a_decoupling_line_is_not_ripped_up_for_a_line_of_a_later_class() {
        let board = led_board();
        let mut router = Router::new(&board, &RouteOptions::default());
        // Two lines of different nets, neither to a plane.
        let (a, b) = (0..router.lines.len())
            .flat_map(|a| (0..router.lines.len()).map(move |b| (a, b)))
            .find(|&(a, b)| {
                router.nets[a] != router.nets[b]
                    && !router.lines[a].to_plane
                    && !router.lines[b].to_plane
            })
            .unwrap();
        let state = across(&router, a, b);
        let mut class = vec![Class::Rest; router.lines.len()];
        class[a] = Class::Decoupling;
        router.class = class.clone();
        let names = |router: &Router| {
            let found = router.contested(b, &state, ROOM_LOOSE).map(|(_, n)| n);
            let chain = router
                .contested_lines(b, &state, ROOM_LOOSE, &[])
                .map(|l| l.iter().map(|&(l, _)| l).collect::<Vec<_>>());
            (found, chain)
        };
        // A try without classes rips up whatever is in the way.
        router.classed.set(false);
        let (found, chain) = names(&router);
        assert!(found.unwrap().contains(&router.nets[a]));
        assert!(chain.unwrap().contains(&a));
        // A try with them never names the decoupling line for a later one.
        router.classed.set(true);
        assert_eq!(router.protected(b, &state), [a]);
        let (found, chain) = names(&router);
        assert!(!found.unwrap_or_default().contains(&router.nets[a]));
        assert!(!chain.unwrap_or_default().contains(&a));
        // A power line keeps its place in the order only: it may be ripped up.
        class[a] = Class::Power;
        router.class = class.clone();
        assert!(router.protected(b, &state).is_empty());
        assert!(names(&router).0.unwrap().contains(&router.nets[a]));
        // And a decoupling line is ripped up for one of its own class or a more important one.
        class[a] = Class::Decoupling;
        class[b] = Class::Escape;
        router.class = class;
        assert!(router.protected(b, &state).is_empty());
    }

    #[test]
    fn a_board_that_routes_on_its_first_try_has_the_same_copper_with_or_without_classes() {
        for design in [crate::testlib::led_design(), four_layer_design()] {
            let route_once = |classes| {
                let mut board = crate::board_from_design(
                    &design,
                    &netsmith_kicad::Mapping::standard(),
                    &fixtures(),
                )
                .unwrap();
                let routing = route_with(
                    &mut board,
                    RouteOptions {
                        classes,
                        ..RouteOptions::default()
                    },
                );
                assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
                assert_eq!(routing.tries, 1);
                (board.tracks, board.vias)
            };
            assert_eq!(
                route_once(LineClasses::Off),
                route_once(LineClasses::FromSecondTry)
            );
        }
    }

    #[test]
    fn the_coarse_plan_gives_each_line_a_corridor_round_its_ends_and_is_the_same_every_time() {
        let board = crate::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let options = RouteOptions {
            plan: CoarsePlan::EveryTry,
            ..RouteOptions::default()
        };
        let router = Router::new(&board, &options);
        let order = router.orders(false).remove(0);
        let plan = router.make_plan(&order);
        let tiles = &plan.tiles;
        // Every edge holds a track somewhere: the board is not walled.
        assert!(tiles.across_x[FRONT].iter().any(|&c| c > 0));
        assert!(plan.corridors.iter().any(Option::is_some));
        for (line, corridor) in plan.corridors.iter().enumerate() {
            // A line to a plane, or one wider than the narrowest, has none.
            let Some(inside) = corridor else { continue };
            for &(i, j, _) in router.ends[line].0.iter().chain(&router.ends[line].1) {
                let (a, b) = tiles.of(i, j);
                assert!(inside[b * tiles.nx + a], "line {line}");
            }
        }
        let again = Router::new(&board, &options).make_plan(&order);
        assert_eq!(plan.corridors, again.corridors);
    }

    #[test]
    fn a_line_of_the_pours_net_or_of_a_diff_pair_has_no_corridor() {
        let mut board = crate::board_from_design(
            &crate::testlib::led_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let options = RouteOptions {
            plan: CoarsePlan::EveryTry,
            ..RouteOptions::default()
        };
        let router = Router::new(&board, &options);
        let order = router.orders(false).remove(0);
        let plan = router.make_plan(&order);
        let mut steered: Vec<usize> = (0..router.lines.len())
            .filter(|&l| plan.corridors[l].is_some())
            .map(|l| router.nets[l])
            .collect();
        steered.dedup();
        assert!(steered.len() >= 3, "{steered:?}");
        let (poured, paired) = (steered[0], steered[1]);
        board.pours.push(netsmith_pcb::board::BoardPour {
            layer: netsmith_pcb::board::CopperLayer::Back,
            net: board.nets[poured].name.clone(),
            stitch_pitch: None,
        });
        let mut router = Router::new(&board, &options);
        router.partners[paired] = Some(poured);
        let plan = router.make_plan(&order);
        for (l, corridor) in plan.corridors.iter().enumerate() {
            let net = router.nets[l];
            if net == poured || net == paired {
                assert!(corridor.is_none(), "line {l}");
            }
        }
        assert!(
            (0..router.lines.len())
                .any(|l| router.nets[l] == steered[2] && plan.corridors[l].is_some())
        );
    }

    #[test]
    fn a_board_routed_with_the_coarse_plan_is_complete_and_the_same_every_time() {
        let route_once = || {
            let mut board = crate::board_from_design(
                &crate::testlib::led_design(),
                &netsmith_kicad::Mapping::standard(),
                &fixtures(),
            )
            .unwrap();
            let routing = route_with(
                &mut board,
                RouteOptions {
                    plan: CoarsePlan::EveryTry,
                    ..RouteOptions::default()
                },
            );
            assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
            (board.tracks, board.vias)
        };
        assert_eq!(route_once(), route_once());
    }

    /// The LED board with its two signal nets named `names` and `GND` poured on the back, routed
    /// on the front, with each signal line's copper then moved to the back between two vias at
    /// its pads, as a try might leave it; and what the pass off the pours makes of it.
    fn off_the_pours_of_the_led_board(names: [&str; 2]) -> (Board, Vec<Routed>) {
        let mut design = crate::testlib::led_design();
        design.nets[1].name = names[0].into();
        design.nets[2].name = names[1].into();
        let mut board =
            crate::board_from_design(&design, &netsmith_kicad::Mapping::standard(), &fixtures())
                .unwrap();
        board.pours.push(netsmith_pcb::BoardPour {
            layer: CopperLayer::Back,
            net: "GND".into(),
            stitch_pitch: None,
        });
        let router = Router::new(&board, &RouteOptions::default());
        let order = router.orders(false).remove(0);
        let mut state = router.run_stages(&order, true, 1, 1);
        assert!(state.failed.is_empty());
        for r in &mut state.routed {
            let line = &router.lines[r.line];
            if !names.contains(&line.net.as_str()) {
                continue;
            }
            let width = r.tracks[0].width;
            let rules = &board.rules;
            r.tracks = vec![Track {
                net: line.net.clone(),
                layer: CopperLayer::Back,
                from: line.from_at,
                to: line.to_at,
                width,
            }];
            r.vias = [line.from_at, line.to_at]
                .map(|at| Via {
                    net: line.net.clone(),
                    at,
                    diameter: rules.via_diameter,
                    drill: rules.via_drill,
                })
                .to_vec();
        }
        router.off_the_pours(&mut state, 1, 1);
        let routed = state.routed.clone();
        drop(router);
        (board, routed)
    }

    #[test]
    fn a_line_on_another_nets_pour_is_moved_off_it_after_a_complete_try() {
        let (_, routed) = off_the_pours_of_the_led_board(["VCC", "net-led.anode"]);
        for r in routed.iter().filter(|r| r.tracks[0].net != "GND") {
            assert!(
                r.tracks.iter().all(|t| t.layer == CopperLayer::Front) && r.vias.is_empty(),
                "{} is still on the pour: {:?} {:?}",
                r.tracks[0].net,
                r.tracks,
                r.vias
            );
        }
    }

    #[test]
    fn the_nets_of_a_diff_pair_are_not_moved_off_the_pour() {
        let (_, routed) = off_the_pours_of_the_led_board(["USB_P", "USB_N"]);
        for r in routed.iter().filter(|r| r.tracks[0].net != "GND") {
            assert!(
                r.tracks.iter().all(|t| t.layer == CopperLayer::Back) && r.vias.len() == 2,
                "{} moved: {:?}",
                r.tracks[0].net,
                r.tracks
            );
        }
    }

    #[test]
    fn the_four_layer_board_is_routed_with_a_via_to_the_plane_beside_each_surface_mount_pad() {
        let mut board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let left = route(&mut board);
        assert!(left.is_empty(), "{:?}", left.lines);
        // Tracks are on the two outer layers only: the type has no other.
        for (refdes, pad, net) in [("D1", "1", "GND"), ("R1", "1", "3V3")] {
            let part = board.part(refdes).unwrap();
            let at = part.pad_centre(part.footprint.pads_numbered(pad).next().unwrap());
            let via = board
                .vias
                .iter()
                .find(|v| v.net == net && v.at.distance(at) < 2.0)
                .unwrap_or_else(|| panic!("no {net} via beside {refdes}.{pad}"));
            // The via is not on the pad, and a track on the pad's layer joins them.
            assert!(via.at.distance(at) > 0.2, "{refdes}.{pad}");
            let touches = |p: Pt| {
                board.tracks.iter().any(|t| {
                    t.net == net
                        && t.layer == CopperLayer::Front
                        && (t.from.distance(p) < 1e-6 || t.to.distance(p) < 1e-6)
                })
            };
            assert!(touches(at) && touches(via.at), "{refdes}.{pad}");
        }
        // The through-hole pads are on the planes already, so nothing else has a via.
        assert_eq!(board.vias.iter().filter(|v| v.net == "GND").count(), 1);
        assert_eq!(board.vias.iter().filter(|v| v.net == "3V3").count(), 1);
    }

    /// A track of the net `3V3` over the first pad of the net `GND`, `gap` mm from its top
    /// edge, `width` wide, settled against that pad alone.
    fn settle_beside_a_foreign_pad(gap: f64, width: f64) -> (Option<Track>, f64) {
        let board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let router = Router::new(&board, &RouteOptions::default());
        let part = board.part("D1").unwrap();
        let pad = part.footprint.pads_numbered("1").next().unwrap();
        let bounds = part.pad_bounds(pad);
        let net = board.nets.iter().position(|n| n.name == "3V3").unwrap();
        let pad_only: Vec<&Obstacle> = router
            .fixed
            .list
            .iter()
            .filter(|o| matches!(o.form, Form::Rect(r) if r == bounds))
            .collect();
        assert_eq!(pad_only.len(), 1, "the pad is one obstacle");
        let y = bounds.min.y - gap - width / 2.0;
        let track = Track {
            net: "3V3".into(),
            layer: CopperLayer::Front,
            from: Pt::new(bounds.min.x, y),
            to: Pt::new(bounds.max.x, y),
            width,
        };
        let routed = Routed {
            line: 0,
            tracks: vec![track],
            vias: Vec::new(),
        };
        let settled = router.settled(routed, net, &pad_only, true);
        (
            settled.ok().map(|r| r.tracks[0].clone()),
            board.rules.min_track_width,
        )
    }

    fn a_track(net: &str, from: Pt, to: Pt) -> Track {
        Track {
            net: net.into(),
            layer: CopperLayer::Front,
            from,
            to,
            width: 0.2,
        }
    }

    #[test]
    fn the_print_of_a_window_changes_with_the_copper_near_it_only() {
        let board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let router = Router::new(&board, &RouteOptions::default());
        let window = Window {
            i0: 0,
            j0: 0,
            nx: 5,
            ny: 5,
        };
        let corner = router.grid.centre(0, 0);
        let far = router.grid.centre(router.grid.nx - 1, router.grid.ny - 1);
        assert!(
            far.distance(corner) > 2.0 * router.print_reach + 1.0,
            "a far corner"
        );
        // Line 0 is of one net and the line `other` of another.
        let net = router.nets[0];
        let other = (0..router.lines.len())
            .find(|&l| router.nets[l] != net)
            .unwrap();
        let (mine, theirs) = (router.lines[0].net.clone(), router.lines[other].net.clone());
        let routed = |line: usize, net: &str, at: Pt| Routed {
            line,
            tracks: vec![a_track(net, at, at + Pt::new(0.5, 0.0))],
            vias: Vec::new(),
        };
        let near = routed(other, &theirs, corner);
        let print = |others: &[&Routed], soft: bool, locked: &[usize]| {
            router.print(window, net, others, soft, locked)
        };
        let bare = print(&[], false, &[]);
        // The net's own tracks, copper far away, and the order of the copper are not in it.
        assert_eq!(print(&[&routed(0, &mine, corner)], false, &[]), bare);
        assert_eq!(print(&[&routed(other, &theirs, far)], false, &[]), bare);
        let far_too = routed(other, &theirs, far);
        assert_eq!(
            print(&[&near, &far_too], false, &[]),
            print(&[&far_too, &near], false, &[])
        );
        // Another net's track near the window is, and whether a soft search may cross it.
        let hard = print(&[&near], false, &[]);
        assert_ne!(hard, bare);
        assert_ne!(print(&[&near], true, &[]), hard);
        assert_eq!(print(&[&near], true, &[other]), hard);
        // So is what the negotiation has learnt about the window's cells.
        let cells = router.grid.nx * router.grid.ny;
        *router.history.borrow_mut() = [vec![0; cells], vec![0; cells]];
        let learnt = print(&[], false, &[]);
        router.learn(&[(1, 1, FRONT)]);
        assert_ne!(print(&[], false, &[]), learnt);
    }

    #[test]
    fn a_search_whose_print_is_unchanged_finds_what_it_found_before() {
        let board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let router = Router::new(&board, &RouteOptions::default());
        let state = State::default();
        let copper =
            |found: Option<(Routed, Vec<Spot>)>| found.map(|(r, path)| (r.tracks, r.vias, path));
        let first = copper(router.try_line(0, &state, &[], ROOM_TIGHT, false));
        assert!(first.is_some());
        let remembered = router.memo.borrow().len();
        assert!(remembered > 0);
        let again = copper(router.try_line(0, &state, &[], ROOM_TIGHT, false));
        assert_eq!(
            router.memo.borrow().len(),
            remembered,
            "nothing searched again"
        );
        assert_eq!(again, first);
        router.memo.borrow_mut().clear();
        assert_eq!(
            copper(router.try_line(0, &state, &[], ROOM_TIGHT, false)),
            first
        );
    }

    /// Whether a line with `mine` joins cleanly a line of its net with `theirs`, all on the
    /// front, away from the net's pads.
    fn joins(mine: &[(Pt, Pt)], theirs: &[(Pt, Pt)]) -> bool {
        let board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let router = Router::new(&board, &RouteOptions::default());
        let net = board.nets[router.nets[0]].name.clone();
        let routed = |tracks: &[(Pt, Pt)]| Routed {
            line: 0,
            tracks: tracks
                .iter()
                .map(|&(from, to)| Track {
                    net: net.clone(),
                    layer: CopperLayer::Front,
                    from,
                    to,
                    width: 0.1,
                })
                .collect(),
            vias: Vec::new(),
        };
        let other = routed(theirs);
        router.joins_cleanly(routed(mine), &[&other], true).is_ok()
    }

    /// A point far off the board, where no pad of any net is.
    fn off(x: f64, y: f64) -> Pt {
        Pt::new(-50.0 + x, -50.0 + y)
    }

    #[test]
    fn a_line_that_meets_another_of_its_net_at_45_degrees_is_refused() {
        // Issue #476: the other line ends at the point coming along x; this one ends there
        // coming along the diagonal, on the same side, which leaves an acute pocket.
        let theirs = [(off(1.0, 0.0), off(0.0, 0.0))];
        let mine = [(off(1.0, 1.0), off(0.0, 0.0))];
        assert!(!joins(&mine, &theirs));
        // A crossing at the point: both lines run on through it.
        let theirs = [
            (off(-1.0, 0.0), off(0.0, 0.0)),
            (off(0.0, 0.0), off(1.0, 0.0)),
        ];
        let mine = [
            (off(-1.0, -1.0), off(0.0, 0.0)),
            (off(0.0, 0.0), off(1.0, 1.0)),
        ];
        assert!(!joins(&mine, &theirs));
    }

    #[test]
    fn a_line_that_goes_on_from_another_by_a_bend_of_45_degrees_or_meets_it_square_is_kept() {
        // On from the other line's end by a bend of 45 degrees.
        let theirs = [(off(-1.0, 0.0), off(0.0, 0.0))];
        let mine = [(off(0.0, 0.0), off(1.0, 1.0))];
        assert!(joins(&mine, &theirs));
        // A tee: square into a line that runs on through the point.
        let theirs = [
            (off(-1.0, 0.0), off(0.0, 0.0)),
            (off(0.0, 0.0), off(1.0, 0.0)),
        ];
        let mine = [(off(0.0, 1.0), off(0.0, 0.0))];
        assert!(joins(&mine, &theirs));
        // Over the other line the same way: one direction, not a corner.
        let theirs = [(off(0.0, 0.0), off(1.0, 0.0))];
        let mine = [(off(0.0, 0.0), off(2.0, 0.0))];
        assert!(joins(&mine, &theirs));
        // Lines that do not meet at an end.
        let theirs = [(off(0.0, 0.0), off(1.0, 0.0))];
        let mine = [(off(0.0, 1.0), off(1.0, 2.0))];
        assert!(joins(&mine, &theirs));
    }

    #[test]
    fn a_wide_track_nearer_a_pad_than_the_rules_allow_is_drawn_at_the_narrowest_width() {
        // 0.05 mm from the pad at 0.6 mm; at the narrowest width the same centre line is
        // far enough away.
        let (track, narrowest) = settle_beside_a_foreign_pad(0.05, 0.6);
        let track = track.expect("a narrow track fits");
        assert_eq!(track.width, narrowest);
    }

    #[test]
    fn a_wide_track_that_keeps_the_rules_clear_is_left_wide() {
        let (track, _) = settle_beside_a_foreign_pad(0.3, 0.6);
        assert_eq!(track.expect("it fits").width, 0.6);
    }

    #[test]
    fn a_track_over_a_pad_of_another_net_is_refused_whatever_its_width() {
        let (track, _) = settle_beside_a_foreign_pad(-0.4, 0.6);
        assert_eq!(track, None);
    }

    #[test]
    fn a_via_to_a_plane_keeps_the_rules_clear_of_other_copper() {
        let mut board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        route(&mut board);
        let rules = board.rules.clone();
        for via in &board.vias {
            // Off every pad of another net by the clearance.
            for part in &board.parts {
                for pad in &part.footprint.pads {
                    let own = board
                        .net_of(&part.refdes, &pad.number)
                        .is_some_and(|n| n.name == via.net);
                    if own {
                        continue;
                    }
                    let b = part.pad_bounds(pad);
                    let dx = (b.min.x - via.at.x).max(via.at.x - b.max.x).max(0.0);
                    let dy = (b.min.y - via.at.y).max(via.at.y - b.max.y).max(0.0);
                    assert!(
                        dx.hypot(dy) - via.diameter / 2.0 >= rules.clearance - 1e-9,
                        "{} via at {:?} too near {}.{}",
                        via.net,
                        via.at,
                        part.refdes,
                        pad.number
                    );
                }
            }
        }
    }

    #[test]
    fn a_via_keeps_the_masks_least_web_from_an_opening_the_footprint_draws_in_the_mask() {
        let mut board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        // An opening in the mask the library draws beside R1, where its plane via would go.
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .rotation = 0.0;
        let r1 = board.part("R1").unwrap().courtyard();
        let opening = Rect::new(
            Pt::new(r1.max.x + 0.2, r1.min.y - 1.0),
            Pt::new(r1.max.x + 2.2, r1.max.y + 1.0),
        );
        let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        let local = opening.translated(Pt::new(-r1.at.x, -r1.at.y));
        r1.footprint.mask_openings.push(local);
        route(&mut board);
        let rules = board.rules.clone();
        let reach = rules.via_diameter / 2.0 + rules.mask_bridge;
        for via in &board.vias {
            let form = Form::Rect(opening);
            assert!(
                form.distance(via.at) >= reach - 1e-9,
                "{} via at {:?} is {:.3} mm from the opening, under {reach}",
                via.net,
                via.at,
                form.distance(via.at)
            );
        }
        assert!(!board.vias.is_empty());
    }

    /// Copper of another net across the whole grid on both layers: a wall two cells thick at
    /// columns `i` and `i + 1`, except for the rows in `gap`.
    fn soft_wall(grid: &Grid, masks: &mut Masks, i: usize, gap: std::ops::Range<usize>) {
        let cells = masks.window.cells();
        let mut soft = Soft {
            track: [vec![false; cells], vec![false; cells]],
            via: vec![false; cells],
        };
        for j in (0..grid.ny).filter(|j| !gap.contains(j)) {
            for i in [i, i + 1] {
                let at = masks.at(i, j);
                soft.track[FRONT][at] = true;
                soft.track[BACK][at] = true;
                soft.via[at] = true;
            }
        }
        masks.soft = Some(soft);
    }

    #[test]
    fn a_soft_search_crosses_copper_that_a_hard_one_cannot() {
        let (grid, mut masks) = open_grid();
        soft_wall(&grid, &mut masks, 15, 0..0);
        // Hard masks do not see the wall; the soft ones price it, and the path goes through
        // because no way round exists.
        let path = find_soft(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert_eq!(path.len(), 31);
        assert!(path.contains(&(15, 5, FRONT)));
    }

    #[test]
    fn a_soft_search_goes_round_copper_by_a_gap_when_the_gap_is_not_far() {
        let (grid, mut masks) = open_grid();
        // The wall is open at rows 8 and 9, three rows from the line: the detour costs 160
        // (six steps and four turns), crossing the wall's two cells 200 (SOFT, 100 each).
        soft_wall(&grid, &mut masks, 15, 8..10);
        let path = find_soft(&masks, &[(0, 5, FRONT)], Goal::Spots(&[(30, 5, FRONT)])).unwrap();
        assert!(path.contains(&(15, 8, FRONT)), "{path:?}");
        assert!(!path.contains(&(15, 5, FRONT)));
    }

    #[test]
    fn a_soft_path_does_not_stop_at_a_gap_that_costs_more_than_the_wall() {
        let (grid, mut masks) = open_grid();
        // Open only at row 9, eight rows from the line, whose detour costs 260: the path
        // crosses the wall.
        soft_wall(&grid, &mut masks, 15, 9..10);
        let path = find_soft(&masks, &[(0, 1, FRONT)], Goal::Spots(&[(30, 1, FRONT)])).unwrap();
        assert!(path.contains(&(15, 1, FRONT)), "{path:?}");
    }

    #[test]
    fn masks_made_from_the_cached_fixed_copper_leave_a_nets_own_pads_open() {
        let board = crate::board_from_design(
            &four_layer_design(),
            &netsmith_kicad::Mapping::standard(),
            &fixtures(),
        )
        .unwrap();
        let router = Router::new(&board, &RouteOptions::default());
        let window = Window::full(&router.grid);
        let reach = Reach {
            room: ROOM_LOOSE,
            width: board.rules.track_width,
        };
        // For every net, a cell on its own pad is open to it, and shut to every other net.
        for (n, net) in board.nets.iter().enumerate() {
            let own = net.pads.first().expect("a net has pads");
            let part = board.part(&own.refdes).unwrap();
            let pad = part.footprint.pads_numbered(&own.number).next().unwrap();
            let (i, j) = router.grid.cell(part.pad_centre(pad));
            let (front, _) = part.pad_layers(pad);
            let layer = if front { FRONT } else { BACK };
            let mine = router.masks(window, n, (&[], &[]), (&[], &[]), reach);
            assert!(
                !mine.track[layer][mine.at(i, j)],
                "{}: its own pad {own:?} is shut to it",
                net.name
            );
            let other = (n + 1) % board.nets.len();
            {
                let theirs = router.masks(window, other, (&[], &[]), (&[], &[]), reach);
                assert!(
                    theirs.track[layer][theirs.at(i, j)],
                    "{}: the pad of {} is open to {}",
                    net.name,
                    net.name,
                    board.nets[other].name
                );
            }
        }
    }

    #[test]
    fn distances_are_to_the_edge_of_the_form() {
        let rect = Form::Rect(Rect::new(Pt::new(0.0, 0.0), Pt::new(2.0, 1.0)));
        assert_eq!(rect.distance(Pt::new(1.0, 0.5)), 0.0);
        assert_eq!(rect.distance(Pt::new(3.0, 0.5)), 1.0);
        assert_eq!(rect.distance(Pt::new(5.0, 5.0)), 5.0);
        let track = Form::Segment(Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), 0.1);
        assert!((track.distance(Pt::new(2.0, 1.0)) - 0.9).abs() < 1e-12);
        assert!((track.distance(Pt::new(6.0, 0.0)) - 1.9).abs() < 1e-12);
        let via = Form::Circle(Pt::new(0.0, 0.0), 0.3);
        assert!((via.distance(Pt::new(1.0, 0.0)) - 0.7).abs() < 1e-12);
    }
}
