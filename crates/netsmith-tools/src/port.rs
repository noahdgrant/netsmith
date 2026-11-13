//! What a port of a KiCad board keeps about the original (#456): `reference/parts.toml`, the
//! original's reference designator of each part of the design, by the part's member path.
//!
//! It is the one map of a port. The nets proof of the port's tests reads it to name the design's
//! parts as the original does, the parity checker (`netsmith parity`) to pair the original's parts
//! with the board `fab` wrote (each footprint of which holds its member path, `netsmith_path`); the
//! command that writes the first `.nsm` of a port (#452) is to write it too. A member renamed in
//! the source is renamed here, and a map of the fab board's designators, which went stale when a
//! member was renamed, is no longer kept.

use std::collections::BTreeMap;
use std::path::Path;

/// The original's reference designator of each part, by member path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortParts {
    by_path: BTreeMap<String, String>,
}

impl PortParts {
    /// `parts.toml`: a table `parts` of `<member path> = "<the original's designator>"`. `file` is
    /// what a message calls it. A designator given to two members is refused.
    pub fn parse(text: &str, file: &str) -> Result<PortParts, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("{file}: {e}"))?;
        if let Some(extra) = table.keys().find(|k| *k != "parts") {
            return Err(format!(
                "{file}: `{extra}`; the file holds only the table `parts`"
            ));
        }
        let parts = table
            .get("parts")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| format!("{file}: it has no table `parts`"))?;
        let mut by_path = BTreeMap::new();
        let mut by_reference: BTreeMap<String, String> = BTreeMap::new();
        for (path, value) in parts {
            let reference = value.as_str().ok_or_else(|| {
                format!("{file}: `{path}` must be the original's designator as text, not {value}")
            })?;
            if let Some(other) = by_reference.insert(reference.to_string(), path.clone()) {
                return Err(format!(
                    "{file}: `{other}` and `{path}` are both {reference}; a designator is one part"
                ));
            }
            by_path.insert(path.clone(), reference.to_string());
        }
        Ok(PortParts { by_path })
    }

    /// The file at `path`.
    pub fn read(path: &Path) -> Result<PortParts, String> {
        let file = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|e| format!("{file}: {e}"))?;
        PortParts::parse(&text, &file)
    }

    /// The original's designator of the part at `path`.
    pub fn reference_of(&self, path: &str) -> Option<&str> {
        self.by_path.get(path).map(String::as_str)
    }

    /// The member path of the original's part `reference`.
    pub fn path_of(&self, reference: &str) -> Option<&str> {
        self.by_path
            .iter()
            .find(|(_, r)| r.as_str() == reference)
            .map(|(p, _)| p.as_str())
    }

    /// `(member path, the original's designator)` for each part, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.by_path.iter().map(|(p, r)| (p.as_str(), r.as_str()))
    }

    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parts_file_is_read_and_a_designator_is_one_part() {
        let parts = PortParts::parse(
            "[parts]\nmcu = \"U1\"\n\"stage.c1\" = \"C3\"\n",
            "parts.toml",
        )
        .unwrap();
        assert_eq!(parts.reference_of("stage.c1"), Some("C3"));
        assert_eq!(parts.path_of("U1"), Some("mcu"));
        assert_eq!(parts.len(), 2);
        let twice = PortParts::parse("[parts]\na = \"U1\"\nb = \"U1\"\n", "p.toml").unwrap_err();
        assert!(twice.contains("`a` and `b` are both U1"), "{twice}");
        assert!(
            PortParts::parse("x = 1", "p.toml")
                .unwrap_err()
                .contains("`x`")
        );
        assert!(
            PortParts::parse("[parts]\na = 1\n", "p.toml")
                .unwrap_err()
                .contains("`a` must be the original's designator as text")
        );
    }
}
