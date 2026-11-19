//! netsmith's own design rules check (ADR 0037): the gate in front of packaging, and the
//! findings the editor lists, over the board file the writers read. KiCad is not needed;
//! its check (`netsmith_fab::compare_kicad_drc`) is an independent witness that the cross-check compares with.
//!
//! The check reads the same [`Artwork`] the Gerber writers draw from, and the project file
//! beside the board for the numbers (`min_clearance`, `min_track_width`, net classes...), so a
//! board ported from KiCad is checked as KiCad would, and a board netsmith wrote is checked
//! to the manufacturing process it was routed for. A finding is data: a rule (named as KiCad
//! names it, so the two reports compare), a severity, a message with the numbers, a place
//! and the items involved.
//!
//! The fill netsmith makes keeps the clearance, hole clearance and edge clearance of its zone
//! by construction (the fill is "everything but the clearances"), so the copper of a pour is
//! never checked against other copper; a fill the file already holds is not checked against it
//! either. The pour itself is judged by its fill (`starved_thermal`, `isolated_copper` and
//! `copper_sliver`, `pour.rs`, issues #838 and #993): the fill the file holds, or else the fill
//! worked out here. A pour connects the pads, vias and tracks
//! of its net that lie inside its outline. A teardrop (a zone KiCad 9 marks
//! `(attr (teardrop ...))`) is no pour: its fill is copper of its net, checked as a track is,
//! and [`teardrops`] draws the ones a routed board gets (see `teardrop.rs`).
//!
//! # Why the check is here, what it reads, and what it was checked against (ADR 0037)
//!
//! The owner accepted these choices on 2026-11-04. ADR 0034 made the fabrication files netsmith's
//! own and left the check to `kicad-cli`: without KiCad `fab` refused to package, or packaged with
//! a note that nothing had been checked. This check is now the gate in `fab`, `project order` and
//! the editor's Fab button. KiCad's check stays as a witness (`board fab --cross-check`, the tests) and
//! as a second opinion in the editor (the button "Also ask KiCad", through `Project.kicad`).
//!
//! **Where it lives: here, over the `.kicad_pcb` text and the `.kicad_pro` beside it, and not in
//! `netsmith-pcb` over its `Board`.** Three reasons. The package, the Gerbers and the
//! cross-check already work from the file, through one reader ([`crate::artwork`]), so the check
//! sees exactly what the board house will make, and a board ported from KiCad is checked like one
//! netsmith wrote. `netsmith_fab::package` calls the check directly, so the check sits below it,
//! beside `artwork`, the font and the outline it needs (in this crate since issue #611, which the
//! board model and the router share). And KiCad reads the same two files, so the two checks can be compared on identical input.
//! The editor writes the file in memory and checks that. The `Board` model's own
//! `check_placement` (parts, courtyards as boxes, before anything is routed) stays and is not
//! replaced.
//!
//! **The numbers are the project's:** `min_clearance`, `min_track_width`, `min_via_diameter`,
//! `min_via_annular_width`, `min_through_hole_diameter`, `min_hole_to_hole`, `min_hole_clearance`,
//! `min_copper_edge_clearance`, `min_silk_clearance` (between silkscreen and to the edge), the text sizes, each net class's clearance
//! with its net patterns and priority, and each rule's severity. A number the file lacks is KiCad
//! 9's default. The board file's `solder_mask_min_width` is the least mask web (the exporter
//! writes the process's `mask_bridge` there, so KiCad reads it too), and `fab` also passes the
//! process's `via_in_pad` as an extra, because no KiCad file holds it. The process's rules that
//! the project file cannot hold but KiCad's custom rules can go into the `.kicad_dru` beside it
//! (`rules_file.rs`, issue #1185), which a caller that checks a board file from disk reads back
//! ([`Extras::with_rules_file`]), so both checks judge the board to them. The project's
//! `rule_severities` decide what stops `fab`: the exporter writes `silk_over_copper`,
//! `silk_overlap`, `silk_edge_clearance` and `solder_mask_bridge` as warnings (a fabricator clips
//! silkscreen over a mask opening and keeps a web only where it can; neither stops a board being
//! made) and `via_dangling` as a warning (KiCad's own default: it counts a via of a cluster side
//! by side on its top layer alone, so its check of a board `fab` made would otherwise fail on
//! the extra vias, issue #1207), and everything else as errors. The owner confirmed on
//! 2026-11-04 that the four silkscreen and mask rules stay warnings.
//!
//! **Findings are data.** [`check`] returns a `Report` of findings: the rule (named as KiCad
//! names it), the severity, a message with the numbers (`clearance is 0.1000 mm between SMD pad 1
//! [A] of S1 and Via [E]...; the least is 0.1270 mm`), the place in mm and the layer, and the items
//! (what they are, where KiCad would say they are, the part and pad, the net). The package's notes
//! say `design rules check: passed` or `... NOT clean, packaged anyway: N violation(s), M
//! unconnected pad(s), K footprint error(s)` and add `design rules warnings: N`. The editor's
//! Findings tab lists the findings of a routed board with source `drc`; it leaves out the missing
//! connections (the ratsnest's, listed as `unrouted`) and overlapping courtyards (the placement
//! check's). `--allow-drc-violations` lets a package through with errors, and never waives a
//! placement requirement: a `constraint` finding stops `fab` unless `constraints = "warn"` or
//! `--constraints warn` says otherwise (ADR 0045 section 8).
//!
//! **Rules that are not KiCad's** (the cross-check counts them and does not compare them, except
//! the six of the `.kicad_dru`, `pad_to_pad`, `hole_to_track`, `pad_hole_to_hole`, `npth_drill`,
//! `slot_width` and `plated_ring`, which it compares when the file is beside the board):
//! `hole_to_edge` (a hole with no copper against the edge, by the same least as copper; KiCad has
//! no rule), `via_in_pad` (a via in a pad wider than the process fills), `constraint` (a
//! requirement on where a part is, in [`constraints`]; it needs the design, so `fab` and the
//! editor add it to the findings, as an error or, with `constraints = "warn"`, a warning), the
//! rules of the fabricator's page for fiducials and test points (`fiducial_edge`, `fiducial_mask`
//! and `fiducial_keepout`, limits, errors; `fiducial_count`, `fiducial_size`, `fiducial_angle`,
//! `finish_fine_pitch`, `assembly_size` and the `test_point_*` rules, guidelines, warnings:
//! `marks.rs`, issues #761, #1006 and 1285), `acid_trap` (a track
//! that meets a pad's straight edge or another track of its net at an acute angle, a warning:
//! `acid.rs`, issue #838), `stackup` (a board file whose copper layers, thickness or stackup
//! the process does not make, an error: `stackup.rs`, issue #993), `unmatched_waiver` (a waiver of
//! the layout file that covers no finding of a part's own footprint, an error: `waivers.rs`, issue
//! #1196; a finding one covers goes to [`Report::waived`] and stops nothing), and,
//! from the placement check and shown in the editor, `too_tall` (a part with a stated `.height`
//! under a lower height limit of a keepout, ADR 0045 section 11) and `height_unchecked` (a part
//! with no `.height` under a height limit, a warning unless constraints are strict). The other
//! rules are KiCad's by name: clearance, `shorting_items`, `tracks_crossing`, `unconnected_items`
//! (the pieces of each net joined by the shortest connections between them),
//! `track_width`, `via_diameter`, `annular_width`, `drill_out_of_range`, `hole_to_hole`,
//! `hole_clearance`, `copper_edge_clearance`, `courtyards_overlap`, `malformed_courtyard`,
//! `silk_over_copper`, `silk_overlap`, `silk_edge_clearance`, `text_height`, `text_thickness`,
//! `solder_mask_bridge`, `items_not_allowed` (copper and parts in a rule area of the board or of a
//! footprint, by the kinds it forbids; a footprint's rule area forbids its own pads but not the
//! footprint itself, issue #1291),
//! `track_dangling`, `via_dangling`, `invalid_outline`, and, judged by a pour's fill,
//! `starved_thermal`, `isolated_copper` (issue #838) and `copper_sliver` (KiCad's name with
//! netsmith's measure, issue #993), warnings. Geometry is exact for what a board is
//! made of: every shape is a path grown by a radius (a track, a round pad, an oval, a rounded
//! rectangle's polygon) or a solid polygon (custom pads are their anchor and primitives), the gap
//! between two shapes is the least distance between their paths less both radii, and a uniform
//! grid finds the pairs that can be near. A measurement may fall short of its limit by half a
//! micrometre and pass, as in KiCad.
//!
//! **What KiCad taught by comparison, now mirrored.** A pad's angle in the file is absolute and a
//! missing angle is 0 (`artwork` took the footprint's turn; this changed the Gerbers of a board
//! that omits it, as KiCad's do). Two tracks of different nets that cross are `tracks_crossing`,
//! not a short. KiCad reports each layer of a via's clearance but a pair of pads once, as does
//! netsmith. A zero-length missing connection is not drawn, and an open courtyard is reported
//! twice (not closed, self-intersecting). Text is compared with any silkscreen but two drawings
//! only if they do not belong to the same part or both to the board. Mask openings of pads with no
//! net count against pads with one, two copper pads with no net are two nets (a QFN's unconnected
//! pins side by side, issue #1185) where two holes with no plating are none, and openings that only
//! touch are not a bridge when the process asks for no web. A track's place is its start and an arc's its centre, a drawn arc's too. A missing connection's
//! ends are the nearest anchors, and where several are equally near KiCad takes the one on a
//! shared layer, then a track, then the longer (the two checks still pick differently at times,
//! and the comparison counts a missing connection that names other ends as agreeing).
//!
//! **The pour.** A pour joins every pad, via and track of its net that touches its outline on its
//! layer; a zone or a pad set to no connection does not join the pad. What the fill does to a pad
//! is judged by the fill (`pour.rs`, issue #838): `starved_thermal`, a pad fewer than two of
//! whose spokes reach the fill (a spoke into a keepout or another net's clearance is taken out),
//! and `isolated_copper`, an island the zone keeps (of a fill the file holds, any island), and
//! `copper_sliver`, copper of the fill narrower than the least track, all warnings. A zone that
//! holds a fill in the file is judged by that fill. KiCad's `copper_sliver` has thresholds of
//! its own (an angle and a width no fabricator publishes), so the gate can pass a board KiCad
//! flags there. A rule area a library footprint carries is enforced as
//! KiCad 9 enforces it (issue #1291): a track, a via, a pad or a part inside it on its layers where
//! it forbids that kind is `items_not_allowed`; the footprint's own pads are flagged too, as KiCad
//! flags them, and the footprint itself is not (a keepout a part carries in the source is the
//! board's, ADR 0029). A pour's fill is not judged against a rule area, as KiCad's check does not
//! judge it: the fill keeps out of one that forbids pours. A footprint can state `solder_mask_margin`, `solder_paste_margin`,
//! `solder_paste_ratio`, `clearance` and `zone_connect` for all its pads; they are read, and a
//! pad's own wins. A courtyard drawn with lines of no length is closed by the other lines.
//!
//! **What was checked against KiCad's check.** The cross-check compares findings by rule and by the
//! places of the items (0.1 mm), in both directions, and fails on a finding either side lacks. A
//! rule only netsmith has and a kind of finding netsmith does not check (`OUT_OF_SCOPE` in
//! `netsmith_fab::compare_kicad_drc`: library mismatches, slivers) are counted and not
//! compared, and so are starved thermals, isolated copper and slivers on both sides, since
//! netsmith judges its own fill where KiCad judges the fill the file holds (`kicad-cli` cannot
//! refill a zone) and the two measure slivers differently. A corpus of 29 broken boards
//! (`tests/drc_corpus.rs`, generated by `tests/common`): 129 findings agree, none differ, with
//! the places of the items within 0.01 mm (issues #1185 and #1291). The synthetic feature board (every pad
//! shape, turned footprints, slots, arcs, planes, a pour): 20 findings, all agree. The tests of `netsmith-pcb`
//! that ran KiCad's check (`kicad_drc.rs`: the examples routed and unrouted, 33 hard boards, the
//! 70-part synthetic board, land grids with vias in pads, holes, custom pads, keepouts, cutouts,
//! rounded outlines, parts on both faces, wide tracks, four-layer planes) now run this one on the
//! same files: 37 boards, 163 findings agree, none differ (the 193 findings of KiCad's that are
//! left out are about its library tables, because the tests' footprints are not KiCad's). Random
//! boards (`netsmith-pcb`'s `tests/drc.rs`): 100 seeded boards routed by netsmith have no error and
//! the same number of missing connections as the ratsnest; a board of 70 parts, 711 tracks and 161
//! vias is checked in 0.2 s in a debug build. A copy of a real project's board routed by netsmith
//! (nothing of it is in the repository, so no figures from it are kept): no error, and KiCad agrees
//! on the warnings except three (two silkscreen texts that our font puts a hair nearer a pad, and
//! a mask opening drawn as a graphic on `F.Mask`, which was not checked then and is now, issue
//! #1186).
//!
//! **Against KiCad's check, kind by kind** (issue #1185; the table, with what each rule holds, is
//! in the guide's chapter on the commands, _The design rules check against KiCad's_). The same
//! rule, number and places: `clearance`, `shorting_items`, `unconnected_items`, `hole_clearance`,
//! `hole_to_hole`, `drill_out_of_range`, `annular_width`, `track_width`, `via_diameter`,
//! `copper_edge_clearance`, `courtyards_overlap`, `malformed_courtyard`, `text_height`,
//! `text_thickness`, `items_not_allowed`, `track_dangling`, `invalid_outline`, `solder_mask_bridge`,
//! and the six rules of the `.kicad_dru`. `via_dangling` is the same rule counted otherwise:
//! KiCad counts a via or a through-hole pad that a via touches on its top layer alone, so the
//! second via of a cluster side by side (issue #431) dangles to it; here the vias whose copper
//! touches are judged as one, and need copper of the net that is not one of them on two layers
//! (`connect.rs`), and the cross-check leaves KiCad's finding of a via of a cluster out
//! (`netsmith_fab::drop_dangling_vias_of_clusters`). Reported by both and not compared:
//! `tracks_crossing`, `starved_thermal` and `isolated_copper` (each of its own fill),
//! `copper_sliver` (KiCad's thresholds are its own). The silkscreen's `silk_over_copper`,
//! `silk_overlap` and `silk_edge_clearance` are KiCad's rules with the process's numbers
//! (`silk.rs`, issue #1186), a text drawn in netsmith's font: the report lists the pairs the font
//! decides ([`Report::font_dependent`]), which the cross-check does not count as differences.
//! `copper_edge_clearance` and `silk_edge_clearance` near a circle on `Edge.Cuts` are found here
//! within the least and by KiCad 9 only where the two cross. Not wanted:
//! `lib_footprint_mismatch` and `lib_footprint_issues`, which compare each footprint with a library
//! of its name in KiCad's tables, while netsmith writes its own footprints into the board file
//! (ADR 0087), and the comparisons with a schematic, which netsmith writes from the same design.
//!
//! **What is not checked.** Impedance and controlled-impedance rules, acid traps at round
//! copper (`acid.rs` says why), differential pairs and lengths, custom rules a person writes
//! into the `.kicad_dru` (only netsmith's own are read back),
//! blind, buried and micro vias, unrouted-by-design nets, footprint-against-library and schematic parity (KiCad's
//! `lib_footprint_*`, `extra_footprint`), and the electrical rules check (the solver does most of
//! it). Silkscreen text uses netsmith's own font, so a text that only just clears a pad can be
//! judged differently by KiCad ([`FONT_MARGIN`]). The pour categories and the rules above are not built (issue #61).
//! JLCPCB's own check has not been run on a package: the cross-check says netsmith agrees with
//! KiCad, not that a fabricator will accept the board (issue #12; a person must do it, checklist
//! in issue #10).

