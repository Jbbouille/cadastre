//! A database on another one: the instances or schemas of a shared database, its machine
//! reached through it by the frames, `lift`, the impact analysis and the checks.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// The plan file a text goes in: a `.cac` for checks, a `.cav` for views.
fn plan_path(text: &str) -> &'static str {
    if text.trim_start().starts_with("check") { "c.cac" } else { "v.cav" }
}

const REGISTRY: &str = "team t
site dc1 {
  hosting: on-prem
  network lan { subnet data { cidr: 10.0.5.0/24, server ora01 { owner: t }, server app01 { owner: t } } }
}
site dc2 {
  hosting: on-prem
  network lan { subnet data { cidr: 10.1.5.0/24, server ora-dr01 { owner: t } } }
}
database shared-ora    { owner: t, on: ora01 }
database shared-ora-dr { owner: t, on: ora-dr01 }
shared-ora -> shared-ora-dr : oracle-dataguard [replication]
system billing {
  owner: t
  database invoices { on: shared-ora }
  app api { on: app01, -> invoices : jdbc/1521 [flow] }
}
system crm {
  owner: t
  database contacts { on: shared-ora }
}
";

fn compilation(registry: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: plan_path(views).into(), text: views.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

fn text(views: &str) -> String {
    let c = compilation(REGISTRY, views);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    outputs[0].content.clone()
}

#[test]
fn frames_within_frames() {
    let out = text("view v {\n  select server, database, app group-by on\n  render text\n}");
    // The shared database in its server, its databases in it.
    assert!(
        out.contains(
            "  ora01 [server]\n    shared-ora [database]\n      invoices [database]\n      contacts [database]\n"
        ),
        "{out}"
    );
}

#[test]
fn lifted_to_the_machine() {
    let out = text("view v {\n  select server\n  edges lift([flow], via: on)\n  render text\n}");
    assert!(out.contains("dc1.lan.data.app01 -> dc1.lan.data.ora01  [jdbc/1521]"), "{out}");
}

#[test]
fn down_with_its_database() {
    let c = compilation(REGISTRY, "");
    let failures = |name: &str| -> Vec<String> {
        let id = c.registry.ids().find(|&i| c.registry.qualified_name(i) == name).unwrap();
        c.model.hosting_failures_of(&c.registry, id).iter().map(|&f| c.registry.qualified_name(f)).collect()
    };
    assert_eq!(failures("billing.invoices"), ["dc1", "dc1.lan", "dc1.lan.data", "dc1.lan.data.ora01", "shared-ora"]);
}

#[test]
fn a_standby_on_another_site() {
    let check = "check standby-elsewhere {
  select database
  edges [replication] where source.on.ancestor(site) = target.on.ancestor(site)
  expect none
}";
    assert!(errors(&compilation(REGISTRY, check)).is_empty());
    let same_site =
        REGISTRY.replace("shared-ora-dr { owner: t, on: ora-dr01 }", "shared-ora-dr { owner: t, on: app01 }");
    assert_eq!(
        errors(&compilation(&same_site, check)),
        ["check standby-elsewhere failed: 1 edge selected, none expected: shared-ora -> shared-ora-dr"]
    );
}

#[test]
fn a_cycle_is_an_error() {
    let c = compilation("team t\ndatabase a { owner: t, on: b }\ndatabase b { owner: t, on: a }\n", "");
    assert_eq!(errors(&c), ["a runs on itself: a -> b -> a"]);
}
