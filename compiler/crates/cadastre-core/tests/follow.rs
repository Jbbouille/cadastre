//! `follow`: the links carrying a data object, walked in the direction of the data (spec § 6).

use cadastre_core::view::{self, Graph};
use cadastre_core::{Compilation, Diagnostic, Severity, SourceFile, build, compile};

/// A price chain: a scheduler triggers the pricing UI, which drops a file on a file hub;
/// a processor picks it up and publishes it on a topic, consumed by two apps.
const REGISTRY: &str = r#"
data item-prices
data articles

app scheduler {
  -> pricing-ui : http [trigger] { carries: [item-prices] }
}

app pricing-ui {
  -> filehub   : sftp [flow] { exchange: push, carries: [item-prices] }
  -> filehub   : http [flow] { exchange: push, carries: [articles] }
}

server filehub

app processor {
  -> filehub : sftp [flow] { exchange: pull, carries: [item-prices], confidence: doubtful }
  -> kafka   : kafka/9092 [flow] { exchange: push, channel: prices }
}

saas kafka {
  channel prices { type: topic, carries: [item-prices] }
}

app catalog { -> kafka : kafka/9092 [flow] { exchange: pull, channel: prices } }
app stores  { -> kafka : kafka/9092 [flow] { exchange: pull, channel: prices } }

// Upstream of the starting point: never reached.
app pricing-tool { -> processor : sftp [flow] { exchange: push, carries: [item-prices] } }
"#;

fn compilation(views: &str) -> Compilation {
    let c = compile(&[
        SourceFile { path: "registry.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "views.cav".into(), text: views.into() },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

fn evaluate(c: &Compilation, name: &str) -> (Graph, Vec<Diagnostic>) {
    let view = c.plans.views.iter().find(|v| v.name == name).expect("view exists");
    let mut diagnostics = Vec::new();
    let graph = view::evaluate(view, &c.registry, &c.model, &mut diagnostics);
    (graph, diagnostics)
}

/// Edges as `step. source -> target`, sorted by step.
fn steps(c: &Compilation, graph: &Graph) -> Vec<String> {
    let name = |id| c.registry.resource(id).name.clone();
    let mut out: Vec<(Option<usize>, String)> = graph
        .edges
        .iter()
        .map(|e| {
            (
                e.step,
                format!(
                    "{}. {} -> {}",
                    e.step.map_or("-".to_owned(), |s| s.to_string()),
                    name(e.source),
                    name(e.target)
                ),
            )
        })
        .collect();
    out.sort();
    out.into_iter().map(|(_, s)| s).collect()
}

#[test]
fn walks_in_the_direction_of_the_data_from_the_starting_point() {
    let c = compilation("view chain {\n  select app, saas, server\n  edges follow(item-prices, from: scheduler)\n}");
    let (g, d) = evaluate(&c, "chain");
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(
        steps(&c, &g),
        [
            "1. scheduler -> pricing-ui",
            "2. pricing-ui -> filehub",
            "3. processor -> filehub",
            "4. processor -> kafka",
            "5. catalog -> kafka",
            "5. stores -> kafka",
        ]
    );
}

#[test]
fn without_a_starting_point_every_carrying_link_is_kept_without_step() {
    let c = compilation("view all {\n  select app, saas, server\n  edges follow(item-prices)\n}");
    let (g, _) = evaluate(&c, "all");
    assert_eq!(g.edges.len(), 7);
    assert!(g.edges.iter().all(|e| e.step.is_none()));
}

#[test]
fn the_filter_applies_before_the_walk() {
    let c = compilation(
        "view confirmed {\n  select app, saas, server\n  edges follow(item-prices, from: scheduler) where confidence = confirmed\n}",
    );
    let (g, _) = evaluate(&c, "confirmed");
    // The doubtful pull from the file hub breaks the chain: the processor is never reached.
    assert_eq!(steps(&c, &g), ["1. scheduler -> pricing-ui", "2. pricing-ui -> filehub"]);
}

#[test]
fn the_step_prefixes_the_edge_label() {
    let c = compilation(
        "view chain {\n  select app, saas, server\n  edges follow(item-prices, from: scheduler)\n  render text { edge_labels: [step, port] }\n}",
    );
    let (outputs, _) = build(&c, Some("chain"));
    let text = &outputs[0].content;
    assert!(text.contains("pricing-ui -> filehub  [2. sftp]"), "{text}");
    assert!(text.contains("step: 4"), "{text}");
}

#[test]
fn reports_wrong_names_and_unreachable_starts() {
    let c = compile(&[
        SourceFile { path: "registry.ca".into(), text: REGISTRY.into() },
        SourceFile {
            path: "views.cav".into(),
            text: "view v {\n  select app\n  edges follow(item-price, from: filehub, to: kafka), follow(scheduler)\n}"
                .into(),
        },
    ]);
    let errors: Vec<String> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
    assert_eq!(
        errors,
        ["dangling reference: item-price", "invalid follow option: to: kafka", "scheduler is a app, data expected",]
    );
    assert_eq!(
        c.diagnostics.iter().find(|d| d.message == "dangling reference: item-price").and_then(|d| d.help.as_deref()),
        Some("did you mean `item-prices`?")
    );

    let c = compilation("view v {\n  select app, saas, server\n  edges follow(articles, from: catalog)\n}");
    let (_, d) = evaluate(&c, "v");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "view v: follow(articles, from: catalog) reaches no link carrying articles");
}

#[test]
fn side_calls_of_the_processing() {
    // The promotions go from the database through the extractor to the broker; the extractor
    // also asks the directory where to send them: a call serving the processing, not carrying
    // the promotions. `side: [stores]` draws it, without a step; an unrelated call stays out.
    let registry = "data promotions
data stores
data invoices
        database promo-db
        app extractor {
  -> promo-db : jdbc [flow] { exchange: pull, carries: [promotions] }
  -> directory : https [flow] { exchange: pull, carries: [stores] }
  -> broker : https [flow] { exchange: push, carries: [promotions] }
}
        app directory
app broker { -> billing : https [flow] { carries: [invoices] } }
app billing
";
    let text = |side: &str| {
        let views = format!(
            "view v {{
  select app, database
  edges follow(promotions, from: promo-db{side})
  render text {{ edge_labels: [step] }}
}}"
        );
        let c = compile(&[
            SourceFile { path: "a.ca".into(), text: registry.into() },
            SourceFile { path: "v.cav".into(), text: views },
        ]);
        let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{errors:?}");
        let (outputs, _) = cadastre_core::build(&c, None);
        let content = outputs[0].content.clone();
        content[content
            .find(
                "
edges
",
            )
            .unwrap()..]
            .to_owned()
    };
    let path = text("");
    assert!(path.contains("extractor -> promo-db") && path.contains("extractor -> broker"), "{path}");
    assert!(!path.contains("directory"), "{path}");
    let with_side = text(", side: [stores]");
    assert!(with_side.contains("extractor -> directory"), "{with_side}");
    assert!(!with_side.contains("billing"), "{with_side}");
    // `all`: every link leaving the resources on the way, whatever it carries.
    let all = text(", side: all");
    assert!(all.contains("extractor -> directory") && all.contains("broker -> billing"), "{all}");
}
