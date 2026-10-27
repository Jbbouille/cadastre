//! Grouping (spec § 6, `group-by`): servers with the system of what they host, frames of values
//! (a free key, the components of the apps).

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
team it
site dc {
  hosting: on-prem
  network lan {
    subnet apps {
      cidr: 10.0.0.0/24
      server billing-srv { owner: it }
      server orders-srv { owner: it }
      server shared-srv { owner: it }
    }
  }
}
system billing {
  owner: it
  app portal { on: billing-srv, middleware: "JBoss 4.2", component jboss { version: 4.2.0 } }
  app engine { on: shared-srv, middleware: "WildFly", component wildfly { version: 15.0.1 } }
}
system orders {
  owner: it
  app front { on: orders-srv, middleware: "JBoss 4.2", component jboss { version: 4.2.0 } }
  app batch { on: shared-srv }
}
"#;

fn text(views: &str, view: &str) -> String {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let errors: Vec<_> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let (outputs, _) = build(&c, None);
    outputs.into_iter().find(|o| o.view == view).unwrap().content
}

fn compilation(views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

#[test]
fn a_server_goes_with_the_system_of_what_it_hosts() {
    let out = text("view v {\n  select app, server group-by system\n  render text\n}", "v");
    // A dedicated server in the frame of its system; the shared one outside any.
    assert!(out.contains("  billing\n    billing-srv [server]\n    portal [app]\n    engine [app]\n"), "{out}");
    assert!(out.contains("  orders\n    orders-srv [server]\n    front [app]\n    batch [app]\n"), "{out}");
    assert!(out.contains("\n  shared-srv [server]\n"), "{out}");
    // The shared servers gathered by a zone.
    let out = text(
        "view v {\n  select app, server group-by system\n  zone shared \"Shared servers\" where kind = server and shared\n  render text\n}",
        "v",
    );
    assert!(out.contains("  Shared servers [zone]\n    shared-srv [server]\n"), "{out}");
}

#[test]
fn frames_of_a_free_key() {
    let out = text("view v {\n  select app group-by middleware\n  render text\n}", "v");
    assert!(out.contains("  JBoss 4.2\n    portal [app]\n    front [app]\n  WildFly\n    engine [app]\n"), "{out}");
    // Without the key: outside any frame.
    assert!(out.contains("\n  batch [app]\n"), "{out}");
}

#[test]
fn frames_of_components_within_sites() {
    let out = text("view v {\n  select app group-by site > component\n  render text\n}", "v");
    // The apps reach their site through their server; within it, a frame per component.
    assert!(
        out.contains("  dc (on-prem)\n    jboss 4.2.0\n      portal [app]\n      front [app]\n    wildfly 15.0.1\n      engine [app]\n    batch [app]\n"),
        "{out}"
    );
}

#[test]
fn an_unknown_level() {
    let c = compilation("view v {\n  select app group-by middlewares\n}");
    let errors: Vec<_> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
    assert!(errors.iter().any(|e| e.contains("middlewares")), "{errors:?}");
}
