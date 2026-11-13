//! The composer of views: what the page that writes a view needs from the compiler.
//!
//! - [`parts`]: the views of a text, each clause with where it is written, so that the page
//!   rewrites one clause and leaves the rest of the text, comments included, as it is;
//! - [`facts`]: what a project offers to choose from: its kinds, layers, keys and their values,
//!   resources, and the recipes to start a view from;
//! - [`insight`]: what a view draws, and the links it loses because one of their ends is not
//!   selected, the surprise of most views.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::{Value, json};
use tree_sitter::Node;

use crate::Compilation;
use crate::plan::Selector;
use crate::registry::ResourceId;
use crate::syntax;

/// Offsets in UTF-16 code units, as JavaScript strings index them, from byte offsets.
struct Units(Vec<usize>);

impl Units {
    fn of(text: &str) -> Self {
        let mut units = vec![0; text.len() + 1];
        let mut unit = 0;
        for (byte, c) in text.char_indices() {
            units[byte..byte + c.len_utf8()].fill(unit);
            unit += c.len_utf16();
        }
        units[text.len()] = unit;
        Units(units)
    }

    fn range(&self, node: Node) -> [usize; 2] {
        [self.0[node.start_byte()], self.0[node.end_byte()]]
    }
}

/// The views of a `.cav` text: for each, its name and range, its attributes (`title`,
/// `description`, `group`, `failing`…), and its clauses with their ranges, in UTF-16 code units.
///
/// `select`: `kinds`, `where` (the text of the filter), `with_ends`, `without_isolated`,
/// `group_by`; `edges`: `selectors` (the text of each) and `options`; `render`: `formats` and
/// `options` (key and the text of the value); `style` and `zones`: their range only. `end` is
/// the offset of the closing brace, where a missing clause goes; `indent`, the indentation of
/// the clauses.
pub fn parts(text: &str) -> Value {
    let tree = syntax::parse(text);
    let units = Units::of(text);
    let source = |node: Node| text[node.byte_range()].to_owned();
    let root = tree.root_node();
    let mut cursor = root.walk();
    let mut views = Vec::new();
    for view in root.named_children(&mut cursor).filter(|n| n.kind() == "view") {
        let Some(name) = view.child_by_field_name("name") else { continue };
        let mut attributes = serde_json::Map::new();
        let (mut select, mut edges, mut render, mut style) = (Value::Null, Value::Null, Value::Null, Value::Null);
        let mut zones = Vec::new();
        let mut indent: Option<String> = None;
        let mut inner = view.walk();
        for clause in view.named_children(&mut inner) {
            if clause.id() == name.id() {
                continue;
            }
            if indent.is_none() {
                let line = text[..clause.start_byte()].rsplit('\n').next().unwrap_or_default();
                if line.trim().is_empty() && !line.is_empty() {
                    indent = Some(line.to_owned());
                }
            }
            let range = units.range(clause);
            match clause.kind() {
                "attribute" => {
                    let (Some(key), Some(value)) =
                        (clause.child_by_field_name("key"), clause.child_by_field_name("value"))
                    else {
                        continue;
                    };
                    let raw = source(value);
                    let shown = raw
                        .strip_prefix('"')
                        .and_then(|r| r.strip_suffix('"'))
                        .map_or_else(|| raw.clone(), |r| r.replace("\\\"", "\"").replace("\\\\", "\\"));
                    attributes.insert(source(key), json!({ "value": shown, "raw": raw, "range": range }));
                }
                "select_clause" => {
                    let mut c = clause.walk();
                    let kinds: Vec<String> = clause.children_by_field_name("kind", &mut c).map(source).collect();
                    let mut c = clause.walk();
                    let named: Vec<Node> = clause.named_children(&mut c).collect();
                    let filter = named.iter().find(|n| n.kind() == "where_clause").map(|w| {
                        let mut c = w.walk();
                        w.named_children(&mut c).next().map(source).unwrap_or_default()
                    });
                    let group_by: Vec<String> = named
                        .iter()
                        .find(|n| n.kind() == "group_by")
                        .map(|g| {
                            let mut c = g.walk();
                            g.children_by_field_name("level", &mut c).map(source).collect()
                        })
                        .unwrap_or_default();
                    select = json!({
                        "range": range,
                        "kinds": kinds,
                        "where": filter,
                        "with_ends": named.iter().any(|n| n.kind() == "with_ends"),
                        "without_isolated": named.iter().any(|n| n.kind() == "without_isolated"),
                        "group_by": group_by,
                    });
                }
                "edges_clause" => {
                    let mut c = clause.walk();
                    let mut selectors = Vec::new();
                    let mut options = Vec::new();
                    for item in clause.named_children(&mut c) {
                        match item.kind() {
                            "edge_selector" => selectors.push(source(item)),
                            "edges_option" => options.push(json!({
                                "key": item.child_by_field_name("key").map(source),
                                "value": item.child_by_field_name("value").map(source),
                            })),
                            _ => {}
                        }
                    }
                    edges = json!({ "range": range, "selectors": selectors, "options": options });
                }
                "render_clause" => {
                    let mut c = clause.walk();
                    let formats: Vec<String> = clause.children_by_field_name("format", &mut c).map(source).collect();
                    let mut c = clause.walk();
                    let options: Vec<Value> = clause
                        .named_children(&mut c)
                        .filter(|n| n.kind() == "attribute")
                        .filter_map(|a| {
                            Some(json!({
                                "key": source(a.child_by_field_name("key")?),
                                "value": source(a.child_by_field_name("value")?),
                            }))
                        })
                        .collect();
                    render = json!({ "range": range, "formats": formats, "options": options });
                }
                "style_block" => style = json!({ "range": range }),
                "zone_clause" => zones.push(json!({ "range": range })),
                _ => {}
            }
        }
        views.push(json!({
            "name": source(name),
            "name_range": units.range(name),
            "range": units.range(view),
            // The closing brace: where a missing clause is written.
            "end": units.0[view.end_byte().saturating_sub(1)],
            "indent": indent.unwrap_or_else(|| "  ".to_owned()),
            "error": view.has_error(),
            "attributes": attributes,
            "select": select,
            "edges": edges,
            "render": render,
            "style": style,
            "zones": zones,
        }));
    }
    Value::Array(views)
}