mod acid;
mod branches;
mod checks;
mod connect;
pub(crate) mod constraints;
mod fab_rules;
mod marks;
mod model;
mod pour;
mod room;
mod rules;
mod rules_file;
pub(crate) mod shape;
mod silk;
mod stackup;
mod teardrop;
mod vias;
mod waivers;

use crate::artwork::Artwork;
use crate::geom::Pt;
pub use model::outline_shapes;
pub use room::Room;
pub use rules::matches_pattern;
pub use rules::{Class, Rules};
pub use rules_file::{PREFIX as RULES_FILE_PREFIX, kicad_dru, rules_file_of};
pub use silk::FONT_MARGIN;
pub use teardrop::{Teardrop, teardrops};
pub use waivers::{PLACE_TOLERANCE as WAIVER_PLACE_TOLERANCE, Waived, Waiver};

/// How bad a finding is. A rule the project file says to ignore produces none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
    /// Only in a project file: the person chose not to check this rule.
    Ignore,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> {
        match s {
            "error" => Some(Severity::Error),
            "warning" => Some(Severity::Warning),
            "ignore" => Some(Severity::Ignore),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Ignore => "ignore",
        }
    }
}

/// A rule of the check. The ids are KiCad's names for the same rule, so that the two
/// checks' reports can be compared; [`Rule::HoleToEdge`] and [`Rule::ViaInPad`] are
/// netsmith's own, which KiCad has no rule for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// Copper of two nets nearer than the clearance: track, pad and via against each other.
    Clearance,
    /// Copper of two nets that touch.
    ShortingItems,
    /// Two tracks of different nets that cross.
    TracksCrossing,
    /// Pads, vias and tracks of one net that no copper joins.
    UnconnectedItems,
    /// A hole nearer than the hole clearance to copper of another net.
    HoleClearance,
    /// Two holes nearer than the least space between holes.
    HoleToHole,
    /// A drilled hole smaller than the process drills.
    DrillOutOfRange,
    /// A via or plated hole with less copper round it than the process leaves.
    AnnularWidth,
    TrackWidth,
    ViaDiameter,
    /// Copper nearer the board edge than the process allows.
    CopperEdgeClearance,
    /// A hole nearer the board edge than the process allows (netsmith's: KiCad has no rule
    /// for a hole with no copper).
    HoleToEdge,
    CourtyardsOverlap,
    MalformedCourtyard,
    /// Silkscreen on a pad's mask opening, where the fabricator clips it.
    SilkOverCopper,
    /// Silkscreen of two parts nearer than the silkscreen clearance.
    SilkOverlap,
    SilkEdgeClearance,
    TextHeight,
    TextThickness,
    /// Two pads of different nets whose mask openings are nearer than the mask web the
    /// process can make.
    SolderMaskBridge,
    /// A track, via, pad or part inside a rule area that forbids it.
    ItemsNotAllowed,
    TrackDangling,
    ViaDangling,
    /// The board outline is not one closed path, or a cutout is not inside it.
    InvalidOutline,
    /// A via sunk into a pad, which the process fills only up to a diameter (netsmith's).
    ViaInPad,
    /// A requirement of the design on where a part goes that the board does not meet
    /// (ADR 0045; netsmith's, and not in the board file: see [`constraints`]).
    Constraint,
    /// A track nearer a hole than the process allows (netsmith's, issue #431).
    HoleToTrack,
    /// Surface-mount pads of different nets nearer than the process allows (netsmith's).
    PadToPad,
    /// A track nearer another net's solder mask opening than the process allows (netsmith's).
    MaskToTrack,
    /// Two holes, one of them a pad's, nearer than the process allows; two vias' holes are
    /// `hole_to_hole` (netsmith's).
    PadHoleToHole,
    /// A slot narrower than the process makes (netsmith's).
    SlotWidth,
    /// A non-plated hole smaller than the process drills (netsmith's).
    NpthDrill,
    /// A plated pad with less copper round its hole than the process leaves (netsmith's; a
    /// via's ring is `annular_width`).
    PlatedRing,
    /// A track narrower than its net's current needs, away from the net's pads (netsmith's,
    /// issue #431).
    TrackCurrent,
    /// A track of a net with an impedance target whose width is not the one that gives the
    /// target on its layer, away from the net's pads (netsmith's, issue #1025): a guideline.
    TrackImpedance,
    /// Fewer vias side by side than a net's current needs (netsmith's, issue #431).
    ViaCurrent,
    /// A corner of 90 degrees or sharper where two tracks of a net meet outside a pad
    /// (netsmith's, issue #415).
    TrackAngle,
    /// A fiducial nearer the board edge than the board house can see it (netsmith's, issue
    /// #761): a limit.
    FiducialEdge,
    /// A fiducial whose copper the solder mask covers (netsmith's): a limit.
    FiducialMask,
    /// Other copper inside a fiducial's mask opening, its keep-out (netsmith's): a limit.
    FiducialKeepout,
    /// Fewer fiducials on a side with assembled parts than the board house asks (netsmith's):
    /// a guideline.
    FiducialCount,
    /// A fiducial that is not the size the board house recommends, or whose mask opening is
    /// narrower than it recommends (netsmith's): a guideline.
    FiducialSize,
    /// The fiducials of one side of a round board that are not three in an L about its centre,
    /// about 90 degrees apart (netsmith's, issue #1006): a guideline.
    FiducialAngle,
    /// A part of ultra-fine pitch on a board finished with HASL, whose surface is uneven
    /// (netsmith's, issue #1006): a guideline.
    FinishFinePitch,
    /// A board smaller than the least the board house assembles as a single board with its
    /// parts on one face, or on both, so it is ordered as a panel (netsmith's, issue 1285): a
    /// guideline.
    AssemblySize,
    /// A test point smaller than the probe pad the board house advises (netsmith's): a
    /// guideline.
    TestPointPad,
    /// Two test points on one side nearer than a probe pitch (netsmith's): a guideline.
    TestPointPitch,
    /// A test point nearer a part's body than advised (netsmith's): a guideline.
    TestPointBody,
    /// A test point nearer the board edge than advised (netsmith's): a guideline.
    TestPointEdge,
    /// A pad that fewer than two of its thermal spokes join to the fill of its pour (issue #838).
    StarvedThermal,
    /// A pad that the fill of its pour can join by none of its thermal spokes and no track, with
    /// the reason (netsmith's, issue #1222).
    UnjoinableThermal,
    /// A piece of a pour's fill that touches no copper of its net, which the zone's island
    /// setting keeps (issue #838).
    IsolatedCopper,
    /// A track that meets a pad's straight edge or another track of its net at an acute angle,
    /// a wedge that traps etchant (netsmith's, issue #838).
    AcidTrap,
    /// Copper of a pour's fill narrower than the least track the process makes, reaching
    /// further than that width from the rest of the fill (issue #993; KiCad's name, netsmith's
    /// measure).
    CopperSliver,
    /// A board file whose copper layers, thickness or stackup the process does not make
    /// (netsmith's, issue #993).
    Stackup,
    /// A blind or a buried via between layers the process does not join with one, or whose hole
    /// or copper the process does not make for it (netsmith's, issue #1015).
    ViaSpan,
    /// A via deeper for its hole than the process drills: a blind via through a dielectric
    /// thicker than its aspect ratio allows, a drilled via through more (netsmith's, issue
    /// #1015).
    ViaAspectRatio,
    /// A waiver of the layout file that covers no finding of the part's own footprint
    /// (netsmith's, issue #1196; `waivers.rs`).
    UnmatchedWaiver,
}

