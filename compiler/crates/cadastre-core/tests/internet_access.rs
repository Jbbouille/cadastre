//! The accesses to the internet: the WAN interface of a firewall cabled to the `internet`
//! (`peer: internet`), or a `wan_link` from a router to it, without a site nor a router standing
//! for the provider.

use cadastre_core::path::Network;
use cadastre_core::{Compilation, Severity, SourceFile, build_with, compile};

const REGISTRY: &str = r#"
team orange { scope: external }
internet internet
site west {
  firewall fw-west {
    interface wan { peer: internet, operator: orange, bandwidth: "1 Gbps" }
    interface lan { peer: sw-west.uplink }
  }
  switch sw-west { interface uplink }
}
site east {
  firewall fw-east { interface wan { peer: internet } }
  router rt-east
}
wan_link fiber-east { ends: [rt-east, internet], operator: orange }
"#;

fn compilation(plans: &str) -> Compilation {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: plans.into() },
    ]);
    let errors: Vec<String> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

#[test]
fn interfaces_of_several_sites_are_cabled_to_the_internet() {
    // No `… is cabled to several peers`: the internet is reached by many.
    compilation("");
}

#[test]
fn a_view_draws_the_accesses_to_the_internet() {
    let c = compilation(
        "view accesses {\n  select firewall, router, internet group-by site\n  edges lift(peer, via: parent), wan_link\n  render text { edge_labels: [operator, bandwidth] }\n}\n",
    );
    let (outputs, _) = build_with(&c, Some("accesses"), None);
    let text = &outputs[0].content;
    assert!(text.contains("internet -- west.fw-west  [orange, 1 Gbps]"), "{text}");
    assert!(text.contains("internet -- east.fw-east"), "{text}");
    assert!(text.contains("internet -- east.rt-east  [orange]"), "{text}");
}

#[test]
fn the_sites_are_not_joined_through_the_internet() {
    let c = compilation("");
    let id = |name: &str| c.registry.ids().find(|&r| c.registry.resource(r).name == name).unwrap();
    let network = Network::new(&c.registry, &c.model);
    // The cables of the paths: the LAN of the west, none through the internet.
    assert_eq!(network.cables_between(&[id("fw-west")], &[id("sw-west")]).len(), 1);
    assert!(network.cables_between(&[id("fw-west")], &[id("internet")]).is_empty());
    assert!(network.cables_between(&[id("internet")], &[id("fw-east")]).is_empty());
    assert!(network.between(id("sw-west"), id("fw-east")).is_err());
}
