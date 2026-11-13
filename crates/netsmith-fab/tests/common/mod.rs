//! Boards written by hand for the design rules check: a small builder of `.kicad_pcb` text
//! (a board outline, footprints with pads, courtyards and silkscreen, tracks, vias, zones and
//! rule areas) with the project rules the netsmith writer gives KiCad, so that a violation
//! can be put in on purpose and found by netsmith's check and by KiCad's.
#![allow(dead_code)]

use netsmith_fab::KicadCli;

use std::fmt::Write;

/// A pad of a test footprint, in the footprint's own frame.
#[derive(Clone)]
pub struct P {
    pub number: &'static str,
    pub kind: &'static str,
    pub shape: &'static str,
    pub at: (f64, f64),
    pub size: (f64, f64),
    pub drill: Option<f64>,
    pub net: Option<&'static str>,
}

/// A surface-mount rectangle pad.
pub fn smd(number: &'static str, x: f64, y: f64, w: f64, h: f64, net: &'static str) -> P {
    P {
        number,
        kind: "smd",
        shape: "rect",
        at: (x, y),
        size: (w, h),
        drill: None,
        net: Some(net),
    }
}

/// A plated through-hole round pad.
pub fn thru(number: &'static str, x: f64, y: f64, d: f64, drill: f64, net: &'static str) -> P {
    P {
        number,
        kind: "thru_hole",
        shape: "circle",
        at: (x, y),
        size: (d, d),
        drill: Some(drill),
        net: Some(net),
    }
}

/// A hole with no plating and no copper.
pub fn hole(x: f64, y: f64, drill: f64) -> P {
    P {
        number: "",
        kind: "np_thru_hole",
        shape: "circle",
        at: (x, y),
        size: (drill, drill),
        drill: Some(drill),
        net: None,
    }
}

/// What a footprint draws round its pads.
#[derive(Clone, Copy)]
pub struct Draw {
    /// The courtyard, a rectangle about the footprint's origin: half width and half height.
    pub courtyard: Option<(f64, f64)>,
    /// A silkscreen line from one point to another in the footprint's frame.
    pub silk: Option<((f64, f64), (f64, f64))>,
    pub back: bool,
}

impl Default for Draw {
    fn default() -> Self {
        Draw {
            courtyard: Some((1.0, 0.7)),
            silk: None,
            back: false,
        }
    }
}

#[derive(Clone)]
pub struct Pcb {
    stubs: usize,
    pub layers: usize,
    pub nets: Vec<&'static str>,
    pub outline: (f64, f64, f64, f64),
    pub rules: Rules,
    body: String,
    extra_setup: String,
}

/// The numbers the project file hands both checks.
#[derive(Clone)]
pub struct Rules {
    pub clearance: f64,
    pub track: f64,
    pub via_diameter: f64,
    pub annular: f64,
    pub through_hole: f64,
    pub hole_to_hole: f64,
    pub hole_clearance: f64,
    pub edge: f64,
    pub silk: f64,
    pub text_height: f64,
    pub text_thickness: f64,
}

impl Default for Rules {
    /// JLCPCB's two-layer process, as netsmith writes it.
    fn default() -> Self {
        Rules {
            clearance: 0.127,
            track: 0.127,
            via_diameter: 0.5,
            annular: 0.13,
            through_hole: 0.3,
            hole_to_hole: 0.45,
            hole_clearance: 0.127,
            edge: 0.3,
            silk: 0.15,
            text_height: 1.0,
            text_thickness: 0.15,
        }
    }
}

impl Pcb {
    pub fn new(layers: usize) -> Pcb {
        Pcb {
            stubs: 0,
            layers,
            nets: Vec::new(),
            outline: (0.0, 0.0, 40.0, 30.0),
            rules: Rules::default(),
            body: String::new(),
            extra_setup: String::new(),
        }
    }

    fn net(&mut self, name: &'static str) -> usize {
        match self.nets.iter().position(|n| *n == name) {
            Some(i) => i + 1,
            None => {
                self.nets.push(name);
                self.nets.len()
            }
        }
    }

    fn net_ref(&mut self, name: Option<&'static str>) -> String {
        match name {
            Some(n) => format!("(net {} \"{n}\")", self.net(n)),
            None => String::new(),
        }
    }

    pub fn clone_for_run(&self) -> Pcb {
        self.clone()
    }

