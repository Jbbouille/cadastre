//! The import of an inventory in CSV into a `.ca` file.

use cadastre_core::inventory::{Mapping, import};
use cadastre_core::{Severity, SourceFile, compile};

/// The imported file, compiled: no error.
fn imported(csv: &str, name: &str, kind: Option<&str>) -> String {
    let files = import(csv, name, kind, &Mapping::default()).unwrap();
    assert_eq!(files.len(), 1);
    let sources: Vec<SourceFile> =
        files.iter().map(|f| SourceFile { path: f.path.clone(), text: f.text.clone() }).collect();
    let c = compile(&sources);
    let errors: Vec<String> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.render(&sources)).collect();
    assert!(errors.is_empty(), "{}\n{}", errors.join(""), files[0].text);
    files[0].text.clone()
}

#[test]
fn rows_of_every_kind_nested_by_their_parent() {
    let csv = "Kind;Name;Parent;ip;os;owner;description;url\n\
               team;ops;;;;;;\n\
               team;sales;;;;;;\n\
               site;Paris DC;;;;;;\n\
               server;esx01;Paris DC;10.0.0.4;;ops;;\n\
               server;web01;Paris DC;10.0.0.5;debian12;ops;Front server;\n\
               virtual server;vm 02;paris-dc;10.0.0.6;;ops;;\n\
               app;shop;;;;sales;\"The shop, online\";https://shop.example.com\n";
    let text = imported(csv, "data/inventory.csv", None);
    for expected in [
        "// Imported from inventory.csv",
        "site paris-dc {",
        "label: \"Paris DC\"",
        "  server web01 {",
        "ip: 10.0.0.5",
        "  virtual server vm-02 {",
        "label: \"vm 02\"",
        "owner: sales",
        "description: \"The shop, online\"",
        "https://shop.example.com",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
}

#[test]
fn one_kind_for_every_row_and_what_is_not_known() {
    let csv = "Name,Vendor ID,os\nlaser-1,42,\n,7,x\nlaser-1,43,\n";
    assert!(import(csv, "printers.csv", None, &Mapping::default()).is_err());
    let text = imported(csv, "printers.csv", Some("server"));
    for expected in [
        "server laser-1 {",
        "vendor_id: 42",
        "server laser-1-2 {",
        "line 3: no name, not imported",
        "Keys the schema does not know, to declare in the project or to rename: vendor_id.",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
}

#[test]
fn a_parent_inside_its_child_is_left_at_the_root() {
    let csv = "kind,name,parent\nsite,a,b\nsite,b,a\n";
    let files = import(csv, "loop.csv", None, &Mapping::default()).unwrap();
    assert!(files[0].text.contains("is inside it, left at the root"), "{}", files[0].text);
}

#[test]
fn an_export_in_its_own_words() {
    let csv = "Type CI;Hostname;Adresse IP;Commentaire
Serveur physique;srv01;10.0.0.1;old
VM;vm01;10.0.0.2;
";
    let mapping = Mapping::parse(
        "Hostname=name, type ci=kind, Adresse IP=ip, Commentaire=",
        "serveur physique=server,VM=virtual server",
    )
    .unwrap();
    let files = import(csv, "cmdb.csv", None, &mapping).unwrap();
    let text = &files[0].text;
    assert!(text.contains("server srv01 { ip: 10.0.0.1 }"), "{text}");
    assert!(text.contains("virtual server vm01 { ip: 10.0.0.2 }"), "{text}");
    assert!(!text.contains("old") && !text.contains("commentaire"), "{text}");
    // A column of the mapping missing from the file, a pair without `=`.
    let wrong = Mapping::parse("Host=name", "").unwrap();
    assert!(import(csv, "cmdb.csv", None, &wrong).unwrap_err().contains("no column `host`"));
    assert!(Mapping::parse("Hostname", "").is_err());
}