/// The byte range of each view of a `.cav` text, by name: where a view is replaced in its file.
pub fn view_ranges(text: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let tree = syntax::parse(text);
    let root = tree.root_node();
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter(|n| n.kind() == "view")
        .filter_map(|view| Some((text[view.child_by_field_name("name")?.byte_range()].to_owned(), view.byte_range())))
        .collect()
}

/// A recipe: a view to start from, a template of its body, `{x}` the qualified name of the
/// resource asked, of the kind `asks` (`any`: any resource), `{kind}` its kind.
pub struct Recipe {
    pub id: &'static str,
    pub asks: Option<&'static str>,
    pub body: &'static str,
}

/// The recipes of the composer; their titles are the page's, in its language.
pub static RECIPES: &[Recipe] = &[
    Recipe {
        id: "around",
        asks: Some("any"),
        body: "select {kind} where name = {x} with ends\nedges  [flow]\nrender svg { orientation: data }",
    },
    Recipe {
        id: "system-flows",
        asks: Some("system"),
        body: "select app, job, database, saas where ancestor(system) = {x} with ends group-by system\nedges  [flow]\nrender svg { orientation: data }",
    },
    Recipe {
        id: "landscape",
        asks: None,
        body: "select system, saas\nedges  lift([flow], via: ancestor(system))\nrender svg { orientation: data }",
    },
    Recipe {
        id: "site-network",
        asks: Some("site"),
        body: "select server, saas without isolated group-by site > subnet\nedges  lift([flow], via: on) where source.ancestor(site) = {x} or target.ancestor(site) = {x}\nrender svg",
    },
    Recipe {
        id: "personal-data",
        asks: None,
        body: "select app, job, database, saas without isolated\nedges  [flow] where carries.classification = personal\nrender svg { orientation: data, edge_labels: [port, carries] }",
    },
    Recipe {
        id: "data-trail",
        asks: Some("data"),
        body: "select app, job, database, saas without isolated\nedges  follow({x})\nrender svg { orientation: data, edge_labels: [step, port] }",
    },
    Recipe {
        id: "failure",
        asks: Some("any"),
        body: "failing: [{x}]\nselect app, job, database, saas\nedges  [flow]\nrender svg { legend: true }",
    },
    Recipe {
        id: "firewall",
        asks: Some("firewall"),
        body: "select server, saas\nedges  lift([flow] where through has {x}, via: on, merge: port)\nrender svg",
    },
    Recipe {
        id: "ownership",
        asks: None,
        body: "select app, database, saas group-by owner\nrender svg { node_labels: [label, operator] }",
    },
    Recipe {
        id: "inventory",
        asks: None,
        body: "select app, database\nrender html { columns: [\"name as Name\", \"label as Label\", \"ancestor(system) as System\", \"on as Server\", \"on.ancestor(site) as Site\"] }",
    },
];

