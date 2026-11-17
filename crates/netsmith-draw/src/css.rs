//! The CSS of an SVG's `<style>`, as much as a static drawing needs: rules whose selectors are
//! element names and classes joined by spaces (descendants), with their specificity and order.
//! A selector with anything else (`:hover`, `:not(...)`, `>`, an attribute) is about a page that
//! is being used, not about the drawing, and is left out; `!important` is ignored.

/// One part of a selector: an element name, its classes, or both (`text.on`, `.nc.must`).
#[derive(Debug, Clone, PartialEq)]
struct Compound {
    tag: Option<String>,
    classes: Vec<String>,
}

impl Compound {
    fn matches(&self, tag: &str, classes: &[String]) -> bool {
        self.tag.as_ref().is_none_or(|t| t == tag)
            && self.classes.iter().all(|c| classes.contains(c))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Rule {
    selector: Vec<Compound>,
    /// Classes, then element names: what makes one rule win over another.
    specificity: (usize, usize),
    order: usize,
    declarations: Vec<(String, String)>,
}

/// The rules of a style sheet.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Sheet {
    rules: Vec<Rule>,
}

/// An element as a selector sees it: its name and its classes.
pub(crate) struct Element<'a> {
    pub tag: &'a str,
    pub classes: Vec<String>,
}

impl Sheet {
    /// The rules of `text`, added after the ones there are.
    pub fn add(&mut self, text: &str) {
        let text = strip_comments(text);
        let mut rest = text.as_str();
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let selectors = &rest[..open];
            let body = &rest[open + 1..open + close];
            rest = &rest[open + close + 1..];
            let declarations = declarations(body);
            if declarations.is_empty() {
                continue;
            }
            for selector in selectors.split(',') {
                if let Some(selector) = parse_selector(selector) {
                    let specificity = (
                        selector.iter().map(|c| c.classes.len()).sum(),
                        selector.iter().filter(|c| c.tag.is_some()).count(),
                    );
                    let order = self.rules.len();
                    self.rules.push(Rule {
                        selector,
                        specificity,
                        order,
                        declarations: declarations.clone(),
                    });
                }
            }
        }
    }

    /// The declarations of the rules that match the last of `path` (the element, after its
    /// ancestors from the root), the weakest first, so that a later one wins.
    pub fn matching(&self, path: &[Element]) -> Vec<(String, String)> {
        let Some((element, ancestors)) = path.split_last() else {
            return Vec::new();
        };
        let mut hits: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|rule| {
                let Some((last, before)) = rule.selector.split_last() else {
                    return false;
                };
                if !last.matches(element.tag, &element.classes) {
                    return false;
                }
                // Each compound before the last matches an ancestor, further out than the one
                // after it.
                let mut up = ancestors.len();
                for compound in before.iter().rev() {
                    let Some(found) = ancestors[..up]
                        .iter()
                        .rposition(|a| compound.matches(a.tag, &a.classes))
                    else {
                        return false;
                    };
                    up = found;
                }
                true
            })
            .collect();
        hits.sort_by_key(|r| (r.specificity, r.order));
        hits.iter()
            .flat_map(|r| r.declarations.iter().cloned())
            .collect()
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// `name: value; ...` as pairs, the names in lower case.
pub(crate) fn declarations(body: &str) -> Vec<(String, String)> {
    body.split(';')
        .filter_map(|d| {
            let (name, value) = d.split_once(':')?;
            let name = name.trim();
            let value = value.trim().trim_end_matches("!important").trim();
            (!name.is_empty() && !value.is_empty()).then(|| {
                let name = if name.starts_with("--") {
                    name.to_string()
                } else {
                    name.to_ascii_lowercase()
                };
                (name, value.to_string())
            })
        })
        .collect()
}

fn parse_selector(text: &str) -> Option<Vec<Compound>> {
    let text = text.trim();
    if text.is_empty() || text.contains([':', '>', '+', '~', '[', '*', '#']) {
        return None;
    }
    text.split_whitespace()
        .map(|part| {
            let mut pieces = part.split('.');
            let tag = pieces.next().filter(|t| !t.is_empty()).map(str::to_string);
            let classes: Vec<String> = pieces.map(str::to_string).collect();
            if classes.iter().any(String::is_empty) {
                return None;
            }
            Some(Compound { tag, classes })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el<'a>(tag: &'a str, classes: &str) -> Element<'a> {
        Element {
            tag,
            classes: classes.split_whitespace().map(str::to_string).collect(),
        }
    }

    #[test]
    fn descendant_rules_match_by_specificity_then_order() {
        let mut sheet = Sheet::default();
        sheet.add(
            ".schematic text{fill:red;stroke:blue} /* x */ .schematic .port text{stroke:none}\n\
             .a:hover .b{fill:green} .x, .schematic .net-label{fill:#075c2b}",
        );
        let path = [el("svg", "schematic"), el("g", "port"), el("text", "")];
        assert_eq!(
            sheet.matching(&path),
            vec![
                ("fill".to_string(), "red".to_string()),
                ("stroke".to_string(), "blue".to_string()),
                ("stroke".to_string(), "none".to_string()),
            ]
        );
        let label = [
            el("svg", "schematic"),
            el("g", "labels"),
            el("text", "net-label"),
        ];
        let got = sheet.matching(&label);
        assert_eq!(got.last().unwrap().1, "#075c2b");
    }
}
