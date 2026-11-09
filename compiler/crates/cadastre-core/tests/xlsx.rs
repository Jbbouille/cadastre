//! Excel workbooks of the table views: typed numbers, links, the header frozen and filtered.

use cadastre_core::{SourceFile, build_with, compile, xlsx};

const REGISTRY: &str = r#"
team t
site dc { hosting: on-prem, rack r1 { units: 42 }
  network lan { subnet apps { cidr: 10.0.0.0/24, server web01 { ip: 10.0.0.10, console: "https://web01.example.com" } } }
}
system shop { owner: t
  app front { on: web01, version: 16.2, -> store : tcp/5432 [flow] }
  database store { on: web01 }
}
"#;

const VIEWS: &str = "view racks { select rack render xlsx { columns: [name, units] } }
view servers { select server render xlsx { columns: [name, ip, console] } }
view flows { select app, database edges [flow] render xlsx { columns: [source, target, port] } }
view apps { select app render xlsx { columns: [name, version] } }
view cmdb { select app render xlsx { columns: [name, on] sheets: [servers, flows] } }";

/// The workbook of a view, as text: its entries are stored without compression.
fn workbook(view: &str) -> String {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: VIEWS.into() },
    ]);
    String::from_utf8_lossy(&xlsx(&c, view).unwrap()).into_owned()
}

#[test]
fn a_zip_of_spreadsheet_parts() {
    let book = workbook("racks");
    assert!(book.starts_with("PK\u{3}\u{4}"), "a zip archive");
    for part in ["[Content_Types].xml", "xl/workbook.xml", "xl/styles.xml", "xl/worksheets/sheet1.xml"] {
        assert!(book.contains(part), "{part}");
    }
    // The sheet is named after the view; the header is bold, frozen and filtered.
    assert!(book.contains("<sheet name=\"racks\""), "{book}");
    assert!(book.contains("<c r=\"A1\" t=\"inlineStr\" s=\"1\"><is><t>name</t></is></c>"), "{book}");
    assert!(book.contains("state=\"frozen\"") && book.contains("<autoFilter ref=\"A1:B2\"/>"), "{book}");
}

#[test]
fn numbers_and_text() {
    // A port is a number cell; a free key (`units`) and a version stay text, as Excel would
    // otherwise read `16.2` as a number.
    assert!(workbook("flows").contains("<c r=\"C2\"><v>5432</v></c>"));
    assert!(workbook("racks").contains("<c r=\"B2\" t=\"inlineStr\"><is><t xml:space=\"preserve\">42</t></is></c>"));
    let apps = workbook("apps");
    assert!(apps.contains("<c r=\"B2\" t=\"inlineStr\"><is><t xml:space=\"preserve\">16.2</t></is></c>"), "{apps}");
}

#[test]
fn a_sheet_per_view() {
    // The sheet of the view, then those of `sheets`, each with its filter and its links.
    let book = workbook("cmdb");
    for part in ["xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml", "xl/worksheets/sheet3.xml"] {
        assert_eq!(book.matches(part).count(), 3, "{part}: an entry, its central record and its content type");
    }
    assert!(book.contains("<sheet name=\"cmdb\" sheetId=\"1\" r:id=\"rId1\"/><sheet name=\"servers\" sheetId=\"2\" r:id=\"rId2\"/><sheet name=\"flows\" sheetId=\"3\" r:id=\"rId3\"/>"), "{book}");
    assert!(book.contains("localSheetId=\"2\" hidden=\"1\">'flows'!$A$1:$C$2"), "{book}");
    // The server of an app leads to its row in the sheet of the servers.
    assert!(book.contains("<hyperlink ref=\"B2\" location=\"'servers'!A2\"/>"), "{book}");
    assert!(
        book.contains("xl/worksheets/_rels/sheet2.xml.rels")
            && book.contains(
                "Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\""
            ),
        "{book}"
    );
}

#[test]
fn unknown_sheets() {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile {
            path: "v.cav".into(),
            text: "view v { select app render xlsx { sheets: [servrs] } }
view servers { select server }"
                .into(),
        },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == cadastre_core::Severity::Error).collect();
    assert!(
        errors
            .iter()
            .any(|d| d.message == "unknown view: servrs" && d.help.as_deref() == Some("did you mean `servers`?")),
        "{errors:?}"
    );
}

#[test]
fn urls_as_links() {
    let book = workbook("servers");
    assert!(book.contains("<hyperlink ref=\"C2\" r:id=\"rId1\"/>"), "{book}");
    assert!(book.contains("Target=\"https://web01.example.com\" TargetMode=\"External\""), "{book}");
}

#[test]
fn in_base64_among_the_outputs() {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: VIEWS.into() },
    ]);
    let (outputs, _) = build_with(&c, Some("racks"), None);
    // `PK\3\4` in base64.
    assert!(outputs[0].content.starts_with("UEsDB"), "{}", outputs[0].content);
}
