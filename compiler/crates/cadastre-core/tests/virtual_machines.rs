//! Virtual machines: `virtual server web01 { on: esx01 }`, a machine running on a host, drawn in
//! it, stopped by its failure; `on` only for a virtual machine, `rack` only for a physical one.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  rack r1
  network lan {
    subnet servers {
      cidr: 10.0.0.0/24
      server esx01 { ip: 10.0.0.10, rack: r1 }
      virtual server web01 { ip: 10.0.0.20, on: esx01 }
    }
  }
  virtual firewall fw { on: esx01 }
}
app shop { on: web01 }
"#;

fn compilation(registry: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

fn text(views: &str, view: &str) -> String {
    let c = compilation(REGISTRY, views);
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let (outputs, _) = build(&c, None);
    outputs.into_iter().find(|o| o.view == view).unwrap().content
}

#[test]
fn a_virtual_machine_runs_on_its_host() {
    // Drawn in the frame of its host, what runs on it in its own frame.
    let drawn = text("view v {\n  select server, app group-by site > on\n  render text\n}", "v");
    assert!(drawn.contains("    esx01 [server]\n      web01 [server]\n        shop [app]\n"), "{drawn}");
    // Selected by `virtual`.
    let selected = text("view w {\n  select server, firewall where virtual\n  render text\n}", "w");
    assert!(selected.contains("web01 [server]") && selected.contains("fw [firewall]"), "{selected}");
    assert!(!selected.contains("esx01"), "{selected}");
    // Stopped by the failure of its host, and what runs on it too.
    let impact = text("view f {\n  select app, server where failures has esx01\n  render text\n}", "f");
    assert!(impact.contains("web01 [server]") && impact.contains("shop [app]"), "{impact}");
}

#[test]
fn virtual_on_and_rack_agree() {
    let c = compilation(REGISTRY, "");
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let wrong = REGISTRY
        .replace("virtual server web01", "server web01")
        .replace("ip: 10.0.0.20, on: esx01 }", "ip: 10.0.0.20, on: esx01, rack: r1 }")
        .replace("app shop { on: web01 }", "virtual app shop { on: web01 }");
    assert_eq!(
        messages(&compilation(&wrong, ""), Severity::Error),
        ["only a virtual machine runs on a host: server web01", "only a machine can be virtual: app shop"]
    );
    let racked = REGISTRY.replace("ip: 10.0.0.20, on: esx01 }", "ip: 10.0.0.20, on: esx01, rack: r1 }");
    assert_eq!(messages(&compilation(&racked, ""), Severity::Error), ["a virtual machine is in no rack: server web01"]);
    // A proxy on a host, written before `virtual`: a warning.
    let proxy = format!("{REGISTRY}system web {{ proxy nginx {{ on: web01 }} }}\n");
    let c = compilation(&proxy, "");
    assert!(messages(&c, Severity::Error).is_empty());
    assert_eq!(messages(&c, Severity::Warning), ["only a virtual machine runs on a host: proxy nginx"]);
}

#[test]
fn virtual_machines_are_formatted_and_exported() {
    let source = "server esx01\nvirtual server web01 { on: esx01 }\n";
    let formatted = cadastre_core::format::format(source).unwrap();
    assert!(formatted.contains("virtual server web01 { on: esx01 }"), "{formatted}");
    assert_eq!(cadastre_core::format::format(&formatted).unwrap(), formatted);
    // ArchiMate: a physical server is a device, a virtual machine a node.
    let c = compilation(REGISTRY, "");
    let archimate = cadastre_core::export::export(&c, "archimate").unwrap();
    let element = |name: &str| {
        let at = archimate.find(&format!("<name xml:lang=\"en\">{name}</name>")).unwrap();
        let start = archimate[..at].rfind("xsi:type=\"").unwrap() + 10;
        archimate[start..start + archimate[start..].find('"').unwrap()].to_owned()
    };
    assert_eq!(element("esx01"), "Device");
    assert_eq!(element("web01"), "Node");
    assert_eq!(element("fw"), "Node");
}
