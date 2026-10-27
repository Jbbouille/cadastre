//! `title` and `description` of a view, for its readers.

use cadastre_core::{Severity, SourceFile, compile};

fn compiled(views: &str) -> cadastre_core::Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: "team t\nsystem s { owner: t }\n".into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

#[test]
fn a_view_has_a_title_and_a_description() {
    let c = compiled(
        "view flows {\n  title: \"Application flows\"\n  description: \"Who calls whom.\"\n  select system\n  render text\n}\n\
         view bare { select system render text }\n",
    );
    assert!(c.diagnostics.iter().all(|d| d.severity != Severity::Error), "{:?}", c.diagnostics);
    let flows = &c.plans.views[0];
    assert_eq!(flows.title(), "Application flows");
    assert_eq!(flows.description().as_deref(), Some("Who calls whom."));
    let bare = &c.plans.views[1];
    assert_eq!(bare.title(), "bare");
    assert_eq!(bare.description(), None);
}

#[test]
fn view_attributes_are_checked() {
    let c = compiled("view v {\n  titel: \"x\"\n  description: plain\n  select system\n  render text\n}\n");
    let errors: Vec<&str> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.as_str()).collect();
    assert_eq!(errors, ["unknown view attribute: titel", "description is a quoted text"], "{errors:?}");
    let help = c.diagnostics.iter().find(|d| d.message.contains("titel")).and_then(|d| d.help.clone());
    assert_eq!(help.as_deref(), Some("did you mean `title`?"));
}
