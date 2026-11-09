//! Table views (spec § 6, `render`): CSV, TSV, Markdown and HTML, a row per node or per edge.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  network lan {
    subnet apps {
      cidr: 10.0.0.0/24
      server web01 { ip: 10.0.0.10, os: debian12 }
      server db01 { ip: 10.0.0.20, label: "Database, primary" }
    }
  }
}
data invoices
data customers
system billing {
  app portal { on: web01, -> db : jdbc/5432 [flow] { carries: [invoices, customers] } }
  database db { on: db01 }
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

fn output(c: &Compilation, format: &str) -> String {
    let (outputs, _) = build(c, None);
    outputs.into_iter().find(|o| o.format == format).unwrap().content
}

#[test]
fn a_row_per_node() {
    let c = compilation(
        "view servers {\n  select server\n  render csv, tsv, markdown { columns: [name, ip, os, label, \"ancestor(site)\"] }\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    // CSV: a byte order mark for Excel, the separator quoted inside a field.
    assert_eq!(
        output(&c, "csv"),
        "\u{feff}name,ip,os,label,ancestor(site)\r\n\
         dc.lan.apps.web01,10.0.0.10,debian12,,dc\r\n\
         dc.lan.apps.db01,10.0.0.20,,\"Database, primary\",dc\r\n"
    );
    assert!(output(&c, "tsv").contains("dc.lan.apps.db01\t10.0.0.20\t\tDatabase, primary\tdc\r\n"));
    assert_eq!(
        output(&c, "markdown"),
        "| name | ip | os | label | ancestor(site) |\n|---|---|---|---|---|\n\
         | dc.lan.apps.web01 | 10.0.0.10 | debian12 |  | dc |\n\
         | dc.lan.apps.db01 | 10.0.0.20 |  | Database, primary | dc |\n"
    );
}

#[test]
fn an_html_page() {
    let c = compilation("view servers {\n  select server\n  render html { columns: [name, label] }\n}");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let page = output(&c, "html");
    assert!(page.starts_with("<!doctype html>\n"), "{page}");
    assert!(page.contains("<title>servers</title>"), "{page}");
    assert!(page.contains("<thead>\n<tr><th>name</th><th>label</th></tr>\n</thead>"), "{page}");
    assert!(page.contains("<tr><td>dc.lan.apps.db01</td><td>Database, primary</td></tr>"), "{page}");
    assert!(page.ends_with("</table>\n</body>\n</html>\n"), "{page}");
}

#[test]
fn a_row_per_edge() {
    let c = compilation(
        "view flows {\n  select app, database\n  edges [flow]\n  render markdown { columns: [source, target, proto, port, carries, \"target.on\"] }\n}\n\
         view defaults {\n  select app, database\n  edges [flow]\n  render markdown\n}\n\
         view nodes {\n  select app, database\n  edges [flow]\n  render markdown { rows: nodes }\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let table = |view: &str| outputs.iter().find(|o| o.view == view).unwrap().content.clone();
    assert_eq!(
        table("flows"),
        "| source | target | proto | port | carries | target.on |\n|---|---|---|---|---|---|\n\
         | portal | db | jdbc | 5432 | invoices, customers | db01 |\n"
    );
    // Default columns: for edges, and for nodes when asked.
    assert!(table("defaults").starts_with("| source | target | port | carries |\n"), "{}", table("defaults"));
    assert!(table("nodes").starts_with("| name | kind | label |\n"), "{}", table("nodes"));
}

#[test]
fn invalid_columns() {
    let c = compilation(
        "view v {\n  select server\n  render csv { columns: [\"ancestor(site\", ipp, \"count(onn) as N\"]  rows: \"each(on\" }\n}",
    );
    let found = errors(&c);
    assert!(found.contains(&"invalid column: ancestor(site".to_owned()), "{found:?}");
    assert!(found.iter().any(|m| m.contains("ipp")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("onn")), "{found:?}");
    assert!(found.contains(&"invalid value for rows: each(on".to_owned()), "{found:?}");
}

/// Instances on logical servers, on physical servers in two buildings.
const CMDB: &str = r#"
site north { server phys-1 }
site south { server phys-2 }
virtual server vm-1 { on: phys-1 }
virtual server vm-2 { on: phys-2 }
virtual server vm-3 { on: phys-2 }
app ledger { on: [vm-1, vm-2] }
app wiki { on: vm-3 }
app draft
"#;

fn cmdb(render: &str) -> String {
    let views = format!("view v {{\n  select app\n  render markdown {{ {render} }}\n}}\n");
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: CMDB.into() },
        SourceFile { path: "v.cav".into(), text: views },
    ]);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    output(&c, "markdown")
}

