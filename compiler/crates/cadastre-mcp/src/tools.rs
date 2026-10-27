//! The tools of the MCP server, all read only.

use std::collections::BTreeSet;
use std::path::Path;

use cadastre_core::registry::{Registry, ResourceId};
use cadastre_core::schema::{self, AttrDef, Ty};
use cadastre_core::source::{FileId, SourceFile};
use cadastre_core::{Compilation, Severity, compile_modules};
use serde_json::{Value, json};

/// The most items a result lists: beyond, it says how many there are.
const LIMIT: usize = 200;

/// The file holding the view or check an agent proposes, compiled with the project: a `.cac`
/// for a check, a `.cav` otherwise.
const PROPOSED: &str = "(proposed)";

fn proposed_path(text: &str) -> String {
    if text.trim_start().starts_with("check ") { format!("{PROPOSED}.cac") } else { format!("{PROPOSED}.cav") }
}

pub fn definitions() -> Value {
    let text = |description: &str| json!({ "type": "string", "description": description });
    let object = |properties: Value, required: &[&str]| json!({ "type": "object", "properties": properties, "required": required });
    json!([
        {
            "name": "list_kinds",
            "description": "The kinds of resources (built in and of the project, with what each is like), where each may be placed, and the groups of kinds usable wherever a kind is.",
            "inputSchema": object(json!({}), &[]),
        },
        {
            "name": "list_keys",
            "description": "The keys of a kind: those of the schema with their type, cardinality and closed values, the keys of every resource, and the free keys written on resources of that kind in the project.",
            "inputSchema": object(json!({ "kind": text("a kind, such as server or app") }), &["kind"]),
        },
        {
            "name": "search_resources",
            "description": "Resources whose name, qualified name or label matches a text, best first (exact, prefix, substring, close spelling).",
            "inputSchema": object(json!({
                "query": text("a name, part of a name, or words of a label"),
                "kind": text("only resources of this kind or group (optional)"),
                "limit": { "type": "integer", "description": "at most this many (20 by default)" },
            }), &["query"]),
        },
        {
            "name": "get_resource",
            "description": "A resource: kind, qualified name, where it is declared, its keys, the resources it references, its content, what runs on it, its incoming and outgoing links.",
            "inputSchema": object(json!({ "name": text("its name, or a suffix of its qualified name (dc1.lan.web01)") }), &["name"]),
        },
        {
            "name": "list_views",
            "description": "The views and checks of the project, with their files.",
            "inputSchema": object(json!({}), &[]),
        },
        {
            "name": "check_view",
            "description": "Compiles a view or a check against the project, without writing it, and returns its diagnostics (errors, warnings, hints). The text is a `view <name> { … }` or `check <name> { … }` block, or the clauses of a view alone (`select app where owner = ops`).",
            "inputSchema": object(json!({ "text": text("the block, or the clauses of a view") }), &["text"]),
        },
        {
            "name": "eval_view",
            "description": "Evaluates a view of the project (by name) or a proposed one (by text): its nodes, frames and edges; with a format, its rendering too (text, mermaid, markdown, csv, dot, sequence…).",
            "inputSchema": object(json!({
                "name": text("a view of the project"),
                "text": text("or a proposed view: a `view` block, or its clauses alone"),
                "format": text("a rendering to return as well (optional)"),
            }), &[]),
        },
        {
            "name": "run_checks",
            "description": "Runs the checks of the project, or one of them: passed or failed, what was selected against what was expected, and what fails it.",
            "inputSchema": object(json!({ "name": text("one check only (optional)") }), &[]),
        },
    ])
}

