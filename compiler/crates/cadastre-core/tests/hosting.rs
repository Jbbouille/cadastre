//! What runs where: the `hosts` and `shared` pseudo-attributes, derived from `on` (spec § 6).

use cadastre_core::view;
use cadastre_core::{Compilation, Severity, SourceFile, compile};

const REGISTRY: &str = r#"
site paris {
  hosting: on-prem
  network lan {
    subnet apps {
      cidr: 10.1.2.0/24
      server mutu01 { ip: 10.1.2.40 }
      server pricing-srv { ip: 10.1.2.41, system: pricing }
      server idle { ip: 10.1.2.42 }
    }
  }
}

system pricing {
  app ui { on: mutu01 }
  database db { on: pricing-srv }
  job extractor { on: pricing-srv }
}
system catalog {
  app api { on: mutu01 }
}
"#;

fn compilation(views: &str) -> Compilation {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

fn nodes(c: &Compilation, name: &str) -> Vec<String> {
    let view = c.plans.views.iter().find(|v| v.name == name).unwrap();
    let mut diagnostics = Vec::new();
    let graph = view::evaluate(view, &c.registry, &c.model, &mut diagnostics);
    assert!(diagnostics.iter().all(|d| d.severity != Severity::Error), "{diagnostics:?}");
    graph.nodes.iter().map(|&id| c.registry.resource(id).name.clone()).collect()
}

#[test]
fn hosts_inverts_on() {
    let c = compilation("view v { select server render text }");
    let mutu = c.registry.ids().find(|&id| c.registry.resource(id).name == "mutu01").unwrap();
    let hosted: Vec<String> = c.model.hosts[&mutu].iter().map(|&id| c.registry.qualified_name(id)).collect();
    assert_eq!(hosted, ["pricing.ui", "catalog.api"]);
    let systems: Vec<String> =
        c.model.hosted_systems(&c.registry, mutu).iter().map(|&s| c.registry.resource(s).name.clone()).collect();
    assert_eq!(systems, ["pricing", "catalog"]);
}

#[test]
fn shared_servers_in_views() {
    let c = compilation(
        "view shared { select server where shared render text }\n\
         view hosting-api { select server where hosts has catalog.api render text }\n\
         view databases { select server where hosts.kind = database render text }\n\
         view unused { select server where not exists(hosts) render text }",
    );
    assert_eq!(nodes(&c, "shared"), ["mutu01"]);
    assert_eq!(nodes(&c, "hosting-api"), ["mutu01"]);
    assert_eq!(nodes(&c, "databases"), ["pricing-srv"]);
    assert_eq!(nodes(&c, "unused"), ["idle"]);
}
