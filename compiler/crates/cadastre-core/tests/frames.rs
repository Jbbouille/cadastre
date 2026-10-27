//! Nodes drawn as frames (spec § 6, `group-by`): a server that is a node of the view and also
//! the frame of what runs on it (`group-by … > on`) is drawn once, as that frame.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  network lan {
    subnet servers {
      cidr: 10.0.0.0/24
      cluster vcenter
      server esx1 { cluster: vcenter }
      server esx2 { cluster: vcenter }
    }
  }
}
app vm { on: esx1 }
app console { -> esx1 : https/443 [flow], -> vm : ssh [flow] }
"#;

fn compilation(style: &str) -> Compilation {
    let view = format!(
        "view hosts {{\n  select server, app group-by cluster > on\n  edges [flow]\n  style {{ {style} }}\n  render dot, text, mermaid\n}}"
    );
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: view },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

fn output(c: &Compilation, format: &str) -> String {
    let (outputs, _) = build(c, None);
    outputs.into_iter().find(|o| o.format == format).unwrap().content
}

/// The DOT identifier of a resource (`n12`), from the registry order.
fn id(c: &Compilation, name: &str) -> String {
    format!("n{}", c.registry.ids().find(|&i| c.registry.resource(i).name == name).unwrap().0)
}

#[test]
fn a_hosting_server_is_drawn_once_as_a_frame() {
    let c = compilation("");
    let dot = output(&c, "dot");
    let (esx1, esx2, vm, console) = (id(&c, "esx1"), id(&c, "esx2"), id(&c, "vm"), id(&c, "console"));

    // esx1 hosts vm: a frame with its text and icon, square corners, a black outline; inside it,
    // vm and an invisible point standing for esx1.
    let frame = dot.lines().position(|l| l.contains("class=\"kind_server\";")).unwrap();
    let header = dot.lines().nth(frame).unwrap();
    assert!(header.contains("<td>esx1</td>") && header.contains("color=\"black\";"), "{header}");
    assert!(!header.contains("rounded"), "{header}");
    assert!(dot.contains(&format!("{esx1} [shape=point, style=invis")), "{dot}");
    assert!(!dot.lines().any(|l| l.trim_start().starts_with(&format!("{esx1} [label="))), "{dot}");
    // esx2 hosts nothing: a node.
    assert!(dot.lines().any(|l| l.trim_start().starts_with(&format!("{esx2} [label="))), "{dot}");

    // An edge to esx1 stops at the border of its frame; an edge to vm, inside it, does not.
    let to_esx1 = dot.lines().find(|l| l.contains(&format!(" -> {esx1} "))).unwrap();
    assert!(to_esx1.contains("lhead=cluster_"), "{to_esx1}");
    let to_vm = dot.lines().find(|l| l.contains(&format!(" -> {vm} "))).unwrap();
    assert!(!to_vm.contains("lhead"), "{to_vm}");
    assert!(dot.lines().any(|l| l.contains(&format!("{console} -> "))));

    // Text: esx1 once, with its kind, and vm under it.
    let text = output(&c, "text");
    assert_eq!(text.matches("esx1").count(), 3, "{text}"); // the frame line, and two edges
    assert!(text.contains("    esx1 [server]\n      vm [app]\n"), "{text}");

    // Mermaid: esx1 is its subgraph, and links to it target the subgraph.
    let mermaid = output(&c, "mermaid");
    let group = mermaid.lines().find(|l| l.contains("[\"esx1\"]")).unwrap().trim().to_owned();
    let subgraph = group.trim_start_matches("subgraph ").split('[').next().unwrap().to_owned();
    assert!(mermaid.contains(&format!("{console} -->|\"https/443\"| {subgraph}")), "{mermaid}");
    assert!(!mermaid.contains(&format!("  {esx1}[")), "{mermaid}");
}

#[test]
fn node_rules_style_a_node_drawn_as_a_frame() {
    let c = compilation("nodes where cluster = vcenter { color: \"#2563eb\" }");
    let dot = output(&c, "dot");
    let header = dot.lines().find(|l| l.contains("class=\"kind_server\";")).unwrap();
    assert!(header.contains("color=\"#2563eb\";"), "{header}");
}

#[test]
fn a_resource_on_several_hosts_stays_in_the_outer_frame() {
    let view = "view hosts {\n  select server, app group-by cluster > on\n  render text\n}";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "b.ca".into(), text: "app ha { on: [esx1, esx2] }".into() },
        SourceFile { path: "v.cav".into(), text: view.into() },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let text = output(&c, "text");
    // Beside the two servers, in the frame of their cluster.
    assert!(
        text.contains(
            "  vcenter
    esx1 [server]
      vm [app]
    esx2 [server]
    ha [app]
"
        ),
        "{text}"
    );
}