impl Rule {
    pub const ALL: [Rule; 58] = [
        Rule::Clearance,
        Rule::ShortingItems,
        Rule::TracksCrossing,
        Rule::UnconnectedItems,
        Rule::HoleClearance,
        Rule::HoleToHole,
        Rule::DrillOutOfRange,
        Rule::AnnularWidth,
        Rule::TrackWidth,
        Rule::ViaDiameter,
        Rule::CopperEdgeClearance,
        Rule::HoleToEdge,
        Rule::CourtyardsOverlap,
        Rule::MalformedCourtyard,
        Rule::SilkOverCopper,
        Rule::SilkOverlap,
        Rule::SilkEdgeClearance,
        Rule::TextHeight,
        Rule::TextThickness,
        Rule::SolderMaskBridge,
        Rule::ItemsNotAllowed,
        Rule::TrackDangling,
        Rule::ViaDangling,
        Rule::InvalidOutline,
        Rule::ViaInPad,
        Rule::Constraint,
        Rule::HoleToTrack,
        Rule::PadToPad,
        Rule::MaskToTrack,
        Rule::PadHoleToHole,
        Rule::SlotWidth,
        Rule::NpthDrill,
        Rule::PlatedRing,
        Rule::TrackCurrent,
        Rule::TrackImpedance,
        Rule::ViaCurrent,
        Rule::TrackAngle,
        Rule::FiducialEdge,
        Rule::FiducialMask,
        Rule::FiducialKeepout,
        Rule::FiducialCount,
        Rule::FiducialSize,
        Rule::FiducialAngle,
        Rule::FinishFinePitch,
        Rule::AssemblySize,
        Rule::TestPointPad,
        Rule::TestPointPitch,
        Rule::TestPointBody,
        Rule::TestPointEdge,
        Rule::StarvedThermal,
        Rule::UnjoinableThermal,
        Rule::IsolatedCopper,
        Rule::AcidTrap,
        Rule::CopperSliver,
        Rule::Stackup,
        Rule::ViaSpan,
        Rule::ViaAspectRatio,
        Rule::UnmatchedWaiver,
    ];

