//! A board's fit in its case models (ADR 0109): each `CaseModel` file the design names and
//! [`crate::models::locate`] found is read by netsmith's STEP reader, placed in the scene's
//! frame and measured against the board and its parts' bodies (`netsmith_3d::case`).
//!
//! **Reading.** A file is read as a part's model is ([`crate::models::read`]): once per path
//! and SHA-256 of its bytes, so the editor and a `fab` of several boards read a case once, and
//! a changed file is read again. The B-spline faces of a moulded case are the cost
//! (ADR 0109 measured up to 6 s for one file); the check itself is milliseconds.
//!
//! **What each finding stops** (the owner's answers on #430):
//! - `case_collision` (a body of a stated or catalog height, or the board, in a case solid) is
//!   an error: `fab` refuses, and neither `--allow-drc-violations` nor `--constraints warn`
//!   waives it, as `too_tall` is.
//! - `case_unchecked` (a part of unknown height under the case, a standard height that
//!   collides, a shell that is not closed, a file that is not there) is a warning, and an error
//!   when constraints are strict, as `height_unchecked` is. A file that is there and that the
//!   reader cannot read stops `fab`, as a path outside the project does.
//! - `case_clearance`, `case_reach` and `case_boss` are warnings.
//!
//! The check needs a placement, so `fab`, `board3d` and the editor run it; `build` and `check`
//! place nothing and only say that a file is not there.

use netsmith_3d::case::{self, FitFinding, FitKind, PlacedCase, Subject, Why};
use netsmith_3d::scene3d::Scene;
use netsmith_core::{CaseModel, Design};

/// The case models of a board, read and placed, and what the check found.
#[derive(Debug, Clone, Default)]
pub struct CaseFit {
    /// The models read, in the design's order (by name).
    pub cases: Vec<PlacedCase>,
    /// The files not read first (one that is not there, one the reader cannot read), then
    /// the check's findings.
    pub findings: Vec<FitFinding>,
}

impl CaseFit {
    /// The lines of `fab-notes.txt`: one note per model read, then each finding that does not
    /// stop `fab` as a warning. Nothing for a board with no case model.
    pub fn notes(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.cases.iter().map(note).collect();
        lines.extend(
            self.findings
                .iter()
                .map(|f| format!("warning: {}: {f}", f.kind.word())),
        );
        lines
    }
}

/// The note of one case model read: its file, hash, solids and triangles.
pub fn note(c: &PlacedCase) -> String {
    let open = c.solids.iter().filter(|s| !s.closed).count();
    format!(
        "note: case model `{}` (`{}`, SHA-256 {}): {} solid{}{}, {} triangles; the board and its parts' bodies are checked against it (ADR 0109); the file is the maker's, shared under its license",
        c.name,
        c.file,
        c.sha256,
        c.solids.len(),
        if c.solids.len() == 1 { "" } else { "s" },
        if open > 0 {
            format!(" ({open} not closed)")
        } else {
            String::new()
        },
        c.triangle_count()
    )
}

/// A finding about a case model as a whole.
fn about_model(c: &CaseModel, why: Why) -> FitFinding {
    FitFinding {
        kind: FitKind::Unchecked,
        case: c.name.clone(),
        solid: String::new(),
        subject: Subject::Model,
        why,
        at: [0.0; 3],
    }
}

/// The case models of `design` read and placed. Its `case_models` must have been found on disk
/// ([`crate::models::locate`]): one with no `resolved` file is reported as not there.
pub fn read(design: &Design) -> (Vec<PlacedCase>, Vec<FitFinding>) {
    let mut cases = Vec::new();
    let mut findings = Vec::new();
    for c in &design.features.case_models {
        let Some(path) = &c.resolved else {
            findings.push(about_model(
                c,
                Why::Missing {
                    message: format!(
                        "`{}.file` names `{}` ({}), and there is no such file, so the board's fit in its case model `{}` is not checked; add the file, or leave it out on a machine that may not have it",
                        c.name, c.file, c.source, c.name
                    ),
                },
            ));
            continue;
        };
        let unreadable = |why: String| {
            about_model(
                c,
                Why::Unreadable {
                    message: format!(
                        "`{}.file` names `{}` ({}), and netsmith's STEP reader cannot read it ({why}), so the board's fit in its case model `{}` is not checked: export the case again as AP214 or AP242, or leave the case model out",
                        c.name, c.file, c.source, c.name
                    ),
                },
            )
        };
        // Read as a part's model is, from the same cache (by path and SHA-256 of the bytes).
        match crate::models::read(path) {
            Ok((sha, read)) => cases.push(case::place(
                &c.name, &c.file, &sha, c.figures, &read.mesh, c.rotation, c.offset,
            )),
            Err(e) => findings.push(unreadable(e)),
        }
    }
    (cases, findings)
}

/// The fit of `design`'s board, as `scene` draws it placed, in its case models.
pub fn check(design: &Design, scene: &Scene) -> CaseFit {
    if design.features.case_models.is_empty() {
        return CaseFit::default();
    }
    let (cases, mut findings) = read(design);
    findings.extend(case::check(&scene.board, &scene.bodies, &cases));
    CaseFit { cases, findings }
}
