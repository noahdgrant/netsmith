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
//! The board edge is [`Board::edge`]: the outline's rounded rectangle, or its polygon (issue
//! #495). The grid lies over the outline's bounding box, and a cell outside a polygon (in a
//! notch, past an arc) is closed to tracks and vias; the edge's lines and arcs, as short
//! straight pieces, are obstacles at the copper to edge space, as each cutout's edge is. The
//! coarse plan, the arcs and the clean-up take the edge from these, and the meanders and
//! teardrops from `Edge.Cuts`, so all of them keep to the polygon.
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
//! refuses a corner of more than 45 degrees outside a pad or a via of the net. Once the board is
//! routed and has its teardrops, each bend of 45 degrees becomes a tangent arc where one keeps the
//! rules' space
//! ([`crate::arcs`], [`RouteOptions::arcs`]); the search and the copper it settles know nothing
//! of arcs.
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
//!    again. At most six rounds, fewer when a round spreads the lines left (see "What a try
//!    costs"), and the best state is kept. The earlier
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
//! 1 mm first, negotiated for the tiles' capacities and for the lines that cross in a tile,
//! and the detailed search then pays up to [`OFF_PLAN`] for each step outside its line's
//! corridor on a tile the plan crowds. It is off by default: it makes RP2350 route at every
//! turn cost, but costs it 18 to 21 % more vias and leaves a line on Orbit at the least track. The design, the numbers and what to try next are in the
//! header of `plan.rs`.
//!
//! # Teardrops (issue #431, S11)
//!
//! When a board is routed, after the meanders (below), the vias beside vias and the stitching
//! of pours, each track that ends in a pad or a via gets a teardrop where it leaves the copper
//! (`crate::teardrop::add`, unless [`RouteOptions::teardrops`] is off), where it keeps every
//! space the design rules check asks of a track. They are drawn round the copper the search
//! settled, before the arcs at the bends ([`crate::arcs`]), which keep clear of them, and the
//! router never searches round them; a board's vias are the same with them or without.
//! netsmith draws them itself, with the check's geometry, and writes them as KiCad 9's teardrop
//! zones (decided by the implementing agent under the owner's delegation, 2026-11-13); the sizes,
//! the rules and why are in the header of `netsmith_geom::drc::teardrop`.
//!
//! # Length-matching meanders (issue #431, S11)
//!
//! **What is built** (`crate::meander::add`). When the best try routed every line, its
//! copper is on the board and before the vias beside vias, the stitching of pours, the
//! teardrops and the arcs, the shorter net of each diff pair whose two nets' track lengths
//! differ by more than 0.1 mm gets a meander sized to add the difference: bumps with 45 degree corners on one side of
//! one of its straight tracks, the longest first, legs three widths apart edge to edge (or the
//! clearance, if larger), at most 2 mm high, the fewest bumps first; on an interval of the track
//! more than the scorecard's coupled 0.5 mm from the partner, away from the partner's side first,
//! a pitch from the track's ends, and only where it keeps every space the design rules check asks
//! of a track (`netsmith_geom::drc::Room`, the teardrops' check) and the pitch from its own net's
//! other copper. A run short of room takes what it can and the next the rest. No other copper
//! moves and no line is left; [`Routing::pairs`] has each pair's skew before and after, and
//! `fab` says which pair keeps a skew over the target. The target, 0.1 mm (the grid), and the
//! shape are decided by the agent under the owner's delegation (2026-11-13): USB 2.0 high speed
//! allows a few mm, so the target is no constraint on where a meander fits, only on how long it
//! is; the sizes and why are in that module's header.
//!
//! The arcs (`arcs.rs`) run after it and round the meander's turns too, which shortens it, so a
//! pair the arcs move back over the target gets another meander and its bends are rounded in
//! turn, at most [`MEANDER_TOP_UPS`] times.
//!
//! **Measured** (2026-11-13, main at 0f70950f with the arcs and #595's teardrops before them, `scripts/route-scorecard` without
//! and with the pass, the same binary and placements; skew, coupled share):
//!
//! | | RP2350 `mcu.usb` / `usb` | Orbit `usb` | EasyDuino `bridge` |
//! |---|---|---|---|
//! | without | 12.37 mm, 85 % / 0.92 mm, 0 % | 0.17 mm, 72 % | 0.87 mm, 51 % |
//! | **with (built)** | 12.33 mm, 85 % / **0.05 mm**, 0 % | **0.04 mm**, 70 % | **0.10 mm**, 47 % |
//!
//! Before the arcs merged (main at 88a62705) the same three pairs ended at 0.00 mm. Track grows
//! by the meanders' own length only (about 1, 0.2 and 0.8 mm); vias, other nets' track on the
//! pours, `fab-notes.txt` and the check (0 errors) are unchanged; the arcs round the meanders'
//! turns (arcs 270 to 288, 177 to 181, 145 to 153), and the coupled share falls only by the
//! meander's own track. The pass takes 8 to 23 ms; `fab` took 106 / 106 s on RP2350 against
//! 115 / 116 s without, 14 / 13 s on Orbit against 14 / 16 s, 7 / 8 s on EasyDuino against
//! 7 / 7 s. Each board wrote the same copper on both runs. RP2350's `mcu.usb` keeps its skew:
//! its `dp` is 5.8 mm long, its one run of more than 2 mm runs 0.4 mm from `dm` the whole way,
//! and `dm` reaches a pad 10 mm further off, so the skew is the pair's pads', for the placement
//! to close.
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
//! - **The negotiation stops when a round spreads the lines left** ([`spreads`]): after a round
//!   that leaves more than twice the lines of the best state seen and [`SPREAD_BEYOND`] 3 more.
//!   Such rounds push the conflict onto other lines instead of trading it: RP2350's rounds left
//!   7 to 20 lines against a start of 2 to 4 at both margins and every turn cost, and none ever
//!   left fewer. A negotiation that trades lines stays near its best: Orbit leaves 1, 3, 2, 1,
//!   1 and routes in round 5 at the least track (1, 2, 2, 2, 1, 1 and round 6 at 0.15 mm), and
//!   over 77 negotiations (the three boards at both margins and turn costs 24 to 27, tight and
//!   mixed seeds 0 to 11) no round before an improvement left more than twice the best and
//!   one. The first form of a stop, after any round that did not improve, lost Orbit at the
//!   least track, which only improves after four such rounds; so did "the best and five more"
//!   on a tight seed.
//!
//! Measured on the same machine, back to back with main at fda6cd4f, the copper byte-identical:
//! RP2350's `fab` takes 136 and 133 s against main's 138 and 166 s (a loaded machine, load
//! average 6 to 12), and Orbit routed at the least track from a saved placement
//! (`netsmith route --margin 1 --route-timeout 0`) takes 18.5 s against 32.6 s.
//!
//! The negotiation's stop, measured back to back with main at 8691e071 (the two runs side by
//! side), the copper byte-identical everywhere and the same lines left: one RP2350 try
//! (`netsmith route --tries 1 --route-timeout 0 --turn T`, T = 24 to 27) takes 28 to 43 s
//! against 47 to 68 s at the least track and 59 to 64 s against 95 to 101 s with the 0.15 mm
//! trial first; RP2350's `fab` 103 s against 119 s. Orbit and EasyDuino take what they took
//! (their negotiations never spread). The tight and mixed seeds 0 to 11 take 857 and 607 s
//! against 979 and 852 s, the same copper.
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
//! # Decoupling: pin, capacitor, via (issue #431, S7)
//!
//! **What is built** (`decouple.rs`). The router's lines are the ratsnest's with, on each supply
//! net, the line from a decoupled power pin to its capacitor's pad made first (the pin is the
//! nearest power pin the placer's decoupling pair names, within 5 mm), so the pin's current
//! passes the capacitor. These are the lines of [`Class::Decoupling`]. Then, after a complete try
//! and the two passes above, [`Router::to_the_pours`] gives each such capacitor's pad on a return
//! net poured on the other outer layer (ground on the back of RP2350 and Orbit) a via of its own,
//! on at most [`RETURN_TRACK`] 1.5 mm of track, unless a via of the net is that near already. It
//! never moves other copper or leaves a line. EasyDuino's ground is a plane, so its capacitors
//! had their vias already; its 3.3 V plane pins now reach the plane through the capacitor's via.
//!
//! **Measured** (2026-11-13, main at 19861713 against this, `netsmith fab` back to back, the same
//! placements; the scorecard's decoupling track from the capacitor to the part's nearest pad, mean
//! / max, and from its ground pad to the nearest via, mean / max; length over the spanning length,
//! vias, other nets' track on the GND pour, pair coupling):
//!
//! | | RP2350 | Orbit | EasyDuino |
//! |---|---|---|---|
//! | main | 11.0 / 37.5, 2.3 / 7.8 mm; 1.29x, 129 vias, 423 mm, 69 % | 10.3 / 38.5, 8.1 / 29.6 mm; 1.22x, 87, 304 mm, 0 % | 21.0 / 56.4, 0.7 / 1.0 mm; 1.22x, 105, 54 % |
//! | **built** | **6.3 / 22.4, 1.3 / 4.0 mm**; 1.33x, 142 vias, 433 mm, 85 % | 10.4 / 40.9, **2.6 / 12.8 mm**; 1.20x, 94, 303 mm, 72 % | = main |
//! | the pin a leaf too (not kept) | 7.0 / 25.3; 1.42x, 162 vias, 480 mm | 10.5 / 45.8; 1.18x, 91 | 24.9 / 56.4; 1.23x, 107 |
//!
//! The vias added are mostly the capacitors' own (about 6 on RP2350 and 7 on Orbit). RP2350's
//! length and vias at turn cost 25 are inside the spread main shows over turn costs 24 to 27
//! (1.29 to 1.43x, 129 to 162 vias; built 1.33 to 1.36x, 129 to 157). Robustness (one try at turn
//! costs 24 / 25 / 26 / 27, the default margin and `--margin 1`): main leaves 0 / 0 / 0 / 1 on
//! RP2350 at both, this leaves 0 / 0 / 0 / 0; Orbit and EasyDuino 0 / 0 / 0 / 0 on both. `fab`
//! took 120 / 127 s on RP2350 against main's 122 / 124 s, 14 / 12 s on Orbit against 17 / 17 s,
//! 7 s on EasyDuino as main; each board gives the same copper on both runs. The scorecard's
//! decoupling counts every two-pad capacitor between a pour and a part of 8 pads or more, some of
//! them bulk or crystal capacitors the placer does not pair, which keep their length.
//!
//! # Fewer vias (issue #431)
//!
//! KiCad's push-and-shove optimizer has a pass that removes vias, kept only where the result is
//! clean and cheaper. This is its equivalent.
//!
//! **What is built** ([`Router::fewer_vias`]). When a try has routed every line, after the
//! pairs' pass, the pass off the pours and the decoupling vias, and so before the pairs'
//! meanders (which match the lengths it leaves), the stitching, the teardrops and the arcs,
//! each line with vias is routed again, the most vias first, against all the other
//! copper with a via costing [`DEAR_VIA`] more (three times [`VIA`], 45 mm of track), at the
//! usual room and failing that at the least. The new copper is kept only if it has fewer vias,
//! is at most [`VIA_SLACK`] longer (2 mm a via, and no more than 10 % of the line, of 20 mm at
//! least) and lays at most [`VIA_POUR`] (5 mm) more track a via on another net's pour
//! ([`keeps_fewer_vias`]), and it passed the exact checks every search's copper passes. A line
//! that cannot lose a via so is tried once more with the one line its cheapest way crosses
//! ([`Router::contested_lines`]) ripped up too, at most [`VIA_NEIGHBOURS`]: the line first, the
//! other after, both with dear vias, kept only if both route and the two together lose vias as
//! [`keeps_fewer_vias`] weighs them. Left alone ([`Router::may_lose_vias`]): a line to a plane,
//! every line of a net that is a plane or a pour (its vias join it), a decoupling capacitor's
//! via to the pour, a net whose current needs vias side by side, and the nets of a diff pair
//! (the pairs' pass shaped them). A line is never left and a board routes as completely as
//! before. One sweep ([`VIA_SWEEPS`]).
//!
//! **Measured** (2026-11-13, decided by the agent under the owner's delegation; the placed
//! boards of `fab`, one try, `netsmith route --tries 1 --route-timeout 0`; vias, length over the
//! spanning length, other nets' track on the GND pour):
//!
//! | | RP2350 | Orbit | EasyDuino |
//! |---|---|---|---|
//! | main | 142, 1.33x, 432 mm | 94, 1.19x, 302 mm | 105, 1.22x, 508 + 353 mm |
//! | the line alone, no line in its way ripped up | 140 | 92 | 101 |
//! | **and one line in its way (built)** | **134**, 1.30x, 397 mm | **85**, 1.19x, 305 mm | **98**, 1.21x, 505 + 352 mm |
//! | up to 2 or 4 lines in its way | 134, the pass 13 and 24 s | 85 | |
//! | via price 0 more / 1000 more | 138 / 136 | 92 / 88 | 101 / 99 |
//! | three sweeps | 134, 10 to 13 s | 85 | 98 |
//! | 5 mm and 25 % a via | 134 | 83, 317 mm | 96, 514 + 345 mm |
//! | the pour unbounded, 5 mm and 25 % | 117, 506 mm | 80, 366 mm | 96 |
//! | the search pays for the pour too (pour bound as the length's) | | 90 | 99 |
//!
//! The pass takes 6 to 8 s of RP2350's try (75 to 130 s), 5 to 6 s of Orbit's and 2 s of
//! EasyDuino's. Most of the vias that might go would go only by laying track on the GND pour,
//! which the pass off the pours took off it: the two targets of the epic pull against each other
//! here, and a looser pour bound buys Orbit two vias for 12 mm of pour. Of the vias left, the
//! pour's net's own (RP2350 36, Orbit 28, many of them the joins to the pour and the capacitors'
//! vias) and the power nets' are not this pass's to take; the rest need a line that crosses
//! several others to move, which more lines ripped up with it did not find.
//!
//! Robustness (one try at turn costs 24 / 25 / 26 / 27, the default margin and `--margin 1`, main
//! at 804705a4 against this): no board leaves a line, as on main, every run has fewer vias and
//! none is longer: RP2350 142 / 142 / 129 / 129 to 134 / 134 / 127 / 127, and at `--margin 1` 142
//! / 142 / 138 / 157 to 134 / 134 / 132 / 144; Orbit 94 / 94 / 97 / 103 to 85 / 85 / 88 / 99 (101
//! / 101 / 103 / 103 to 97 / 97 / 99 / 99); EasyDuino 105 to 98 (103 to 96). The pairs' meanders
//! come after the pass, and every pair's lengths, skew and coupling are main's. `fab` writes the
//! same board on every run; back to back with main it took 115 / 103 s on RP2350 against 130 / 96
//! s, 20 / 17 s on Orbit against 16 / 13 s and 9 / 10 s on EasyDuino against 7 / 8 s (the first
//! round at a load of 5 to 20, the second at 1 to 2).
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

