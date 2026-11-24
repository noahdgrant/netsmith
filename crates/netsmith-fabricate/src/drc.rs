//! `netsmith board drc` (issue #1095): netsmith's own design rules check (ADR 0037) of a board
//! as it stands, with nothing routed and no package written.
//!
//! It is the check `fab` runs after routing, on the files `fab` writes, called alone: the same
//! `.kicad_pcb` and `.kicad_pro` text of the board ([`netsmith_pcb::kicad_pcb`]), the same extras
//! ([`crate::fab::drc_extras`]) and the same requirements of the design on where parts go
//! ([`netsmith_pipeline::constraints::of_design`]), through [`netsmith_fab::check`], which
//! `netsmith_fab::package` calls too. The board is the placed one with the routes file's copper
//! on it ([`crate::routes::apply_beside`]), as `fab` has it before the router: a connection the
//! file does not make is a finding, as it is for `fab` when the router leaves it.

use std::path::Path;

use netsmith_fab::{Checked, Options};
use netsmith_geom::{Constraints, DrcBoardRules, DrcRuleArea};
use netsmith_pcb::{Board, kicad_pcb, kicad_pro};

/// The check of `board` as it is, with the design's `constraints` (`None` when it states none).
/// With `strict_guidelines` (`guidelines = "error"` in netsmith.toml), a guideline of the check
/// that `fab --strict` makes an error is one here too (`pad_island_cut`, issue #1509).
pub fn of_board(
    board: &Board,
    constraints: Option<Constraints>,
    strict_guidelines: bool,
) -> Result<Checked, String> {
    let mut options = options_of(board, constraints);
    options.drc.strict_guidelines = strict_guidelines;
    netsmith_fab::check(&kicad_pcb(board), Some(&kicad_pro(board)), &options)
        .map_err(|e| e.to_string())
}

/// The options [`of_board`] checks `board` with.
fn options_of(board: &Board, constraints: Option<Constraints>) -> Options {
    Options {
        drc: crate::fab::drc_extras(board),
        constraints,
        pairs: board.diff_pairs_with_skew(),
        length_groups: board.length_groups.clone(),
        track_paths: board.track_paths.clone(),
        track_path_losses: board
            .track_paths
            .iter()
            .map(|p| board.track_path_loss(p))
            .collect(),
        regulators: board.switching_regulators.clone(),
        switch_node_keepout: board.process.switch_node_keepout,
        replaced_pours: replaced_pours(board),
        ..Options::default()
    }
}

