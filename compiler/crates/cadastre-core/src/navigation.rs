//! The links of the API of the site: an entry point (`api/index.json`), a `_links` block in
//! every record leading to the records it names, the resources by kind (`api/kinds/`), and the
//! schema of the project (`api/schema.json`). A client starting from `index.json` reaches every
//! file by following them. Documented in `docs/api.md`.
//!
//! The records keep their fields: `_links` is added beside them, its `href` relative to the file
//! it is written in, as a URL in a web page.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::project::Compilation;
use crate::schema::{AttrDef, Card, Ty};
use crate::site::file_name;

/// The keys of a full record of a resource naming one resource.
const ONE: &[&str] = &["parent", "owner", "operator", "site", "system"];
/// The keys of a full record of a resource naming several.
const MANY: &[&str] = &["path", "children", "hosts", "members", "runs_here", "failures"];

/// Paths from a file of the API: `depth` is 0 for `api/*.json`, 1 for `api/<folder>/*.json`.
#[derive(Clone, Copy)]
pub(crate) struct Hrefs {
    depth: usize,
}

impl Hrefs {
    pub(crate) const ROOT: Hrefs = Hrefs { depth: 0 };
    pub(crate) const FOLDER: Hrefs = Hrefs { depth: 1 };

    /// A file of the API, given from `api/`.
    fn api(self, path: &str) -> Value {
        json!({ "href": format!("{}{path}", "../".repeat(self.depth)) })
    }

    /// A page of the site, given from its root.
    fn page(self, path: &str) -> Value {
        json!({ "href": format!("{}{path}", "../".repeat(self.depth + 1)), "type": "text/html" })
    }

    fn resource(self, id: &str) -> Value {
        self.api(&format!("resources/{}.json", file_name(id)))
    }

    fn link(self, id: &str) -> Value {
        self.api(&format!("links/{}.json", file_name(id)))
    }

    fn kind(self, kind: &str) -> Value {
        self.api(&format!("kinds/{}.json", file_name(kind)))
    }

    fn view(self, name: &str) -> Value {
        self.api(&format!("views/{}.json", file_name(name)))
    }
}

/// Adds the block `_links` to a record, last.
fn put(record: &mut Value, links: Map<String, Value>) {
    if let Value::Object(map) = record {
        map.insert("_links".to_owned(), Value::Object(links));
    }
}

fn strings(value: &Value) -> Vec<&str> {
    value.as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}

/// The entry point of the API: what it holds, and the files to start from.
pub(crate) fn index(compilation: &Compilation, name: &str) -> Value {
    let at = Hrefs::ROOT;
    let templated = |path: &str| json!({ "href": path, "templated": true });
    json!({
        "format": "cadastre-model",
        "version": crate::api::VERSION,
        "cadastre": env!("CARGO_PKG_VERSION"),
        "name": name,
        "resources": compilation.registry.resources.len(),
        "links": compilation.model.link_ends.iter().flatten().count(),
        "views": compilation.plans.views.len(),
        "checks": compilation.plans.checks.len(),
        "_links": {
            "self": at.api("index.json"),
            "project": at.api("project.json"),
            "schema": at.api("schema.json"),
            "kinds": at.api("kinds.json"),
            "kind": templated("kinds/{kind}.json"),
            "resources": at.api("resources.json"),
            "resource": templated("resources/{id}.json"),
            "links": at.api("links.json"),
            "link": templated("links/{id}.json"),
            "views": at.api("views.json"),
            "view": templated("views/{name}.json"),
            "checks": at.api("checks.json"),
            "search": at.api("search.json"),
            "site": at.page("index.html"),
            "help": at.page("api.html"),
            // How to read all this, for an agent: the files, the rules, the tasks, the paths.
            "agents": { "href": "AGENTS.md", "type": "text/markdown" },
        },
    })
}

/// The short record of a resource, in a list: its own file.
pub(crate) fn summary(record: &mut Value, at: Hrefs) {
    let id = record["id"].as_str().unwrap_or_default().to_owned();
    let mut links = Map::new();
    links.insert("self".into(), at.resource(&id));
    put(record, links);
}