    pub fn setup(&mut self, text: &str) {
        self.extra_setup.push_str(text);
    }

    pub fn raw(&mut self, text: &str) {
        let _ = writeln!(self.body, "\t{text}");
    }

    pub fn part(&mut self, refdes: &str, at: (f64, f64), angle: f64, pads: &[P], draw: Draw) {
        let (side, fab, silk, crt) = if draw.back {
            ("B.Cu", "B.Fab", "B.SilkS", "B.CrtYd")
        } else {
            ("F.Cu", "F.Fab", "F.SilkS", "F.CrtYd")
        };
        let mut s = format!(
            "(footprint \"Test:{refdes}\" (layer \"{side}\") (at {} {} {angle})\n\t\t(property \"Reference\" \"{refdes}\" (at 0 -3 0) (layer \"{fab}\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))\n\t\t(property \"Value\" \"v\" (at 0 3 0) (layer \"{fab}\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))\n",
            at.0, at.1
        );
        if pads.iter().any(|p| p.kind != "smd") {
            s.push_str("\t\t(attr through_hole)\n");
        } else {
            s.push_str("\t\t(attr smd)\n");
        }
        if let Some((hx, hy)) = draw.courtyard {
            let _ = writeln!(
                s,
                "\t\t(fp_rect (start {} {}) (end {hx} {hy}) (stroke (width 0.05) (type solid)) (fill no) (layer \"{crt}\"))",
                -hx, -hy
            );
        }
        if let Some((a, b)) = draw.silk {
            let _ = writeln!(
                s,
                "\t\t(fp_line (start {} {}) (end {} {}) (stroke (width 0.12) (type solid)) (layer \"{silk}\"))",
                a.0, a.1, b.0, b.1
            );
        }
        for p in pads {
            let net = self.net_ref(p.net);
            let layers = match (p.kind, draw.back) {
                ("smd", false) => "\"F.Cu\" \"F.Mask\" \"F.Paste\"".to_string(),
                ("smd", true) => "\"B.Cu\" \"B.Mask\" \"B.Paste\"".to_string(),
                _ => "\"*.Cu\" \"*.Mask\"".to_string(),
            };
            let drill = p.drill.map(|d| format!(" (drill {d})")).unwrap_or_default();
            let _ = writeln!(
                s,
                "\t\t(pad \"{}\" {} {} (at {} {}) (size {} {}){drill} (layers {layers}) {net})",
                p.number, p.kind, p.shape, p.at.0, p.at.1, p.size.0, p.size.1
            );
        }
        s.push_str("\t)");
        self.raw(&s);
    }

    /// A part of one 0.6 mm square surface pad on `net` at (x, y): a place for a track to end.
    pub fn stub(&mut self, net: &'static str, x: f64, y: f64) {
        self.stubs += 1;
        let refdes = format!("S{}", self.stubs);
        let d = Draw {
            courtyard: None,
            silk: None,
            back: false,
        };
        self.part(
            &refdes,
            (x, y),
            0.0,
            &[smd("1", 0.0, 0.0, 0.6, 0.6, net)],
            d,
        );
    }

    /// A stub on the back of the board.
    pub fn back_stub(&mut self, net: &'static str, x: f64, y: f64) {
        self.stubs += 1;
        let refdes = format!("S{}", self.stubs);
        let d = Draw {
            courtyard: None,
            silk: None,
            back: true,
        };
        self.part(
            &refdes,
            (x, y),
            0.0,
            &[smd("1", 0.0, 0.0, 0.6, 0.6, net)],
            d,
        );
    }

    /// A stub at each end of a track, and the track.
    pub fn wire(&mut self, net: &'static str, layer: &str, a: (f64, f64), b: (f64, f64), w: f64) {
        if layer == "B.Cu" {
            self.back_stub(net, a.0, a.1);
            self.back_stub(net, b.0, b.1);
        } else {
            self.stub(net, a.0, a.1);
            self.stub(net, b.0, b.1);
        }
        self.track(net, layer, a, b, w);
    }

    pub fn track(&mut self, net: &'static str, layer: &str, a: (f64, f64), b: (f64, f64), w: f64) {
        let n = format!("(net {})", self.net(net));
        self.raw(&format!(
            "(segment (start {} {}) (end {} {}) (width {w}) (layer \"{layer}\") {n})",
            a.0, a.1, b.0, b.1
        ));
    }

