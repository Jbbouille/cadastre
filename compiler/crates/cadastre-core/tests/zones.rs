//! Zones (spec § 6, `zone`): named frames of a view gathering their members, nodes or frames,
//! whatever their `group-by` frames.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// Sites operated by a provider, some of them for SAP; a network operator; a site on Internet.
const REGISTRY: &str = r#"
team hostco
team telco
site site-east { hosting: colocation, operator: hostco }
site site-south { hosting: colocation, operator: hostco }
site site-north { hosting: colocation, operator: hostco }
site site-west { hosting: colocation, operator: hostco }
site net-site { hosting: colocation, operator: telco }
site internet { hosting: cloud, scope: external }
"#;

fn compilation(view: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: view.into() },
    ])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

fn output(c: &Compilation, format: &str) -> String {
    let (outputs, _) = build(c, None);
    outputs.into_iter().find(|o| o.format == format).unwrap().content
}

#[test]
fn members_leave_their_group_by_frames() {
    let c = compilation(
        "view wan {\n  select site group-by operator\n  \
         zone sap \"Hostco SAP\" [site-east, site-south] { fill: \"#eef\" }\n  \
         zone outside where scope = external\n  render text, dot\n}",
    );
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let text = output(&c, "text");
    assert!(
        text.contains(
            "  hostco\n    site-north [site]\n    site-west [site]\n  telco\n    net-site [site]\n  \
             Hostco SAP [zone]\n    site-east [site]\n    site-south [site]\n  outside [zone]\n    internet [site]\n"
        ),
        "{text}"
    );
    // A dashed, rounded frame, with its fill.
    let dot = output(&c, "dot");
    assert!(
        dot.contains("label=\"Hostco SAP\"; style=\"rounded,dashed,filled\"; color=\"#999999\"; fillcolor=\"#eef\";"),
        "{dot}"
    );
}

#[test]
fn a_frame_comes_with_its_content_and_an_empty_frame_is_not_drawn() {
    // The frame of telco goes into the zone; hostco, emptied by the other zone, is not drawn.
    let c = compilation(
        "view wan {\n  select site group-by operator\n  zone carriers [telco]\n  \
         zone dc [site-east, site-south, site-north, site-west]\n  render text\n}",
    );
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let text = output(&c, "text");
    assert!(
        text.contains("  carriers [zone]\n    telco\n      net-site [site]\n  dc [zone]\n    site-east [site]\n"),
        "{text}"
    );
    assert!(!text.contains("  hostco\n"), "{text}");
}

#[test]
fn zones_in_stack_and_cross() {
    let c = compilation(
        "view wan {\n  select site\n  zone sap [site-east, site-south]\n  zone dc [site-north, site-west]\n  \
         render dot { direction: tb, stack: [sap, net-site, dc], cross: [dc, internet] }\n}",
    );
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let dot = output(&c, "dot");
    let hints = dot.lines().filter(|l| l.contains("style=invis")).count();
    // From the two sites of sap to net-site, from net-site to the two sites of dc.
    assert_eq!(hints, 4, "{dot}");
}

#[test]
fn errors_and_warnings() {
    let c = compilation(
        "view wan {\n  select site\n  zone a [site-east, nowhere]\n  zone b [site-east] { shape: box }\n  zone a [site-north]\n}",
    );
    let errors = messages(&c, Severity::Error);
    assert!(errors.contains(&"dangling reference: nowhere".to_owned()), "{errors:?}");
    assert!(errors.contains(&"unknown zone property: shape".to_owned()), "{errors:?}");
    assert!(errors.contains(&"zone a declared twice".to_owned()), "{errors:?}");

    // Evaluation: a member in two zones; a member that is not in the view.
    let c = compilation(
        "view wan {\n  select site where operator = hostco\n  zone a [site-east]\n  zone b [site-east]\n  zone c [net-site]\n  render text\n}",
    );
    let (_, diagnostics) = build(&c, None);
    let found: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        found,
        ["view wan: site-east is in the zones a and b", "view wan: zone c: net-site is not in the view"],
        "{found:?}"
    );
}

#[test]
fn nested_zones() {
    // The SAP sites in a zone of the provider, itself in the zone of the data centers.
    let c = compilation(
        "view wan {\n  select site\n  zone dc \"Data centers\" [] \n  \
         zone hostco [site-north, site-west] { in: dc }\n  zone sap \"SAP\" [site-east, site-south] { in: hostco }\n  \
         render text, dot { stack: [sap, net-site] }\n}",
    );
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let text = output(&c, "text");
    assert!(
        text.contains(
            "  Data centers [zone]\n    hostco [zone]\n      SAP [zone]\n        site-east [site]\n        site-south [site]\n      \
             site-north [site]\n      site-west [site]\n"
        ),
        "{text}"
    );
    let dot = output(&c, "dot");
    assert!(dot.contains("label=\"SAP\""), "{dot}");
}

#[test]
fn nested_zone_errors() {
    let c = compilation(
        "view wan {\n  select site\n  zone a [site-east] { in: b }\n  zone b [site-west] { in: a }\n  \
         zone c [site-north] { in: dd }\n  render text\n}",
    );
    let errors = messages(&c, Severity::Error);
    assert!(errors.contains(&"zone a: its parents make a cycle (a → b → a)".to_owned()), "{errors:?}");
    assert!(errors.contains(&"zone c: dd is not a zone of the view".to_owned()), "{errors:?}");

    let c = compilation(
        "view wan {
  select site
  zone a [site-east]
  zone b [site-west] { parent: a }
}",
    );
    assert_eq!(messages(&c, Severity::Error), ["the zone a zone is drawn in is written `in:`"]);
}
