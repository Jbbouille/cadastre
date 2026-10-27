//! The `schedule` of jobs: cron expressions checked and shown in plain words, free text to convert.

use cadastre_core::{Compilation, Severity, SourceFile, build_with, compile};

const REGISTRY: &str = r#"team t
app scheduler { owner: t, on: farm, timezone: "Europe/Paris" }
fleet farm
system batch {
  owner: t
  job purge   { schedule: "0 3 * * *" }
  job prices  { schedule: "0 6,18 * * 1-5", scheduler: scheduler }
  job report  { schedule: "10h, 12h10, 19h" }
  job broken  { schedule: "0 25 * * *" }
}
"#;

fn compilation(views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

#[test]
fn checked_and_to_convert() {
    let c = compilation("");
    let messages = |severity: Severity| -> Vec<String> {
        c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
    };
    assert_eq!(messages(Severity::Error), ["invalid schedule: hour: 25 is out of range (0-23)"]);
    assert!(
        messages(Severity::Incomplete)
            .contains(&"job batch.report: schedule in free text, \"10h, 12h10, 19h\"".to_owned()),
        "{:?}",
        messages(Severity::Incomplete)
    );
}

#[test]
fn plain_words_in_labels_and_tooltips() {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.replace("job broken  { schedule: \"0 25 * * *\" }", "") },
        SourceFile {
            path: "v.cav".into(),
            text: "view jobs { select job render text, dot { node_labels: [schedule] } }".into(),
        },
    ]);
    let (outputs, _) = build_with(&c, None, None);
    let text = &outputs[0].content;
    // The time zone of the scheduler, a free text as written.
    assert!(text.contains("every day at 03:00"), "{text}");
    assert!(text.contains("at 06:00 and 18:00 on Monday to Friday (Europe/Paris)"), "{text}");
    assert!(text.contains("10h, 12h10, 19h"), "{text}");
    // Tooltips keep the expression, its words beside it.
    let dot = &outputs[1].content;
    assert!(dot.contains("schedule: 0 3 * * * (every day at 03:00)"), "{dot}");
}
