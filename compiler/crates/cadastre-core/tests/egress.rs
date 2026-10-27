//! `egress` and `ingress`: the devices traversed by the flows to and from the internet, written
//! once on a site, a subnet, a system or an app, and deduced on each flow without `through`.

use cadastre_core::{Compilation, Severity, SourceFile, compile};

/// The plan file a text goes in: a `.cac` for checks, a `.cav` for views.
fn plan_path(text: &str) -> &'static str {
    if text.trim_start().starts_with("check") { "c.cac" } else { "v.cav" }
}

const REGISTRY: &str = "team t
site dc {
  hosting: on-prem
  egress: [fw01, px01]
  ingress: [waf01, fw01]
  firewall fw01
  firewall waf01
  forward_proxy px01
  router nat01
  network lan {
    subnet apps { cidr: 10.0.1.0/24, server web01 { } }
    subnet erp { cidr: 10.0.2.0/24, egress: [fw01], server erp01 { } }
  }
}
internet internet
saas payments { owner: t }
saas bank { owner: t }
system shop {
  owner: t
  app front {
    on: web01
    -> payments : https/443 [flow]
    -> internet : https/443 [flow]
    -> erp : tcp/1521 [flow]
    -> bank : sftp/22 [flow] { through: [nat01] }
  }
  app erp { on: erp01, -> payments : https/443 [flow] }
  app direct { on: web01, egress: [nat01], -> payments : https/443 [flow] }
}
payments -> front : https/443 [flow]
";

fn compilation(plans: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: plan_path(plans).into(), text: plans.into() },
    ])
}

/// `source -> target: through (origins)` for the flows of the registry.
fn flows(plans: &str) -> Vec<String> {
    let c = compilation(plans);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let view = &c.plans.views[0];
    let graph = cadastre_core::view::evaluate(view, &c.registry, &c.model, &mut Vec::new());
    let name = |id| c.registry.resource(id).name.clone();
    let mut out: Vec<String> = graph
        .edges
        .iter()
        .map(|e| {
            let path: Vec<String> = e.through_path.iter().map(|&t| name(t)).collect();
            let origins: Vec<String> = e.through_origins.iter().cloned().collect();
            format!("{} -> {}: [{}] {}", name(e.source), name(e.target), path.join(", "), origins.join("; "))
                .trim_end()
                .to_owned()
        })
        .collect();
    out.sort();
    out
}

#[test]
fn deduced_from_the_nearest_egress_and_ingress() {
    assert_eq!(
        flows("view v { select app, saas, internet edges [flow] }"),
        [
            // The app's own `egress` is nearer than its site's.
            "direct -> payments: [nat01] egress of app shop.direct",
            // `erp` runs on a server of the `erp` subnet: its `egress` is nearer than the site's.
            "erp -> payments: [fw01] egress of subnet dc.lan.erp",
            // A written `through` wins.
            "front -> bank: [nat01]",
            // Internal flows are not concerned.
            "front -> erp: []",
            "front -> internet: [fw01, px01] egress of site dc",
            "front -> payments: [fw01, px01] egress of site dc",
            // From the internet: the `ingress` of the target.
            "payments -> front: [waf01, fw01] ingress of site dc",
        ]
    );
}

#[test]
fn deduced_through_is_a_through() {
    // Filters, and checks: every flow to a SaaS goes through the firewall.
    assert_eq!(
        flows("view v { select app, saas edges [flow] where through has px01 }"),
        ["front -> payments: [fw01, px01] egress of site dc"]
    );
    let c = compilation(
        "check saas-behind-fw01 { select app, saas edges [flow] where target.kind = saas expect all through has fw01 }",
    );
    let failures: Vec<&str> =
        c.diagnostics.iter().map(|d| d.message.as_str()).filter(|m| m.starts_with("check ")).collect();
    assert_eq!(
        failures,
        [
            "check saas-behind-fw01 failed: 2 of 4 edges do not meet the expectation: shop.front -> bank, shop.direct -> payments"
        ]
    );
}

#[test]
fn shown_as_deduced() {
    let c = compilation("view v { select app, saas edges [flow] render text }");
    let (outputs, _) = cadastre_core::build(&c, None);
    assert!(outputs[0].content.contains("through: fw01, px01 (deduced: egress of site dc)"), "{}", outputs[0].content);
}

#[test]
fn only_traversed_devices() {
    let c = compile(&[SourceFile {
        path: "a.ca".into(),
        text: "site s { hosting: cloud, egress: [web01] }\nserver web01".into(),
    }]);
    let messages: Vec<&str> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("web01")), "{messages:?}");
}

/// `reach`: what is reached over the internet, whoever runs it.
const REACH: &str = "team t
site dc {
  hosting: on-prem
  egress: [fw01]
  firewall fw01
  network lan { subnet apps { cidr: 10.0.1.0/24, server web01 { } } }
}
site hoster { hosting: colocation, scope: external, network n { subnet s { cidr: 10.9.0.0/24, server h01 { } } } }
site partner { hosting: cloud, scope: external, reach: internet, network n { subnet s { cidr: 10.8.0.0/24, server p01 { } } } }
saas linked { owner: t, reach: internal }
saas mail { owner: t }
app keycloak { scope: external }
app carrier { scope: external, reach: internet }
app hosted { scope: external, on: h01 }
app shipping { scope: external, on: p01 }
app front {
  owner: t
  on: web01
  -> keycloak : https/443 [flow]
  -> carrier : https/443 [flow]
  -> hosted : https/443 [flow]
  -> shipping : https/443 [flow]
  -> linked : https/443 [flow]
  -> mail : smtp/25 [flow]
}
";

#[test]
fn reached_over_the_internet() {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REACH.into() },
        SourceFile { path: "v.cav".into(), text: "view v { select app, saas edges [flow] }".into() },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let graph = cadastre_core::view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    let mut out: Vec<String> = graph
        .edges
        .iter()
        .map(|e| {
            let path: Vec<&str> = e.through_path.iter().map(|&t| c.registry.resource(t).name.as_str()).collect();
            format!("{}: [{}]", c.registry.resource(e.target).name, path.join(", "))
        })
        .collect();
    out.sort();
    assert_eq!(
        out,
        [
            // `reach: internet` on the app, or on the site of the server it runs on.
            "carrier: [fw01]",
            // External, internal to the network: a directory, a hosting provider joined by the WAN.
            "hosted: []",
            "keycloak: []",
            // A SaaS through a private link.
            "linked: []",
            "mail: [fw01]",
            "shipping: [fw01]",
        ]
    );
}
