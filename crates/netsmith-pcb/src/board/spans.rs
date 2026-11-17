//! The vias a board's process makes: through vias on every process, and the blind and buried
//! vias of an HDI process (issue #1015).

use super::*;

/// One kind of via the board's process makes: its span, how it is drilled, and the via of that
/// kind the router draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViaOffer {
    /// The layers it joins; `None` for a through via.
    pub span: Option<ViaSpan>,
    /// Whether it is drilled by laser, a blind via through one dielectric layer; else it is
    /// drilled mechanically.
    pub laser: bool,
    pub drill: f64,
    pub diameter: f64,
}

/// The dielectric a via from the copper layer at place `from` to the one at `to` (from the top,
/// `from <= to`) is drilled through, in mm: the dielectric layers between, which the aspect
/// ratio is reckoned on. 0 where the stackup states no build-up.
pub fn span_depth(process: &netsmith_core::Process, from: usize, to: usize) -> f64 {
    process.stackup.inner.as_ref().map_or(0.0, |inner| {
        inner
            .dielectrics
            .iter()
            .skip(from)
            .take(to.saturating_sub(from))
            .map(|d| d.thickness)
            .sum()
    })
}

/// The kinds of via `process` makes, with the via of each the router draws (`via_drill` and
/// `via_diameter` for a drilled one): the laser-drilled blind vias of each build-up layer from
/// the top, then from the bottom, the buried via, and the through via last. A process that is
/// not HDI makes the through via only.
pub fn via_offers(
    process: &netsmith_core::Process,
    via_drill: f64,
    via_diameter: f64,
) -> Vec<ViaOffer> {
    let n = process.stackup.layers;
    let through = ViaOffer {
        span: None,
        laser: false,
        drill: via_drill,
        diameter: via_diameter,
    };
    let Some(hdi) = &process.hdi else {
        return vec![through];
    };
    let k = hdi.order as usize;
    let at = |p: usize| CopperLayer::at_position(p, n);
    let laser = |a: usize| ViaOffer {
        span: Some(ViaSpan {
            from: at(a),
            to: at(a + 1),
        }),
        laser: true,
        drill: hdi.laser_drill,
        diameter: hdi.laser_via_diameter,
    };
    let last = n as usize - 1;
    let mut offers: Vec<ViaOffer> = (0..k).map(laser).collect();
    offers.extend((0..k).map(|i| laser(last - 1 - i)));
    if hdi.buried && k < last - k {
        offers.push(ViaOffer {
            span: Some(ViaSpan {
                from: at(k),
                to: at(last - k),
            }),
            laser: false,
            drill: via_drill,
            diameter: via_diameter,
        });
    }
    offers.push(through);
    offers
}

impl Board {
    /// How many copper layers the board has.
    pub fn copper_count(&self) -> u32 {
        self.process.stackup.layers.max(2)
    }

    /// The kinds of via the board's process makes ([`via_offers`]), with the router's vias.
    pub fn via_offers(&self) -> Vec<ViaOffer> {
        via_offers(&self.process, self.rules.via_drill, self.rules.via_diameter)
    }

    /// Whether the board's process makes blind or buried vias (issue #1015).
    pub fn has_spans(&self) -> bool {
        self.process.hdi.is_some()
    }

    /// The via the router draws between layers `a` and `b`: of the kinds the process makes
    /// that join both, the one of fewest layers, the first of [`Board::via_offers`] on a tie.
    /// A through via on any process that is not HDI.
    pub fn via_between(&self, a: CopperLayer, b: CopperLayer) -> ViaOffer {
        let n = self.copper_count();
        let offers = self.via_offers();
        let size = |o: &ViaOffer| {
            o.span
                .map_or(n as usize, |s| s.positions(n).1 - s.positions(n).0)
        };
        offers
            .iter()
            .filter(|o| o.span.is_none_or(|s| s.holds(a, n) && s.holds(b, n)))
            .min_by_key(|o| size(o))
            .copied()
            .unwrap_or(offers[offers.len() - 1])
    }

    /// The copper layers a via is on, from the top.
    pub fn via_layers(&self, via: &Via) -> Vec<CopperLayer> {
        let n = self.copper_count();
        (0..n as usize)
            .map(|p| CopperLayer::at_position(p, n))
            .filter(|&l| via.on(l, n))
            .collect()
    }

