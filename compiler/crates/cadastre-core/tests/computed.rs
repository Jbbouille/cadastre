//! Computed keys (spec § 4, Computed keys): a key of a kind computed from other keys of the
//! resource, `max` or `min` of numbers or of values of a list ordered as written.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const SCHEMA: &str = r#"
kind app {
  availability: low | medium | high
  integrity: low | medium | high
  confidentiality: low | medium | high
  security: max(availability, integrity, confidentiality)
  floor: min(availability, integrity)
  cost: number
  run: number
  dearest: max(cost, run)
}
"#;

fn compilation(registry: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "schema.ca".into(), text: SCHEMA.into() },
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

const APPS: &str = "app shop { availability: low, integrity: high, cost: 12, run: 30 }
app wiki { availability: medium, confidentiality: medium, cost: 4 }
app draft
";

#[test]
fn computed_from_the_written_keys() {
    let c = compilation(
        APPS,
        "view v {\n  select app\n  render markdown { columns: [name, security, floor, dearest] }\n}\n\
         view high {\n  select app where security = high\n  render text\n}\n\
         view pivot {\n  select app\n  render markdown { group: security }\n}\n",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let output = |view: &str| outputs.iter().find(|o| o.view == view).unwrap().content.clone();
    // The highest value of the list, the lowest, the highest number; none without any key.
    assert_eq!(
        output("v"),
        "| name | security | floor | dearest |\n|---|---|---|---|\n\
         | shop | high | low | 30 |\n| wiki | medium | medium | 4 |\n| draft |  |  |  |\n"
    );
    // Filtered and counted like a written key.
    let high = output("high");
    assert!(high.contains("shop") && !high.contains("wiki"), "{high}");
    assert!(output("pivot").contains("| high | 1 |"), "{}", output("pivot"));
}

#[test]
fn written_by_hand() {
    let c = compilation("app shop { availability: low, security: high }", "");
    assert_eq!(errors(&c), ["security is computed: max(availability, integrity, confidentiality)"]);
    // Replaced by its value.
    let shop = c.registry.resources.iter().find(|r| r.name == "shop").unwrap();
    assert_eq!(shop.attribute("security").unwrap().value.raw, "low");
}

#[test]
fn invalid_declarations() {
    let schema = "kind app {
  availability: low | medium | high
  impact: minor | major
  cost: number
  label2: string
  a: maxi(availability)
  b: max(availabilty)
  c: max(availability, impact)
  d: max(availability, cost)
  e: max(label2)
  f: max(availability) (required)
  g: max(f)
}
link { h: max(volume) }
";
    let c = compile(&[SourceFile { path: "schema.ca".into(), text: schema.into() }]);
    let found = errors(&c);
    for expected in [
        "unknown function: maxi",
        "availabilty is not a key of app",
        "max: availability and impact have different types (low | medium | high, minor | major)",
        "max: availability and cost have different types (low | medium | high, number)",
        "max: label2 is a string, not a number nor a list of values",
        "f is computed: it is not required",
        "f is computed: a computed key is computed from written keys",
        "a key of the links is written, not computed",
    ] {
        assert!(found.iter().any(|m| m == expected), "{expected}: {found:?}");
    }
}