/// The pours and planes of `board`'s process that a zone of the routes file replaces on their
/// layer (issue #1612, [`Board::replaced`]), each with the zones that do: what `board drc` and
/// `fab` note as `pour_replaced`. The board file holds the zone and no pour, so the check is
/// told.
pub fn replaced_pours(board: &Board) -> Vec<netsmith_fab::replaced_pour::ReplacedPour> {
    use netsmith_fab::replaced_pour::ReplacedPour;
    let number = |v: f64| {
        let text = format!("{v:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let pours = board
        .pours
        .iter()
        .map(|p| (p.layer.name().to_string(), p.net.clone(), "pour"));
    let planes = board
        .planes
        .iter()
        .map(|p| (p.layer.clone(), p.net.clone(), "plane"));
    pours
        .chain(planes)
        .filter(|(layer, net, _)| board.replaced(layer, net))
        .map(|(layer, net, kind)| {
            let zones: Vec<&netsmith_pcb::BoardZone> = board
                .zones
                .iter()
                .filter(|z| z.net == net && z.layers.contains(&layer))
                .collect();
            let describe = |z: &netsmith_pcb::BoardZone| {
                let (mut lo, mut hi) = (z.outline[0], z.outline[0]);
                for p in &z.outline {
                    (lo.x, lo.y) = (lo.x.min(p.x), lo.y.min(p.y));
                    (hi.x, hi.y) = (hi.x.max(p.x), hi.y.max(p.y));
                }
                let (x, y) = (lo.x - board.outline.min.x, lo.y - board.outline.min.y);
                format!(
                    "{},{},{},{}",
                    number(x),
                    number(y),
                    number(hi.x - lo.x),
                    number(hi.y - lo.y),
                )
            };
            ReplacedPour {
                at: {
                    let p = zones
                        .first()
                        .and_then(|z| z.outline.first().copied())
                        .unwrap_or(board.outline.min);
                    netsmith_geom::Pt::new(p.x, p.y)
                },
                zones: zones.iter().map(|z| describe(z)).collect(),
                layer,
                net,
                kind,
            }
        })
        .collect()
}

/// The rules [`of_board`] holds `board` to, as data (`netsmith board rules`, issue #1513): read
/// from the same board file, project file and extras, by the function the check reads them with
/// ([`netsmith_geom::drc_rules_of`]).
pub fn rules_of_board(board: &Board) -> Result<DrcBoardRules, String> {
    let options = options_of(board, None);
    netsmith_geom::drc_rules_of(&kicad_pcb(board), Some(&kicad_pro(board)), &options.drc)
        .map_err(|e| e.to_string())
}

/// The rule areas of `board` that a picture of it draws (issue #1543, `netsmith view pcb` and
/// the editor's Rule areas layer): the `rule_areas` of [`rules_of_board`], the footprints' (a
/// library's keep-out, an RF module's antenna area) and the board's, less the keepouts of the
/// physical file, which the picture and the 2D view already draw as keepouts. A board rule area
/// is such a keepout when it has the name of one of [`Board::keepout_areas`] that forbids
/// something.
pub fn rule_areas_to_draw(board: &Board) -> Result<Vec<DrcRuleArea>, String> {
    let keepouts: Vec<String> = board
        .keepout_areas()
        .into_iter()
        .filter(|k| k.forbids_anything())
        .map(|k| k.name)
        .collect();
    Ok(rules_of_board(board)?
        .rule_areas
        .into_iter()
        .filter(|a| a.part.is_some() || !keepouts.contains(&a.name))
        .collect())
}

/// The rules [`of_kicad_pcb`] holds the KiCad board `pcb` to, as data.
pub fn rules_of_kicad_pcb(pcb: &Path) -> Result<DrcBoardRules, String> {
    let (src, options) = kicad_options(pcb)?;
    netsmith_geom::drc_rules_of(
        &src,
        netsmith_geom::drc_project_of(pcb).as_deref(),
        &options.drc,
    )
    .map_err(|e| format!("{}: {e}", pcb.display()))
}

/// The text of the KiCad board `pcb` and the options it is checked with: the rules of the
/// `.kicad_dru` beside it.
fn kicad_options(pcb: &Path) -> Result<(String, Options), String> {
    let src =
        std::fs::read_to_string(pcb).map_err(|e| format!("cannot read {}: {e}", pcb.display()))?;
    let mut options = Options::default();
    if let Some(text) = netsmith_geom::drc_rules_file_of(pcb) {
        options.drc = options.drc.with_rules_file(&text);
    }
    Ok((src, options))
}

/// The check of the KiCad board `pcb`, with the `.kicad_pro` beside it when there is one, and
/// the process's rules of the `.kicad_dru` that `fab` writes beside it (issue #1185: the space
/// between surface-mount pads, from holes to tracks, between pad holes, the least non-plated
/// hole and slot, the copper round a plated hole). The board says nothing else of the process it
/// was made for, so none of `fab`'s other extras (the via rules, the fiducials, the stackup) and
/// no requirement of a design.
pub fn of_kicad_pcb(pcb: &Path) -> Result<Checked, String> {
    let (src, options) = kicad_options(pcb)?;
    netsmith_fab::check(
        &src,
        netsmith_geom::drc_project_of(pcb).as_deref(),
        &options,
    )
    .map_err(|e| format!("{}: {e}", pcb.display()))
}
