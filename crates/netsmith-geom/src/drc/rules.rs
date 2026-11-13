//! The numbers a board is checked to: what KiCad reads from the project file beside the
//! board (`.kicad_pro`, which netsmith writes from the board's process), the solder mask
//! setting in the board's own setup, and what KiCad assumes when a number is missing.

use super::{Rule, Severity};
use std::collections::BTreeMap;

/// A net class: the clearance its nets keep and the nets it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub name: String,
    pub clearance: Option<f64>,
    pub priority: i64,
    /// Net name patterns: `*` matches any run of characters.
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    /// The least space between copper of different nets (the board's, and the floor of any
    /// class's).
    pub clearance: f64,
    pub track_width: f64,
    pub via_diameter: f64,
    pub annular_width: f64,
    pub through_hole: f64,
    pub hole_to_hole: f64,
    pub hole_clearance: f64,
    pub copper_edge: f64,
    pub silk_clearance: f64,
    pub text_height: f64,
    pub text_thickness: f64,
    /// The least solder mask between the openings of two pads of different nets; zero if the
    /// board does not say (KiCad then reports only openings that run together).
    pub mask_web: f64,
    pub classes: Vec<Class>,
    /// The class every net not named belongs to.
    pub default_clearance: f64,
    /// The widest via a fabricator fills and caps, so that a via in a pad is not a problem
    /// (see [`Extras`](super::Extras)).
    pub severities: BTreeMap<String, Severity>,
}

impl Default for Rules {
    /// KiCad 9's defaults for a board with no project file.
    fn default() -> Self {
        Rules {
            clearance: 0.0,
            track_width: 0.0,
            via_diameter: 0.5,
            annular_width: 0.1,
            through_hole: 0.3,
            hole_to_hole: 0.25,
            hole_clearance: 0.25,
            copper_edge: 0.5,
            silk_clearance: 0.0,
            text_height: 0.8,
            text_thickness: 0.08,
            mask_web: 0.0,
            classes: Vec::new(),
            default_clearance: 0.2,
            severities: BTreeMap::new(),
        }
    }
}

impl Rules {
    /// The rules in a `.kicad_pro` text; a number it lacks keeps KiCad's default. A text that
    /// is not JSON is an error.
    pub fn from_project(text: &str) -> Result<Rules, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("the project file is not JSON: {e}"))?;
        let mut r = Rules::default();
        let settings = &json["board"]["design_settings"];
        let rules = &settings["rules"];
        let num = |key: &str| rules[key].as_f64();
        let set = |slot: &mut f64, key: &str| {
            if let Some(v) = num(key) {
                *slot = v;
            }
        };
        set(&mut r.clearance, "min_clearance");
        set(&mut r.track_width, "min_track_width");
        set(&mut r.via_diameter, "min_via_diameter");
        set(&mut r.annular_width, "min_via_annular_width");
        set(&mut r.through_hole, "min_through_hole_diameter");
        set(&mut r.hole_to_hole, "min_hole_to_hole");
        set(&mut r.hole_clearance, "min_hole_clearance");
        set(&mut r.copper_edge, "min_copper_edge_clearance");
        set(&mut r.silk_clearance, "min_silk_clearance");
        set(&mut r.text_height, "min_text_height");
        set(&mut r.text_thickness, "min_text_thickness");
        if let Some(sev) = settings["rule_severities"].as_object() {
            for (rule, level) in sev {
                if let Some(level) = level.as_str().and_then(Severity::parse) {
                    r.severities.insert(rule.clone(), level);
                }
            }
        }
        let net_settings = &json["net_settings"];
        if let Some(classes) = net_settings["classes"].as_array() {
            for c in classes {
                let name = c["name"].as_str().unwrap_or("").to_string();
                let clearance = c["clearance"].as_f64();
                if name == "Default" {
                    if let Some(c) = clearance {
                        r.default_clearance = c;
                    }
                    continue;
                }
                r.classes.push(Class {
                    name,
                    clearance,
                    priority: c["priority"].as_i64().unwrap_or(i64::MAX),
                    patterns: Vec::new(),
                });
            }
        }
        if let Some(patterns) = net_settings["netclass_patterns"].as_array() {
            for p in patterns {
                let (Some(class), Some(pattern)) = (p["netclass"].as_str(), p["pattern"].as_str())
                else {
                    continue;
                };
                if let Some(c) = r.classes.iter_mut().find(|c| c.name == class) {
                    c.patterns.push(pattern.to_string());
                }
            }
        }
        Ok(r)
    }

    /// The clearance a net keeps: its class's (the class with the lowest priority number
    /// that names it), or the default class's, and never less than the board's least.
    pub fn net_clearance(&self, net: Option<&str>) -> f64 {
        let class = net.and_then(|n| {
            self.classes
                .iter()
                .filter(|c| c.patterns.iter().any(|p| matches_pattern(p, n)))
                .min_by_key(|c| c.priority)
        });
        let own = class
            .and_then(|c| c.clearance)
            .unwrap_or(self.default_clearance);
        own.max(self.clearance)
    }

    /// How a rule that is broken is reported: the project file's choice, else KiCad's.
    /// `None` if the person told KiCad to ignore it.
    pub fn severity(&self, rule: Rule) -> Option<Severity> {
        match self.severities.get(rule.id()) {
            Some(Severity::Ignore) => None,
            Some(s) => Some(*s),
            None => rule.default_severity(),
        }
    }
}

