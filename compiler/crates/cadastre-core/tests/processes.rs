//! Business processes: steps in their order, each a declared link, the reply to one, or a
//! processing; the links of a process in a view; its sequence diagram.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
data orders
app scheduler { -> broker [trigger] }
app broker {
  -> extractor : hessian [flow] { carries: [orders] }
  -> store : hessian [flow]
  -> directory : https [flow] { processes: [orders-to-stores] }
}
app extractor { -> db : jdbc [flow] { exchange: pull, carries: [orders] } }
app directory
database db
app store { -> store-db : jdbc [flow] }
database store-db

system sales {
  process orders-to-stores {
    description: "Orders sent to the stores"
    step scheduler -> broker [trigger]
    step broker -> extractor : hessian { async }
    step extractor -> broker { reply, label: "ack" }
    step extractor -> db : jdbc { note: "reads the orders" }
    step broker "transforms the orders"
    loop "for each store" {
      step broker -> store : hessian
      step store -> store-db
    }
  }
}
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

fn output(views: &str, format: &str) -> String {
    let c = compilation(REGISTRY, views);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    outputs.into_iter().find(|o| o.format == format).unwrap().content
}

#[test]
fn steps_are_links() {
    assert!(errors(&compilation(REGISTRY, "")).is_empty());
    // A step without its link, a reply without the call, a flag or a key unknown.
    let wrong = REGISTRY
        .replace("step store -> store-db", "step store-db -> store")
        .replace("step extractor -> broker { reply, label: \"ack\" }", "step db -> broker { reply }")
        .replace("{ async }", "{ asap, color: red }");
    assert_eq!(
        errors(&compilation(&wrong, "")),
        [
            "unknown flag of a step: asap",
            "unknown key of a step: color",
            "no link broker -> db for this step",
            "no link store-db -> store for this step",
        ]
    );
}

#[test]
fn the_links_of_a_process() {
    let text = output(
        "view v {\n  select app, database\n  edges process(orders-to-stores)\n  render text { edge_labels: [step] }\n}",
        "text",
    );
    let edges = &text[text.find("\nedges\n").unwrap()..];
    // In the order of the steps, a link once; the processing and the reply have no link of their
    // own; a link serving the process (`processes:`), without a step.
    for (edge, step) in [
        ("scheduler -> broker", "[1.]"),
        ("broker -> extractor", "[2.]"),
        ("extractor -> db", "[4.]"),
        ("broker -> store", "[6.]"),
        ("store -> store-db", "[7.]"),
    ] {
        assert!(edges.contains(&format!("{edge}  {step}")), "{edge} {step}: {edges}");
    }
    assert!(edges.contains("broker -> directory\n"), "{edges}");
}

#[test]
fn a_sequence_diagram() {
    let sequence =
        output("view v {\n  select app, database\n  edges process(orders-to-stores)\n  render sequence\n}", "sequence");
    let id = |name: &str| {
        let line = sequence.lines().find(|l| l.ends_with(&format!(" as {name}"))).unwrap();
        line.split_whitespace().nth(1).unwrap().to_owned()
    };
    let (scheduler, broker, extractor, db, store) =
        (id("scheduler"), id("broker"), id("extractor"), id("db"), id("store"));
    for line in [
        "sequenceDiagram".to_owned(),
        "  autonumber".to_owned(),
        "  title orders-to-stores".to_owned(),
        format!("  {scheduler}->>{broker}: trigger"),
        format!("  {broker}-){extractor}: Hessian · orders"),
        format!("  {extractor}-->>{broker}: ack"),
        format!("  {extractor}->>{db}: JDBC · orders"),
        format!("  Note over {extractor},{db}: reads the orders"),
        format!("  {broker}->>{broker}: transforms the orders"),
        "  loop for each store".to_owned(),
        format!("    {broker}->>{store}: Hessian"),
        "  end".to_owned(),
    ] {
        assert!(sequence.lines().any(|l| l == line), "{line}\n{sequence}");
    }
    // A view without a process: a warning, an empty diagram.
    let c = compilation(REGISTRY, "view v {\n  select app\n  render sequence\n}");
    let (_, diagnostics) = build(&c, None);
    assert!(diagnostics.iter().any(|d| d.message == "view v: render sequence draws the steps of a process"));
}