    /// The dielectric a via is drilled through, in mm ([`span_depth`]).
    pub fn via_depth(&self, via: &Via) -> f64 {
        let n = self.copper_count();
        let (a, b) = via.span.map_or((0, n as usize - 1), |s| s.positions(n));
        span_depth(&self.process, a, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::hdi_board;

    fn span(from: CopperLayer, to: CopperLayer) -> Option<ViaSpan> {
        Some(ViaSpan { from, to })
    }

    #[test]
    fn the_via_between_two_layers_is_the_one_of_fewest_layers_that_joins_both() {
        let board = hdi_board();
        assert_eq!(board.planes[0].layer, "In1.Cu");
        use CopperLayer::{Back, Front, Inner};
        let between = |a, b| board.via_between(a, b);
        // A blind via from the top to the layer under it: the laser's hole and copper.
        let blind = between(Front, Inner(1));
        assert_eq!(blind.span, span(Front, Inner(1)));
        assert!(blind.laser && blind.drill == 0.1 && blind.diameter == 0.25);
        assert_eq!(between(Inner(4), Back).span, span(Inner(4), Back));
        // Inside the core, the buried via; from the top into the core, the through via.
        let buried = between(Inner(2), Inner(3));
        assert_eq!(buried.span, span(Inner(1), Inner(4)));
        assert!(!buried.laser && buried.drill == board.rules.via_drill);
        assert_eq!(between(Front, Inner(2)).span, None);
        assert_eq!(between(Front, Back).span, None);
        // A via's layers and depth: the blind via passes the first dielectric layer only.
        let via = Via {
            net: "GND".into(),
            at: crate::Pt::new(0.0, 0.0),
            diameter: 0.25,
            drill: 0.1,
            span: blind.span,
        };
        assert_eq!(board.via_layers(&via), [Front, Inner(1)]);
        assert!((board.via_depth(&via) - 0.086).abs() < 1e-9);
        // A board that is not HDI draws through vias only.
        let six = crate::board::tests::six_layer_board();
        assert_eq!(six.via_between(Front, Inner(1)).span, None);
    }

    #[test]
    fn the_board_file_writes_a_blind_via_as_a_micro_via_and_a_buried_one_as_blind() {
        let mut board = hdi_board();
        let via = |span| Via {
            net: "GND".into(),
            at: crate::Pt::new(5.0, 5.0),
            diameter: 0.45,
            drill: 0.2,
            span,
        };
        board
            .vias
            .push(via(span(CopperLayer::Front, CopperLayer::Inner(1))));
        board
            .vias
            .push(via(span(CopperLayer::Inner(1), CopperLayer::Inner(4))));
        board.vias.push(via(None));
        let pcb = crate::kicad_pcb(&board);
        assert!(
            pcb.contains("(via micro\n") || pcb.contains("(via micro "),
            "{pcb}"
        );
        assert!(pcb.contains("(layers \"F.Cu\" \"In1.Cu\")"), "{pcb}");
        assert!(pcb.contains("(via blind"), "{pcb}");
        assert!(pcb.contains("(layers \"In1.Cu\" \"In4.Cu\")"), "{pcb}");
        assert_eq!(
            pcb.matches("(layers \"F.Cu\" \"B.Cu\")").count(),
            1,
            "{pcb}"
        );
        // The project file holds the laser's least hole and copper for KiCad's check.
        let pro = crate::kicad_pro(&board);
        assert!(pro.contains("\"min_microvia_drill\": 0.075"), "{pro}");
        assert!(pro.contains("\"min_microvia_diameter\": 0.225"), "{pro}");
        assert!(!crate::kicad_pro(&crate::board::tests::six_layer_board()).contains("microvia"));
    }

    #[test]
    fn the_fab_notes_say_the_board_is_ordered_as_hdi() {
        let mut board = hdi_board();
        board.vias.push(Via {
            net: "GND".into(),
            at: crate::Pt::new(5.0, 5.0),
            diameter: 0.25,
            drill: 0.1,
            span: span(CopperLayer::Front, CopperLayer::Inner(1)),
        });
        let notes = board.process_extras();
        let hdi = notes
            .iter()
            .find(|n| n.starts_with("HDI: "))
            .expect("an HDI note");
        assert!(
            hdi.contains("6-layer board of order 1 with the build-up `HDI-6L-1.2`")
                && hdi.contains("1 blind via(s)")
                && hdi.contains("0 buried via(s)")
                && hdi.contains("F.Cu-In1.Cu, In4.Cu-B.Cu, In1.Cu-In4.Cu"),
            "{hdi}"
        );
        let six = crate::board::tests::six_layer_board();
        assert!(!six.process_extras().iter().any(|n| n.starts_with("HDI")));
    }

    #[test]
    fn a_routes_file_keeps_a_blind_vias_layers() {
        let mut board = hdi_board();
        let net = board.nets[0].name.clone();
        board.vias.push(Via {
            net: net.clone(),
            at: board.outline.min + crate::Pt::new(2.0, 3.0),
            diameter: 0.25,
            drill: 0.1,
            span: span(CopperLayer::Front, CopperLayer::Inner(1)),
        });
        let record = crate::record_of(&board, &net, false).unwrap();
        let mut routes = crate::Routes::default();
        routes
            .boards
            .entry(board.name.clone())
            .or_default()
            .nets
            .insert(net.clone(), record);
        let text = routes.to_text();
        assert!(
            text.contains(
                "{ at = [2.0, 3.0], diameter = 0.25, drill = 0.1, layers = [\"top\", \"inner1\"] }"
            ),
            "{text}"
        );
        let read = crate::Routes::parse(&text, "t.routes.toml").unwrap();
        assert_eq!(read, routes);
        let mut fresh = hdi_board();
        crate::apply_routes(&mut fresh, &read, "t.routes.toml").unwrap();
        assert_eq!(
            fresh.vias[0].span,
            span(CopperLayer::Front, CopperLayer::Inner(1))
        );
        // A through via writes no layers, as before.
        fresh.vias[0].span = None;
        let record = crate::record_of(&fresh, &net, false).unwrap();
        assert!(record.vias[0].layers.is_empty());
        // A via between layers the board does not have is an error of the file.
        let bad = text.replace("\"inner1\"]", "\"inner9\"]");
        let err = crate::apply_routes(
            &mut hdi_board(),
            &crate::Routes::parse(&bad, "t.routes.toml").unwrap(),
            "t.routes.toml",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("inner9"), "{err}");
    }

    #[test]
    fn a_blind_via_joins_the_copper_of_its_own_layers_only() {
        let mut board = hdi_board();
        let net = "net-led.anode";
        let pad = |board: &Board, r: &str, n: &str| {
            let part = board.part(r).unwrap();
            part.pad_centre(part.footprint.pads_numbered(n).next().unwrap())
        };
        // R1.2 and D1.2, surface-mount pads on the top, joined by a run on the second inner
        // layer between two vias.
        let (r1, d1) = (pad(&board, "R1", "2"), pad(&board, "D1", "2"));
        // R1 is under D1: the run leaves R1 downwards and D1 upwards, so neither top track
        // passes the other pad.
        let (a, b) = (
            crate::Pt::new(r1.x, r1.y + 1.0),
            crate::Pt::new(d1.x, d1.y - 2.0),
        );
        let track = |layer, from, to| Track {
            net: net.into(),
            layer,
            from,
            to,
            width: 0.2,
            mid: None,
        };
        board.tracks.push(track(CopperLayer::Front, r1, a));
        board.tracks.push(track(CopperLayer::Inner(2), a, b));
        board.tracks.push(track(CopperLayer::Front, b, d1));
        let lines = |board: &Board| {
            crate::Ratsnest::of(board)
                .lines
                .iter()
                .filter(|l| l.net == net)
                .count()
        };
        assert_eq!(lines(&board), 1);
        // Blind vias from the top to the first inner layer do not reach the run on the second.
        for at in [a, b] {
            board.vias.push(Via {
                net: net.into(),
                at,
                diameter: 0.25,
                drill: 0.1,
                span: span(CopperLayer::Front, CopperLayer::Inner(1)),
            });
        }
        assert_eq!(lines(&board), 1);
        // Through vias do.
        for v in &mut board.vias {
            v.span = None;
        }
        assert_eq!(lines(&board), 0);
    }
}
