//! The ASCII art rendering.

use cadastre_core::{Severity, SourceFile, build_with, compile};

/// Renders `view` of a project given as (path, text) pairs.
fn ascii(files: &[(&str, &str)], view: &str) -> String {
    let files: Vec<SourceFile> =
        files.iter().map(|(p, t)| SourceFile { path: (*p).to_owned(), text: (*t).to_owned() }).collect();
    let c = compile(&files);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let (outputs, _) = build_with(&c, Some(view), Some(&["ascii".to_owned()]));
    assert_eq!(outputs.len(), 1);
    outputs[0].content.clone()
}

const REGISTRY: &str = "team t
site s {
  hosting: cloud
  owner: t
  vpc v {
    subnet front { cidr: 10.0.1.0/24, server web { } }
    subnet back  { cidr: 10.0.2.0/24, server db { } }
  }
}
data d
system shop {
  owner: t
  app api   { on: web, -> store : tcp/5432 [flow] }
  database store { on: db }
}
";

#[test]
fn boxes_arrows_and_labels() {
    let out = ascii(
        &[("a.ca", REGISTRY), ("v.cav", "view flows { select app, database edges [flow] render ascii }")],
        "flows",
    );
    // A database has rounded corners; the edge leaves `api` from a junction on its border.
    assert_eq!(
        out,
        "\
┌─────┐              ╭───────╮
│ api ├───tcp/5432──▶│ store │
└─────┘              ╰───────╯
"
    );
}

#[test]
fn frames_keep_their_whole_title() {
    let out = ascii(
        &[
            ("a.ca", REGISTRY),
            ("v.cav", "view net { select server group-by site > subnet edges lift([flow], via: on) render ascii }"),
        ],
        "net",
    );
    assert!(out.contains("s (cloud)"), "{out}");
    assert!(out.contains("front (10.0.1.0/24)"), "{out}");
    assert!(out.contains("back (10.0.2.0/24)"), "{out}");
    assert!(!out.contains('…'), "{out}");
}

#[test]
fn top_to_bottom() {
    let out = ascii(
        &[
            ("a.ca", REGISTRY),
            ("v.cav", "view flows { select app, database edges [flow] render ascii { direction: tb } }"),
        ],
        "flows",
    );
    assert!(out.contains('▼'), "{out}");
    let api = out.lines().position(|l| l.contains("api")).unwrap();
    let store = out.lines().position(|l| l.contains("store")).unwrap();
    assert!(api < store, "{out}");
}

#[test]
fn isolated_nodes_are_spread_over_a_grid() {
    let registry: String = (0..16).map(|i| format!("data d{i:02}\n")).collect();
    let out = ascii(&[("a.ca", &registry), ("v.cav", "view all { select data render ascii }")], "all");
    // 16 boxes of 3 lines stacked would take about 64 lines; a 4 x 4 grid about 16.
    assert!(out.lines().count() < 24, "{out}");
    assert!(out.contains("d00") && out.contains("d15"), "{out}");
}

#[test]
fn undirected_edges_have_no_arrowhead() {
    let registry = "switch sw { interface p1 { peer: a } }\nserver a";
    let out = ascii(
        &[
            ("a.ca", registry),
            ("v.cav", "view cables { select switch, server edges lift(peer, via: parent) render ascii }"),
        ],
        "cables",
    );
    assert!(out.contains("peer"), "{out}");
    assert!(!out.contains(['▶', '◀', '▼', '▲']), "{out}");
}

#[test]
fn empty_view() {
    let out = ascii(&[("a.ca", "data d"), ("v.cav", "view nothing { select server render ascii }")], "nothing");
    assert_eq!(out, "(view nothing has no nodes)\n");
}
