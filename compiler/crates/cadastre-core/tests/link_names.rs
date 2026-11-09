//! The names of the links (`as F-042`) and their former ones (`aliases`): designated by a step
//! of a process, filtered, listed in a table, shown in the tooltips and the exports.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
app shop {
  -> db : jdbc [flow] as F-042 { aliases: [F-17, "OLD 3"] }
  -> stock : https [flow] as F-043
}
app db
app stock
shop -> stock : amqp [flow]
"#;

fn compilation(more: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: format!("{REGISTRY}{more}") },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

#[test]
fn filtered_and_listed() {
    let c = compilation(
        "",
        "view one {\n  select app\n  edges [flow] where name = F-042\n  render markdown { columns: [name, source, target, aliases] }\n}\n\
         view all {\n  select app\n  edges [flow]\n  render markdown { columns: [name, proto] }\n}\n\
         view drawn {\n  select app\n  edges [flow]\n  render dot\n}\n",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let output = |view: &str| outputs.iter().find(|o| o.view == view).unwrap().content.clone();
    assert_eq!(
        output("one"),
        "| name | source | target | aliases |\n|---|---|---|---|\n| F-042 | shop | db | F-17, OLD 3 |\n"
    );
    // A row per link, a link without a name with an empty cell.
    assert_eq!(output("all"), "| name | proto |\n|---|---|\n| F-042 | jdbc |\n| F-043 | https |\n|  | amqp |\n");
    // In the tooltip of its edge, as written.
    assert!(output("drawn").contains("from: shop -> db : jdbc as F-042"), "{}", output("drawn"));
}

#[test]
fn a_step_designating_a_link_by_its_name() {
    let c = compilation("process orders {\n  step F-042\n  step F-17 { reply }\n  step shop -> stock : https\n}\n", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let process = c.registry.resources.iter().position(|r| r.name == "orders").unwrap();
    let events = &c.model.processes[&cadastre_core::registry::ResourceId(process as u32)];
    let steps: Vec<(String, String, bool)> = events
        .iter()
        .filter_map(|e| e.step())
        .map(|s| {
            let name = |id| c.registry.resource(id).name.clone();
            (name(s.source), name(s.target.unwrap()), s.reply)
        })
        .collect();
    // By its name, then by a former one as a reply: from db back to shop.
    assert_eq!(
        steps,
        [
            ("shop".into(), "db".into(), false),
            ("db".into(), "shop".into(), true),
            ("shop".into(), "stock".into(), false)
        ]
    );
}

#[test]
fn names_designate_one_link() {
    let c =
        compilation("app other { -> db : jdbc [flow] as F-043 { aliases: [F-17] } }\nprocess p { step F-99 }\n", "");
    let found = errors(&c);
    assert_eq!(found.iter().filter(|m| m.as_str() == "the link name F-043 is given twice").count(), 1, "{found:?}");
    assert!(found.contains(&"the link name F-17 is given twice".to_owned()), "{found:?}");
    assert!(found.contains(&"no link named F-99".to_owned()), "{found:?}");
}

#[test]
fn go_to_the_link_of_a_step() {
    let text = format!("{REGISTRY}process orders {{\n  step F-042\n  step F-17 {{ reply }}\n}}\n");
    let c = compile(&[SourceFile { path: "a.ca".into(), text: text.clone() }]);
    let file = cadastre_core::source::FileId(0);
    let at = |needle: &str| text.find(needle).unwrap();
    // From the step to the name of the link, or to the former name written.
    let span = c.link_definition(file, at("step F-042") + 6).unwrap();
    assert_eq!(span.start, at("as F-042") + 3);
    let span = c.link_definition(file, at("step F-17") + 6).unwrap();
    assert_eq!(&text[span.start..span.end], "F-17");
    assert!(c.link_definition(file, at("app db")).is_none());
}
