//! Satellites: a node exchanging with several frames is drawn once in each of them (SVG).

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// A directory used by the apps of three systems, and a database used by one.
const REGISTRY: &str = r#"
team t
site dc { hosting: on-prem, network lan { subnet s { cidr: 10.0.0.0/24, server srv { owner: t } } } }
saas directory { owner: t }
system sales { owner: t, app shop { on: srv, -> directory : ldap/389 [auth] } }
system stock { owner: t, app wms { on: srv, -> directory : ldap/389 [auth] } }
system billing {
  owner: t
  app invoices { on: srv, -> directory : ldap/389 [auth], -> db : tcp/5432 [flow] }
  database db { on: srv }
}
"#;

fn dot(views: &str) -> String {
    let c: Compilation = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    assert!(c.diagnostics.iter().all(|d| d.severity != Severity::Error), "{:?}", c.diagnostics);
    let (outputs, _) = build(&c, None);
    outputs.into_iter().find(|o| o.format == "svg").unwrap().content
}

fn directory_id(dot: &str) -> String {
    // The first satellite is `n<id>s<frame>`.
    let line = dot.lines().find(|l| l.contains(" satellite\"")).expect("a satellite");
    let id = line.trim().split(' ').next().unwrap();
    id[..id.find('s').unwrap()].to_owned()
}

#[test]
fn a_shared_node_is_drawn_once_per_frame() {
    let dot = dot("view v { select app, database, saas group-by system\n  edges [auth], [flow]\n  render svg }");
    // Three copies, one per system frame, in gray.
    assert_eq!(dot.matches(" satellite\"").count(), 3, "{dot}");
    assert!(dot.contains("color=\"#9ca3af\""), "{dot}");
    // Each copy says in how many places the directory is drawn: the three frames.
    assert_eq!(dot.matches("directory ×3").count(), 3, "{dot}");
    // Every edge to the directory goes to a copy; the original, at the root with no edge left,
    // is not drawn.
    let id = directory_id(&dot);
    assert!(!dot.contains(&format!("-> {id} ")), "{dot}");
    assert!(!dot.lines().any(|l| l.trim().starts_with(&format!("{id} [")) && !l.contains("satellite")), "{dot}");
    // The database is used by one frame only: no satellite of it.
    assert!(!dot.contains("kind_database satellite"), "{dot}");
}

#[test]
fn satellites_off() {
    let dot = dot(
        "view v { select app, database, saas group-by system\n  edges [auth], [flow]\n  render svg { satellites: false } }",
    );
    assert_eq!(dot.matches(" satellite\"").count(), 0, "{dot}");
}
