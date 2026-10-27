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
    let c = compilation("view v {\n  select server\n  render csv { columns: [\"ancestor(site\", ipp] }\n}");
    let found = errors(&c);
    assert!(found.contains(&"invalid column: ancestor(site".to_owned()), "{found:?}");
    assert!(found.iter().any(|m| m.contains("ipp")), "{found:?}");
}
