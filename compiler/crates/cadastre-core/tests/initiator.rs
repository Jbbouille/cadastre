//! `initiator`: with `orientation: data`, a dot at the end that opens the connection, the
//! arrowhead following the data.

use cadastre_core::{SourceFile, build, compile};

const REGISTRY: &str = r#"
app reader { -> files : sftp [flow] { exchange: pull } }
app files
app sender { -> files : sftp [flow] { exchange: push } }
"#;

fn dot(render: &str) -> String {
    let views = format!("view v {{\n  select app\n  edges [flow]\n  render dot {{ {render} }}\n}}\n");
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views },
    ]);
    build(&c, None).0.into_iter().find(|o| o.format == "dot").unwrap().content
}

#[test]
fn a_dot_at_the_end_that_opens_the_connection() {
    let data = dot("orientation: data  legend: true  edge_labels: none");
    let edge = |from: &str, to: &str| {
        data.lines()
            .find(|l| l.contains(&format!("{from} -> ")) && l.contains(to) && l.contains("dir="))
            .unwrap_or_else(|| panic!("{from} -> {to}: {data}"))
            .to_owned()
    };
    // `sender` pushes: drawn from it, its dot at the tail. `reader` pulls: drawn from `files`,
    // the data going to it, its dot against it at the head.
    let (sender, files, reader) = ("n2", "n1", "n0");
    let push = edge(sender, files);
    assert!(push.contains("arrowtail=dot") && push.contains("arrowhead=normal"), "{push}");
    let pull = edge(files, reader);
    assert!(pull.contains("arrowtail=none") && pull.contains("arrowhead=dotnormal"), "{pull}");
    assert!(data.contains("the end that opens the connection"), "{data}");
    // A labeled edge, in two halves through its label: the dot on the half at the initiator.
    let labeled = dot("orientation: data");
    assert!(labeled.lines().any(|l| l.starts_with("  n2 -> e") && l.contains("arrowtail=dot")), "{labeled}");
    assert!(labeled.lines().any(|l| l.contains(" -> n0 ") && l.contains("arrowhead=dotnormal")), "{labeled}");
    // Off on demand, and in the orientation of the connections, where the arrow says it.
    for render in ["orientation: data  initiator: false", ""] {
        let plain = dot(render);
        assert!(!plain.contains("arrowtail=dot") && !plain.contains("dotnormal"), "{plain}");
    }
}
