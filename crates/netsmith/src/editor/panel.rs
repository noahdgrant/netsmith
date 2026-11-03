//! A panel for the editor (ADR 0040): the grid of its board's placed size, drawn, with what
//! the panel checks say about it. Read only: nothing here writes a file.

use netsmith_editor::{
    ApiError,
    api::{PanelFinding, PanelView},
};
use netsmith_fab::panel::{self, CellFacts, Rules, Spec};

use super::{Workspace, pcb};

/// The rules of the board house for the checks, from the language's.
pub fn rules(r: &netsmith_core::PanelRules) -> Rules {
    Rules {
        fabricator: r.fabricator.clone(),
        source: r.source.clone(),
        min_rail: r.min_rail,
        min_size: r.min_size,
        max_size: r.max_size,
        max_assembled_size: r.max_assembled_size,
        max_cuts: r.max_cuts,
        min_cut_spacing: r.min_cut_spacing,
        advised_cut_spacing: r.advised_cut_spacing,
        min_edge_clearance: r.min_edge_clearance,
        advised_edge_clearance: r.advised_edge_clearance,
        min_thickness: r.min_thickness,
        tooling_hole: r.tooling_hole,
        fiducial: r.fiducial,
        fiducial_offset: r.fiducial_offset,
    }
}

impl Workspace {
    pub(super) fn panel_view(&self, name: &str) -> Result<PanelView, ApiError> {
        let (panel_data, board_data) = {
            let state = self.fresh();
            let panel_data = state.snapshot.panel(name)?;
            let board = state.snapshot.board_handle(&panel_data.spec.cell)?;
            (panel_data, board)
        };
        let spec = &panel_data.spec;
        // The board as it is placed (and routed, if it was) in the PCB view; the panel is of
        // its size.
        let entry = {
            let state = self.fresh();
            state.pcb.get(&spec.cell).map(|e| e.board.clone())
        };
        let board = match entry {
            Some(board) => board,
            None => pcb::place(&board_data, &spec.cell)?.board,
        };
        let (width, height) = (board.outline.width(), board.outline.height());
        let assembled: Vec<_> = board_data
            .built
            .sourcing
            .iter()
            .filter(|s| s.assembled())
            .collect();
        let panel_spec = Spec {
            name: spec.name.clone(),
            columns: spec.columns,
            rows: spec.rows,
            rail: spec.rail,
            rules: rules(&spec.rules),
        };
        let g = panel::geometry(&panel_spec, (0.0, 0.0, width, height));
        let facts = CellFacts {
            board: spec.cell.clone(),
            thickness: board.process.stackup.chosen_thickness(),
            edge_clearance: board.process.min_edge_clearance,
            assembled: !assembled.is_empty(),
            bottom_assembled: false,
        };
        let mut findings: Vec<PanelFinding> = panel::check(&panel_spec, &g, &facts)
            .into_iter()
            .map(|f| PanelFinding {
                error: f.error,
                message: f.message,
            })
            .collect();
        if board.corner_radius > 0.0 {
            findings.push(PanelFinding {
                error: true,
                message: format!(
                    "panel {}: {} has corners rounded by {} mm and a V-cut runs straight across the whole panel, so every board in it is a rectangle with square corners",
                    spec.name,
                    spec.cell,
                    panel::mm(board.corner_radius)
                ),
            });
        }
        Ok(PanelView {
            name: spec.name.clone(),
            board: spec.cell.clone(),
            columns: spec.columns,
            rows: spec.rows,
            cell_width: width,
            cell_height: height,
            width: g.width,
            height: g.height,
            rail: spec.rail,
            svg: panel::svg(&panel_spec, &g, &spec.cell),
            findings,
        })
    }
}