#[test]
fn the_participants_the_title_and_the_protocols() {
    let registry = REGISTRY
        .replace("description: \"Orders sent to the stores\"", "participants: [store, scheduler]")
        .replace("step broker -> store : hessian", "step broker -> store : hessian { label: \"sends the orders\" }");
    let c = compilation(
        &registry,
        "view v {\n  select app, database\n  edges process(orders-to-stores)\n  render sequence { title: false }\n}\n\
         view w {\n  select app, database\n  edges process(orders-to-stores)\n  render sequence { edge_labels: [step] }\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let of = |view: &str| outputs.iter().find(|o| o.view == view && o.format == "sequence").unwrap().content.clone();
    let (v, w) = (of("v"), of("w"));
    // The participants listed first, in their order, then the others as they appear.
    let names: Vec<&str> =
        v.lines().filter(|l| l.starts_with("  participant ")).map(|l| l.rsplit(" as ").next().unwrap()).collect();
    assert_eq!(names, ["store", "scheduler", "broker", "extractor", "db", "store-db"]);
    // A participant shown by its `sequence_label`, in place of its label.
    let c = compilation(
        &registry.replace("app store {", "app store { sequence_label: \"Store\\nlin01\"\n"),
        "view v {\n  select app, database\n  edges process(orders-to-stores)\n  render sequence\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let labelled = &outputs.iter().find(|o| o.format == "sequence").unwrap().content;
    assert!(labelled.lines().any(|l| l.ends_with(" as Store<br/>lin01")), "{labelled}");
    // No title; a label followed by the protocol of its link, unless the view hides the ports.
    assert!(!v.contains("title"), "{v}");
    assert!(w.contains("title orders-to-stores"), "{w}");
    assert!(v.lines().any(|l| l.ends_with(": sends the orders (Hessian)")), "{v}");
    assert!(w.lines().any(|l| l.ends_with(": sends the orders")), "{w}");
    // A reply keeps its label alone, its port being that of the call.
    assert!(v.lines().any(|l| l.ends_with(": ack")), "{v}");
    // A participant that is not an end of a flow is an error.
    let wrong = REGISTRY.replace("description: \"Orders sent to the stores\"", "participants: [orders]");
    assert!(!errors(&compilation(&wrong, "")).is_empty());
}

const GROUPS: &str = r#"
site dc {
  cluster prod
  cluster empty
  server esx1 { cluster: prod }
  server esx2 { cluster: prod }
  server esx3
}
app vcenter {
  -> esx1 : https [flow]
  -> esx2 : https [flow]
  -> esx3 : https [flow]
}
app proxy {
  -> esx1 : nfc/902 [flow]
  -> esx2 : nfc/902 [flow]
  -> esx3 : nfc/902 [flow]
  -> vcenter : https [flow]
}
process backup {
  participant esx "ESXi hosts" [prod, esx3]
  participants: [vcenter, esx, proxy]
  step vcenter -> esx { label: "snapshot" }
  step proxy -> esx : nfc/902 { label: "reads the blocks" }
  step esx -> proxy : nfc/902 { reply, label: "blocks" }
  step proxy -> vcenter
}
"#;

#[test]
fn a_participant_standing_for_several_resources() {
    let c = compilation(
        GROUPS,
        "view s {\n  select app, server\n  edges process(backup)\n  render sequence, text { edge_labels: [step] }\n}",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, _) = build(&c, None);
    let of = |format: &str| outputs.iter().find(|o| o.format == format).unwrap().content.clone();
    let sequence = of("sequence");
    // One participant for the group, with its label, in the place `participants` gives it.
    let names: Vec<&str> = sequence
        .lines()
        .filter(|l| l.starts_with("  participant "))
        .map(|l| l.rsplit(" as ").next().unwrap())
        .collect();
    assert_eq!(names, ["vcenter", "ESXi hosts", "proxy"], "{sequence}");
    let id = |name: &str| {
        let line = sequence.lines().find(|l| l.ends_with(&format!(" as {name}"))).unwrap();
        line.split_whitespace().nth(1).unwrap().to_owned()
    };
    let (vcenter, esx, proxy) = (id("vcenter"), id("ESXi hosts"), id("proxy"));
    for line in [
        format!("  {vcenter}->>{esx}: snapshot"),
        format!("  {proxy}->>{esx}: reads the blocks"),
        format!("  {esx}-->>{proxy}: blocks"),
    ] {
        assert!(sequence.lines().any(|l| l == line), "{line}\n{sequence}");
    }
    // In the views, a link per member (the servers of the cluster, then the server), numbered
    // as their step.
    let text = of("text");
    let edges = &text[text.find("\nedges\n").unwrap()..];
    for (edge, step) in [
        ("vcenter -> dc.esx1", "[1.]"),
        ("vcenter -> dc.esx2", "[1.]"),
        ("vcenter -> dc.esx3", "[1.]"),
        ("proxy -> dc.esx1", "[2.]"),
        ("proxy -> dc.esx3", "[2.]"),
        ("proxy -> vcenter", "[4.]"),
    ] {
        assert!(edges.contains(&format!("{edge}  {step}")), "{edge} {step}: {edges}");
    }
}

#[test]
fn the_members_of_a_participant_are_checked() {
    // A member without the link of a step: the step names it, the help what the participant
    // stands for.
    let missing = GROUPS.replace("  -> esx2 : https [flow]\n", "");
    let c = compilation(&missing, "");
    assert_eq!(errors(&c), ["no link vcenter -> dc.esx2 for this step"]);
    let help = c.diagnostics.iter().find(|d| d.severity == Severity::Error).unwrap().help.clone().unwrap_or_default();
    assert!(help.contains("esx stands for dc.esx1, dc.esx2, dc.esx3"), "{help}");
    // A reply checked the other way, for each member.
    let reply = GROUPS.replace("  -> esx3 : nfc/902 [flow]\n", "");
    assert_eq!(
        errors(&compilation(&reply, "")),
        ["no link proxy -> dc.esx3 : nfc/902 for this step", "no link proxy -> dc.esx3 : nfc/902 for this step"]
    );
    // A participant standing for nothing; members that are not ends of flows nor clusters.
    let empty = GROUPS.replace("[prod, esx3]", "[empty]");
    assert!(errors(&compilation(&empty, "")).contains(&"participant esx stands for no resource".to_owned()));
    // Without a label, its members as written.
    let unlabeled = GROUPS.replace("participant esx \"ESXi hosts\" [prod, esx3]", "participant esx [prod, esx3]");
    let c = compilation(&unlabeled, "view s {\n  select app, server\n  edges process(backup)\n  render sequence\n}");
    let (outputs, _) = build(&c, None);
    assert!(outputs[0].content.contains(" as prod, esx3\n"), "{}", outputs[0].content);
}

#[test]
fn participants_are_formatted() {
    let formatted = cadastre_core::format::format(GROUPS.trim_start()).unwrap();
    assert!(formatted.contains("  participant esx \"ESXi hosts\" [prod, esx3]\n"), "{formatted}");
    assert_eq!(cadastre_core::format::format(&formatted).unwrap(), formatted);
}