    /// A track that is an arc from `a` through `m` to `b`.
    pub fn arc(&mut self, net: &'static str, layer: &str, [a, m, b]: [(f64, f64); 3], w: f64) {
        let n = format!("(net {})", self.net(net));
        self.raw(&format!(
            "(arc (start {} {}) (mid {} {}) (end {} {}) (width {w}) (layer \"{layer}\") {n})",
            a.0, a.1, m.0, m.1, b.0, b.1
        ));
    }

    pub fn via(&mut self, net: &'static str, at: (f64, f64), size: f64, drill: f64) {
        let n = format!("(net {})", self.net(net));
        let last = "B.Cu";
        self.raw(&format!(
            "(via (at {} {}) (size {size}) (drill {drill}) (layers \"F.Cu\" \"{last}\") {n})",
            at.0, at.1
        ));
    }

    /// A pour of `net` on `layer` over a rectangle.
    pub fn zone(&mut self, net: &'static str, layer: &str, r: (f64, f64, f64, f64)) {
        let n = format!("(net {}) (net_name \"{net}\")", self.net(net));
        self.raw(&format!(
            "(zone {n} (layer \"{layer}\") (hatch edge 0.5) (connect_pads (clearance 0.2)) (min_thickness 0.2) (filled_areas_thickness no) (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.3)) (polygon (pts (xy {} {}) (xy {} {}) (xy {} {}) (xy {} {}))))",
            r.0, r.1, r.2, r.1, r.2, r.3, r.0, r.3
        ));
    }

    /// A rule area that forbids what the flags say.
    pub fn keepout(
        &mut self,
        layers: &str,
        r: (f64, f64, f64, f64),
        tracks: bool,
        vias: bool,
        pads: bool,
    ) {
        let f = |b: bool| if b { "not_allowed" } else { "allowed" };
        self.raw(&format!(
            "(zone (net 0) (net_name \"\") (layers {layers}) (name \"ko\") (hatch edge 0.5) (connect_pads (clearance 0)) (min_thickness 0.25) (filled_areas_thickness no) (keepout (tracks {}) (vias {}) (pads {}) (copperpour allowed) (footprints allowed)) (fill (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy {} {}) (xy {} {}) (xy {} {}) (xy {} {}))))",
            f(tracks), f(vias), f(pads), r.0, r.1, r.2, r.1, r.2, r.3, r.0, r.3
        ));
    }

    pub fn edge_line(&mut self, a: (f64, f64), b: (f64, f64)) {
        self.raw(&format!(
            "(gr_line (start {} {}) (end {} {}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))",
            a.0, a.1, b.0, b.1
        ));
    }

    pub fn silk_line(&mut self, a: (f64, f64), b: (f64, f64)) {
        self.raw(&format!(
            "(gr_line (start {} {}) (end {} {}) (stroke (width 0.12) (type default)) (layer \"F.SilkS\"))",
            a.0, a.1, b.0, b.1
        ));
    }

    pub fn silk_text(&mut self, text: &str, at: (f64, f64), size: f64, thickness: f64) {
        self.raw(&format!(
            "(gr_text \"{text}\" (at {} {} 0) (layer \"F.SilkS\") (effects (font (size {size} {size}) (thickness {thickness}))))",
            at.0, at.1
        ));
    }

