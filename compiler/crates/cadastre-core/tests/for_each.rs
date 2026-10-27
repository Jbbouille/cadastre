//! Series of resources (spec § 3, `for_each`) and pools of servers to run on.

use cadastre_core::registry::ValueKind;
use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const STORES: &str = r#"
team it
pool store-servers { label: "Store servers" }

for_each (code, ip) in [[lil, 10.1.1.10], [lyo, 10.1.2.10]] {
  site store-$code {
    hosting: office
    label: "Store $code"
    network lan {
      subnet main {
        cidr: 10.1.0.0/16
        server srv-$code { ip: $ip, pool: store-servers, owner: it }
      }
    }
  }
}

app till {
  owner: it
  description: "Run by $Q, the scheduler"
  on: store-servers
}
"#;

fn compilation(registry: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

#[test]
fn a_site_and_a_server_per_store() {
    let c = compilation(STORES, "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let names: Vec<String> = c.registry.ids().map(|id| c.registry.qualified_name(id)).collect();
    assert!(names.contains(&"store-lil.lan.main.srv-lil".to_owned()), "{names:?}");
    assert!(names.contains(&"store-lyo.lan.main.srv-lyo".to_owned()), "{names:?}");
    // A variable alone keeps the kind of the item: an address, checked against the subnet.
    let srv = c.registry.ids().find(|&id| c.registry.resource(id).name == "srv-lyo").unwrap();
    let ip = c.registry.resource(srv).attribute("ip").unwrap();
    assert_eq!((ip.value.raw.as_str(), &ip.value.kind), ("10.1.2.10", &ValueKind::Ipv4));
    // Inside a string; a `$` that is no variable stays.
    let site = c.registry.ids().find(|&id| c.registry.resource(id).name == "store-lil").unwrap();
    assert_eq!(c.registry.resource(site).attribute("label").unwrap().value.raw, "\"Store lil\"");
    let till = c.registry.ids().find(|&id| c.registry.resource(id).name == "till").unwrap();
    assert!(c.registry.resource(till).attribute("description").unwrap().value.raw.contains("$Q"));
}

#[test]
fn an_app_on_a_pool() {
    let c = compilation(
        STORES,
        "view hosting {\n  select app, server group-by pool\n  render text\n}\n\
         view by-pool {\n  select pool, app where kind = app or hosts.kind = app\n  render text\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let text = |view: &str| outputs.iter().find(|o| o.view == view).unwrap().content.clone();
    // The servers in the frame of their pool, beside the app running on it.
    assert!(
        text("hosting").contains("  Store servers\n    srv-lil [server]\n    srv-lyo [server]\n    till [app]\n"),
        "{}",
        text("hosting")
    );
    // The pool hosts the app.
    assert!(text("by-pool").contains("Store servers [pool]"), "{}", text("by-pool"));
}

#[test]
fn errors_in_a_series() {
    // Not as many values as variables; a variable outside its for_each; twice the same item.
    let c = compilation(
        "for_each (code, ip) in [[lil, 10.1.1.10], [lyo]] { server srv-$code }\n\
         server outside-$code\n\
         for_each code in [nte, nte] { site store-$code { hosting: office } }",
        "",
    );
    let found = errors(&c);
    assert!(found.contains(&"for_each expects lists of 2 values: [lyo]".to_owned()), "{found:?}");
    assert!(found.contains(&"unknown variable: $code".to_owned()), "{found:?}");
    assert!(found.contains(&"qualified name declared twice: store-nte".to_owned()), "{found:?}");
}

#[test]
fn nested_series_and_links() {
    let c = compilation(
        "site hub {\n  hosting: on-prem\n  network lan {\n    subnet s {\n      cidr: 10.0.0.0/24\n      \
         for_each n in [a, b] { server node-$n }\n    }\n  }\n}\n\
         for_each from in [x, y] {\n  for_each to in [a, b] { app $from-$to { on: node-$to } }\n  \
         app $from { on: node-a, -> $from-a : https [flow] }\n}",
        "",
    );
    let found: Vec<String> = errors(&c);
    assert!(found.is_empty(), "{found:?}");
    for name in ["node-a", "node-b", "x-a", "x-b", "y-a", "y-b", "x", "y"] {
        assert!(c.registry.ids().any(|id| c.registry.resource(id).name == name), "{name}");
    }
    assert_eq!(c.registry.links.len(), 2);
}
