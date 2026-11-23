//! The API of the site navigated by its links: from `api/index.json`, every file is reached by
//! following the `href` of the `_links` blocks, and each `href` leads to a file of the site.

use std::collections::{BTreeSet, HashMap};

use cadastre_core::{SourceFile, compile, site};
use serde_json::Value;

const SCHEMA: &str = r#"
kind network { security_zone: string }
kind address_group { like: device, parents: site | network, ranges: cidr*, includes: ref(server | subnet)* }
layer flow { routed_host: host* }
"#;

const REGISTRY: &str = r#"
team ops
site dc1 {
  hosting: on-prem
  owner: ops
  network lan {
    security_zone: "DMZ Back"
    subnet apps { cidr: 10.0.1.0/24, server web01 { ip: 10.0.1.10 } }
    address_group backs { ranges: [10.0.0.0/16], includes: [web01, apps] }
  }
}
data order
system sales {
  owner: ops
  app shop { on: web01, -> orders : https/443 [flow] { carries: [order], routed_host: "orders.example.com" } }
  app orders { on: web01 }
}
"#;

const VIEWS: &str = "view apps {\n  select app group-by system\n  edges [flow]\n  render svg\n}\n";

fn site() -> HashMap<String, String> {
    let files = vec![
        SourceFile { path: "schema.ca".into(), text: SCHEMA.into() },
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: VIEWS.into() },
    ];
    let c = compile(&files);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == cadastre_core::Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let options = site::Options {
        name: "Acme",
        svgs: &HashMap::new(),
        elements: &Default::default(),
        viz: None,
        playground: false,
        language: cadastre_core::Language::En,
    };
    site::build(&c, &files, &options).into_iter().map(|f| (f.path, f.content)).collect()
}

fn json(site: &HashMap<String, String>, path: &str) -> Value {
    serde_json::from_str(site.get(path).unwrap_or_else(|| panic!("no {path}"))).unwrap()
}

/// A path relative to a file, resolved from the root of the site (`../x` from `api/a/b.json`).
fn resolve(from: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in href.split('/') {
        match part {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// The `href` of every `_links` block of a document, templates left out. (The `href` of the
/// entries of `search.json` are those of the pages, for the search of the site.)
fn hrefs(value: &Value, out: &mut Vec<String>) {
    fn within(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let (Some(Value::String(href)), None) = (map.get("href"), map.get("templated")) {
                    out.push(href.clone());
                }
                if let Some(Value::String(with)) = map.get("with") {
                    out.push(with.clone());
                }
                map.values().for_each(|v| within(v, out));
            }
            Value::Array(items) => items.iter().for_each(|v| within(v, out)),
            _ => {}
        }
    }
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "_links" { within(value, out) } else { hrefs(value, out) }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| hrefs(v, out)),
        _ => {}
    }
}

#[test]
fn every_file_of_the_api_is_reached_from_its_index() {
    let site = site();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue = vec!["api/index.json".to_owned()];
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let document = json(&site, &path);
        let mut found = Vec::new();
        hrefs(&document, &mut found);
        for href in found {
            let target = resolve(&path, &href);
            assert!(site.contains_key(&target), "{path}: {href} leads to {target}, which is not a file of the site");
            if target.starts_with("api/") && target.ends_with(".json") {
                queue.push(target);
            }
        }
    }
    let api: BTreeSet<String> = site.keys().filter(|p| p.starts_with("api/")).cloned().collect();
    let unreached: Vec<&String> = api.difference(&seen).collect();
    assert!(unreached.is_empty(), "not reached from api/index.json: {unreached:?}");
}

