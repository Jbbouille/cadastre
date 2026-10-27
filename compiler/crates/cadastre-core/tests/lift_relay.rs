//! `lift` with a path per kind (`via_<kind>`), `follow` relayed by databases (`relay`).

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// Scripts of a system on a file server exchange with a SaaS of the same system; prices go
/// through a database that an API reads without declaring them.
const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  network lan {
    subnet apps {
      cidr: 10.0.0.0/24
      server filehub { label: "FILE-HUB" }
      server stockdb-srv
    }
  }
}
data prices
system step {
  saas pim
  job recup { on: filehub, -> pim : sftp [flow] }
  job envoi { on: filehub, -> pim : sftp [flow] }
}
system stock {
  app api { -> db : jdbc [flow] { exchange: pull }, -> labels : https [flow] { carries: [prices] } }
  database db { on: stockdb-srv }
}
app pricing {
  -> stock.db : jdbc [flow] { exchange: push, carries: [prices] }
  -> shared.db : jdbc [flow] { exchange: push, carries: [prices] }
}
app labels
system shared {
  database db { on: stockdb-srv }
  app reader { -> db : jdbc [flow] { exchange: pull } }
  job export { -> db : jdbc [flow] { exchange: pull } }
}
"#;

fn compilation(views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

fn edges(c: &Compilation, view: &str) -> String {
    let (outputs, _) = build(c, None);
    let text = outputs.into_iter().find(|o| o.view == view).unwrap().content;
    text[text.find("\nedges\n").unwrap()..].to_owned()
}

#[test]
fn a_path_per_kind() {
    let c = compilation(
        "view systems {\n  select system, server\n  edges lift([flow] where source.ancestor(system) = step, via: ancestor(system)?)\n  render text\n}\n\
         view by-kind {\n  select system, server\n  edges lift([flow] where source.ancestor(system) = step, via: ancestor(system)?, via_job: on)\n  render text\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    // One path for all: the scripts and the SaaS fold onto their system, a loop, dropped.
    assert!(edges(&c, "systems").contains("(none)"), "{}", edges(&c, "systems"));
    // The jobs go up to their server, the SaaS to its system: the exchanges stay.
    let by_kind = edges(&c, "by-kind");
    assert!(by_kind.contains("dc.lan.apps.filehub -> step  [sftp]"), "{by_kind}");
}

#[test]
fn follow_relayed_by_a_database() {
    let c = compilation(
        "view plain {\n  select app, database\n  edges follow(prices, from: pricing)\n  render text\n}\n\
         view relayed {\n  select app, database\n  edges follow(prices, from: pricing, relay: [database])\n  render text\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    // Without relay, the walk stops at the database.
    let plain = edges(&c, "plain");
    assert!(plain.contains("pricing -> stock.db") && !plain.contains("labels"), "{plain}");
    // With it, through the API reading the database, to its consumer.
    let relayed = edges(&c, "relayed");
    assert!(relayed.contains("pricing -> stock.db"), "{relayed}");
    assert!(relayed.contains("stock.api -> stock.db  [jdbc]\n      step: 2"), "{relayed}");
    assert!(relayed.contains("stock.api -> labels"), "{relayed}");
}

#[test]
fn unknown_kinds() {
    let c = compilation(
        "view v {\n  select system\n  edges lift([flow], via: ancestor(system)?, via_jobs: on), follow(prices, from: pricing, relay: [databse])\n}",
    );
    let found = errors(&c);
    assert!(found.iter().any(|m| m.contains("jobs")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("databse")), "{found:?}");
}

#[test]
fn relayed_by_a_given_resource_towards_given_kinds() {
    let c = compilation(
        "view all {\n  select app, job, database\n  edges follow(prices, from: pricing, relay: [database])\n  render text\n}\n\
         view stock {\n  select app, job, database\n  edges follow(prices, from: pricing, relay: [stock.db])\n  render text\n}\n\
         view apps {\n  select app, job, database\n  edges follow(prices, from: pricing, relay: [database], relay_to: [app])\n  render text\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    // Every database relays: the readers of the shared one too, the job included.
    let all = edges(&c, "all");
    assert!(all.contains("shared.reader -> shared.db") && all.contains("shared.export -> shared.db"), "{all}");
    // Only the STOCK database.
    let stock = edges(&c, "stock");
    assert!(stock.contains("stock.api -> stock.db") && !stock.contains("shared.reader"), "{stock}");
    // Towards apps only: the job reading the shared database is left out.
    let apps = edges(&c, "apps");
    assert!(apps.contains("shared.reader -> shared.db") && !apps.contains("shared.export"), "{apps}");
}

#[test]
fn a_relay_that_is_no_kind_nor_resource() {
    let c =
        compilation("view v {\n  select app, database\n  edges follow(prices, from: pricing, relay: [stock.dbb])\n}");
    let found = errors(&c);
    assert!(found.iter().any(|m| m.contains("stock.dbb")), "{found:?}");
}
