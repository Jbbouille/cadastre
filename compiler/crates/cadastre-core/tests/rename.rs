//! Renaming a resource: its declaration and every name designating it.

use cadastre_core::rename::{apply, rename};
use cadastre_core::{SourceFile, compile};

const REGISTRY: &str = r#"
team t
site dc {
  hosting: on-prem
  network lan {
    subnet s {
      cidr: 10.0.0.0/24
      server web01 { owner: t }
      server db01 { owner: t }
    }
  }
}
system middleware {
  owner: t
  app esb { on: web01 }
  app router { on: dc.lan.s.web01, -> esb : https/443 }
}
system shop {
  owner: t
  app front { on: web01, -> middleware.esb : https/443 }
}
"#;

const PLANS: &str = "view v { select app where on = web01 render text { stack: [middleware, shop] } }\n";

fn files() -> Vec<SourceFile> {
    vec![
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: PLANS.into() },
    ]
}

fn renamed(name: &str, to: &str) -> Result<Vec<SourceFile>, String> {
    let files = files();
    let c = compile(&files);
    let id = c.registry.ids().find(|&id| c.registry.resource(id).name == name).unwrap();
    rename(&c, &files, id, to).map(|edits| apply(&files, &edits))
}

#[test]
fn a_server_and_its_references() {
    let files = renamed("web01", "web-01").unwrap();
    let registry = &files[0].text;
    assert!(registry.contains("server web-01 { owner: t }"), "{registry}");
    assert!(registry.contains("on: dc.lan.s.web-01,"), "{registry}");
    assert_eq!(registry.matches("on: web-01").count(), 2, "{registry}");
    assert!(!registry.contains("web01"), "{registry}");
    assert!(files[1].text.contains("where on = web-01"), "{}", files[1].text);
}

#[test]
fn a_qualified_name_keeps_its_prefix() {
    let files = renamed("esb", "broker").unwrap();
    let registry = &files[0].text;
    assert!(registry.contains("app broker { on: web01 }"), "{registry}");
    assert!(registry.contains("-> broker : https/443"), "{registry}");
    assert!(registry.contains("-> middleware.broker : https/443"), "{registry}");
}

#[test]
fn renaming_a_system_renames_the_prefix_of_its_descendants() {
    let files = renamed("middleware", "integration").unwrap();
    let registry = &files[0].text;
    assert!(registry.contains("system integration {"), "{registry}");
    assert!(registry.contains("-> integration.esb : https/443"), "{registry}");
    assert!(files[1].text.contains("stack: [integration, shop]"), "{}", files[1].text);
}

#[test]
fn the_new_name_is_checked() {
    assert_eq!(renamed("web01", "db01").unwrap_err(), "db01 is already taken in dc.lan.s");
    assert!(renamed("web01", "2web").unwrap_err().starts_with("2web is not a valid name"));
    assert!(renamed("web01", "web.01").is_err());
}
