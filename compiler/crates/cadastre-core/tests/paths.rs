//! Network paths deduced from the cabling: the five cases of the backup of a retail map,
//! modeled on two sites, west (gs) and east (col).

use cadastre_core::path::{Network, NoPath};
use cadastre_core::{Compilation, Severity, SourceFile, compile};

const REGISTRY: &str = r#"
site gs {
  hosting: colocation
  network adm {
    subnet esx     { cidr: 10.140.65.0/24, vlan: 65 }
    subnet dmz     { cidr: 10.140.40.0/24, vlan: 40 }
    subnet servers { cidr: 10.140.50.0/24, vlan: 50 }
  }
  switch gs-sw-01 {
    interface eth1-45 { peer: col-sw-01.eth1-47, vlans: [65] }
    interface eth1-11 { vlan: 65 }
    interface eth1-12 { peer: esx-02.p1, vlan: 65 }
    interface eth1-20 { peer: web01.p1, vlan: 40 }
    interface fw-dmz  { peer: fw.dmz, vlan: 40 }
  }
  switch gs-sw-02 {
    interface eth1-45 { peer: col-sw-02.eth1-47, vlans: [65] }
    interface eth1-12 { peer: esx-02.p2, vlan: 65 }
    interface eth1-21 { peer: db01.p1, vlan: 50 }
    interface fw-srv  { peer: fw.srv, vlan: 50 }
  }
  firewall fw {
    interface dmz { subnet: dmz, ip: 10.140.40.1 }
    interface srv { subnet: servers, ip: 10.140.50.1 }
  }
  server esx-02 {
    ip: 10.140.65.102
    interface p1
    interface p2
  }
  server web01 {
    ip: 10.140.40.10
    interface p1
  }
  server db01 {
    ip: 10.140.50.10
    interface p1
  }
}

site col {
  hosting: on-prem
  switch col-sw-01 {
    interface eth1-47
    interface eth1-11 { peer: esx-01.p1 }
    interface eth1-29 { peer: esx-05.p1 }
  }
  switch col-sw-02 {
    interface eth1-47
    interface eth1-11 { peer: esx-01.p2 }
    interface eth1-29 { peer: esx-05.p2 }
  }
  server esx-01 {
    ip: 10.140.65.101
    interface p1
    interface p2
  }
  server esx-05 {
    ip: 10.140.65.105
    interface p1
    interface p2
  }
  virtual server prox01 { ip: 10.140.65.32, on: esx-01 }
}

system veeam {
  app proxy { on: prox01 }
}
app web { on: web01 }
database db { on: db01 }
"#;

