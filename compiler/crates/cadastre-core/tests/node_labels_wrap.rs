//! `node_labels_wrap`: the lines of the nodes and frames of a diagram cut between words at a
//! width, whatever the key shown (a long `description` no longer stretches its box).

use cadastre_core::{Severity, SourceFile, build_with, compile};

const REGISTRY: &str = r#"
system middleware {
  description: "The integration platform: the extractors read their source base, the integrators write their target base"
  app hub {
    description: "Orchestrates the flows between the applications"
    -> loader [flow]
  }
  app loader
}
"#;

fn outputs(render: &str) -> Vec<(String, String)> {
    let view = format!(
        "view flows {{\n  select app group-by system\n  edges [flow]\n  render {render} {{ node_labels: [label, description], node_labels_wrap: 30 }}\n}}\n"
    );
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: view },
    ]);
    let errors: Vec<&str> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.as_str()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    build_with(&c, Some("flows"), None).0.into_iter().map(|o| (o.format, o.content)).collect()
}

#[test]
fn the_lines_of_nodes_and_frames_are_cut_between_words() {
    let dot = &outputs("dot")[0].1;
    // The node: its name, then its description in lines of 30 characters at most.
    assert!(dot.contains("hub<br/>Orchestrates the flows between<br/>the applications"), "{dot}");
    // The frame: its description too.
    assert!(dot.contains("middleware<br/>The integration platform: the<br/>extractors read their source"), "{dot}");
}

#[test]
fn the_text_keeps_whole_lines() {
    let text = &outputs("text")[0].1;
    assert!(text.contains("hub / Orchestrates the flows between the applications"), "{text}");
}

/// The DOT of the view with these render options and this style.
fn dot_with(options: &str, style: &str) -> String {
    let view = format!(
        "view flows {{\n  select app group-by system\n  edges [flow]\n  style {{ {style} }}\n  render dot {{ node_labels: [label, description]{options} }}\n}}\n"
    );
    let registry = REGISTRY.replace("-> loader [flow]", "-> loader [flow] { carries: [orders, invoices, payments] }")
        + "data orders\ndata invoices\ndata payments\n";
    let c =
        compile(&[SourceFile { path: "a.ca".into(), text: registry }, SourceFile { path: "v.cav".into(), text: view }]);
    let errors: Vec<&str> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.as_str()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    build_with(&c, Some("flows"), None).0.remove(0).content
}

#[test]
fn a_style_rule_wraps_a_node_or_a_frame_alone() {
    let dot = dot_with("", "nodes where name = middleware.hub { wrap: 30 }");
    assert!(dot.contains("hub<br/>Orchestrates the flows between<br/>the applications"), "{dot}");
    // The frame keeps its line: the rule is for the node.
    assert!(dot.contains("The integration platform: the extractors read their source base"), "{dot}");
    let dot = dot_with("", "kind system { wrap: 30 }");
    assert!(dot.contains("middleware<br/>The integration platform: the<br/>"), "{dot}");
    assert!(dot.contains("Orchestrates the flows between the applications"), "{dot}");
}

#[test]
fn a_style_rule_wins_over_the_option_of_the_view() {
    let dot = dot_with(", node_labels_wrap: 30", "nodes where name = middleware.hub { wrap: 60 }");
    assert!(dot.contains("hub<br/>Orchestrates the flows between the applications"), "{dot}");
    assert!(dot.contains("middleware<br/>The integration platform: the<br/>"), "{dot}");
}

#[test]
fn a_style_rule_wraps_the_label_of_an_edge() {
    let dot = dot_with(", edge_labels: [carries]", "edges where source.name = middleware.hub { wrap: values }");
    assert!(dot.contains(r"orders\ninvoices\npayments"), "{dot}");
}