/// Whether `net` matches `pattern`, where `*` stands for any run of characters.
pub fn matches_pattern(pattern: &str, net: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == net;
    }
    let mut rest = net;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRO: &str = r#"{
      "board": {"design_settings": {
        "rules": {"min_clearance": 0.1, "min_track_width": 0.09, "min_copper_edge_clearance": 0.2},
        "rule_severities": {"clearance": "error", "silk_over_copper": "warning", "text_height": "ignore"}}},
      "net_settings": {
        "classes": [
          {"name": "Default", "clearance": 0.15, "priority": 2147483647},
          {"name": "Power", "clearance": 0.3, "priority": 0}],
        "netclass_patterns": [{"netclass": "Power", "pattern": "V*"}]}}"#;

    #[test]
    fn numbers_come_from_the_project_and_the_rest_are_kicads_defaults() {
        let r = Rules::from_project(PRO).unwrap();
        assert_eq!(
            (r.clearance, r.track_width, r.copper_edge),
            (0.1, 0.09, 0.2)
        );
        assert_eq!(r.hole_to_hole, 0.25);
        assert_eq!(r.default_clearance, 0.15);
    }

    #[test]
    fn a_net_keeps_its_classes_clearance_and_the_default_is_the_rest() {
        let r = Rules::from_project(PRO).unwrap();
        assert_eq!(r.net_clearance(Some("VIN")), 0.3);
        assert_eq!(r.net_clearance(Some("GND")), 0.15);
        assert_eq!(r.net_clearance(None), 0.15);
    }

    #[test]
    fn severities_are_the_projects_and_ignored_rules_are_dropped() {
        let r = Rules::from_project(PRO).unwrap();
        assert_eq!(r.severity(Rule::SilkOverCopper), Some(Severity::Warning));
        assert_eq!(r.severity(Rule::TextHeight), None);
        assert_eq!(r.severity(Rule::Clearance), Some(Severity::Error));
        // Not in the file: KiCad's own default.
        assert_eq!(r.severity(Rule::SolderMaskBridge), Some(Severity::Error));
    }

    #[test]
    fn wildcards_match_runs() {
        assert!(matches_pattern("V*", "VIN"));
        assert!(matches_pattern("*3V3*", "A3V3B"));
        assert!(!matches_pattern("V*N", "VIA"));
        assert!(matches_pattern("GND", "GND") && !matches_pattern("GND", "GND2"));
    }

    #[test]
    fn text_that_is_not_json_is_refused() {
        assert!(Rules::from_project("not json").is_err());
    }
}