/// The full record of a resource (`api/resources/<id>.json`): the resources it names, its kind,
/// its links, its page.
pub(crate) fn resource(record: &mut Value) {
    let at = Hrefs::FOLDER;
    let id = record["id"].as_str().unwrap_or_default().to_owned();
    let mut links = Map::new();
    links.insert("self".into(), at.resource(&id));
    links.insert("page".into(), at.page(&format!("resources/{}.html", file_name(&id))));
    if let Some(kind) = record["kind"].as_str() {
        links.insert("kind".into(), at.kind(kind));
    }
    for key in ONE {
        if let Some(named) = record[*key].as_str() {
            links.insert((*key).into(), at.resource(named));
        }
    }
    for key in MANY {
        let named = strings(&record[*key]);
        if !named.is_empty() {
            links.insert((*key).into(), Value::Array(named.iter().map(|n| at.resource(n)).collect()));
        }
    }
    // The references by key (`pool`, `backup`, the keys of the project).
    let mut references = Map::new();
    for (key, named) in record["references"].as_object().into_iter().flatten() {
        let named = strings(named);
        if !named.is_empty() {
            references.insert(key.clone(), Value::Array(named.iter().map(|n| at.resource(n)).collect()));
        }
    }
    if !references.is_empty() {
        links.insert("references".into(), Value::Object(references));
    }
    // Its links, each to its record, with the resource at its other end.
    for way in ["outgoing", "incoming"] {
        let list: Vec<Value> = record[way]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|link| {
                let id = link["id"].as_str()?;
                let mut entry = at.link(id);
                if let (Value::Object(entry), Some(with)) = (&mut entry, link["with"].as_str()) {
                    entry.insert("with".into(), at.resource(with)["href"].clone());
                }
                Some(entry)
            })
            .collect();
        if !list.is_empty() {
            links.insert(way.into(), Value::Array(list));
        }
    }
    put(record, links);
}

/// The record of a link, in `links.json` or its own file (`api/links/<id>.json`): its ends, what
/// it carries and goes through, its page.
pub(crate) fn link(record: &mut Value, at: Hrefs) {
    let mut links = Map::new();
    if let Some(id) = record["id"].as_str() {
        links.insert("self".into(), at.link(id));
        links.insert("page".into(), at.page(&format!("links/{}.html", file_name(id))));
    }
    for end in ["source", "target"] {
        if let Some(named) = record[end].as_str() {
            links.insert(end.into(), at.resource(named));
        }
    }
    for key in ["carries", "through"] {
        let named = strings(&record[key]);
        if !named.is_empty() {
            links.insert(key.into(), Value::Array(named.iter().map(|n| at.resource(n)).collect()));
        }
    }
    put(record, links);
}

/// The declared record of a view, in `views.json`: its own file and its page.
pub(crate) fn view_summary(record: &mut Value, at: Hrefs) {
    let name = record["name"].as_str().unwrap_or_default().to_owned();
    let mut links = Map::new();
    links.insert("self".into(), at.view(&name));
    links.insert("page".into(), at.page(&format!("views/{}.html", file_name(&name))));
    put(record, links);
}

/// A view evaluated (`api/views/<name>.json`): its nodes to their resources, its edges to their
/// ends and links.
pub(crate) fn view(record: &mut Value) {
    let at = Hrefs::FOLDER;
    view_summary(record, at);
    for node in record["nodes"].as_array_mut().into_iter().flatten() {
        let mut links = Map::new();
        if let Some(id) = node["id"].as_str() {
            links.insert("resource".into(), at.resource(id));
        }
        put(node, links);
    }
    for frame in record["frames"].as_array_mut().into_iter().flatten() {
        let mut links = Map::new();
        if let Some(id) = frame["resource"].as_str() {
            links.insert("resource".into(), at.resource(id));
        }
        put(frame, links);
    }
    for edge in record["edges"].as_array_mut().into_iter().flatten() {
        let mut links = Map::new();
        for end in ["source", "target"] {
            if let Some(named) = edge[end].as_str() {
                links.insert(end.into(), at.resource(named));
            }
        }
        let ids = strings(&edge["links"]);
        if !ids.is_empty() {
            links.insert("links".into(), Value::Array(ids.iter().map(|id| at.link(id)).collect()));
        }
        put(edge, links);
    }
}

/// The kinds of the project, each with its number of resources and its file
/// (`api/kinds.json`), and the file of each (`api/kinds/<kind>.json`), its resources as short
/// records. A kind without resources is listed, its file empty.
pub(crate) fn kinds(compilation: &Compilation, summaries: &[Value]) -> (Value, Vec<(String, Value)>) {
    let schema = &compilation.registry.schema;
    let mut by_kind: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for summary in summaries {
        if let Some(kind) = summary["kind"].as_str() {
            by_kind.entry(kind).or_default().push(summary.clone());
        }
    }
    let mut list = Vec::new();
    let mut files = Vec::new();
    for kind in schema.kind_names() {
        let resources = by_kind.remove(kind).unwrap_or_default();
        let mut entry = json!({
            "kind": kind,
            "like": schema.like(kind),
            "resources": resources.len(),
        });
        let mut links = Map::new();
        links.insert("self".into(), Hrefs::ROOT.kind(kind));
        links.insert("schema".into(), json!({ "href": format!("schema.json#/kinds/{kind}") }));
        put(&mut entry, links);
        list.push(entry);
        let mut resources = resources;
        for resource in &mut resources {
            summary(resource, Hrefs::FOLDER);
        }
        let mut file = json!({ "kind": kind, "like": schema.like(kind), "resources": resources });
        let mut links = Map::new();
        links.insert("self".into(), Hrefs::FOLDER.kind(kind));
        links.insert("kinds".into(), Hrefs::FOLDER.api("kinds.json"));
        links.insert("schema".into(), json!({ "href": format!("../schema.json#/kinds/{kind}") }));
        put(&mut file, links);
        files.push((kind.to_owned(), file));
    }
    let mut index = json!({ "kinds": list });
    let mut links = Map::new();
    links.insert("self".into(), Hrefs::ROOT.api("kinds.json"));
    links.insert("index".into(), Hrefs::ROOT.api("index.json"));
    put(&mut index, links);
    (index, files)
}