/// The body of a recipe, filled with a resource (its qualified name and kind).
pub fn recipe_body(recipe: &Recipe, resource: Option<(&str, &str)>) -> String {
    let (x, kind) = resource.unwrap_or_default();
    recipe.body.replace("{x}", x).replace("{kind}", kind)
}

/// What a project offers to choose from, for the composer: `kinds` and `layers` with their
/// counts, the most frequent first; `keys`, the keys written on resources with their values
/// (up to a hundred each) and the kinds carrying them; `references`, the keys designating
/// resources (levels of `group-by`); `containers`, the kinds holding others; `resources`, each
/// with its kind, label, site and system; `recipes`.
pub fn facts(compilation: &Compilation) -> Value {
    let registry = &compilation.registry;
    let model = &compilation.model;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut containers: BTreeSet<String> = BTreeSet::new();
    let mut keys: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    let mut resources = Vec::new();
    for id in registry.ids() {
        let resource = registry.resource(id);
        *kinds.entry(resource.kind.clone()).or_default() += 1;
        if let Some(parent) = resource.parent {
            containers.insert(registry.resource(parent).kind.clone());
        }
        for attribute in &resource.attributes {
            let (values, carried_by) = keys.entry(attribute.key.clone()).or_default();
            carried_by.insert(resource.kind.clone());
            if values.len() < 100 {
                match &attribute.value.kind {
                    crate::registry::ValueKind::List(items) => {
                        values.extend(items.iter().map(crate::query::value_text));
                    }
                    _ => {
                        values.insert(crate::query::value_text(&attribute.value));
                    }
                }
            }
        }
        resources.push(json!({
            "id": registry.qualified_name(id),
            "kind": resource.kind,
            "label": crate::api::label(registry, id),
            "site": crate::api::site_of(compilation, id).map(|s| registry.qualified_name(s)),
            "system": model.system_of(registry, id).map(|s| registry.qualified_name(s)),
        }));
    }
    let mut layers: BTreeMap<String, usize> = BTreeMap::new();
    for link in &registry.links {
        for (layer, _) in &link.layers {
            *layers.entry(layer.clone()).or_default() += 1;
        }
    }
    let references: BTreeSet<&str> = model.references.keys().map(|(_, key)| key.as_str()).collect();
    let by_count = |map: BTreeMap<String, usize>, field: &str| {
        let mut items: Vec<(String, usize)> = map.into_iter().collect();
        items.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        items.into_iter().map(|(name, count)| json!({ field: name, "count": count })).collect::<Vec<_>>()
    };
    let keys: Vec<Value> = keys
        .into_iter()
        .map(|(key, (values, kinds))| json!({ "key": key, "values": values, "kinds": kinds }))
        .collect();
    let recipes: Vec<Value> = RECIPES.iter().map(|r| json!({ "id": r.id, "asks": r.asks, "body": r.body })).collect();
    json!({
        "kinds": by_count(kinds, "kind"),
        "layers": by_count(layers, "layer"),
        "keys": keys,
        "references": references,
        "containers": containers,
        "resources": resources,
        "recipes": recipes,
    })
}

