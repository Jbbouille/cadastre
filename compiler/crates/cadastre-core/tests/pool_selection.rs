//! A pool whose members are a selection: `pool stores { select server where … }`.

use cadastre_core::{Compilation, Severity, SourceFile, compile};

const REGISTRY: &str = r#"
site store-lil {
  hosting: on-prem
  role: store
  network lan {
    subnet main {
      cidr: 10.1.1.0/24
      server srv-lil { ip: 10.1.1.10 }
    }
  }
}
site store-lyo {
  hosting: on-prem
  role: store
  network lan {
    subnet main {
      cidr: 10.1.2.0/24
      server srv-lyo { ip: 10.1.2.10 }
    }
  }
}
site hub {
  hosting: on-prem
  network lan {
    subnet main {
      cidr: 10.9.0.0/24
      server central { ip: 10.9.0.10 }
    }
  }
}
pool stores {
  select server where ancestor(site).role = store
}
app till { on: stores }
"#;

fn compilation(extra: &str) -> Compilation {
    compile(&[SourceFile { path: "a.ca".into(), text: format!("{REGISTRY}{extra}") }])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

fn named(c: &Compilation, name: &str) -> cadastre_core::registry::ResourceId {
    c.registry.ids().find(|&id| c.registry.resource(id).name == name).unwrap()
}

#[test]
fn the_selected_servers_are_members() {
    let c = compilation("");
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", c.diagnostics);
    assert!(messages(&c, Severity::Warning).is_empty(), "{:?}", c.diagnostics);
    let stores = named(&c, "stores");
    for (server, member) in [("srv-lil", true), ("srv-lyo", true), ("central", false)] {
        let pools = c.model.references.get(&(cadastre_core::check::Holder::Resource(named(&c, server)), "pool".into()));
        assert_eq!(pools.is_some_and(|p| p.contains(&stores)), member, "{server}");
    }
}

#[test]
fn only_a_pool_selects() {
    let c = compilation("system shop {\n  select server\n}\n");
    assert_eq!(messages(&c, Severity::Error), ["a system does not select members: only a pool does"]);
}

#[test]
fn a_pool_holds_hosts_only() {
    let c = compilation("pool odd {\n  select app\n}\n");
    assert_eq!(messages(&c, Severity::Error), ["a pool holds what `on` designates, not a app"]);
}

#[test]
fn hosts_is_not_a_filter_of_a_pool() {
    let c = compilation("pool busy {\n  select server where hosts has till\n}\n");
    assert_eq!(messages(&c, Severity::Error), ["`hosts` depends on the pools: a pool cannot select by it"]);
}

#[test]
fn an_empty_selection_is_a_warning() {
    let c = compilation("pool none {\n  select server where ancestor(site).role = warehouse\n}\n");
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", c.diagnostics);
    assert_eq!(messages(&c, Severity::Warning), ["pool none selects nothing"]);
}

#[test]
fn select_stays_a_key() {
    let c = compilation("app other { select: 1 }\n");
    assert!(!messages(&c, Severity::Error).iter().any(|m| m.contains("select members")), "{:?}", c.diagnostics);
}