pub fn call(directory: &Path, name: &str, arguments: &Value) -> Result<Value, String> {
    let argument = |key: &str| arguments.get(key).and_then(Value::as_str).map(str::to_owned);
    let project = cadastre_project::load(directory)?;
    match name {
        "list_kinds" => Ok(list_kinds(&compile(&project, None).0)),
        "list_keys" => list_keys(&compile(&project, None).0, &argument("kind").ok_or("kind is required")?),
        "search_resources" => {
            let limit = arguments.get("limit").and_then(Value::as_u64).map_or(20, |l| l as usize).min(LIMIT);
            let query = argument("query").ok_or("query is required")?;
            Ok(search(&compile(&project, None).0, &query, argument("kind").as_deref(), limit))
        }
        "get_resource" => {
            let (compilation, files) = compile(&project, None);
            get_resource(&compilation, &files, &argument("name").ok_or("name is required")?)
        }
        "list_views" => {
            let (compilation, files) = compile(&project, None);
            Ok(list_views(&compilation, &files))
        }
        "check_view" => {
            let text = proposed(&argument("text").ok_or("text is required")?);
            let (compilation, files) = compile(&project, Some(&text));
            Ok(check_view(&compilation, &files))
        }
        "eval_view" => {
            let text = argument("text").map(|t| proposed(&t));
            let (compilation, files) = compile(&project, text.as_deref());
            eval_view(&compilation, &files, argument("name").as_deref(), text.is_some(), argument("format").as_deref())
        }
        "run_checks" => {
            let (compilation, files) = compile(&project, None);
            run_checks(&compilation, &files, argument("name").as_deref())
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

/// The project compiled, with a proposed `.cav` file after its own when given.
fn compile(project: &cadastre_project::Project, proposed: Option<&str>) -> (Compilation, Vec<SourceFile>) {
    let mut files = project.files.clone();
    if let Some(text) = proposed {
        files.push(SourceFile { path: proposed_path(text), text: text.to_owned() });
    }
    let options = cadastre_core::Options { strict: project.strict(), language: project.language() };
    (compile_modules(&files, &project.modules, options), files)
}

/// A view or check block as written, or clauses alone wrapped in a view.
fn proposed(text: &str) -> String {
    let trimmed = text.trim_start();
    if ["view ", "check ", "style "].iter().any(|k| trimmed.starts_with(k)) {
        text.to_owned()
    } else {
        // Titled, as a view of the project would be: no information about a missing title.
        format!("view proposed {{\n  title: \"Proposed view\"\n{text}\n}}\n")
    }
}

fn is_proposed(files: &[SourceFile], file: FileId) -> bool {
    files.get(file.0 as usize).is_some_and(|f| f.path.starts_with(PROPOSED))
}

fn errors(compilation: &Compilation) -> usize {
    compilation.diagnostics.iter().filter(|d| d.severity == Severity::Error).count()
}

fn place(files: &[SourceFile], span: cadastre_core::source::Span) -> String {
    let path = files.get(span.file.0 as usize).map_or("", |f| f.path.as_str());
    format!("{path}:{}:{}", span.line + 1, span.column + 1)
}

fn severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Incomplete => "incomplete",
    }
}

fn list_kinds(compilation: &Compilation) -> Value {
    let schema = &compilation.registry.schema;
    let kinds: Vec<Value> = schema
        .kind_names()
        .into_iter()
        .filter_map(|name| schema.kind_def(name))
        .map(|kind| {
            let parents: Vec<&str> = kind.parents.iter().map(|p| if *p == "none" { "root" } else { p }).collect();
            let like = schema.base(kind.name);
            json!({
                "kind": kind.name,
                "like": (like != kind.name).then_some(like),
                "parents": parents,
            })
        })
        .collect();
    let groups: Vec<Value> = schema.groups().map(|(name, kinds)| json!({ "group": name, "kinds": kinds })).collect();
    json!({ "kinds": kinds, "groups": groups })
}

fn key(def: &AttrDef) -> Value {
    let values = match def.ty {
        Ty::Symbol(values) => Some(values),
        _ => None,
    };
    json!({ "key": def.key, "type": schema::type_text(def), "required": def.required, "values": values })
}

fn list_keys(compilation: &Compilation, kind: &str) -> Result<Value, String> {
    let registry = &compilation.registry;
    let Some(def) = registry.schema.kind_def(kind) else {
        let known = registry.schema.kind_names();
        let hint = cadastre_core::suggest::closest(kind, known.iter().copied())
            .map(|s| format!(": did you mean {s}?"))
            .unwrap_or_default();
        return Err(format!("unknown kind {kind}{hint}"));
    };
    let schema_keys: BTreeSet<&str> = def.attrs.iter().chain(schema::RESOURCE_KEYS).map(|a| a.key).collect();
    let free: BTreeSet<&str> = registry
        .resources
        .iter()
        .filter(|r| r.kind == kind)
        .flat_map(|r| r.attributes.iter().map(|a| a.key.as_str()))
        .filter(|k| !schema_keys.contains(k))
        .collect();
    Ok(json!({
        "kind": kind,
        "keys": def.attrs.iter().map(key).collect::<Vec<_>>(),
        "keys_of_every_resource": schema::RESOURCE_KEYS.iter().map(key).collect::<Vec<_>>(),
        "free_keys_in_the_project": free,
    }))
}

fn label(registry: &Registry, id: ResourceId) -> Option<String> {
    registry.resource(id).attribute("label").map(|a| cadastre_core::query::value_text(&a.value))
}

/// How well a resource matches a query: higher is better, 0 not at all.
fn score(registry: &Registry, id: ResourceId, query: &str) -> u32 {
    let query = query.to_lowercase();
    let name = registry.resource(id).name.to_lowercase();
    let qualified = registry.qualified_name(id).to_lowercase();
    let label = label(registry, id).unwrap_or_default().to_lowercase();
    if name == query || qualified == query {
        100
    } else if qualified.ends_with(&format!(".{query}")) {
        90
    } else if name.starts_with(&query) {
        80
    } else if name.contains(&query) || qualified.contains(&query) {
        60
    } else if !label.is_empty() && (label.contains(&query) || query.split_whitespace().all(|w| label.contains(w))) {
        50
    } else if query.len() >= 4 && cadastre_core::suggest::distance(&name, &query) <= 2 {
        40
    } else {
        0
    }
}

fn search(compilation: &Compilation, query: &str, kind: Option<&str>, limit: usize) -> Value {
    let registry = &compilation.registry;
    let kinds = kind.map(|k| registry.schema.expand(k));
    let mut found: Vec<(u32, ResourceId)> = registry
        .ids()
        .filter(|&id| kinds.as_ref().is_none_or(|k| k.contains(&registry.resource(id).kind.as_str())))
        .map(|id| (score(registry, id, query), id))
        .filter(|(s, _)| *s > 0)
        .collect();
    found.sort_by_key(|&(s, id)| (std::cmp::Reverse(s), registry.qualified_name(id)));
    let total = found.len();
    let results: Vec<Value> = found
        .into_iter()
        .take(limit)
        .map(|(_, id)| {
            let resource = registry.resource(id);
            json!({ "name": registry.qualified_name(id), "kind": resource.kind, "label": label(registry, id) })
        })
        .collect();
    json!({ "total": total, "results": results })
}

/// The resources a written name designates: its qualified name, or a suffix of it.
fn resolve(registry: &Registry, name: &str) -> Vec<ResourceId> {
    let exact: Vec<ResourceId> = registry.ids().filter(|&id| registry.qualified_name(id) == name).collect();
    if !exact.is_empty() {
        return exact;
    }
    registry
        .ids()
        .filter(|&id| {
            let qualified = registry.qualified_name(id);
            qualified.ends_with(&format!(".{name}"))
        })
        .collect()
}

fn get_resource(compilation: &Compilation, files: &[SourceFile], name: &str) -> Result<Value, String> {
    let registry = &compilation.registry;
    let model = &compilation.model;
    let id = match resolve(registry, name).as_slice() {
        [id] => *id,
        [] => {
            let close = search(compilation, name, None, 5);
            return Err(format!("no resource named {name}; close ones: {}", close["results"]));
        }
        several => {
            let names: Vec<String> = several.iter().map(|&id| registry.qualified_name(id)).collect();
            return Err(format!("{name} designates several resources: {}", names.join(", ")));
        }
    };
    let _ = model;
    let mut record = cadastre_core::api::resource(compilation, files, id);
    // The links capped, their totals kept.
    if let Value::Object(map) = &mut record {
        let mut totals = serde_json::Map::new();
        for key in ["outgoing", "incoming"] {
            if let Some(Value::Array(links)) = map.get_mut(key) {
                totals.insert(key.to_owned(), Value::from(links.len()));
                links.truncate(LIMIT);
            }
        }
        map.insert("links_total".to_owned(), Value::Object(totals));
    }
    Ok(record)
}

fn list_views(compilation: &Compilation, files: &[SourceFile]) -> Value {
    let views: Vec<Value> = compilation
        .plans
        .views
        .iter()
        .map(|v| {
            let formats: Vec<String> = v.render.iter().flat_map(|r| r.formats.iter().map(|f| f.text.clone())).collect();
            json!({ "name": v.name, "declared": place(files, v.span), "formats": formats })
        })
        .collect();
    let checks: Vec<Value> = compilation
        .plans
        .checks
        .iter()
        .map(|c| json!({ "name": c.name, "declared": place(files, c.span), "severity": severity(c.severity()) }))
        .collect();
    json!({ "views": views, "checks": checks })
}

fn diagnostics_of_proposed(compilation: &Compilation, files: &[SourceFile]) -> Vec<Value> {
    compilation
        .diagnostics
        .iter()
        .filter(|d| d.spans.iter().any(|s| is_proposed(files, s.file)))
        .map(|d| {
            json!({
                "severity": severity(d.severity),
                "message": d.message,
                "help": d.help,
                "at": d.spans.first().map(|s| place(files, *s)),
            })
        })
        .collect()
}

fn check_view(compilation: &Compilation, files: &[SourceFile]) -> Value {
    let diagnostics = diagnostics_of_proposed(compilation, files);
    let own = diagnostics.iter().filter(|d| d["severity"] == "error").count();
    let project_errors = errors(compilation) - own;
    let views: Vec<&str> =
        compilation.plans.views.iter().filter(|v| is_proposed(files, v.span.file)).map(|v| v.name.as_str()).collect();
    json!({
        "valid": own == 0,
        "diagnostics": diagnostics,
        "views": views,
        "errors_elsewhere_in_the_project": project_errors,
        "note": (project_errors > 0).then_some("the project has errors of its own: views are not evaluated until they are fixed"),
    })
}

fn eval_view(
    compilation: &Compilation,
    files: &[SourceFile],
    name: Option<&str>,
    proposed: bool,
    format: Option<&str>,
) -> Result<Value, String> {
    let view = match (proposed, name) {
        (true, _) => compilation.plans.views.iter().rev().find(|v| is_proposed(files, v.span.file)),
        (false, Some(name)) => compilation.plans.views.iter().find(|v| v.name == name),
        (false, None) => return Err("a view: its name, or the text of a proposed one".to_owned()),
    };
    let Some(view) = view else {
        let diagnostics = diagnostics_of_proposed(compilation, files);
        return Err(match (proposed, diagnostics.is_empty()) {
            (true, false) => format!("the proposed view does not compile: {}", Value::from(diagnostics)),
            (true, true) => "the text holds no view".to_owned(),
            (false, _) => format!("unknown view: {}", name.unwrap_or_default()),
        });
    };
    let mut record = match cadastre_core::api::view(compilation, files, view) {
        Ok(record) => record,
        Err(errors) => {
            let shown: Vec<&str> = errors.iter().take(20).map(String::as_str).collect();
            return Err(format!("the project has errors, views are not evaluated:\n{}", shown.join("")));
        }
    };
    let rendering = match format {
        Some(format) => {
            let formats = [format.to_owned()];
            let (outputs, _) = cadastre_core::build_with(compilation, Some(&view.name), Some(&formats));
            outputs.into_iter().next().map(|o| o.content)
        }
        None => None,
    };
    if let Value::Object(map) = &mut record {
        let mut totals = serde_json::Map::new();
        let mut truncated = false;
        for key in ["nodes", "edges"] {
            if let Some(Value::Array(items)) = map.get_mut(key) {
                totals.insert(key.to_owned(), Value::from(items.len()));
                truncated |= items.len() > LIMIT;
                items.truncate(LIMIT);
            }
        }
        totals.insert("truncated".to_owned(), Value::from(truncated));
        map.insert("totals".to_owned(), Value::Object(totals));
        map.insert("rendering".to_owned(), rendering.map_or(Value::Null, Value::from));
    }
    Ok(record)
}

fn run_checks(compilation: &Compilation, files: &[SourceFile], name: Option<&str>) -> Result<Value, String> {
    if let Some(name) = name
        && !compilation.plans.checks.iter().any(|c| c.name == name)
    {
        return Err(format!("unknown check: {name}"));
    }
    if errors(compilation) > 0 {
        return Err(format!("the project has {} error(s): checks run on a valid model only", errors(compilation)));
    }
    let results: Vec<Value> = compilation
        .plans
        .checks
        .iter()
        .filter(|c| name.is_none_or(|n| n == c.name))
        .map(|check| cadastre_core::api::check(compilation, files, check))
        .collect();
    Ok(json!({ "checks": results }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposed_clauses_become_a_view() {
        assert_eq!(proposed("select app"), "view proposed {\n  title: \"Proposed view\"\nselect app\n}\n");
        assert_eq!(proposed("view x { select app }"), "view x { select app }");
    }
}