/// What the view `name` draws, for the composer: `counts` of its nodes, edges and frames;
/// `nodes`, by their name in the DOT (`n12`), the resource each stands for, with its kind,
/// label, site and system; `edges`, the layers of each edge, in the order of the DOT ids
/// (`graph-edge-3`); `lost`, the links of the layers the view selects, written without a filter,
/// that it does not draw because one of their ends is not selected, by the kind of that end;
/// `None` when the project has errors or no such view.
pub fn insight(compilation: &Compilation, name: &str) -> Option<Value> {
    if compilation.diagnostics.iter().any(|d| d.severity == crate::diagnostic::Severity::Error) {
        return None;
    }
    let registry = &compilation.registry;
    let view = compilation.plans.views.iter().find(|v| v.name == name)?;
    let mut diagnostics = Vec::new();
    let graph = crate::view::evaluate(view, registry, &compilation.model, &mut diagnostics);
    let drawn: HashSet<ResourceId> = graph.nodes.iter().copied().collect();
    let nodes: serde_json::Map<String, Value> = graph
        .nodes
        .iter()
        .map(|&id| {
            (
                crate::render::node_id(id),
                json!({
                    "id": registry.qualified_name(id),
                    "kind": registry.resource(id).kind,
                    "label": crate::api::label(registry, id),
                    "site": crate::api::site_of(compilation, id).map(|s| registry.qualified_name(s)),
                    "system": compilation.model.system_of(registry, id).map(|s| registry.qualified_name(s)),
                }),
            )
        })
        .collect();
    let edges: Vec<Value> = graph.edges.iter().map(|e| json!({ "layers": e.layers })).collect();
    // The layers selected as such, without a filter: a link of theirs with an end out of the
    // view is lost, unless `with ends` adds that end.
    let layers: BTreeSet<&str> = view
        .edges
        .iter()
        .filter(|e| e.filter.is_none())
        .filter_map(|e| match &e.selector {
            Selector::Layers(names) => Some(names.iter().map(|n| n.text.as_str())),
            _ => None,
        })
        .flatten()
        .collect();
    let with_ends = view.select.as_ref().is_some_and(|s| s.with_ends);
    let mut lost: BTreeMap<(String, String), (usize, BTreeSet<String>)> = BTreeMap::new();
    if !with_ends {
        for (index, link) in registry.links.iter().enumerate() {
            let Some(Some((source, target))) = compilation.model.link_ends.get(index) else { continue };
            let Some((layer, _)) = link.layers.iter().find(|(l, _)| layers.contains(l.as_str())) else { continue };
            let out = match (drawn.contains(source), drawn.contains(target)) {
                (true, false) => *target,
                (false, true) => *source,
                _ => continue,
            };
            let entry = lost.entry((registry.resource(out).kind.clone(), layer.clone())).or_default();
            entry.0 += 1;
            if entry.1.len() < 5 {
                entry.1.insert(registry.qualified_name(out));
            }
        }
    }
    let lost: Vec<Value> = lost
        .into_iter()
        .map(|((kind, layer), (count, examples))| {
            json!({ "kind": kind, "layer": layer, "count": count, "examples": examples })
        })
        .collect();
    Some(json!({
        "counts": { "nodes": graph.nodes.len(), "edges": graph.edges.len(), "frames": graph.groups.len() },
        "nodes": nodes,
        "edges": edges,
        "lost": lost,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SourceFile, compile};

    #[test]
    fn the_parts_of_a_view_with_their_ranges() {
        let text = "// the flows\nview flows {\n  title: \"Les flux — tous\"\n  select app, database where ancestor(system) = billing with ends group-by system\n  edges [flow], lift([auth], via: on)\n  render svg { orientation: data, legend: true }\n}\n";
        let views = parts(text);
        let view = &views[0];
        assert_eq!(view["name"], "flows");
        assert_eq!(view["attributes"]["title"]["value"], "Les flux — tous");
        assert_eq!(view["select"]["kinds"], json!(["app", "database"]));
        assert_eq!(view["select"]["where"], "ancestor(system) = billing");
        assert_eq!(view["select"]["with_ends"], true);
        assert_eq!(view["select"]["group_by"], json!(["system"]));
        assert_eq!(view["edges"]["selectors"], json!(["[flow]", "lift([auth], via: on)"]));
        assert_eq!(view["render"]["formats"], json!(["svg"]));
        assert_eq!(view["render"]["options"][1], json!({ "key": "legend", "value": "true" }));
        assert_eq!(view["indent"], "  ");
        // The ranges count UTF-16 code units, as JavaScript does: the dash after the title.
        let units: Vec<u16> = text.encode_utf16().collect();
        let range = |v: &Value| {
            let [s, e] = [v[0].as_u64().unwrap() as usize, v[1].as_u64().unwrap() as usize];
            String::from_utf16(&units[s..e]).unwrap()
        };
        assert!(
            range(&view["select"]["range"]).starts_with("select app")
                && range(&view["select"]["range"]).ends_with("group-by system")
        );
        assert_eq!(range(&view["render"]["range"]), "render svg { orientation: data, legend: true }");
        let end = view["end"].as_u64().unwrap() as usize;
        assert_eq!(units[end], u16::from(b'}'));
    }

    fn project(views: &str) -> Compilation {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../examples/reference");
        let mut files: Vec<SourceFile> = std::fs::read_dir(root)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "ca"))
            .map(|p| SourceFile {
                path: p.file_name().unwrap().to_string_lossy().into_owned(),
                text: std::fs::read_to_string(&p).unwrap(),
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files.push(SourceFile { path: "composed.cav".into(), text: views.into() });
        compile(&files)
    }

    #[test]
    fn every_recipe_compiles_on_the_reference_example() {
        let facts = facts(&project(""));
        let resources = facts["resources"].as_array().unwrap();
        for recipe in RECIPES {
            let resource = recipe.asks.map(|kind| {
                let r = resources
                    .iter()
                    .find(|r| kind == "any" && r["kind"] == "app" || r["kind"] == kind)
                    .unwrap_or_else(|| panic!("no {kind} in the reference example"));
                (r["id"].as_str().unwrap().to_owned(), r["kind"].as_str().unwrap().to_owned())
            });
            let body = recipe_body(recipe, resource.as_ref().map(|(x, k)| (x.as_str(), k.as_str())));
            let c = project(&format!("view composed {{\n{body}\n}}\n"));
            let errors: Vec<&str> = c
                .diagnostics
                .iter()
                .filter(|d| d.severity == crate::diagnostic::Severity::Error)
                .map(|d| d.message.as_str())
                .collect();
            assert!(errors.is_empty(), "{}: {errors:?}\n{body}", recipe.id);
            let insight = insight(&c, "composed").unwrap();
            assert!(insight["counts"]["nodes"].as_u64().unwrap() > 0, "{}: nothing drawn\n{body}", recipe.id);
        }
    }

    #[test]
    fn the_links_a_view_loses() {
        // The apps alone: their flows to the databases and the SaaS are lost, by kind.
        let c = project("view composed {\n  select app\n  edges [flow]\n}\n");
        let insight = insight(&c, "composed").unwrap();
        let lost = insight["lost"].as_array().unwrap();
        assert!(lost.iter().any(|l| l["kind"] == "database" && l["layer"] == "flow"), "{lost:?}");
        assert!(insight["nodes"].as_object().unwrap().values().any(|n| n["kind"] == "app"));
        // `with ends` adds them: nothing is lost.
        let c = project("view composed {\n  select app with ends\n  edges [flow]\n}\n");
        assert_eq!(insight_of(&c)["lost"], json!([]));
    }

    fn insight_of(c: &Compilation) -> Value {
        insight(c, "composed").unwrap()
    }

    #[test]
    fn the_facts_of_a_project() {
        let facts = facts(&project(""));
        assert!(facts["kinds"].as_array().unwrap().iter().any(|k| k["kind"] == "app"));
        assert!(facts["layers"].as_array().unwrap().iter().any(|l| l["layer"] == "flow"));
        assert!(facts["containers"].as_array().unwrap().iter().any(|k| k == "site"));
        assert!(facts["references"].as_array().unwrap().iter().any(|k| k == "owner"));
        assert!(facts["keys"].as_array().unwrap().iter().any(|k| k["key"] == "hosting"));
        assert_eq!(facts["recipes"].as_array().unwrap().len(), RECIPES.len());
    }

    #[test]
    fn the_range_of_a_view_in_its_file() {
        let text = "view a { select app }\n\nview b {\n  select server\n}\n";
        let ranges = view_ranges(text);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&text[ranges[1].1.clone()], "view b {\n  select server\n}");
    }
}
