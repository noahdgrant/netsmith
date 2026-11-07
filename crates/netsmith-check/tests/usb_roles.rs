//! A receptacle passes the USB bus through (ADR 0064, section 10): `UsbRole::Passthrough` is
//! not an end of the bus, so it counts toward no limit, satisfies the `bus role` guideline, and
//! the rules judge the end behind it.
//!
//! The parts are invented for these tests.

use netsmith_check::*;

const PARTS: &str = r#"
import std::bus::{Usb2, UsbRole}

/// Zjack, an invented receptacle: the bus goes through it.
part Zjack {
    .usb: Usb2
    .usb.role &= UsbRole::Passthrough

    pins {
        .usb.vbus.pos power         "VBUS" "1"
        .usb.vbus.neg power         "GND"  "2"
        .usb.dp       bidirectional "D+"   "3"
        .usb.dm       bidirectional "D-"   "4"
    }
}

/// Zhost, an invented USB host.
part Zhost {
    .usb: Usb2
    .usb.role &= UsbRole::Host

    pins {
        .usb.vbus.pos power         "VBUS" "1"
        .usb.vbus.neg power         "GND"  "2"
        .usb.dp       bidirectional "D+"   "3"
        .usb.dm       bidirectional "D-"   "4"
    }
}

/// Zdevice, an invented USB device.
part Zdevice {
    .usb: Usb2
    .usb.role &= UsbRole::Device

    pins {
        .usb.vbus.pos power         "VBUS" "1"
        .usb.vbus.neg power         "GND"  "2"
        .usb.dp       bidirectional "D+"   "3"
        .usb.dm       bidirectional "D-"   "4"
    }
}

/// Zunknown, an invented part whose datasheet is not at hand: it states no role.
part Zunknown {
    .usb: Usb2

    pins {
        .usb.vbus.pos power         "VBUS" "1"
        .usb.vbus.neg power         "GND"  "2"
        .usb.dp       bidirectional "D+"   "3"
        .usb.dm       bidirectional "D-"   "4"
    }
}
"#;

fn run(parts: &[(&str, &str)], joins: &str) -> Checked {
    let mut body = String::new();
    for (name, ty) in parts {
        body.push_str(&format!("    .{name}: {ty}\n"));
    }
    let src = format!("{PARTS}\nboard B {{\n{body}{joins}\n}}\n");
    check("main.nsm", &src, &Loader::std())
}

/// The messages of the checker, the guideline and the note for the bus roles, in that order.
fn findings(c: &Checked) -> (Vec<String>, Vec<String>, Vec<String>) {
    let model = &c.board("B").unwrap().model;
    (
        c.diagnostics.iter().map(|d| d.message.clone()).collect(),
        joined_without_role(model)
            .into_iter()
            .map(|(_, m)| m)
            .collect(),
        unstated_roles(model).into_iter().map(|(_, m)| m).collect(),
    )
}

fn clean(c: &Checked) {
    let (errors, guideline, note) = findings(c);
    assert!(
        errors.is_empty() && guideline.is_empty() && note.is_empty(),
        "{errors:#?}\n{guideline:#?}\n{note:#?}"
    );
}

#[test]
fn a_receptacle_joined_to_a_host_says_nothing() {
    clean(&run(
        &[("jack", "Zjack"), ("host", "Zhost")],
        "    .jack.usb ~ .host.usb\n",
    ));
}

#[test]
fn a_receptacle_joined_to_a_device_says_nothing() {
    clean(&run(
        &[("jack", "Zjack"), ("dev", "Zdevice")],
        "    .jack.usb ~ .dev.usb\n",
    ));
}

#[test]
fn a_receptacle_between_a_host_and_a_device_says_nothing() {
    // The host reaches the device through the jack: the pins are joined one at a time, and the
    // jack's role disagrees with neither end.
    clean(&run(
        &[("host", "Zhost"), ("jack", "Zjack"), ("dev", "Zdevice")],
        "    .host.usb ~ .jack.usb\n    .jack.usb ~ .dev.usb\n",
    ));
}

