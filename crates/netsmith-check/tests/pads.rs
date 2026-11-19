//! The `pads` block of a part (issue #1321): a row for a pad of the part's pin table, the one
//! field `.contact_offset`, a point of two lengths, and what the model keeps of it. The part
//! `SmdRow4` is invented.

use netsmith_check::*;

fn part(pads: &str) -> String {
    format!(
        "\
import std::parts::connector::Connector

part SmdRow4 provides Connector {{
    pins \"SMD\" \"THT\" {{
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
        .p3 passive \"P3\" \"3\"
        .p4 passive \"P4\" \"4\"
    }}
    .package: Package
{pads}
}}

board B {{
    .j: SmdRow4 {{ .package &= \"SMD\" }}
}}
"
    )
}

fn checked(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn messages(src: &str) -> Vec<String> {
    checked(src)
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn a_pads_block_states_where_each_contact_is() {
    let src = part(
        "    pads \"SMD\" {\n        \"1\" .contact_offset = (1.175mm, 0mm)\n        \"2\" .contact_offset = (-1.175mm, 0mm)\n    }",
    );
    let c = checked(&src);
    assert!(c.diagnostics.is_empty(), "{:#?}", c.diagnostics);
    let model = &c.boards[0].model;
    let found: Vec<(Option<&str>, &str, (f64, f64))> = model
        .pad_contacts
        .iter()
        .map(|p| (p.package.as_deref(), p.pad.as_str(), p.offset))
        .collect();
    assert_eq!(
        found,
        [
            (Some("SMD"), "1", (1.175, 0.0)),
            (Some("SMD"), "2", (-1.175, 0.0))
        ]
    );
}

#[test]
fn a_pads_block_is_printed_back_as_it_is_written() {
    let src = part("    pads {\n        \"3\" .contact_offset = (0mm, -0.5mm)\n    }");
    let printed = netsmith_lang::format(&src).unwrap();
    assert!(
        printed.contains("    pads {\n        \"3\" .contact_offset = (0mm, -0.5mm)\n    }"),
        "{printed}"
    );
    assert!(messages(&src).is_empty(), "{:#?}", messages(&src));
}

#[test]
fn a_pad_the_pin_table_lacks_a_field_a_pad_lacks_and_a_value_not_a_length_are_refused() {
    let unknown = messages(&part(
        "    pads {\n        \"9\" .contact_offset = (1mm, 0mm)\n    }",
    ));
    assert!(
        unknown.iter().any(|m| m.contains("the part's pin table has no pad \"9\"")),
        "{unknown:#?}"
    );
    let field = messages(&part("    pads {\n        \"1\" .contact = (1mm, 0mm)\n    }"));
    assert!(
        field.iter().any(|m| m.contains("a pad has no field `contact`")),
        "{field:#?}"
    );
    let volts = messages(&part(
        "    pads {\n        \"1\" .contact_offset = (1V, 0mm)\n    }",
    ));
    assert!(
        volts
            .iter()
            .any(|m| m.contains("`.contact_offset` of pad \"1\"") && m.contains("`1V`")),
        "{volts:#?}"
    );
    let package = messages(&part(
        "    pads \"QFN\" {\n        \"1\" .contact_offset = (1mm, 0mm)\n    }",
    ));
    assert!(
        package
            .iter()
            .any(|m| m.contains("no pin table for the package \"QFN\"")),
        "{package:#?}"
    );
}

#[test]
fn only_a_parts_own_definition_holds_a_pads_block() {
    let src = "\
board B {
    pads {
        \"1\" .contact_offset = (1mm, 0mm)
    }
}
";
    let all = messages(src);
    assert!(
        all.iter()
            .any(|m| m.contains("`pads` can only appear directly inside a part's own definition")),
        "{all:#?}"
    );
}
