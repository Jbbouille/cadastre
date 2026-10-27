//! The role of a file comes from its extension: `.ca` facts, `.cav` views and styles, `.cac`
//! checks; a block in the wrong file is an error.

use cadastre_core::{Severity, SourceFile, compile};

fn errors(path: &str, text: &str) -> Vec<String> {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: "team t\nsystem s { owner: t }\n".into() },
        SourceFile { path: path.into(), text: text.into() },
    ]);
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

#[test]
fn a_check_belongs_in_a_cac_file() {
    let check = "check owned { select system where not exists(owner) expect none }";
    assert_eq!(errors("c.cac", check), Vec::<String>::new());
    assert_eq!(errors("v.cav", check), ["a check belongs in a checks file (.cac)"]);
    assert_eq!(errors("b.ca", check), ["a check belongs in a checks file (.cac)"]);
}

#[test]
fn a_view_belongs_in_a_cav_file() {
    let view = "view v { select system render text }";
    assert_eq!(errors("v.cav", view), Vec::<String>::new());
    assert_eq!(errors("c.cac", view), ["a view or a style cannot be declared in a checks file (.cac)"]);
    assert_eq!(
        errors("c.cac", "style { layer flow { color: red } }"),
        ["a view or a style cannot be declared in a checks file (.cac)"]
    );
}

#[test]
fn a_check_in_a_cav_file_is_not_run() {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: "team t\nsystem s\n".into() },
        SourceFile {
            path: "v.cav".into(),
            text: "check owned { select system where not exists(owner) expect none }".into(),
        },
    ]);
    assert!(c.plans.checks.is_empty());
}
