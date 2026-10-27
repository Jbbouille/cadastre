//! Kinds within a resource (`participant`, `interface`, `component`, `channel`): details of their
//! resource, resources like the others; `select participant` is warned about.

use cadastre_core::{Severity, SourceFile, compile};

const REGISTRY: &str = r#"
team t
site dc { hosting: on-prem, network lan { subnet s { cidr: 10.0.0.0/24, server esx1 { owner: t }, server esx2 { owner: t } } } }
app vcenter { owner: t, on: esx1, -> esx1 : https/443 [flow], -> esx2 : https/443 [flow] }
process backup {
  participant esx "ESXi hosts" [esx1, esx2]
  step vcenter -> esx
}
"#;

fn diagnostics(views: &str) -> Vec<(Severity, String)> {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    c.diagnostics.iter().map(|d| (d.severity, d.message.clone())).collect()
}

#[test]
fn selecting_a_participant_is_warned_about() {
    let found = diagnostics("view v { select participant render text }");
    assert!(
        found.contains(&(Severity::Warning, "a participant stands for its members in the steps of a process".into())),
        "{found:?}"
    );
    assert!(found.iter().all(|(s, _)| *s != Severity::Error), "{found:?}");
}

#[test]
fn a_participant_is_a_resource_like_the_others() {
    // Resolved by name, with a qualified name: a reference to it from a view filter.
    let found = diagnostics("view v { select server where name = esx1 render text }");
    assert!(found.iter().all(|(s, _)| *s != Severity::Error), "{found:?}");
    let c = compile(&[SourceFile { path: "a.ca".into(), text: REGISTRY.into() }]);
    let esx = c.registry.ids().find(|&id| c.registry.resource(id).name == "esx").unwrap();
    assert_eq!(c.registry.qualified_name(esx), "backup.esx");
    assert!(c.registry.schema.kind_def("participant").unwrap().within);
    assert!(!c.registry.schema.kind_def("server").unwrap().within);
}