fn compilation(registry: &str) -> Compilation {
    let c = compile(&[SourceFile { path: "a.ca".into(), text: registry.into() }]);
    let errors: Vec<String> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

/// The steps of a path, as names, each step's variants joined by ` or `.
fn steps(c: &Compilation, from: &str, to: &str) -> Result<Vec<String>, NoPath> {
    let id = |name: &str| c.registry.ids().find(|&r| c.registry.resource(r).name == name).unwrap();
    let network = Network::new(&c.registry, &c.model);
    network.between(id(from), id(to)).map(|path| {
        path.steps
            .iter()
            .map(|step| step.iter().map(|&d| c.registry.resource(d).name.clone()).collect::<Vec<_>>().join(" or "))
            .collect()
    })
}

#[test]
fn the_backup_reads_an_esx_of_the_other_site_through_the_interconnection() {
    let c = compilation(REGISTRY);
    // Case 1: the proxy, a virtual machine on esx-01 at east (col), to esx-02 at west (gs), in VLAN 65:
    // the switches of both sites in pairs, the interconnection, no firewall.
    assert_eq!(steps(&c, "proxy", "esx-02").unwrap(), ["col-sw-01 or col-sw-02", "gs-sw-01 or gs-sw-02"]);
    // Case 2: to an ESX of east, the switches of east only.
    assert_eq!(steps(&c, "proxy", "esx-05").unwrap(), ["col-sw-01 or col-sw-02"]);
}

#[test]
fn between_subnets_the_path_goes_through_their_gateway() {
    let c = compilation(REGISTRY);
    // Case 3: from the DMZ to the servers, through the firewall holding both gateways.
    assert_eq!(steps(&c, "web", "db").unwrap(), ["gs-sw-01", "fw", "gs-sw-02"]);
}

#[test]
fn a_port_without_a_cable_and_a_vlan_not_carried_make_no_path() {
    let c = compilation(REGISTRY);
    // Case 5: gs-sw-01 eth1-11, left tagged for esx-01 without a cable, makes no way to it.
    assert_eq!(steps(&c, "esx-01", "esx-02").unwrap(), ["col-sw-01 or col-sw-02", "gs-sw-01 or gs-sw-02"]);
    // The interconnection not carrying VLAN 65: the cables stop at the switches of east.
    let pruned = REGISTRY.replace("vlans: [65] }", "vlans: [64] }");
    let c = compilation(&pruned);
    let error = steps(&c, "proxy", "esx-02").unwrap_err();
    let network = Network::new(&c.registry, &c.model);
    assert_eq!(network.explain(&error), "the cables stop at col.col-sw-01, col.col-sw-02 in VLAN 65");
    // A subnet without a gateway.
    let ungated = REGISTRY.replace("interface dmz { subnet: dmz, ip: 10.140.40.1 }", "interface dmz");
    let c = compilation(&ungated);
    let network = Network::new(&c.registry, &c.model);
    assert_eq!(
        network.explain(&steps(&c, "web", "db").unwrap_err()),
        "no router nor firewall routes subnet gs.adm.dmz (declare its `gateway`)"
    );
}

#[test]
fn flows_get_the_through_of_the_cabling() {
    let registry = REGISTRY
        .replace("  app proxy { on: prox01 }", "  app proxy {\n    on: prox01\n    -> esx-02 : nfc/902 [flow]\n  }")
        .replace(
            "app web { on: web01 }",
            "app web {\n  on: web01\n  -> stripe : https/443 [flow]\n  -> db : jdbc [flow]\n  -> db : https [flow] { through: [gs-sw-02] }\n}\nsaas stripe",
        )
        .replace("site gs {\n  hosting: colocation", "site gs {\n  hosting: colocation\n  egress: [fw]");
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry },
        SourceFile {
            path: "v.cav".into(),
            text: "view v {\n  select app, database, saas, server\n  edges [flow]\n  render text\n}".into(),
        },
    ]);
    let (outputs, _) = cadastre_core::build(&c, None);
    let text = &outputs[0].content;
    for line in [
        // Case 1, on a flow: each step with its variants.
        "through: col-sw-01 or col-sw-02, gs-sw-01 or gs-sw-02 (deduced: cabling)",
        // Case 4: the way to the egress, then the egress.
        "through: gs-sw-01, fw (deduced: cabling, egress of site gs)",
        // Case 3, on a flow.
        "through: gs-sw-01, fw, gs-sw-02 (deduced: cabling)",
        // A written `through` wins.
        "through: gs-sw-02\n",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    // Filters see it.
    let c2 = compile(&[
        SourceFile {
            path: "a.ca".into(),
            text: REGISTRY.replace(
                "  app proxy { on: prox01 }",
                "  app proxy {\n    on: prox01\n    -> esx-02 : nfc/902 [flow]\n  }",
            ),
        },
        SourceFile {
            path: "v.cav".into(),
            text: "view v {\n  select app, server\n  edges [flow] where through has gs-sw-02\n  render text\n}".into(),
        },
    ]);
    let (outputs, _) = cadastre_core::build(&c2, None);
    assert!(outputs[0].content.contains("veeam.proxy -> gs.esx-02"), "{}", outputs[0].content);
    // A written `through` off the path of the cabling: a warning.
    let off =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.clone()).collect::<Vec<_>>();
    assert!(off.is_empty(), "{off:?}");
    let wrong = compile(&[SourceFile {
        path: "a.ca".into(),
        text: REGISTRY.replace(
            "app web { on: web01 }",
            "app web {\n  on: web01\n  -> db : jdbc [flow] { through: [col-sw-01] }\n}",
        ),
    }]);
    assert!(
        wrong
            .diagnostics
            .iter()
            .any(|d| d.message == "col.col-sw-01 is not on the path of the cabling between web and db"),
        "{:?}",
        wrong.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

fn view(registry: &str, views: &str) -> (String, Vec<String>) {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let errors: Vec<String> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let (outputs, diagnostics) = cadastre_core::build(&c, None);
    let warnings = diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.clone()).collect();
    (outputs[0].content.clone(), warnings)
}