    /// The `.kicad_pcb` text.
    pub fn pcb(&mut self) -> String {
        let (x0, y0, x1, y1) = self.outline;
        let mut out = String::new();
        out.push_str("(kicad_pcb\n\t(version 20241229)\n\t(generator \"netsmith-test\")\n\t(general (thickness 1.6) (legacy_teardrops no))\n\t(paper \"A4\")\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n");
        if self.layers == 4 {
            out.push_str("\t\t(4 \"In1.Cu\" signal)\n\t\t(6 \"In2.Cu\" signal)\n");
        }
        out.push_str("\t\t(2 \"B.Cu\" signal)\n\t\t(13 \"F.Paste\" user)\n\t\t(15 \"B.Paste\" user)\n\t\t(5 \"F.SilkS\" user \"F.Silkscreen\")\n\t\t(7 \"B.SilkS\" user \"B.Silkscreen\")\n\t\t(1 \"F.Mask\" user)\n\t\t(3 \"B.Mask\" user)\n\t\t(25 \"Edge.Cuts\" user)\n\t\t(31 \"F.CrtYd\" user \"F.Courtyard\")\n\t\t(29 \"B.CrtYd\" user \"B.Courtyard\")\n\t\t(35 \"F.Fab\" user)\n\t\t(33 \"B.Fab\" user)\n\t)\n");
        let _ = writeln!(
            out,
            "\t(setup\n\t\t(pad_to_mask_clearance 0)\n\t\t(allow_soldermask_bridges_in_footprints no)\n\t\t(tenting front back)\n{}\t)",
            self.extra_setup
        );
        let body = std::mem::take(&mut self.body);
        // The outline is written last so that the nets are all numbered first.
        let mut edge = String::new();
        for (a, b) in [
            ((x0, y0), (x1, y0)),
            ((x1, y0), (x1, y1)),
            ((x1, y1), (x0, y1)),
            ((x0, y1), (x0, y0)),
        ] {
            let _ = writeln!(
                edge,
                "\t(gr_line (start {} {}) (end {} {}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))",
                a.0, a.1, b.0, b.1
            );
        }
        out.push_str("\t(net 0 \"\")\n");
        for (i, n) in self.nets.iter().enumerate() {
            let _ = writeln!(out, "\t(net {} \"{n}\")", i + 1);
        }
        out.push_str(&body);
        out.push_str(&edge);
        out.push_str(")\n");
        self.body = body;
        out
    }

    /// The `.kicad_pro` text with this board's rules.
    pub fn pro(&self) -> String {
        let r = &self.rules;
        format!(
            r#"{{
  "board": {{
    "design_settings": {{
      "defaults": {{ "board_outline_line_width": 0.05, "copper_line_width": {track}, "courtyard_line_width": 0.05, "silk_line_width": 0.15, "silk_text_size_h": 1, "silk_text_size_v": 1, "silk_text_thickness": 0.15 }},
      "rules": {{
        "min_clearance": {clearance},
        "min_copper_edge_clearance": {edge},
        "min_hole_clearance": {hole_clearance},
        "min_hole_to_hole": {hole_to_hole},
        "min_silk_clearance": {silk},
        "min_text_height": {text_height},
        "min_text_thickness": {text_thickness},
        "min_through_hole_diameter": {through_hole},
        "min_track_width": {track},
        "min_via_annular_width": {annular},
        "min_via_diameter": {via_diameter}
      }},
      "rule_severities": {{
        "annular_width": "error", "clearance": "error", "copper_edge_clearance": "error",
        "courtyards_overlap": "error", "drill_out_of_range": "error", "hole_clearance": "error",
        "hole_to_hole": "error", "shorting_items": "error", "silk_edge_clearance": "error",
        "silk_over_copper": "error", "silk_overlap": "error", "text_height": "error",
        "text_thickness": "error", "track_dangling": "error", "track_width": "error",
        "unconnected_items": "error", "via_dangling": "error"
      }}
    }}
  }},
  "meta": {{ "filename": "board.kicad_pro", "version": 3 }},
  "net_settings": {{
    "classes": [ {{ "clearance": {clearance}, "name": "Default", "priority": 2147483647, "track_width": {track}, "via_diameter": {via_diameter}, "via_drill": 0.3 }} ],
    "meta": {{ "version": 4 }}
  }}
}}
"#,
            clearance = r.clearance,
            edge = r.edge,
            hole_clearance = r.hole_clearance,
            hole_to_hole = r.hole_to_hole,
            silk = r.silk,
            text_height = r.text_height,
            text_thickness = r.text_thickness,
            through_hole = r.through_hole,
            track = r.track,
            annular = r.annular,
            via_diameter = r.via_diameter,
        )
    }
}

/// KiCad's design rules check on a board and its project, or `None` if KiCad is not installed.
pub fn kicad_findings(
    name: &str,
    pcb: &str,
    pro: &str,
) -> Option<Vec<netsmith_fab::drc::kicad::Reported>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let cli = netsmith_fab::KicadCliExe::locate_runnable()?;
    let dir = netsmith_testkit::TempDir::new(&format!(
        "drc-{name}-{}",
        RUNS.fetch_add(1, Ordering::SeqCst)
    ));
    let path = dir.join(format!("{name}.kicad_pcb"));
    std::fs::write(&path, pcb).ok()?;
    if !pro.is_empty() {
        std::fs::write(path.with_extension("kicad_pro"), pro).ok()?;
    }
    let json = cli
        .drc_json(&path)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    Some(netsmith_fab::drc::kicad::parse(&json).unwrap())
}
