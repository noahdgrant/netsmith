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
pub fn via_offers(process: &netsmith_core::Process, via_drill: f64, via_diameter: f64) -> Vec<ViaOffer> {
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
        let size = |o: &ViaOffer| o.span.map_or(n as usize, |s| s.positions(n).1 - s.positions(n).0);
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
