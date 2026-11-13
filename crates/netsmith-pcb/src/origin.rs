//! Where each placed part's footprint origin is, and how far it is from the centre of the
//! part's body (#457).
//!
//! A requirement of the physical file measures the body (ADR 0045: the fabrication drawing,
//! else the courtyard, else the pads and a margin), and a port's parity check compares the
//! footprint's origin. The offset between the two is fixed for a part on one face and turned
//! one way, so a `.place.x` and `.place.y` that put the origin where it must be can be written
//! once from it: `x - dx` and `y - dy`. `fab` lists it for every part, and `netsmith parity`
//! for every part that must match.

use crate::board::{Board, Placed, Side};

/// One part: its footprint's origin and its body's centre, in mm from the outline's top left
/// corner (y down), the frame of the requirements.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PartOrigin {
    pub refdes: String,
    /// The member path of the part (`stage.c1`), the one a requirement names.
    pub path: String,
    /// `top` or `bottom`, as `.place.side` says it.
    pub side: &'static str,
    /// The turn in degrees, as `.place.rotation` says it.
    pub rotation: f64,
    pub origin: [f64; 2],
    pub body_centre: [f64; 2],
    /// The origin minus the body's centre: `.place.x` is the origin's x minus `offset[0]`.
    pub offset: [f64; 2],
}

/// The line before the list, which says how to read it.
pub const ORIGINS_HEAD: &str = "where each footprint's origin is, in mm from the outline's top left corner (y down), and how far it is from the centre of the body that `.place.x` and `.place.y` measure: to put the origin at (x, y) with the part on that face and turned so, write `.place.x &= x - dx` and `.place.y &= y - dy`";

/// A figure to 4 decimals, with no trailing zeros and no `-0`.
pub fn mm(v: f64) -> String {
    let r = (v * 1e4).round() / 1e4;
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

impl PartOrigin {
    pub fn of(part: &Placed, frame: crate::geometry::Pt) -> PartOrigin {
        let tidy = |v: f64| (v * 1e4).round() / 1e4;
        let centre = part.body().centre();
        let origin = [tidy(part.at.x - frame.x), tidy(part.at.y - frame.y)];
        let body_centre = [tidy(centre.x - frame.x), tidy(centre.y - frame.y)];
        PartOrigin {
            refdes: part.refdes.clone(),
            path: part.path.clone(),
            side: match part.side {
                Side::Front => "top",
                Side::Back => "bottom",
            },
            rotation: tidy(part.rotation.rem_euclid(360.0)),
            origin,
            body_centre,
            offset: [
                tidy(origin[0] - body_centre[0]),
                tidy(origin[1] - body_centre[1]),
            ],
        }
    }

    /// `J1 (`slot`) bottom, turned 90: origin (3.45, 19.65), body centre (3.45, 10.15), dx 0
    /// dy 9.5`.
    pub fn line(&self) -> String {
        format!(
            "{} (`{}`) {}, turned {}: origin ({}, {}), body centre ({}, {}), dx {} dy {}",
            self.refdes,
            self.path,
            self.side,
            mm(self.rotation),
            mm(self.origin[0]),
            mm(self.origin[1]),
            mm(self.body_centre[0]),
            mm(self.body_centre[1]),
            mm(self.offset[0]),
            mm(self.offset[1]),
        )
    }
}

impl Board {
    /// Every part's [`PartOrigin`], in the order of the parts.
    pub fn origins(&self) -> Vec<PartOrigin> {
        self.parts
            .iter()
            .map(|p| PartOrigin::of(p, self.outline.min))
            .collect()
    }
}

/// The list as `fab` prints it: [`ORIGINS_HEAD`], then one indented line for each part.
pub fn origins_text(origins: &[PartOrigin]) -> String {
    let mut s = format!("{ORIGINS_HEAD}:\n");
    for o in origins {
        s.push_str("  ");
        s.push_str(&o.line());
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Pt;

    #[test]
    fn a_part_on_the_back_turned_gives_its_origin_its_body_centre_and_the_offset() {
        // Two fingers with no courtyard: in the footprint's frame the body (the pads and 0.25
        // mm) is x -1.6 to 0.6 and y -2.4 to 2.4. On the back x is negated, and turned 90
        // degrees the body is x -2.4 to 2.4 and y -1.6 to 0.6 from the origin: its centre is
        // 0.5 mm above it.
        let mut board = crate::board::tests::placed(&crate::board::tests::led_design());
        let o = board.outline.min;
        let r1 = board.parts.iter_mut().find(|p| p.refdes == "R1").unwrap();
        r1.footprint = crate::footprint::Footprint::parse(
            "T:Fingers",
            r#"(footprint "Fingers"
                (pad "1" connect rect (at 0 0.55) (size 0.7 3.2) (layers "F.Cu" "F.Mask"))
                (pad "2" connect rect (at -1 0) (size 0.7 4.3) (layers "F.Cu" "F.Mask")))"#,
        )
        .unwrap();
        r1.side = Side::Back;
        r1.rotation = 90.0;
        r1.at = o + Pt::new(3.45, 19.65);
        r1.path = "slot".into();
        let found = board.origins();
        let r1 = found.iter().find(|p| p.refdes == "R1").unwrap();
        assert_eq!(
            r1,
            &PartOrigin {
                refdes: "R1".into(),
                path: "slot".into(),
                side: "bottom",
                rotation: 90.0,
                origin: [3.45, 19.65],
                body_centre: [3.45, 19.15],
                offset: [0.0, 0.5],
            }
        );
        assert_eq!(
            r1.line(),
            "R1 (`slot`) bottom, turned 90: origin (3.45, 19.65), body centre (3.45, 19.15), dx 0 dy 0.5"
        );
        let text = origins_text(&found);
        assert!(text.starts_with(&format!("{ORIGINS_HEAD}:\n  ")), "{text}");
        assert!(text.contains(&format!("\n  {}\n", r1.line())), "{text}");
    }
}
