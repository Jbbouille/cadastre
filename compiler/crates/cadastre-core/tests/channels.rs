//! Channels of a broker: links told apart by their channels, `follow` crossing a broker once per
//! channel, in the order of the data.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// A producer writes raw prices to a topic, a stream reads them and writes them to another topic
/// of the same broker, a sink reads that one.
const REGISTRY: &str = r#"
data prices
saas confluent {
  channel raw-prices { type: topic, carries: [prices] }
  channel store-prices { type: topic, carries: [prices] }
}
app producer { -> confluent : kafka [flow] { exchange: push, channel: raw-prices } }
app streams {
  -> confluent : kafka [flow] { exchange: pull, channel: raw-prices }
  -> confluent : kafka [flow] { exchange: push, channel: store-prices }
}
app sink { -> confluent : kafka [flow] { exchange: pull, channel: store-prices } }
"#;

fn compilation(registry: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

#[test]
fn two_links_to_a_broker_on_two_channels() {
    let c = compilation(REGISTRY, "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    // On the same channel, still twice the same link.
    let twice = REGISTRY.replace(
        "-> confluent : kafka [flow] { exchange: push, channel: store-prices }",
        "-> confluent : kafka [flow] { exchange: push, channel: raw-prices }",
    );
    let c = compilation(&twice, "");
    assert_eq!(errors(&c), ["link declared twice: streams -> confluent : kafka"]);
}

#[test]
fn follow_crosses_a_broker_once_per_channel() {
    let c = compilation(
        REGISTRY,
        "view chain {\n  select app, saas\n  edges follow(prices, from: producer)\n  render text { edge_labels: [step] }\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let text = &outputs[0].content;
    let step = |edge: &str| {
        let at = text.find(edge).unwrap_or_else(|| panic!("{edge} in {text}"));
        text[at..].lines().nth(1).unwrap().trim().to_owned()
    };
    // raw prices → streams → store prices → sink, in order.
    assert_eq!(step("producer -> confluent"), "step: 1");
    let streams: Vec<&str> = text.matches("streams -> confluent  [").collect();
    assert_eq!(streams.len(), 2, "{text}");
    assert!(text.contains("streams -> confluent  [2.]") && text.contains("streams -> confluent  [3.]"), "{text}");
    assert_eq!(step("sink -> confluent"), "step: 4");
}

#[test]
fn a_channel_named_in_a_filter_leads_to_it() {
    // `channel in [raw-prices]`: the name designates the channel, for the editors.
    let views = "view v {
  select app, saas
  edges [flow] where channel in [raw-prices] or carries has prices
}";
    let c = compilation(REGISTRY, views);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let at = |name: &str| {
        let offset = views.find(name).unwrap() + 1;
        c.definitions(cadastre_core::source::FileId(1), offset)
            .iter()
            .map(|&id| c.registry.qualified_name(id))
            .collect::<Vec<_>>()
    };
    assert_eq!(at("raw-prices"), ["confluent.raw-prices"]);
    assert_eq!(
        at("prices
"),
        ["prices"]
    );
}
