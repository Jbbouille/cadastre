//! `parent:`: a resource declared at the root of a file placed in another resource, as if nested.

use cadastre_core::{Compilation, Severity, SourceFile, compile};

const SITE: &str = "team t
site dc-alpha {
  hosting: on-prem
  egress: [fw01]
  firewall fw01
  network lan { subnet servers { cidr: 10.1.0.0/24, server web01 { ip: 10.1.0.10 } } }
}
system billing { owner: t, database db { on: web01 } }
";

fn compilation(other: &str) -> Compilation {
    compile(&[
        SourceFile { path: "sites/dc-alpha.ca".into(), text: SITE.into() },
        SourceFile { path: "infra/billing.ca".into(), text: other.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

fn id(c: &Compilation, name: &str) -> cadastre_core::registry::ResourceId {
    c.registry.ids().find(|&i| c.registry.qualified_name(i) == name).unwrap_or_else(|| panic!("{name} not found"))
}

#[test]
fn placed_as_if_nested() {
    let c = compilation(
        "server billing-srv01 { parent: dc-alpha.lan.servers, ip: 10.1.0.11 }
         server billing-srv02 { parent: servers, ip: 10.1.0.12 }
         app report { parent: billing, on: billing-srv01, -> db : tcp/5432 [flow] }",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    // Qualified names, as if written in the block.
    let srv = id(&c, "dc-alpha.lan.servers.billing-srv01");
    id(&c, "dc-alpha.lan.servers.billing-srv02");
    // Lexical scope of the system: `db` is billing's; ownership inherited from it.
    let report = id(&c, "billing.report");
    assert_eq!(c.model.inherited(&c.registry, report, "owner"), Some(id(&c, "t")));
    assert_eq!(c.model.link_ends[0].map(|(_, t)| c.registry.qualified_name(t)).as_deref(), Some("billing.db"));
    // The editors find the parent from its name.
    assert!(c.references(id(&c, "dc-alpha.lan.servers"), false).len() >= 2);
    assert_eq!(c.registry.resource(srv).parent, Some(id(&c, "dc-alpha.lan.servers")));
}

#[test]
fn checks_apply_where_it_is_placed() {
    // An address outside the subnet, as when nested.
    let c = compilation("server billing-srv01 { parent: servers, ip: 10.9.0.11 }");
    assert!(errors(&c).iter().any(|e| e.contains("10.9.0.11")), "{:?}", errors(&c));
    // An approximate placement makes the resource incomplete; an impossible one is an error.
    let c = compilation("server billing-srv01 { parent: dc-alpha }\nserver x { parent: billing }");
    assert!(
        c.diagnostics.iter().any(|d| d.severity == Severity::Incomplete && d.message.contains("approximate placement"))
    );
    assert_eq!(errors(&c), ["server cannot be placed in system billing"]);
}

#[test]
fn chains_of_placements() {
    // A parent placed by `parent:` itself, written in any order.
    let c = compilation(
        "server s1 { parent: lan.extra }
         subnet extra { parent: dc-alpha.lan, cidr: 10.2.0.0/24 }",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    id(&c, "dc-alpha.lan.extra.s1");
}

#[test]
fn invalid_placements() {
    let c = compilation(
        "server a { parent: nowhere }
         site s2 { hosting: cloud, parent: dc-alpha }
         system x { parent: y }
         system y { parent: x }",
    );
    let messages = errors(&c);
    for expected in ["unknown parent: nowhere", "a site is placed at the root: it has no parent"] {
        assert!(messages.iter().any(|m| m == expected), "{expected} not in {messages:?}");
    }
    assert!(messages.iter().any(|m| m.contains("cannot be placed in itself")), "{messages:?}");

    let nested = compile(&[SourceFile {
        path: "a.ca".into(),
        text: "site s { hosting: cloud, vpc v { subnet n { cidr: 10.0.0.0/24, server a { parent: n } } } }".into(),
    }]);
    assert_eq!(errors(&nested), ["server a is already placed in subnet n by nesting"]);
}
