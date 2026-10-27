//! Exports of the whole registry: ArchiMate, GraphML, JSON.

use cadastre_core::export::export;
use cadastre_core::{Compilation, SourceFile, compile};

const REGISTRY: &str = r#"team t
site dc { hosting: on-prem, network lan { subnet apps { cidr: 10.0.0.0/24, server web01 { ip: 10.0.0.10 }, firewall fw01 } } }
data customer { classification: personal }
system shop {
  owner: t
  description: "Online shop & <checkout>"
  app front { on: web01, -> store : tcp/5432 [flow] { carries: [customer] }, -> payments : https/443 [flow] }
  database store { on: web01 }
}
saas payments { owner: t }
"#;

fn compilation() -> Compilation {
    compile(&[SourceFile { path: "a.ca".into(), text: REGISTRY.into() }])
}

#[test]
fn archimate_elements_and_relationships() {
    let model = export(&compilation(), "archimate").unwrap();
    assert!(model.starts_with("<?xml") && model.contains("xmlns=\"http://www.opengroup.org/xsd/archimate/3.0/\""));
    for (id, kind) in [
        ("id-dc", "Location"),
        ("id-dc.lan.apps", "CommunicationNetwork"),
        ("id-dc.lan.apps.web01", "Device"),
        ("id-shop", "Grouping"),
        ("id-shop.front", "ApplicationComponent"),
        ("id-shop.store", "SystemSoftware"),
        ("id-customer", "DataObject"),
        ("id-t", "BusinessActor"),
    ] {
        assert!(model.contains(&format!("identifier=\"{id}\" xsi:type=\"{kind}\"")), "{id}: {kind}\n{model}");
    }
    // Escaped documentation; containment, hosting and flows typed as ArchiMate allows them.
    assert!(model.contains("<documentation xml:lang=\"en\">Online shop &amp; &lt;checkout&gt;</documentation>"));
    let relationship = |source: &str, target: &str, kind: &str| {
        model.contains(&format!("source=\"{source}\" target=\"{target}\" xsi:type=\"{kind}\""))
    };
    assert!(relationship("id-dc", "id-dc.lan", "Aggregation"), "{model}");
    assert!(relationship("id-shop", "id-shop.front", "Aggregation"));
    assert!(relationship("id-dc.lan.apps.web01", "id-shop.front", "Serving"));
    assert!(relationship("id-dc.lan.apps.web01", "id-shop.store", "Assignment"));
    assert!(relationship("id-shop.front", "id-payments", "Flow"));
    // Between an application and system software, a flow is not allowed: an association, named by its port.
    assert!(model.contains("source=\"id-shop.front\" target=\"id-shop.store\" xsi:type=\"Association\"><name xml:lang=\"en\">tcp/5432</name>"));
}

#[test]
fn graphml_nodes_and_edges() {
    let graph = export(&compilation(), "graphml").unwrap();
    assert!(graph.contains("<node id=\"id-shop.front\"><data key=\"kind\">app</data>"), "{graph}");
    assert!(graph.contains("<data key=\"relation\">parent</data>"));
    assert!(graph.contains("<data key=\"relation\">on</data>"));
    assert!(graph.contains("<data key=\"relation\">link</data><data key=\"port\">tcp/5432</data><data key=\"layers\">flow</data><data key=\"carries\">customer</data>"));
}

#[test]
fn json_model() {
    let text = export(&compilation(), "json").unwrap();
    let model: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(model["format"], "cadastre-model");
    let front = model["resources"].as_array().unwrap().iter().find(|r| r["id"] == "shop.front").unwrap();
    assert_eq!(front["kind"], "app");
    assert_eq!(front["parent"], "shop");
    assert_eq!(front["references"]["on"], serde_json::json!(["dc.lan.apps.web01"]));
    let link = &model["links"][0];
    assert_eq!((link["source"].as_str(), link["target"].as_str()), (Some("shop.front"), Some("shop.store")));
    assert_eq!(link["carries"], serde_json::json!(["customer"]));
    assert!(export(&compilation(), "nowhere").is_none());
}