    /// KiCad's name for the rule (its `type` in a JSON report).
    pub fn id(self) -> &'static str {
        match self {
            Rule::Clearance => "clearance",
            Rule::ShortingItems => "shorting_items",
            Rule::TracksCrossing => "tracks_crossing",
            Rule::UnconnectedItems => "unconnected_items",
            Rule::HoleClearance => "hole_clearance",
            Rule::HoleToHole => "hole_to_hole",
            Rule::DrillOutOfRange => "drill_out_of_range",
            Rule::AnnularWidth => "annular_width",
            Rule::TrackWidth => "track_width",
            Rule::ViaDiameter => "via_diameter",
            Rule::CopperEdgeClearance => "copper_edge_clearance",
            Rule::HoleToEdge => "hole_to_edge",
            Rule::CourtyardsOverlap => "courtyards_overlap",
            Rule::MalformedCourtyard => "malformed_courtyard",
            Rule::SilkOverCopper => "silk_over_copper",
            Rule::SilkOverlap => "silk_overlap",
            Rule::SilkEdgeClearance => "silk_edge_clearance",
            Rule::TextHeight => "text_height",
            Rule::TextThickness => "text_thickness",
            Rule::SolderMaskBridge => "solder_mask_bridge",
            Rule::ItemsNotAllowed => "items_not_allowed",
            Rule::TrackDangling => "track_dangling",
            Rule::ViaDangling => "via_dangling",
            Rule::InvalidOutline => "invalid_outline",
            Rule::ViaInPad => "via_in_pad",
            Rule::Constraint => "constraint",
            Rule::HoleToTrack => "hole_to_track",
            Rule::PadToPad => "pad_to_pad",
            Rule::MaskToTrack => "mask_to_track",
            Rule::PadHoleToHole => "pad_hole_to_hole",
            Rule::SlotWidth => "slot_width",
            Rule::NpthDrill => "npth_drill",
            Rule::PlatedRing => "plated_ring",
            Rule::TrackCurrent => "track_current",
            Rule::TrackImpedance => "track_impedance",
            Rule::ViaCurrent => "via_current",
            Rule::TrackAngle => "track_angle",
            Rule::FiducialEdge => "fiducial_edge",
            Rule::FiducialMask => "fiducial_mask",
            Rule::FiducialKeepout => "fiducial_keepout",
            Rule::FiducialCount => "fiducial_count",
            Rule::FiducialSize => "fiducial_size",
            Rule::FiducialAngle => "fiducial_angle",
            Rule::FinishFinePitch => "finish_fine_pitch",
            Rule::AssemblySize => "assembly_size",
            Rule::TestPointPad => "test_point_pad",
            Rule::TestPointPitch => "test_point_pitch",
            Rule::TestPointBody => "test_point_body",
            Rule::TestPointEdge => "test_point_edge",
            Rule::StarvedThermal => "starved_thermal",
            Rule::UnjoinableThermal => "unjoinable_thermal",
            Rule::IsolatedCopper => "isolated_copper",
            Rule::AcidTrap => "acid_trap",
            Rule::CopperSliver => "copper_sliver",
            Rule::Stackup => "stackup",
            Rule::ViaSpan => "via_span",
            Rule::ViaAspectRatio => "via_aspect_ratio",
            Rule::UnmatchedWaiver => "unmatched_waiver",
        }
    }

    /// Whether the rule is netsmith's own, which KiCad has no rule for: the cross-check counts
    /// its findings and does not compare them.
    pub fn is_netsmiths(self) -> bool {
        matches!(
            self,
            Rule::HoleToEdge
                | Rule::ViaInPad
                | Rule::Constraint
                | Rule::HoleToTrack
                | Rule::PadToPad
                | Rule::MaskToTrack
                | Rule::PadHoleToHole
                | Rule::SlotWidth
                | Rule::NpthDrill
                | Rule::PlatedRing
                | Rule::TrackCurrent
                | Rule::TrackImpedance
                | Rule::ViaCurrent
                | Rule::TrackAngle
                | Rule::FiducialEdge
                | Rule::FiducialMask
                | Rule::FiducialKeepout
                | Rule::FiducialCount
                | Rule::FiducialSize
                | Rule::FiducialAngle
                | Rule::FinishFinePitch
                | Rule::AssemblySize
                | Rule::TestPointPad
                | Rule::TestPointPitch
                | Rule::TestPointBody
                | Rule::TestPointEdge
                | Rule::AcidTrap
                | Rule::UnjoinableThermal
                | Rule::Stackup
                | Rule::ViaSpan
                | Rule::ViaAspectRatio
                | Rule::UnmatchedWaiver
        )
    }

    pub fn from_id(id: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|r| r.id() == id)
    }

    /// How a broken rule is reported when the project file does not say. KiCad's own
    /// defaults, except that what only the silkscreen suffers is a warning.
    pub fn default_severity(self) -> Option<Severity> {
        Some(match self {
            Rule::SilkOverCopper
            | Rule::SilkOverlap
            | Rule::SilkEdgeClearance
            | Rule::TextHeight
            | Rule::TextThickness
            | Rule::TrackDangling
            | Rule::ViaDangling
            | Rule::ViaInPad
            | Rule::TrackImpedance
            | Rule::FiducialCount
            | Rule::FiducialSize
            | Rule::FiducialAngle
            | Rule::FinishFinePitch
            | Rule::AssemblySize
            | Rule::TestPointPad
            | Rule::TestPointPitch
            | Rule::TestPointBody
            | Rule::TestPointEdge
            | Rule::StarvedThermal
            | Rule::UnjoinableThermal
            | Rule::IsolatedCopper
            | Rule::AcidTrap
            | Rule::CopperSliver => Severity::Warning,
            _ => Severity::Error,
        })
    }
}

