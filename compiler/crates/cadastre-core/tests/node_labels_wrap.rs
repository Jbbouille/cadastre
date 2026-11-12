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