#[test]
fn a_view_draws_the_path_between_two_resources() {
    let (text, warnings) = view(
        REGISTRY,
        "view v {\n  select app, server, switch without isolated\n  edges path(veeam.proxy, esx-02)\n  render text\n}",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    // The cables of the path between devices, the app on its virtual machine, the machine on
    // its host; nothing else.
    for edge in [
        "veeam.proxy -> col.prox01",
        "col.prox01 -> col.esx-01",
        "col.col-sw-01 -- col.esx-01",
        "col.col-sw-02 -- col.esx-01",
        "gs.gs-sw-01 -- col.col-sw-01",
        "gs.gs-sw-02 -- col.col-sw-02",
        "gs.gs-sw-01 -- gs.esx-02",
        "gs.gs-sw-02 -- gs.esx-02",
    ] {
        assert!(text.contains(edge), "{edge}\n{text}");
    }
    assert!(!text.contains("esx-05") && !text.contains("web01"), "{text}");
    // No path: the reason, where it stops.
    let (_, warnings) =
        view(REGISTRY, "view v {\n  select app, server, switch\n  edges path(web, esx-01)\n  render text\n}");
    assert_eq!(
        warnings,
        ["view v: path(web, esx-01): no path: no router nor firewall routes subnet gs.adm.esx (declare its `gateway`)"]
    );
    // Two devices routing a subnet without a `gateway`.
    let twice = REGISTRY.replace(
        "  server esx-02 {",
        "  router r2 { interface dmz { subnet: dmz, ip: 10.140.40.2 } }\n  server esx-02 {",
    );
    let (_, warnings) = view(&twice, "view v {\n  select app, server, switch\n  edges path(web, db)\n  render text\n}");
    assert_eq!(
        warnings,
        ["view v: path(web, db): subnet gs.adm.dmz is routed by several devices: gs.fw, gs.r2 (declare its `gateway`)"]
    );
    let declared = twice.replace(
        "subnet dmz     { cidr: 10.140.40.0/24, vlan: 40 }",
        "subnet dmz { cidr: 10.140.40.0/24, vlan: 40, gateway: fw.dmz }",
    );
    let (_, warnings) =
        view(&declared, "view v {\n  select app, server, switch\n  edges path(web, db)\n  render text\n}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn aggregated_cables_are_one_link_and_project_keys_now_built_in_are_warned() {
    // Two cables of a port-channel between the same switches: one edge.
    let doubled = REGISTRY
        .replace(
            "    interface eth1-45 { peer: col-sw-01.eth1-47, vlans: [65] }",
            "    interface eth1-45 { peer: col-sw-01.eth1-47, vlans: [65], aggregate: \"po1\" }\n    interface eth1-46 { peer: col-sw-01.eth1-48, vlans: [65], aggregate: \"po1\" }",
        )
        .replace("    interface eth1-47\n    interface eth1-11 { peer: esx-01.p1 }", "    interface eth1-47\n    interface eth1-48\n    interface eth1-11 { peer: esx-01.p1 }");
    let (text, _) = view(
        &doubled,
        "view v {\n  select app, server, switch without isolated\n  edges path(veeam.proxy, esx-02)\n  render text\n}",
    );
    assert_eq!(text.matches("gs.gs-sw-01 -- col.col-sw-01").count(), 1, "{text}");
    // `vlan: number` declared by a project before Cadastre had it: a warning, not an error.
    let c = compile(&[SourceFile { path: "a.ca".into(), text: format!("kind subnet {{ vlan: number }}\n{REGISTRY}") }]);
    let messages: Vec<(Severity, String)> = c.diagnostics.iter().map(|d| (d.severity, d.message.clone())).collect();
    assert!(messages.contains(&(Severity::Warning, "vlan is now a built-in key of subnet".to_owned())), "{messages:?}");
    assert!(!messages.iter().any(|(s, _)| *s == Severity::Error), "{messages:?}");
}

#[test]
fn a_path_to_or_from_the_internet_goes_through_the_egress_or_the_ingress() {
    let registry = REGISTRY.replace(
        "site gs {\n  hosting: colocation",
        "site gs {\n  hosting: colocation\n  egress: [fw]\n  ingress: [fw]",
    ) + "saas stripe\n";
    let views = "view out {\n  select app, saas, server, switch, firewall without isolated\n  edges path(web, stripe)\n  render text\n}\n\
                 view in {\n  select app, saas, server, switch, firewall without isolated\n  edges path(stripe, web)\n  render text\n}";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.clone() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let (outputs, diagnostics) = cadastre_core::build(&c, None);
    assert!(diagnostics.iter().all(|d| d.severity != Severity::Warning), "{diagnostics:?}");
    let of = |view: &str| outputs.iter().find(|o| o.view == view).unwrap().content.clone();
    // To the SaaS: the cables up to the firewall of the egress, then to the SaaS.
    let out = of("out");
    for edge in ["gs.gs-sw-01 -- gs.web01", "gs.gs-sw-01 -- gs.fw", "gs.fw -> stripe", "web -> gs.web01"] {
        assert!(out.contains(edge), "{edge}\n{out}");
    }
    // From the SaaS: through the ingress, the same cables.
    let inward = of("in");
    assert!(inward.contains("stripe -> gs.fw") && inward.contains("gs.gs-sw-01 -- gs.web01"), "{inward}");
    // Without an egress: why.
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: format!("{REGISTRY}saas stripe\n") },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let (_, diagnostics) = cadastre_core::build(&c, None);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message == "view out: path(web, stripe): no path: web has no `egress` towards the internet"),
        "{:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// The texts of an SVG, in order.
fn texts(svg: &str) -> Vec<&str> {
    svg.split("</text>")
        .filter_map(|chunk| chunk.rfind('>').map(|at| &chunk[at + 1..]))
        .filter(|t| !t.trim().is_empty())
        .collect()
}

#[test]
fn a_traversal_draws_a_row_per_step_through_its_devices() {
    let registry = REGISTRY.replace(
        "  app proxy { on: prox01 }",
        "  app proxy {\n    on: prox01\n    -> esx-02 : tcp/902\n  }\n  process backup {\n    \
         step proxy -> esx-02 : tcp/902 { label: \"reads the blocks\" }\n    step proxy \"compresses\"\n    \
         step esx-02 -> proxy { reply, label: \"the blocks\" }\n  }",
    );
    let views = "view v {\n  select app, server\n  edges process(backup)\n  \
                 render traversal { edge_labels: [port, interface], title: false }\n}";
    let (svg, warnings) = view(&registry, views);
    assert!(warnings.is_empty(), "{warnings:?}");
    let texts = texts(&svg);
    // A row per step between two participants, numbered with the processings; the proxy on its
    // virtual machine on its host; the switches in pairs, crossed again by the reply, the other
    // way; the interfaces of the cables, redundant ones together.
    assert_eq!(
        texts,
        [
            "1. reads the blocks (TCP 902)",
            "proxy",
            "on prox01",
            "on esx-01",
            "p1, p2 → eth1-11",
            "col-sw-01",
            "or col-sw-02",
            "eth1-47 → eth1-45",
            "gs-sw-01",
            "or gs-sw-02",
            "eth1-12 → p1, p2",
            "esx-02",
            "3. the blocks",
            "esx-02",
            "p1, p2 → eth1-12",
            "gs-sw-01",
            "or gs-sw-02",
            "eth1-45 → eth1-47",
            "col-sw-01",
            "or col-sw-02",
            "eth1-11 → p1, p2",
            "proxy",
            "on prox01",
            "on esx-01",
        ]
    );
}

#[test]
fn a_traversal_draws_the_path_between_two_resources() {
    let views = "view v {
  select app, server
  edges path(veeam.proxy, esx-02)
                   edges path(web, esx-01)
  render traversal
}";
    let (svg, _) = view(REGISTRY, views);
    // A row per path, from the first resource to the second; without a path, why.
    assert_eq!(
        texts(&svg),
        [
            "proxy → esx-02",
            "proxy",
            "on prox01",
            "on esx-01",
            "col-sw-01",
            "or col-sw-02",
            "gs-sw-01",
            "or gs-sw-02",
            "esx-02",
            "web → esx-01: no path, no router nor firewall routes subnet gs.adm.esx (declare its `gateway`)",
            "web",
            "on web01",
            "esx-01",
        ]
    );
}