#[test]
fn headers_and_counts() {
    assert_eq!(
        cmdb("columns: [\"name as Application\", \"count(on) as Servers\", \"count(on.on.ancestor(site))\"]"),
        "| Application | Servers | count(on.on.ancestor(site)) |\n|---|---|---|\n\
         | ledger | 2 | 2 |\n| wiki | 1 | 1 |\n| draft | 0 | 0 |\n"
    );
}

#[test]
fn a_row_per_item_of_a_path() {
    // A row per app and physical server, each column read from the item of the chain it starts
    // with: `on` is the logical server of the row, not every server of the app. A count stays
    // that of the app.
    assert_eq!(
        cmdb("rows: \"each(on.on)\"  columns: [name, on, \"on.on\", \"on.on.ancestor(site) as Site\", \"count(on)\"]"),
        "| name | on | on.on | Site | count(on) |\n|---|---|---|---|---|\n\
         | ledger | vm-1 | phys-1 | north | 2 |\n\
         | ledger | vm-2 | phys-2 | south | 2 |\n\
         | wiki | vm-3 | phys-2 | south | 1 |\n\
         | draft |  |  |  | 0 |\n"
    );
}

#[test]
fn what_refers_to_a_resource() {
    let registry = "kind backup_policy { like: data  retention: string }
kind server { backup: ref(backup_policy) }
backup_policy daily { retention: \"30 days\" }
backup_policy weekly { retention: \"1 year\" }
backup_policy unused
server db01 { backup: daily }
server web01 { backup: daily }
server erp01 { backup: weekly }
";
    let views = "view policies {
  select backup_policy
  render markdown { columns: [name, retention, \"referrers(backup) as Servers\", \"count(referrers(backup))\"] }
}
view wrong { select backup_policy  render markdown { columns: [\"referrers(backups)\"] } }
view unused { select backup_policy where count(referrers(backup)) = 0  render markdown }
";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    assert_eq!(errors(&c), ["unknown key: backups"], "{:?}", c.diagnostics);
    let (outputs, _) = build(&c, Some("policies"));
    assert_eq!(
        outputs[0].content,
        "| name | retention | Servers | count(referrers(backup)) |\n|---|---|---|---|\n\
         | daily | 30 days | db01, web01 | 2 |\n| weekly | 1 year | erp01 | 1 |\n| unused |  |  | 0 |\n"
    );
    // In a filter too: the policies no server refers to.
    let (outputs, _) = build(&c, Some("unused"));
    assert!(outputs[0].content.ends_with("|---|---|---|\n| unused | backup_policy |  |\n"), "{}", outputs[0].content);
}

#[test]
fn a_pivot_table() {
    let registry = "app a { tier: gold, criticality: high }
app b { tier: gold, criticality: low }
app c { tier: silver, criticality: high }
app d { criticality: high }
";
    let views = "view v {\n  select app\n  render markdown { group: \"tier as Tier\"  count: [criticality] }\n}\n";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    // A row per tier, the apps without one last, then the total.
    assert_eq!(
        output(&c, "markdown"),
        "| Tier | count | high | low |\n|---|---|---|---|\n\
         | gold | 2 | 1 | 1 |\n| silver | 1 | 1 | 0 |\n|  | 1 | 1 | 0 |\n| Total | 4 | 3 | 1 |\n"
    );
    // `status` has a default: the apps without one are active.
    let views = "view v {\n  select app\n  render markdown { group: status }\n}\n";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    assert!(output(&c, "markdown").contains("| active | 4 |"), "{}", output(&c, "markdown"));
}

#[test]
fn the_ends_of_the_links_of_a_layer() {
    let registry = "app shop { -> stock : https [flow], -> pay : https [flow], -> ldap : ldaps [auth] }
app stock { -> pay : https [flow] }
app pay
app ldap
";
    let views = "view v {\n  select app\n  render markdown { columns: [name, \"count(targets(flow)) as Calls\", \"sources(flow) as Called by\"] }\n}\n\
                 view unused {\n  select app where count(sources(flow)) = 0\n  render text\n}\n\
                 view wrong {\n  select app where count(targets(flows)) = 0\n  render text\n}\n";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    assert_eq!(errors(&c), ["unknown layer: flows"], "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let table = outputs.iter().find(|o| o.view == "v").unwrap().content.clone();
    assert_eq!(
        table,
        "| name | Calls | Called by |\n|---|---|---|\n| shop | 2 |  |\n| stock | 1 | shop |\n| pay | 0 | shop, stock |\n| ldap | 0 |  |\n"
    );
    // The apps nothing calls by a flow: `ldap` is reached by an `auth` link only.
    let unused = outputs.iter().find(|o| o.view == "unused").unwrap().content.clone();
    assert!(unused.contains("shop") && unused.contains("ldap") && !unused.contains("pay"), "{unused}");
}
