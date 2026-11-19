//! Soldered mates (issue #1320): pins in plated holes (`soldered_pin`), edge half-holes
//! (`castellated`) and a module's lands on a land pattern (`land_grid`). A module meets a host,
//! contact N to contact N, with the same checks contact by contact as a header in a socket.

use netsmith_check::*;

fn run(src: &str) -> Checked {
    check("mate.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// A module soldered on a host with a joint of `kind`, the module's gender `module_gender`
/// and the host's `host_gender`, the module's contact 1 on its ground and contact 2 on its
/// supply, the host's on `host1` and `host2` of its rail.
fn kit(kind: &str, module_gender: &str, host_gender: &str, host1: &str, host2: &str) -> String {
    format!(
        "
import std::parts::connector::{{Connector, Mating}}
import std::sources::Rail

/// The lands of a module's edge, two of them. Invented.
part Edge provides Connector {{
    pins {{
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
    }}
    .mating: Mating {{
        .kind &= \"{kind}\"
        .gender &= \"{module_gender}\"
        .pitch &= 1.27mm
        .rows &= 1
        .positions &= 2
    }}
}}

/// The land pattern of a host for `Edge`. Invented.
part Lands provides Connector {{
    pins {{
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
    }}
    .mating: Mating {{
        .kind &= \"{kind}\"
        .gender &= \"{host_gender}\"
        .pitch &= 1.27mm
        .rows &= 1
        .positions &= 2
    }}
}}

board Module {{
    .supply: Rail {{ .pos.net &= \"3V3\"; .current &= -20mA..-5mA }}
    .edge: Edge
    .supply.neg ~ .edge.p1
    .supply.pos ~ .edge.p2
}}

board Host {{
    .rail: Rail {{ .pos.net &= \"3V3\"; .voltage &= 3.3V +/- 5% }}
    .lands: Lands
    .rail.{host1} ~ .lands.p1
    .rail.{host2} ~ .lands.p2
}}

system Kit {{
    .host: Host
    .module: Module
    .host.lands.mating ~ .module.edge.mating
}}
"
    )
}

#[test]
fn a_module_on_its_host_mates_with_each_soldered_kind() {
    for kind in ["soldered_pin", "castellated", "land_grid"] {
        let c = run(&kit(kind, "module", "host", "neg", "pos"));
        assert!(c.is_ok(), "{kind}: {}", c.render());
        let m = &c.system("Kit").expect("the system").model;
        assert_eq!(m.mates().len(), 1, "{kind}");
        let roles: Vec<_> = m.mates()[0]
            .contacts
            .iter()
            .map(|c| (c.a.role, c.b.role))
            .collect();
        assert_eq!(
            roles,
            [(Role::Ground, Role::Ground), (Role::Power, Role::Power)],
            "{kind}"
        );
        assert_eq!(MateKind::of(kind).map(MateKind::soldered), Some(true));
    }
}

#[test]
fn a_soldered_mate_has_the_role_checks_of_a_header_in_a_socket() {
    // The host has its supply where the module has its ground.
    let c = run(&kit("castellated", "module", "host", "pos", "neg"));
    let msgs = messages(&c);
    assert!(
        msgs.iter().any(|m| m.contains("contact 1 of the mated connectors")
            && m.contains("is ground on one board and power on the other")),
        "{msgs:#?}"
    );
}

#[test]
fn a_module_mates_with_a_host_and_never_with_a_module() {
    let c = run(&kit("land_grid", "module", "module", "neg", "pos"));
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("`.host.lands` and `.module.edge` are both modules (Lands and Edge)")
            && msgs[0].contains("a module mates with a host"),
        "{}",
        msgs[0]
    );
}

#[test]
fn a_soldered_kind_is_a_module_or_a_host() {
    let c = run(&kit("soldered_pin", "module", "socket", "neg", "pos"));
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("has the gender `socket`; a `soldered_pin` is a `module` or a `host`"),
        "{}",
        msgs[0]
    );
}

#[test]
fn a_kind_that_is_none_names_the_kinds() {
    let c = run(&kit("press_fit", "module", "host", "neg", "pos"));
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains(
            "are `press_fit` connectors, and the mate kinds are `pin_header`, `soldered_pin`, `castellated`, `land_grid`"
        ),
        "{}",
        msgs[0]
    );
}
