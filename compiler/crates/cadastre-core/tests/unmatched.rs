//! Filters that match nothing in a view (spec § 7): a warning, with a hint when a compared value
//! is the label of a resource, or a short name compared to `name`.

use cadastre_core::{Diagnostic, SourceFile, build, compile};

const REGISTRY: &str = r#"
server filehub-01 { label: "FILE-HUB", -> billing.engine : sftp [flow] }
system billing {
  app engine { -> portal : https [flow] }
  app portal
}
"#;

fn warnings(view: &str) -> Vec<Diagnostic> {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: view.into() },
    ]);
    let (_, diagnostics) = build(&c, None);
    diagnostics
}

#[test]
fn a_style_rule_matching_a_label() {
    let found = warnings(
        "view v {\n  select app, server\n  edges [flow]\n  style { edges where source.name = file-hub { layout: free } }\n}",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].message, "view v: this style rule applies to nothing in the view");
    assert_eq!(
        found[0].help.as_deref(),
        Some("`file-hub` is the label of server filehub-01; filters compare names: `in [filehub-01]`")
    );
}

#[test]
fn an_edge_filter_comparing_a_short_name() {
    let found = warnings("view v {\n  select app, server\n  edges [flow] where source.name = engine\n}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].message, "view v: the where of this edge selector keeps no edge");
    assert_eq!(
        found[0].help.as_deref(),
        Some("`name` is the qualified name (billing.engine): write `source in [engine]`")
    );
    // Inside a lift, the same.
    let found = warnings(
        "view v {\n  select system, server\n  edges lift([flow] where target.name = portal, via: ancestor(system)?)\n}",
    );
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_zone_filter_matching_nothing() {
    let found = warnings("view v {\n  select app\n  zone z where label = \"FILE-HUB\"\n}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].message, "view v: zone z: its filter matches no node nor frame");
}

#[test]
fn no_warning_when_filters_match() {
    let found = warnings(
        "view v {\n  select app, server\n  edges [flow] where source in [engine]\n  \
         zone z where label = \"FILE-HUB\"\n  style { edges where source in [engine] { layout: free } }\n}",
    );
    // The edge filter keeps engine -> portal; the zone holds the server; the rule applies.
    assert!(found.is_empty(), "{found:?}");
}
