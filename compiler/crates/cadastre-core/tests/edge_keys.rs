//! The keys a project declares on its links (`layer flow { routed_host: … }`), read on the edges of
//! a view: in the columns of a table as in a filter.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
layer flow { routed_host: string*, backend: ref(server) }

server web01
server web02
load_balancer lb {
  vip shop {
    ip: 10.0.0.30
    -> front : http/8080 [flow] { routed_host: "shop.example.com", backend: web01 }
    -> admin : http/8081 [flow] { routed_host: ["admin.example.com", "back.example.com"], backend: web02 }
    -> stats : http/9090 [flow]
  }
}
app front { on: web01 }
app admin { on: web02 }
app stats { on: web02 }
app batch {
  -> admin : https/443 [flow] { routed_host: "admin.example.com", status: planned }
  -> admin : sftp/22 [flow] { routed_host: ["back.example.com", "files.example.com"] }
}
"#;

fn compilation(views: &str) -> Compilation {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let errors: Vec<&str> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.as_str()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

fn csv(views: &str) -> String {
    let c = compilation(views);
    let (outputs, _) = build(&c, None);
    outputs.into_iter().find(|o| o.format == "csv").unwrap().content.trim_start_matches('\u{feff}').to_owned()
}

#[test]
fn a_column_shows_a_key_declared_by_the_project() {
    let table = csv("view routing {\n  select vip, app\n  edges [flow] where source.kind = vip\n  \
         render csv { columns: [source, target, port, routed_host, backend] }\n}");
    assert_eq!(
        table,
        "source,target,port,routed_host,backend\r\n\
         shop,front,8080,shop.example.com,web01\r\n\
         shop,admin,8081,\"admin.example.com, back.example.com\",web02\r\n\
         shop,stats,9090,,\r\n"
    );
}

#[test]
fn a_column_and_a_filter_read_the_same_values() {
    // The filter selects the edge by the key, and the column shows it.
    let table = csv("view routing {\n  select vip, app\n  edges [flow] where routed_host has \"back.example.com\"\n  \
         render csv { columns: [source, target, routed_host] }\n}");
    assert!(table.contains("shop,admin,\"admin.example.com, back.example.com\"\r\n"), "{table}");
    assert!(!table.contains("front"), "{table}");
}

#[test]
fn an_edge_of_merged_links_shows_their_values_without_repeats() {
    // Lifted edges are merged by their ends: the two links from batch to admin make one edge.
    let table =
        csv("view batch {\n  select app\n  edges lift([flow] where source.name = batch, via: ancestor(system)?)\n  \
         render csv { columns: [source, target, routed_host] }\n}");
    assert_eq!(
        table,
        "source,target,routed_host\r\nbatch,admin,\"admin.example.com, back.example.com, files.example.com\"\r\n"
    );
}

#[test]
fn a_key_with_a_default_shows_it_when_no_link_writes_it() {
    let table = csv("view statuses {\n  select vip, app\n  edges [flow] where source.kind = vip\n  \
         render csv { columns: [target, status] }\n}");
    assert_eq!(table, "target,status\r\nfront,active\r\nadmin,active\r\nstats,active\r\n");
    let table = csv("view statuses {\n  select app\n  edges [flow] where source.name = batch\n  \
         render csv { columns: [target, port, status] }\n}");
    assert_eq!(table, "target,port,status\r\nadmin,443,planned\r\nadmin,22,active\r\n");
}

#[test]
fn an_edge_without_links_keeps_its_columns_empty() {
    // An edge of a reference (`on`) has no link: neither a value nor a default.
    let table = csv(
        "view hosting {\n  select app, server\n  edges on\n  render csv { columns: [source, target, routed_host, status] }\n}",
    );
    assert!(table.contains("front,web01,,\r\n"), "{table}");
}