mod lines;
mod order;
mod pours;
mod realise;
mod rip_up;
mod settle;
mod setup;
mod stages;
mod vias;

mod grid;
mod obstacles;
mod pads;
mod search;

#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod settle_tests;
#[cfg(test)]
mod stage_tests;

pub(crate) use grid::*;
pub(crate) use obstacles::*;
use pads::*;
use search::*;

/// The distance between the cells' centres, in mm.
const PITCH: f64 = 0.1;
/// How much nearer than the rules an exact measurement of finished copper may come, which is
/// only the rounding of the numbers.
pub(crate) const EXACT_SLACK: f64 = 1e-6;
/// Space kept over the rules, which covers the short stub from a pad's centre to the
/// nearest cell.
pub(crate) const MARGIN: f64 = 0.05;
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

/// A negotiation stops after a round that leaves more than twice the lines of the best state it
/// has seen and this many more ([`spreads`]): its rounds spread the lines left instead of
/// trading them, and no later round of the boards measured left fewer.
const SPREAD_BEYOND: usize = 3;

const HISTORY_COST: i64 = 40;
const HISTORY_MOST: u8 = 25;

/// Whether a round of the negotiation that leaves `left` lines, when the best state it has seen
/// leaves `best`, has spread the lines left so far that the negotiation stops (see "What a try
/// costs" in the module's header).
fn spreads(best: usize, left: usize) -> bool {
    left > 2 * best + SPREAD_BEYOND
}

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
const PAIR_NEAR: f64 = netsmith_geom::pairs::COUPLED;
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
/// What a via costs beyond [`VIA`] in the pass that takes vias out ([`Router::fewer_vias`]):
/// three times its usual price in all, 45 mm of track.
const DEAR_VIA: i64 = 300;
/// How much longer, in mm, lines may become in that pass for each via they give up, and at
/// most as a share of their length (of 20 mm at least).
const VIA_SLACK: (f64, f64) = (2.0, 0.1);
/// How much more track, in mm, lines may lay on another net's pour in that pass for each via
/// they give up.
const VIA_POUR: f64 = 5.0;
/// How many lines in a line's way that pass rips up with it, at most.
const VIA_NEIGHBOURS: usize = 1;
/// How many times at most that pass goes over the lines with vias; it stops after a sweep
/// that takes none out.
const VIA_SWEEPS: usize = 1;
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

