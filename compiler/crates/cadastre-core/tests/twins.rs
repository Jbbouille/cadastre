//! Twins: in a frame, the nodes of one kind with exactly the same edges are drawn as one box (SVG).

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// A database read by four apps the same way, one of them doubtful, and a fifth app writing it.
const REGISTRY: &str = r#"
team t
site dc { hosting: on-prem, network lan { subnet s { cidr: 10.0.0.0/24, server srv { owner: t } } } }
system sales {
  owner: t
  database db { on: srv }
  app a1 { on: srv, -> db : tcp/5432 [flow] }
  app a2 { on: srv, -> db : tcp/5432 [flow] }
  app a3 { on: srv, -> db : tcp/5432 [flow] }
  app a4 { on: srv, -> db : tcp/5432 [flow] { confidence: doubtful } }
  app writer { on: srv, -> db : tcp/5432 [flow], -> a4 : https/443 [flow] }
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

#[test]
fn nodes_with_the_same_edges_are_one_box() {
    let dot = dot("view v { select app, database group-by system\n  edges [flow]\n  render svg }");
    // a2 and a3 join a1 (a4's edge is doubtful, writer has another edge): one box of three.
    let twins: Vec<&str> = dot.lines().filter(|l| l.contains(" twins\"")).collect();
    assert_eq!(twins.len(), 1, "{dot}");
    assert!(twins[0].contains("3 apps"), "{dot}");
    for name in ["a1", "a2", "a3"] {
        assert!(twins[0].contains(&format!(">{name}</td>")), "{name}: {dot}");
    }
    // One edge from the box to the database, instead of three.
    assert_eq!(dot.lines().filter(|l| l.contains("t0 -> ")).count(), 1, "{dot}");
    // The others are still drawn on their own.
    assert!(dot.contains(">a4</td>") && dot.contains(">writer</td>"), "{dot}");
    // The tooltip of the edge says it stands for the three.
    assert!(dot.contains("one edge for the 3 apps of the box of a1"), "{dot}");
}

#[test]
fn twins_can_be_turned_off() {
    let dot = dot("view v { select app, database group-by system\n  edges [flow]\n  render svg { twins: false } }");
    assert!(!dot.contains(" twins\""), "{dot}");
    // Five edges reach the database, one per app.
    assert_eq!(dot.matches("-> n6 [").count(), 5, "{dot}");
}