/// One of the things a finding is about.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// What it is, as a person would say it: `Pad 1 [VIN] of C1 on F.Cu`.
    pub what: String,
    /// Where KiCad would say it is (a pad's or via's centre, a track's start, a part's
    /// origin).
    pub at: Pt,
    /// The part it belongs to, and the pad.
    pub part: Option<String>,
    pub pad: Option<String>,
    pub net: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub rule: Rule,
    pub severity: Severity,
    /// What is wrong, with the numbers: `clearance 0.100 mm is less than the 0.127 mm...`.
    pub message: String,
    /// Where the problem is, in the board's frame (mm, y down).
    pub at: Pt,
    pub layer: Option<String>,
    pub items: Vec<Item>,
    /// For a `track_current` or `via_current` finding: the copper, the current assumed and what
    /// it needs (issue 1373).
    pub carrying: Option<Carrying>,
}

/// The numbers of a `track_current` or `via_current` finding (issue 1373): what the copper is,
/// the current it was held to and where that comes from, and what that current needs, so that a
/// person can choose between wider copper and a lower current.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Carrying {
    /// The current assumed, in amperes: the net's (`current_from` says which loads) or, for
    /// copper that only feeds branches, what those branches draw.
    pub current: Option<f64>,
    /// Where `current` comes from, in words; empty when it is not known.
    pub current_from: String,
    /// A track's width and the least width the current needs on its layer, in mm.
    pub width: Option<f64>,
    pub needed_width: Option<f64>,
    /// The vias side by side a cluster has and the number the current needs.
    pub vias: Option<usize>,
    pub needed_vias: Option<usize>,
    /// What the sizing is based on, as the design stage sizes copper (ADR 0036): the copper's
    /// thickness in mm, the temperature rise in kelvin and the fabricator's width tolerance
    /// (0.2 for 20 %).
    pub copper: Option<f64>,
    pub rise: Option<f64>,
    pub tolerance: Option<f64>,
}