#[test]
fn two_receptacles_back_to_back_say_nothing() {
    clean(&run(
        &[("a", "Zjack"), ("b", "Zjack")],
        "    .a.usb ~ .b.usb\n",
    ));
}

#[test]
fn a_receptacle_counts_toward_no_limit() {
    // Two receptacles and one host on one bus: only the host is a `Host`.
    clean(&run(
        &[("a", "Zjack"), ("b", "Zjack"), ("host", "Zhost")],
        "    .a.usb ~ .b.usb\n    .a.usb ~ .host.usb\n",
    ));
    // Two hosts behind receptacles are still two hosts, and the message lists no other end for
    // the jacks, as it lists the slave beside two masters.
    let c = run(
        &[
            ("h1", "Zhost"),
            ("h2", "Zhost"),
            ("a", "Zjack"),
            ("b", "Zjack"),
        ],
        "    .h1.usb ~ .a.usb\n    .a.usb ~ .b.usb\n    .b.usb ~ .h2.usb\n",
    );
    let (errors, ..) = findings(&c);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(
        errors[0],
        "`h1.usb` and `h2.usb` are both `UsbRole::Host` on one `Usb2` bus, and `Usb2` allows 1 `Host` on a bus: they are on one bus through `net-a.usb.dm` (`a.usb.dm`, `b.usb.dm`, `h1.usb.dm` and `h2.usb.dm`); give `h2.usb` a bus of its own"
    );
}

#[test]
fn the_guideline_names_the_end_behind_a_receptacle_and_not_the_receptacle() {
    let c = run(
        &[("jack", "Zjack"), ("what", "Zunknown")],
        "    .jack.usb ~ .what.usb\n",
    );
    let (errors, guideline, note) = findings(&c);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(
        guideline,
        [
            "`what.usb` is joined to another end of a `Usb2` bus and states no role, so the limit and the table of `what` are not judged: write `.usb.role &=` `UsbRole::Host` or `UsbRole::Device`, as its datasheet says"
        ]
    );
    // No end of the bus states a role, so the note says nothing either.
    assert!(note.is_empty(), "{note:#?}");
}

#[test]
fn the_note_names_the_end_that_states_none_and_never_the_receptacle() {
    let c = run(
        &[("jack", "Zjack"), ("host", "Zhost"), ("what", "Zunknown")],
        "    .jack.usb ~ .host.usb\n    .jack.usb ~ .what.usb\n",
    );
    let (_, guideline, note) = findings(&c);
    assert_eq!(guideline.len(), 1, "{guideline:#?}");
    assert!(
        guideline[0].starts_with("`what.usb` is joined"),
        "{guideline:#?}"
    );
    assert_eq!(
        note,
        [
            "not evaluated: the role of `what.usb` is not stated, so the limit and the table of `what` are not judged"
        ]
    );
}

#[test]
fn without_a_receptacle_two_parts_that_state_no_role_are_both_advised() {
    // The same board with no receptacle advises both ends.
    let c = run(
        &[("what", "Zunknown"), ("other", "Zunknown")],
        "    .what.usb ~ .other.usb\n",
    );
    let (_, guideline, _) = findings(&c);
    assert_eq!(guideline.len(), 2, "{guideline:#?}");
}

#[test]
fn the_receptacle_of_the_library_passes_the_bus_through() {
    let src = format!(
        "import std::parts::connector::UsbC2\n{PARTS}\nboard B {{\n    .jack: UsbC2\n    .host: Zhost\n    .jack.usb ~ .host.usb\n}}\n"
    );
    clean(&check("main.nsm", &src, &Loader::std()));
}

#[test]
fn a_peripheral_cannot_be_a_pass_through() {
    let src = r#"
import std::bus::{Usb2, UsbRole}

part Zmcu {
    peripheral USB0 on APP_GPIO as Usb2 UsbRole::Passthrough {
        USB_DP:bidirectional as dp
    }

    pins {
        .p00 bidirectional "P0.00" "1" fn APP_GPIO
    }
}

board B {
    .mcu: Zmcu
}
"#;
    let c = check("main.nsm", src, &Loader::std());
    let m: Vec<String> = c.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert_eq!(m, ["`Usb2` has no `lines` for `UsbRole::Passthrough`"]);
}
