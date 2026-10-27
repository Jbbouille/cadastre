//! The draw.io export: frames as containers, nodes in them, edges attached to their ends, laid
//! out by Graphviz (`-Tjson`) or by the compiler.

use cadastre_core::{Compilation, SourceFile, build_with, compile, drawio};

const REGISTRY: &str = "team t
site dc {
  hosting: on-prem
  network lan { subnet apps { cidr: 10.0.0.0/24, server web01 { ip: 10.0.0.10 }, server db01 { } } }
}
system shop {
  owner: t
  app front { on: web01, -> store : tcp/5432 [flow] }
  database store { on: db01 }
}
";

fn compilation(view: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: view.into() },
    ])
}

fn id(c: &Compilation, name: &str) -> u32 {
    c.registry.ids().find(|&i| c.registry.qualified_name(i) == name).unwrap().0
}

/// The `<object>` of a cell, from its id to its closing tag.
fn object<'a>(file: &'a str, id: &str) -> &'a str {
    let start = file.find(&format!("<object id=\"{id}\"")).unwrap_or_else(|| panic!("no cell {id} in {file}"));
    let end = start + file[start..].find("</object>").unwrap();
    &file[start..end]
}

const NETWORK: &str = "view net { select server group-by subnet edges lift([flow], via: on) render drawio }";

#[test]
fn laid_out_by_the_compiler() {
    let c = compilation(NETWORK);
    let (outputs, _) = build_with(&c, None, None);
    let file = &outputs[0].content;
    assert!(file.starts_with("<mxfile host=\"cadastre\">"), "{file}");
    // The frame of the subnet, a container with its title and the icon of its kind.
    let frame = object(file, "f0");
    assert!(frame.contains("label=\"apps (10.0.0.0/24)\""), "{frame}");
    assert!(frame.contains("container=1") && frame.contains("image=data:image/svg+xml,"), "{frame}");
    // A node in it, carrying its data; the edge attached to the cells of its ends.
    let web01 = id(&c, "dc.lan.apps.web01");
    let node = object(file, &format!("n{web01}"));
    assert!(node.contains("parent=\"f0\""), "{node}");
    assert!(
        node.contains("name=\"dc.lan.apps.web01\"")
            && node.contains("kind=\"server\"")
            && node.contains("ip=\"10.0.0.10\"")
    );
    let edge = object(file, "e0");
    assert!(edge.contains(&format!("source=\"n{web01}\"")) && edge.contains("label=\"tcp/5432\""), "{edge}");
    assert!(edge.contains("edgeStyle=orthogonalEdgeStyle") && edge.contains("endArrow=block"), "{edge}");
}

#[test]
fn laid_out_by_graphviz() {
    let c = compilation(NETWORK);
    let (web01, db01) = (id(&c, "dc.lan.apps.web01"), id(&c, "dc.lan.apps.db01"));
    // Points, origin at the bottom left; nodes in inches.
    let layout = format!(
        r#"{{"bb": "0,0,300,120", "objects": [
            {{"name": "cluster_0", "bb": "10,10,290,110"}},
            {{"name": "n{web01}", "pos": "60,50", "width": "1", "height": "0.5"}},
            {{"name": "n{db01}", "pos": "240,50", "width": "1", "height": "0.5"}},
            {{"name": "e0", "pos": "150,50", "width": "0.5", "height": "0.2"}}
        ]}}"#
    );
    let file = drawio(&c, "net", Some(&layout)).unwrap();
    // The frame at the margin (20, 20); the nodes relative to it: 60 - 36 - 10 = 14, 120 - 50 - 18 - 10 = 42.
    assert!(object(&file, "f0").contains("x=\"20\" y=\"20\" width=\"280\" height=\"100\""), "{file}");
    assert!(object(&file, &format!("n{web01}")).contains("x=\"14\" y=\"42\" width=\"72\" height=\"36\""), "{file}");
    assert!(object(&file, &format!("n{db01}")).contains("x=\"194\" y=\"42\""), "{file}");
    // A layout of another view is ignored: the compiler lays it out.
    let other = drawio(&c, "net", Some(r#"{"bb": "0,0,10,10", "objects": []}"#)).unwrap();
    assert_eq!(other, drawio(&c, "net", None).unwrap());
    assert!(drawio(&c, "nowhere", None).is_none());
}

#[test]
fn escaped_and_well_formed() {
    let c =
        compilation("view v { select app, database edges [flow] render drawio { node_labels: [label, description] } }");
    let file = drawio(&c, "v", None).unwrap();
    // Every tag closed: as many `<object` as `</object>`.
    assert_eq!(file.matches("<object ").count(), file.matches("</object>").count());
    assert_eq!(file.matches("<object ").count(), 3);
    assert!(!file.contains("\"<") && !file.contains("<br>"), "labels are escaped in attributes: {file}");
}
