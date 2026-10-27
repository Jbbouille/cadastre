//! Modules (the dependencies of a project, read only) and `for_each` over a CSV file.

use cadastre_core::rename::rename;
use cadastre_core::{Compilation, Module, Severity, SourceFile, build, compile, compile_modules};

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: path.into(), text: text.into() }
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

/// A project and its module `shared`, the last two files.
fn with_module(project: &str, views: &str) -> (Vec<SourceFile>, Compilation) {
    let files = vec![
        file("src/apps.ca", project),
        file("src/views.cav", views),
        file("../shared/teams.ca", "team ops\nsystem common { owner: ops }\n"),
        file(
            "../shared/rules.cac",
            "check owned {\n  select system where not exists(owner)\n  expect none\n  message: \"every system has an owner\"\n}\n",
        ),
        file("../shared/views.cav", "view teams { select team render text }\n"),
    ];
    let modules = [Module { name: "shared".into(), files: 2..5 }];
    let c = compile_modules(&files, &modules, Default::default());
    (files, c)
}

#[test]
fn a_module_brings_its_registry_and_its_checks_not_its_views() {
    let (_, c) =
        with_module("system billing { owner: ops }\nsystem orphan\n", "view mine { select system render text }\n");
    // The check of the module fails on the project.
    assert_eq!(messages(&c, Severity::Error), ["check owned failed: 1 node selected, none expected: orphan"]);
    assert_eq!(c.plans.views.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["mine"]);
    let (outputs, _) = build(&c, None);
    assert_eq!(outputs.len(), 1);
    assert_eq!(c.module_of(c.registry.resources[0].name_span.file), None);
}

#[test]
fn a_name_declared_in_the_project_and_in_a_module() {
    let (_, c) = with_module("team ops\n", "");
    let errors = messages(&c, Severity::Error);
    assert!(
        errors.contains(&"qualified name declared twice: ops, in module shared and in the project".to_owned())
            || errors.contains(&"qualified name declared twice: ops, in the project and in module shared".to_owned()),
        "{errors:?}"
    );
}

#[test]
fn a_module_is_read_only_for_renaming() {
    let (files, c) = with_module("system billing { owner: ops }\n", "");
    let ops = c.registry.ids().find(|&id| c.registry.resource(id).name == "ops").unwrap();
    assert_eq!(
        rename(&c, &files, ops, "operations").unwrap_err(),
        "ops is declared in the module shared, read only: rename it there"
    );
}

#[test]
fn for_each_over_a_csv_file() {
    let c = compile(&[
        file(
            "src/sites/stores.ca",
            "for_each (code, ip, label) in csv(\"../data/stores.csv\") {\n  site store-$code {\n    hosting: office\n    label: $label\n    \
             network lan {\n      subnet main {\n        cidr: 10.1.0.0/16\n        server srv-$code { ip: $ip }\n      }\n    }\n  }\n}\n",
        ),
        file("src/data/stores.csv", "code;label;ip;city\nlil;\"Store; Lille\";10.1.1.10;Lille\nlyo;;10.1.2.10;Lyon\n"),
    ]);
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", c.diagnostics);
    let named = |name: &str| c.registry.ids().find(|&id| c.registry.resource(id).name == name).unwrap();
    let lil = c.registry.resource(named("store-lil"));
    assert_eq!(
        lil.attributes.iter().find(|a| a.key == "label").map(|a| a.value.raw.clone()),
        Some("Store; Lille".into())
    );
    // An empty cell: the key is not written.
    assert!(c.registry.resource(named("store-lyo")).attributes.iter().all(|a| a.key != "label"));
    // A cell of one token keeps its kind: an address, checked against the subnet.
    let ip = c.registry.resource(named("srv-lyo")).attributes.iter().find(|a| a.key == "ip").unwrap();
    assert_eq!(ip.value.kind, cadastre_core::registry::ValueKind::Ipv4);
    assert_eq!(ip.value.span.line, 2);
}

#[test]
fn csv_errors() {
    let c = compile(&[
        file("a.ca", "for_each (code, zone) in csv(\"stores.csv\") { site s-$code { hosting: office } }\n"),
        file("stores.csv", "code,ip\nlil,1\n"),
    ]);
    assert_eq!(messages(&c, Severity::Error), ["stores.csv has no column zone"]);
    let c = compile(&[file("a.ca", "for_each code in csv(\"none.csv\") { site s-$code { hosting: office } }\n")]);
    assert_eq!(messages(&c, Severity::Error), ["CSV file not found: none.csv"]);
}