/// Why a line's copper was not kept: it comes too near other copper, or a right angle has no
/// cut into two bends of 45 degrees that keeps the rules' space.
#[derive(Clone, Copy)]
enum Unsettled {
    Clearance,
    Corner,
}

/// How many times a board's diff pairs are topped up with meanders after the arcs have rounded
/// the bends (and shortened them) of the meanders before.
const MEANDER_TOP_UPS: usize = 3;

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

/// Whether [`Router::fewer_vias`] keeps new copper, from its length in mm, its vias and its
/// track on another net's pour in mm, before and after: fewer vias, no longer than
/// [`VIA_SLACK`] allows and with no more on the pour than [`VIA_POUR`] allows for each via
/// given up.
fn keeps_fewer_vias(before: (f64, usize, f64), after: (f64, usize, f64)) -> bool {
    let fewer = before.1.saturating_sub(after.1) as f64;
    let (per_via, share) = VIA_SLACK;
    let longer = (per_via * fewer).min(share * before.0.max(per_via / share));
    after.1 < before.1
        && after.0 <= before.0 + longer + 1e-6
        && after.2 <= before.2 + VIA_POUR * fewer + 1e-6
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
    /// For each line, the tiles of its corridor on each layer, row by row; `None` for a line
    /// the plan does not steer.
    corridors: Vec<Option<[Vec<bool>; 2]>>,
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
    /// What a via costs beyond the usual ([`Router::fewer_vias`]).
    via_price: i64,
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

/// The longest track from a decoupling capacitor's pad to its own via to the return net's
/// pour, and the nearest another via of the net may be for the pad to need none, in mm
/// ([`Router::to_the_pours`]).
const RETURN_TRACK: f64 = 1.5;

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
    /// How many of `lines` are the ratsnest's, which the tries route; the rest are the
    /// decoupling capacitors' vias to the pours ([`Router::to_the_pours`]).
    routed_lines: usize,
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
    /// What a via costs beyond [`VIA`]: [`DEAR_VIA`] in [`Router::fewer_vias`], else nothing.
    via_price: Cell<i64>,
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
    /// Whether the routed board gets teardrops where its tracks meet pads and vias
    /// (`crate::teardrop`); on by default.
    pub teardrops: bool,
    /// The largest radius of the arc drawn at each bend once the board is routed, in widths of
    /// its track ([`crate::arcs`]); 0 draws none, and the bends keep their corners.
    pub arcs: f64,
    /// Whether the shorter net of each diff pair gets a meander that matches the pair's lengths
    /// (`crate::meander`), on a board routed completely; on by default.
    pub meanders: bool,
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
            teardrops: true,
            arcs: crate::arcs::ARC_WIDTHS,
            meanders: true,
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
    /// The end of the try, after the chain of rip-ups; its tracks are the board's, with the arcs
    /// at its bends.
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
    /// Each diff pair's skew before and after its meanders (`crate::meander`); empty
    /// when the board was left with lines or [`RouteOptions::meanders`] is off.
    pub pairs: Vec<crate::meander::Matched>,
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
                router.routed_lines,
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
    let complete = best.failed.is_empty();
    let mut routed = best.routed;
    routed.sort_by_key(|r| r.line);
    for r in routed {
        board.tracks.extend(r.tracks);
        board.vias.extend(r.vias);
    }
    // Diff pairs matched in length, on a board routed completely (issue #431, S11).
    let meanders = options.meanders && complete;
    let mut pairs = if meanders {
        crate::meander::add(board)
    } else {
        Vec::new()
    };
    // A via that does not carry its net's current gets others beside it (issue #431).
    crate::parallel::parallel_vias(board);
    // Pours are stitched to the other layers of their nets in what the routing left free.
    crate::stitch::stitch(board);
    // Teardrops round the copper the search settled (issue #431, S11).
    if options.teardrops {
        crate::teardrop::add(board);
    }
    // The bends become arcs where they keep the rules' space, clear of the teardrops.
    crate::arcs::round_bends(board, options.arcs);
    // An arc shortens its bend, the meanders' too: a pair the arcs moved off its target is
    // topped up with another meander, whose bends are rounded in turn.
    if meanders && options.arcs > 0.0 {
        for _ in 0..MEANDER_TOP_UPS {
            let again = crate::meander::add(board);
            for (p, a) in pairs.iter_mut().zip(&again) {
                p.after = a.after;
            }
            if again.iter().all(|a| a.after == a.before) {
                break;
            }
            crate::arcs::round_bends(board, options.arcs);
            for (p, skew) in pairs.iter_mut().zip(crate::meander::skews(board)) {
                p.after = skew;
            }
        }
    }
    let mut stages = stages;
    if let Some(last) = stages.last_mut()
        && last.after == SnapshotStage::Final
    {
        // The end is the board's own copper, with its arcs.
        last.tracks.clone_from(&board.tracks);
    }
    Routing {
        left: Ratsnest::of(board),
        stopped,
        tries,
        why,
        crowding,
        stages,
        pairs,
    }
}
