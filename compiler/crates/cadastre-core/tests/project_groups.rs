//! Groups of the project: `group storage: database | saas`, usable wherever a kind is.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
group storage: database | saas
group data-stores: storage | bucket
kind bucket { like: saas }
kind app { backup_to: ref(storage) }

team t
site dc {
  hosting: on-prem
  network lan {
    subnet s {
      cidr: 10.0.0.0/24
      server db01 { owner: t }
    }
  }
}
database crm-db { owner: t, on: db01 }
saas archive { owner: t }
bucket photos { owner: t }
app crm { owner: t, on: db01, backup_to: archive }
"#;

fn compilation(extra: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: format!("{REGISTRY}{extra}") },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

#[test]
fn a_group_selects_its_kinds_and_those_like_them() {
    let c = compilation("", "view stores { select data-stores render text }");
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let (outputs, _) = build(&c, None);
    let text = &outputs.iter().find(|o| o.format == "text").unwrap().content;
    for name in ["crm-db", "archive", "photos"] {
        assert!(text.contains(name), "{name}: {text}");
    }
    assert!(!text.contains("crm ["), "{text}");
}

#[test]
fn a_group_in_a_reference_type() {
    let c = compilation("app other { owner: t, on: db01, backup_to: db01 }\n", "");
    let errors = messages(&c, Severity::Error);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("db01"), "{errors:?}");
}

#[test]
fn errors() {
    let c = compilation(
        "group storage: app\ngroup server: app\ngroup network_device: app\ngroup odd: databse\ngroup a: b\ngroup b: a\n",
        "",
    );
    let errors = messages(&c, Severity::Error);
    for expected in [
        "group storage declared twice",
        "server is already a kind",
        "network_device is already a built-in group",
        "unknown kind: databse",
        "group a: its members make a cycle (a → b → a)",
    ] {
        assert!(errors.contains(&expected.to_owned()), "{expected}: {errors:?}");
    }
}
