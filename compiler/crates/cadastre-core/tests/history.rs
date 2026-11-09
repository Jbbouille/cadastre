//! The history of the resources (`[history]` in `cadastre.toml`): the last change of each, from
//! the last change of its own lines, read by filters and tables as `changed` and `changed_by`.

use cadastre_core::history::{Change, Lines};
use cadastre_core::source::FileId;
use cadastre_core::{Options, SourceFile, build, compile_project};

const REGISTRY: &str = "site dc {
  hosting: on-prem
  server old
  server new
}
app shop { on: new }
";

/// The bytes of a line of `text`, from 1, its indentation left out.
fn line(text: &str, n: usize) -> std::ops::Range<usize> {
    let start: usize = text.split_inclusive('\n').take(n - 1).map(str::len).sum();
    let content = text.split_inclusive('\n').nth(n - 1).unwrap().trim_end();
    let leading = content.len() - content.trim_start().len();
    start + leading..start + content.len()
}

fn change(day: i64, author: &str) -> Change {
    Change { time: day * 86_400, author: author.to_owned(), commit: format!("{day:040}") }
}

#[test]
fn the_last_change_of_each_resource() {
    // 2025-01-01 is day 20089 since 1970.
    let lines: Lines = vec![
        (line(REGISTRY, 1), change(20_089, "Ana")),
        (line(REGISTRY, 2), change(20_089, "Ana")),
        (line(REGISTRY, 3), change(20_089, "Ana")),
        // The new server, written later: its own change, not that of its site.
        (line(REGISTRY, 4), change(20_200, "Bo")),
        (line(REGISTRY, 5), change(20_089, "Ana")),
        (line(REGISTRY, 6), change(20_100, "Cy")),
    ];
    let views = "view v {\n  select site, server, app\n  render markdown { columns: [name, changed, changed_by] }\n}\n\
                 view recent {\n  select server where changed >= \"2025-03-01\"\n  render text\n}\n";
    let c = compile_project(
        &[
            SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
            SourceFile { path: "v.cav".into(), text: views.into() },
        ],
        &[],
        Options::default(),
        &[(FileId(0), lines)],
    );
    let (outputs, _) = build(&c, None);
    let output = |view: &str| outputs.iter().find(|o| o.view == view).unwrap().content.clone();
    assert_eq!(
        output("v"),
        "| name | changed | changed_by |\n|---|---|---|\n\
         | dc | 2025-01-01 | Ana |\n| dc.old | 2025-01-01 | Ana |\n| dc.new | 2025-04-22 | Bo |\n| shop | 2025-01-12 | Cy |\n"
    );
    let recent = output("recent");
    assert!(recent.contains("new") && !recent.contains("old"), "{recent}");
}

#[test]
fn without_history() {
    let views = "view v {\n  select server\n  render markdown { columns: [name, changed] }\n}\n";
    let c = compile_project(
        &[
            SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
            SourceFile { path: "v.cav".into(), text: views.into() },
        ],
        &[],
        Options::default(),
        &[],
    );
    assert!(c.diagnostics.iter().all(|d| d.severity != cadastre_core::Severity::Error), "{:?}", c.diagnostics);
    let (outputs, _) = build(&c, None);
    assert!(outputs[0].content.contains("| dc.old |  |"), "{}", outputs[0].content);
}
