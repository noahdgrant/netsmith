//! [`Rule::Naming`]: the names the guide teaches (`docs/language/03-definitions.md`): a
//! definition, a variant of an enum and a type parameter are `CamelCase`; a member, a pin a pin
//! table declares, a layout guideline and an anchor are `snake_case`. A quoted name (`'5'`) is
//! as the datasheet prints it and is not judged, and neither is what a datasheet names: a
//! peripheral, a function of a pin, a shared resource, a net.
//!
//! `CamelCase` is a first letter that is a capital and no `_` but between two digits (an acronym
//! stays one: `USBC`, `Esp32S3`; a part number keeps its numbers apart: `HroK3_1270`);
//! `snake_case` is no capital letter.

use netsmith_lang::ast::{self, DefKind, Ident, Item, LayoutStatement};

use crate::{Context, Lint, Rule, SourceFile};

pub(crate) fn lint(cx: &Context) -> Vec<Lint> {
    let mut out = Vec::new();
    for file in cx.files.values() {
        let mut walk = Walk {
            file,
            out: &mut out,
        };
        for def in &file.ast.definitions {
            walk.definition(def);
        }
        for p in &file.ast.physicals {
            walk.items(&p.items);
        }
        for l in &file.ast.layouts {
            walk.layout(l);
        }
    }
    out
}

/// Whether `name` is `CamelCase`. A `_` between two digits keeps two numbers apart, as a part
/// number does (`HroK3_1270`), and is allowed.
pub(crate) fn is_camel(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    chars.first().is_some_and(|c| c.is_uppercase())
        && chars.iter().enumerate().all(|(i, &c)| {
            c != '_'
                || (i > 0
                    && chars[i - 1].is_ascii_digit()
                    && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        })
}

/// Whether `name` is `snake_case`.
pub(crate) fn is_snake(name: &str) -> bool {
    !name.chars().any(char::is_uppercase)
}

/// `name` in `CamelCase`: each word after a `_` starts with a capital (`power_stage` is
/// `PowerStage`), and so does the first.
pub(crate) fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in name.chars() {
        if c == '_' {
            up = true;
            continue;
        }
        if up {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        up = false;
    }
    out
}

/// `name` in `snake_case`: a capital after a small letter or a digit starts a word (`VinSense`
/// is `vin_sense`), and a run of capitals is one (`ADC` is `adc`, `USBPort` is `usb_port`).
pub(crate) fn snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1);
            let starts = match prev {
                None | Some('_') => false,
                Some(p) if p.is_lowercase() || p.is_ascii_digit() => true,
                Some(p) => p.is_uppercase() && next.is_some_and(|n| n.is_lowercase()),
            };
            if starts {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

struct Walk<'a, 'o> {
    file: &'a SourceFile,
    out: &'o mut Vec<Lint>,
}

impl Walk<'_, '_> {
    fn camel(&mut self, what: &str, name: &Ident) {
        if name.quoted || is_camel(&name.name) {
            return;
        }
        let message = format!(
            "`{}` is {what}, whose name is `CamelCase` in the guide; call it `{}`",
            name.name,
            camel(&name.name)
        );
        self.out
            .push(self.file.lint(Rule::Naming, name.span, message));
    }

    fn snake(&mut self, what: &str, name: &Ident, dot: bool) {
        if name.quoted || is_snake(&name.name) {
            return;
        }
        let dot = if dot { "." } else { "" };
        let message = format!(
            "`{dot}{}` is {what}, whose name is `snake_case` in the guide; call it `{dot}{}`",
            name.name,
            snake(&name.name)
        );
        self.out
            .push(self.file.lint(Rule::Naming, name.span, message));
    }

    fn definition(&mut self, def: &ast::Definition) {
        let what = match def.kind {
            DefKind::Board => "a board",
            DefKind::System => "a system",
            DefKind::Panel => "a panel",
            DefKind::Part => "a part",
            DefKind::Interface => "an interface",
            DefKind::Module => "a module",
            DefKind::Enum => "an enum",
            DefKind::Generic => "a generic",
            DefKind::Enclosure => "an enclosure",
        };
        self.camel(what, &def.name);
        for p in &def.params {
            self.camel("a type parameter", &p.name);
        }
        for v in &def.variants {
            self.camel("a variant of an enum", &v.name);
        }
        self.items(&def.items);
    }

    fn items(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Decl(d) => {
                    if let [name] = d.name.segments.as_slice() {
                        self.snake("a member", name, d.name.leading_dot);
                    }
                    if let Some(body) = &d.body {
                        self.items(body);
                    }
                }
                Item::Pins(p) => {
                    for row in &p.rows {
                        if let [name] = row.pin.segments.as_slice() {
                            self.snake("a pin", name, row.pin.leading_dot);
                        }
                    }
                }
                Item::Connect(c) => {
                    for g in &c.groups {
                        if let Some(body) = &g.body {
                            self.items(body);
                        }
                    }
                }
                Item::Case(c) => self.items(&c.items),
                Item::Support(s) => self.items(&s.items),
                Item::Defaults(d) => self.items(&d.items),
                Item::Layout(l) => self.layout(l),
                Item::Anchor(a) => self.snake("an anchor", &a.name, true),
                _ => {}
            }
        }
    }

    fn layout(&mut self, layout: &ast::Layout) {
        for s in &layout.statements {
            if let LayoutStatement::Guideline(g) = s {
                self.snake("a layout guideline", &g.name, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_spelt_the_guide_s_way() {
        assert_eq!(camel("power_stage"), "PowerStage");
        assert_eq!(camel("buck"), "Buck");
        assert_eq!(snake("VinSense"), "vin_sense");
        assert_eq!(snake("ADC"), "adc");
        assert_eq!(snake("USBPort"), "usb_port");
        assert_eq!(snake("R1"), "r1");
        assert_eq!(snake("led2Out"), "led2_out");
        assert!(is_camel("USBC") && is_camel("Esp32S3") && is_camel("HroK3_1270"));
        assert!(!is_camel("Power_Stage") && !is_camel("Stage_1") && !is_camel("Stage1_"));
        assert!(is_snake("vin_2") && !is_snake("vIn"));
    }
}