impl Finding {
    /// The parts the finding names, in order, once each.
    pub fn parts(&self) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for p in self.items.iter().filter_map(|i| i.part.clone()) {
            if !parts.contains(&p) {
                parts.push(p);
            }
        }
        parts
    }
}

impl Finding {
    /// The message and, after it, the items the finding is about.
    pub fn with_items(&self) -> String {
        if self.items.is_empty() {
            return self.message.clone();
        }
        let items: Vec<&str> = self.items.iter().map(|i| i.what.as_str()).collect();
        format!("{}: {}", self.message, items.join("; "))
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{}] {} at ({:.3}, {:.3})",
            self.severity.word(),
            self.rule.id(),
            self.message,
            self.at.x,
            self.at.y
        )?;
        if let Some(l) = &self.layer {
            write!(f, " on {l}")?;
        }
        if !self.items.is_empty() {
            let items: Vec<&str> = self.items.iter().map(|i| i.what.as_str()).collect();
            write!(f, ": {}", items.join("; "))?;
        }
        // A finding the board cannot change, which the layout file may waive (issue #1196).
        if let Some(part) = self.own_part() {
            write!(f, " (within {part}'s own footprint)")?;
        }
        Ok(())
    }
}

/// The counts a package's notes give: the wording of KiCad's report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Errors that are not a missing connection or a malformed footprint.
    pub violations: u32,
    /// Connections that are missing.
    pub unconnected: u32,
    /// Footprints with a courtyard that is not a closed shape.
    pub footprint_errors: u32,
}

impl Counts {
    pub fn total(self) -> u32 {
        self.violations + self.unconnected + self.footprint_errors
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    /// Errors first, then warnings, each in the order the rules ran.
    pub findings: Vec<Finding>,
    /// The pairs of the silkscreen rules with a text in them whose verdict the font decides
    /// (issue #1186): found or not, KiCad's check, which draws KiCad's font, may judge them
    /// otherwise. Not findings; the cross-check reads them.
    pub font_dependent: Vec<FontDependent>,
    /// The findings of a part's own footprint that a waiver of the layout file covers, with its
    /// reason and source (issue #1196): neither errors nor warnings, and listed as waived.
    pub waived: Vec<Waived>,
}

/// A text of the silkscreen and what a silkscreen rule holds it to (an opening in the mask,
/// other silkscreen, the edge), near enough to the rule's least that the font decides whether
/// it is a finding: within [`FONT_MARGIN`] of the text's size of the box round its ink.
#[derive(Debug, Clone, PartialEq)]
pub struct FontDependent {
    pub rule: Rule,
    /// Where KiCad says the two things are, as a finding of the rule names them.
    pub places: Vec<Pt>,
}

impl Report {
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
    }

    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for f in self.errors() {
            match f.rule {
                Rule::UnconnectedItems => c.unconnected += 1,
                Rule::MalformedCourtyard => c.footprint_errors += 1,
                _ => c.violations += 1,
            }
        }
        c
    }

    pub fn is_clean(&self) -> bool {
        self.errors().next().is_none()
    }

    /// How many findings of a rule there are, of any severity.
    pub fn count(&self, rule: Rule) -> usize {
        self.findings.iter().filter(|f| f.rule == rule).count()
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for finding in &self.findings {
            writeln!(f, "{finding}")?;
        }
        for waived in &self.waived {
            writeln!(f, "{waived}")?;
        }
        Ok(())
    }
}