#[test]
fn a_resource_leads_to_what_it_names() {
    let site = site();
    let shop = json(&site, "api/resources/sales.shop.json");
    // Its fields as they were, its links beside them.
    assert_eq!(shop["system"], "sales");
    let links = &shop["_links"];
    assert_eq!(links["self"]["href"], "../resources/sales.shop.json");
    assert_eq!(links["page"]["href"], "../../resources/sales.shop.html");
    assert_eq!(links["kind"]["href"], "../kinds/app.json");
    assert_eq!(links["parent"]["href"], "../resources/sales.json");
    assert_eq!(links["owner"]["href"], "../resources/ops.json");
    assert_eq!(links["references"]["on"][0]["href"], "../resources/dc1.lan.apps.web01.json");
    let outgoing = &links["outgoing"][0];
    assert!(outgoing["href"].as_str().unwrap().starts_with("../links/sales.shop--sales.orders"), "{outgoing}");
    assert_eq!(outgoing["with"], "../resources/sales.orders.json");
    // A key of the project designating resources is followed too.
    let group = json(&site, "api/resources/dc1.lan.backs.json");
    let includes: Vec<&str> = group["_links"]["references"]["includes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["href"].as_str().unwrap())
        .collect();
    assert_eq!(includes, ["../resources/dc1.lan.apps.web01.json", "../resources/dc1.lan.apps.json"]);
}

#[test]
fn a_link_has_its_own_file() {
    let site = site();
    let links = json(&site, "api/links.json");
    let id = links[0]["id"].as_str().unwrap();
    assert_eq!(links[0]["_links"]["source"]["href"], "resources/sales.shop.json");
    let link = json(&site, &format!("api/links/{id}.json"));
    assert_eq!(link["attributes"]["routed_host"], "orders.example.com");
    assert_eq!(link["_links"]["target"]["href"], "../resources/sales.orders.json");
    assert_eq!(link["_links"]["carries"][0]["href"], "../resources/order.json");
}

#[test]
fn the_resources_by_kind() {
    let site = site();
    let kinds = json(&site, "api/kinds.json");
    let entry = |kind: &str| kinds["kinds"].as_array().unwrap().iter().find(|k| k["kind"] == kind).cloned().unwrap();
    assert_eq!(entry("app")["resources"], 2);
    assert_eq!(entry("address_group")["like"], "device");
    assert_eq!(entry("address_group")["resources"], 1);
    let apps = json(&site, "api/kinds/app.json");
    let ids: Vec<&str> = apps["resources"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["sales.shop", "sales.orders"]);
    assert_eq!(apps["resources"][0]["_links"]["self"]["href"], "../resources/sales.shop.json");
}

#[test]
fn the_schema_tells_the_keys_of_the_project_from_those_of_the_language() {
    let site = site();
    let schema = json(&site, "api/schema.json");
    let network = &schema["kinds"]["network"];
    assert_eq!(network["keys"]["security_zone"]["declared_by"], "project");
    assert_eq!(network["keys"]["security_zone"]["type"], "string");
    let group = &schema["kinds"]["address_group"];
    assert_eq!(group["declared_by"], "project");
    assert_eq!(group["like"], "device");
    assert_eq!(group["parents"], serde_json::json!(["site", "network"]));
    assert_eq!(group["keys"]["includes"]["type"], "ref(server | subnet)*");
    assert_eq!(group["keys"]["includes"]["designates"], serde_json::json!(["server", "subnet"]));
    assert_eq!(group["keys"]["includes"]["list"], true);
    assert_eq!(schema["kinds"]["subnet"]["keys"]["cidr"]["declared_by"], "language");
    assert_eq!(schema["kinds"]["subnet"]["keys"]["cidr"]["required"], true);
    assert_eq!(schema["layers"]["flow"]["keys"]["routed_host"]["type"], "host*");
    assert_eq!(schema["links"]["keys"]["carries"]["declared_by"], "language");
    assert!(schema["every_resource"]["keys"]["label"].is_object());
}

#[test]
fn a_view_leads_to_its_nodes_and_links() {
    let site = site();
    let views = json(&site, "api/views.json");
    assert_eq!(views["views"][0]["_links"]["self"]["href"], "views/apps.json");
    let apps = json(&site, "api/views/apps.json");
    assert_eq!(apps["nodes"][0]["_links"]["resource"]["href"], "../resources/sales.shop.json");
    assert_eq!(apps["frames"][0]["_links"]["resource"]["href"], "../resources/sales.json");
    assert!(apps["edges"][0]["_links"]["links"][0]["href"].as_str().unwrap().starts_with("../links/"));
}

/// A document without its `_links` blocks.
fn without_links(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter().filter(|(k, _)| *k != "_links").map(|(k, v)| (k.clone(), without_links(v))).collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_links).collect()),
        other => other.clone(),
    }
}

#[test]
fn the_records_keep_their_fields() {
    // Without its links, each file is the record of the export and of the MCP server.
    let files = vec![
        SourceFile { path: "schema.ca".into(), text: SCHEMA.into() },
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: VIEWS.into() },
    ];
    let c = compile(&files);
    let site = site();
    use cadastre_core::api;
    assert_eq!(without_links(&json(&site, "api/project.json")), api::project(&c, &files, "Acme"));
    assert_eq!(without_links(&json(&site, "api/resources.json")), api::index(&c, &files));
    assert_eq!(without_links(&json(&site, "api/links.json")), api::links(&c, &files));
    for id in c.registry.ids() {
        let record = api::resource(&c, &files, id);
        let path = format!("api/resources/{}.json", record["id"].as_str().unwrap());
        assert_eq!(without_links(&json(&site, &path)), record, "{path}");
    }
    let view = api::view(&c, &files, &c.plans.views[0]).unwrap();
    assert_eq!(without_links(&json(&site, "api/views/apps.json")), view);
}