/// A key as the schema gives it: its type as written (`ref(server | cluster)*`), required, a
/// list, the kinds it designates, and whether the language or the project declares it.
fn key(def: &AttrDef, builtin: bool) -> Value {
    let mut key = json!({
        "type": crate::schema::type_text(def),
        "required": def.required,
        "list": !matches!(def.card, Card::One),
        "declared_by": if builtin { "language" } else { "project" },
    });
    if let (Ty::Ref(union), Value::Object(map)) = (def.ty, &mut key) {
        map.insert("designates".into(), json!(union.split('|').map(str::trim).collect::<Vec<_>>()));
    }
    if let (Ty::Symbol(values), Value::Object(map)) = (def.ty, &mut key) {
        map.insert("values".into(), json!(values));
    }
    if let (Some(computed), Value::Object(map)) = (def.computed, &mut key) {
        map.insert("computed".into(), json!({ "function": computed.function.name(), "keys": computed.keys }));
    }
    key
}

fn keys(defs: &[AttrDef], builtin: impl Fn(&str) -> bool) -> Value {
    Value::Object(defs.iter().map(|def| (def.key.to_owned(), key(def, builtin(def.key)))).collect())
}

/// The schema of the project (`api/schema.json`): each kind with its parents and its keys, the
/// keys of every resource, of the links and of each layer, the groups of kinds.
pub(crate) fn schema(compilation: &Compilation) -> Value {
    let schema = &compilation.registry.schema;
    let mut kinds = Map::new();
    for name in schema.kind_names() {
        let Some(def) = schema.kind_def(name) else { continue };
        // A key of the language: of the built-in kind, or of the one a kind of the project is like.
        let base = crate::schema::kind_def(schema.like(name).unwrap_or(name));
        let builtin = |key: &str| base.is_some_and(|b| b.attrs.iter().any(|a| a.key == key));
        kinds.insert(
            name.to_owned(),
            json!({
                "like": schema.like(name),
                "declared_by": if schema.like(name).is_some() { "project" } else { "language" },
                "parents": def.parents,
                "within": def.within,
                "connector": def.connector,
                "keys": keys(def.attrs, builtin),
                "_links": { "resources": { "href": format!("kinds/{}.json", file_name(name)) } },
            }),
        );
    }
    let link_builtin = |key: &str| crate::schema::LINK_KEYS.iter().any(|a| a.key == key);
    let layers: Map<String, Value> = schema
        .layer_keys()
        .iter()
        .map(|(layer, defs)| ((*layer).to_owned(), json!({ "keys": keys(defs, link_builtin) })))
        .collect();
    let mut links_keys = keys(crate::schema::LINK_KEYS, link_builtin);
    if let (Value::Object(map), Value::Object(own)) = (&mut links_keys, keys(schema.every_link_keys(), link_builtin)) {
        map.extend(own);
    }
    let groups: Map<String, Value> = schema.groups().map(|(name, members)| (name.to_owned(), json!(members))).collect();
    json!({
        "format": "cadastre-schema",
        "version": crate::api::VERSION,
        "kinds": kinds,
        "every_resource": { "keys": keys(crate::schema::RESOURCE_KEYS, |_| true) },
        "links": { "keys": links_keys },
        "layers": layers,
        "groups": groups,
        "_links": { "self": { "href": "schema.json" }, "index": { "href": "index.json" }, "kinds": { "href": "kinds.json" } },
    })
}

/// The list of the views (`api/views.json`), each declared, with its file and its page.
pub(crate) fn views(compilation: &Compilation, files: &[crate::source::SourceFile]) -> Value {
    let list: Vec<Value> = compilation
        .plans
        .views
        .iter()
        .map(|view| {
            let mut record = crate::api::view_summary(files, view);
            view_summary(&mut record, Hrefs::ROOT);
            record
        })
        .collect();
    json!({
        "views": list,
        "_links": { "self": { "href": "views.json" }, "index": { "href": "index.json" } },
    })
}