/// What the check needs to know that the board file and its project do not say. All of it
/// comes from the manufacturing process the board was made for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Extras {
    /// The least solder mask between two openings of pads of different nets, if the board's
    /// setup does not say (`solder_mask_min_width`).
    pub mask_web: Option<f64>,
    /// The widest via the process fills and caps, so that it may sit in a pad; a wider one
    /// in a pad is a warning. `None` is not checked.
    pub via_in_pad: Option<f64>,
    /// Pairs of parts, by designator, whose courtyards may overlap because one stands over
    /// the other's pins, which the design cuts short after soldering and which the rule of
    /// `netsmith_pcb::judge_protrusion` excuses (ADR 0045 section 15). The check does not judge that
    /// rule; the caller that has the design does, and says which pairs it excused.
    pub excused_courtyards: Vec<(String, String)>,
    /// The process's rules that a KiCad project file cannot say (issue #431), each `None` when it
    /// is not checked: the least space from a via's or a non-plated hole to a track, and from a
    /// plated pad's hole to a track, between
    /// surface-mount pads of different nets and from a solder mask opening to a track of another
    /// net; the narrowest plated and non-plated slot; the smallest non-plated hole; and the
    /// least copper round a plated pad's hole.
    pub hole_to_track: Option<f64>,
    pub plated_hole_clearance: Option<f64>,
    pub pad_to_pad: Option<f64>,
    /// The least space between two holes when one is a pad's (the project file's
    /// `min_hole_to_hole` is the vias' figure).
    pub pad_hole_to_hole: Option<f64>,
    pub mask_to_track: Option<f64>,
    pub plated_slot: Option<f64>,
    pub npth_slot: Option<f64>,
    pub npth_drill: Option<f64>,
    pub pad_ring: Option<f64>,
    /// The least space from a silkscreen text to an opening in the solder mask (the process's
    /// `.min_silk_to_pad`: JLCPCB removes a character within it, issue #1186); `None` is KiCad's,
    /// the ink touching the opening, which a drawing is always held to.
    pub text_to_mask: Option<f64>,
    /// The least width of the nets whose current needs more than the process's least track,
    /// by net (issue #431), and how near one of the net's pads a track may be narrower (the
    /// neck-down out of a fine-pitch pad). Empty when the currents are not known.
    pub least_widths: Vec<(String, f64)>,
    /// The same on an inner signal layer, whose thinner copper and IPC-2221's inner constant
    /// need a wider track (issue #833). A net not here is held to its `least_widths` entry on
    /// an inner layer too.
    pub inner_least_widths: Vec<(String, f64)>,
    /// The width a net's tracks are held to on a layer, by net and layer name (`F.Cu`): the
    /// width that gives the net's impedance target there (issue #1025). A track that is not that
    /// wide, away from the net's pads as for `least_widths`, is a `track_impedance` warning.
    pub target_widths: Vec<(String, String, f64)>,
    pub neck_reach: f64,
    /// How many vias side by side, their copper touching, each net needs where it changes layer,
    /// for the nets that need more than one.
    pub via_counts: Vec<(String, usize)>,
    /// Whether a corner of 90 degrees or sharper between two tracks is refused (issue #415):
    /// set by `fab` for the boards netsmith routes, which bend by 45 degrees only.
    pub track_angle: bool,
    /// The least angle, in degrees, between a track's centre line and the tangent of a round
    /// copper edge where the track leaves it (`acid_trap`, netsmith's own rule: no fabricator
    /// publishes one); a smaller angle is an acid trap. `None`, as when the board states none, is
    /// that round copper (a round pad or via, an oval's end, a rounded corner, a custom pad's
    /// circle or arc) is not judged.
    pub round_track_angle: Option<f64>,
    /// The fabricator's rules for fiducials and test points (issue #761); `None` is not
    /// checked.
    pub marks: Option<Marks>,
    /// The stackup the process makes (issue #993); `None` is not checked.
    pub stackup: Option<Stackup>,
    /// The vias the process makes (issue #1015); `None` is not checked.
    pub vias: Option<ViaRules>,
    /// The pins of parts that state the current of their branch (issue #1113), and how copper
    /// is sized for a current on the board. A track narrower than its net's current needs, or a
    /// via cluster smaller than it needs, passes where it only feeds such branches: its net's
    /// copper without it falls apart into pieces of which one alone has a pad of no branch or a
    /// pour, and it carries what it needs for the branches of the others. Not checked without
    /// `sizing`.
    pub branches: Vec<Branch>,
    /// The pins whose current the model knows, below their net's (issue #1333): a feedback
    /// divider's top on a rail of amps. Read as `branches` are, after them: a pad of a pin in
    /// both is the stated branch's, and a pad in neither is of the trunk.
    pub pin_currents: Vec<Branch>,
    pub sizing: Option<crate::ipc2221::TrackSizing>,
    /// The current each net carries where it is known, with where it comes from (issue 1373),
    /// for the words of a `track_current` and a `via_current` finding.
    pub net_currents: Vec<NetCurrent>,
    /// The layout file's waivers of findings of a part's own footprint (issue #1196, `waivers.rs`):
    /// a finding one covers is moved to [`Report::waived`], and one that covers none is an error.
    pub waivers: Vec<Waiver>,
}

/// A pin of a part that states the current of its branch (issue #1113): its pads, by the
/// footprint's designator and the pad numbers, and the most it carries, in amperes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Branch {
    pub net: String,
    /// The pin's path in the design (`ldo.bias`).
    pub path: String,
    pub part: String,
    pub pads: Vec<String>,
    pub current: f64,
}

/// The current a net carries (issue 1373): in amperes, the worst case, and in words where it
/// comes from (the loads that add up to it).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NetCurrent {
    pub net: String,
    pub current: f64,
    pub source: String,
}

