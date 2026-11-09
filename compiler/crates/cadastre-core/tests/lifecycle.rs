//! `status`: in service (`active`, by default), to come (`planned`) or withdrawn
//! (`decommissioned`): drawn in blue and in grey, left out of the impact analysis when
//! withdrawn, and what is in service but still depends on something withdrawn warned about.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  server old { status: decommissioned }
  server web02
  server next { status: planned }
}
site closed {
  hosting: on-prem
  status: decommissioned
  server left
}
database db { on: old, status: decommissioned }
app shop { on: [old, web02], -> db : jdbc [flow] }
app archive { status: decommissioned, -> db : jdbc [flow] }
"#;

fn compilation(views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

#[test]
fn what_depends_on_something_withdrawn() {
    let c = compilation("");
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let warnings = messages(&c, Severity::Warning);
    for expected in [
        "shop is in service, on: dc.old, decommissioned",
        "closed.left is in service, in closed, decommissioned",
        "the link shop -> db is in service, db decommissioned",
    ] {
        assert!(warnings.iter().any(|w| w == expected), "{expected}: {warnings:?}");
    }
    // What is withdrawn may depend on what is withdrawn.
    assert!(!warnings.iter().any(|w| w.starts_with("archive") || w.contains("archive ->")), "{warnings:?}");
}

#[test]
fn the_withdrawn_neither_fail_nor_are_needed() {
    // `shop` runs on `old` and `web02`: `old` withdrawn, `web02` alone takes it down.
    let c = compilation(
        "view v {\n  select server, app where name in [shop] and failures has web02\n  render text\n}\n\
         view w {\n  select app where failures has old\n  render text\n}\n",
    );
    let (outputs, _) = build(&c, None);
    let text = |view: &str| outputs.iter().find(|o| o.view == view).unwrap().content.clone();
    assert!(text("v").contains("shop"), "{}", text("v"));
    assert!(!text("w").contains("shop"), "{}", text("w"));
}

#[test]
fn drawn_in_their_colors() {
    let c = compilation(
        "view v {\n  select server, app, database\n  edges [flow]\n  render dot { legend: true }\n}\n\
         view active {\n  select server where status = active\n  render text\n}\n",
    );
    let (outputs, _) = build(&c, None);
    let dot = outputs.iter().find(|o| o.view == "v").unwrap().content.clone();
    assert!(dot.contains("#2563eb") && dot.contains("#9ca3af"), "{dot}");
    assert!(dot.contains("Planned") && dot.contains("Decommissioned"), "{dot}");
    // `active` by default.
    let active = outputs.iter().find(|o| o.view == "active").unwrap().content.clone();
    assert!(active.contains("web02") && active.contains("left") && !active.contains("old"), "{active}");
}