/// What the check needs to judge a board's vias against the process (the `via_span` and
/// `via_aspect_ratio` rules, issue #1015). Lengths are in mm.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViaRules {
    /// The blind and buried vias the process makes: the first and last layer of each, as places
    /// of the board's copper layers from the top (the upper first), and whether it is drilled by
    /// laser. Empty for a process that makes through vias only.
    pub spans: Vec<(usize, usize, bool)>,
    /// The thickness of each dielectric layer between two copper layers, from the top: what a
    /// via is drilled through. Empty where the build-up is not known, and then no aspect ratio
    /// is judged.
    pub dielectrics: Vec<f64>,
    /// The laser-drilled holes the process makes, the least copper round a blind or a buried
    /// via's hole, and a buried via's largest hole.
    pub min_laser_drill: f64,
    pub max_laser_drill: f64,
    pub min_ring: f64,
    pub max_buried_drill: f64,
    /// The most a blind via's dielectric may be thick for its hole, and a drilled via's depth
    /// for its hole; 0 is not judged.
    pub max_laser_aspect: f64,
    pub max_drill_aspect: f64,
}

/// What the check needs to judge a board file's stackup (the `stackup` rule, issue #993): the
/// process's copper layers and the thicknesses it makes, in mm.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stackup {
    /// The copper layers of a board of the process.
    pub layers: usize,
    /// The least and the most thickness of the board the process makes (one value once the
    /// board has chosen).
    pub thickness: (f64, f64),
    /// How far the layers of the file's stackup may add up from the board's thickness: the
    /// fabricator's tolerance of the thickness.
    pub tolerance: f64,
}

/// What the check needs to judge the fiducials and the test points of a board: which footprints
/// they are (the design knows their kinds; the board file does not), whether the board house
/// assembles parts on each side, and the process's numbers. Lengths are in mm.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marks {
    /// The designators of the fiducials and of the test points.
    pub fiducials: Vec<String>,
    pub test_points: Vec<String>,
    /// Whether the board house places parts on the front, and on the back.
    pub assembled_front: bool,
    pub assembled_back: bool,
    /// The fewest fiducials a side with assembled parts has.
    pub min_fiducials: usize,
    /// The diameter of a fiducial's copper, and the least ratio of its mask opening to it.
    pub fiducial_diameter: f64,
    pub fiducial_mask_ratio: f64,
    /// The least space from a fiducial's copper to the board edge.
    pub min_fiducial_edge: f64,
    /// The smallest test point (the smaller side of its pad), the least distance between the
    /// centres of two on a side, the least space from its copper to the body of a part on its
    /// side, and to the board edge.
    pub min_test_point_pad: f64,
    pub min_test_point_pitch: f64,
    pub min_test_point_body: f64,
    pub min_test_point_edge: f64,
    /// The nets that are a supply or a ground, and the least width of the pad of a test point on
    /// one (above `min_test_point_pad` where it is larger).
    pub power_nets: Vec<String>,
    pub min_power_test_point_pad: f64,
    /// The parts of known height above the height the process calls tall, with the height; a
    /// part of unknown height is not here and is judged as any other. The least space from a
    /// test point's copper to the body of one of them (above `min_test_point_body` where it is
    /// larger).
    pub tall_parts: Vec<(String, f64)>,
    pub tall_part_height: f64,
    pub min_test_point_tall_body: f64,
    /// The angle between neighbouring fiducials about the centre of a round board, in degrees;
    /// 0 is not checked.
    pub round_angle: f64,
    /// The pitch at or under which a part is ultra-fine-pitch, in mm; 0 is not checked.
    pub fine_pitch: f64,
    /// The finish when it leaves an uneven surface that is harder for the board house's vision
    /// system to read (the HASL finishes): `None` for a flat finish.
    pub uneven_finish: Option<String>,
    /// The least side of a single board with assembled parts on one face, and on both, in mm
    /// (issue 1285): `None` is not judged.
    pub min_single_sided_assembly_size: Option<f64>,
    pub min_double_sided_assembly_size: Option<f64>,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Board(crate::artwork::ArtworkError),
    Project(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Board(e) => write!(f, "{e}"),
            Error::Project(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<crate::artwork::ArtworkError> for Error {
    fn from(e: crate::artwork::ArtworkError) -> Self {
        Error::Board(e)
    }
}

/// Checks the board in `pcb` (the text of a `.kicad_pcb`) to the rules in `project` (the
/// text of the `.kicad_pro` beside it; KiCad's defaults without one).
pub fn check(pcb: &str, project: Option<&str>, extras: &Extras) -> Result<Report, Error> {
    let art = Artwork::parse(pcb)?;
    let mut rules = match project {
        Some(p) => Rules::from_project(p).map_err(Error::Project)?,
        None => Rules::default(),
    };
    // The board's own setup states the mask web the process can make; the caller may too.
    let setup_web = netsmith_sexp::Sexp::parse(pcb)
        .ok()
        .and_then(|root| {
            root.find("setup")?
                .find("solder_mask_min_width")?
                .items()
                .get(1)?
                .text()?
                .parse::<f64>()
                .ok()
        })
        .or(extras.mask_web);
    rules.mask_web = setup_web.unwrap_or(0.0);
    let bridges_in_footprints = netsmith_sexp::Sexp::parse(pcb)
        .ok()
        .and_then(|root| {
            root.find("setup")?
                .find("allow_soldermask_bridges_in_footprints")?
                .items()
                .get(1)?
                .text()
                .map(|t| t == "yes")
        })
        .unwrap_or(false);
    Ok(check_artwork(&art, &rules, extras, bridges_in_footprints))
}

/// Checks a board already read, to rules already known.
pub fn check_artwork(
    art: &Artwork,
    rules: &Rules,
    extras: &Extras,
    bridges_in_footprints: bool,
) -> Report {
    let model = model::Model::new(art, rules, extras, bridges_in_footprints);
    let mut findings = Vec::new();
    checks::run(&model, &mut findings);
    connect::run(&model, &mut findings);
    // Errors before warnings, otherwise in the order found.
    findings.sort_by_key(|f| std::cmp::Reverse(f.severity));
    let mut report = Report {
        findings,
        font_dependent: silk::font_dependent(&model),
        waived: Vec::new(),
    };
    waivers::apply(&mut report, &extras.waivers);
    report
}

/// The `.kicad_pro` beside a board file, if there is one.
pub fn project_of(pcb: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(pcb.with_extension("kicad_pro")).ok()
}
