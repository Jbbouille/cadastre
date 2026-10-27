//! The JSON model of a compiled project, shared by `cadastre export json`, the MCP server and
//! the API of the site (`api/` beside the pages): one record per resource, a project summary,
//! the evaluated views, the checks. Documented in `docs/api.md`.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::check::Holder;
use crate::diagnostic::Severity;
use crate::plan::{Check, View};
use crate::project::Compilation;
use crate::registry::{Registry, ResourceId};
use crate::source::{SourceFile, Span};

/// The version of the JSON model, in `project.json` and the export.
pub const VERSION: u32 = 2;

/// `path:line:column` of a span, 1-based.
pub fn place(files: &[SourceFile], span: Span) -> String {
    let path = files.get(span.file.0 as usize).map_or("", |f| f.path.as_str());
    format!("{path}:{}:{}", span.line + 1, span.column + 1)
}

pub fn severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Incomplete => "incomplete",
    }
}

/// A text attribute of a resource (`label`, `description`), its string content.
pub fn text_attribute(registry: &Registry, id: ResourceId, key: &str) -> Option<String> {
    registry.resource(id).attribute(key).map(|a| crate::query::value_text(&a.value))
}

pub fn label(registry: &Registry, id: ResourceId) -> Option<String> {
    text_attribute(registry, id, "label")
}

/// The site a resource is in: by containment, a site itself included; otherwise the site of
/// what it runs on (`on`), as `group-by site` finds it, when its hosts agree.
fn site_of(compilation: &Compilation, id: ResourceId) -> Option<ResourceId> {
    fn find(compilation: &Compilation, id: ResourceId, depth: usize) -> Option<ResourceId> {
        let registry = &compilation.registry;
        if let Some(site) =
            std::iter::once(id).chain(registry.ancestors(id)).find(|&a| registry.resource(a).kind == "site")
        {
            return Some(site);
        }
        if depth == 0 {
            return None;
        }
        let hosts = compilation.model.reference(Holder::Resource(id), "on");
        let mut sites = hosts.iter().map(|&host| find(compilation, host, depth - 1));
        let first = sites.next()??;
        sites.all(|site| site == Some(first)).then_some(first)
    }
    find(compilation, id, 4)
}

/// What an incomplete resource misses, each reason without the `kind name: ` the diagnostic
/// starts with.
fn missing(compilation: &Compilation, id: ResourceId) -> Vec<String> {
    let prefix = format!("{} {}: ", compilation.registry.resource(id).kind, compilation.registry.qualified_name(id));
    compilation
        .model
        .incomplete_reasons
        .get(&id)
        .into_iter()
        .flatten()
        .map(|reason| reason.strip_prefix(&prefix).unwrap_or(reason).to_owned())
        .collect()
}

/// The attributes of a resource or a link, each value as text, lists as arrays.
fn attributes(list: &[crate::registry::Attribute]) -> Value {
    let mut map = Map::new();
    for attribute in list {
        let items: Vec<String> = attribute.value.items().into_iter().map(crate::query::value_text).collect();
        let value = match &attribute.value.kind {
            crate::registry::ValueKind::List(_) => json!(items),
            _ => json!(items.into_iter().next().unwrap_or_default()),
        };
        map.insert(attribute.key.clone(), value);
    }
    Value::Object(map)
}

/// The short record of a resource, for the indexes and the lists.
pub fn summary(compilation: &Compilation, files: &[SourceFile], id: ResourceId) -> Value {
    let registry = &compilation.registry;
    let resource = registry.resource(id);
    let text = |key: &str| text_attribute(registry, id, key);
    json!({
        "id": registry.qualified_name(id),
        "kind": resource.kind,
        "name": resource.name,
        "label": text("label"),
        "owner": compilation.model.inherited(registry, id, "owner").map(|o| registry.qualified_name(o)),
        "operator": compilation.model.inherited(registry, id, "operator").map(|o| registry.qualified_name(o)),
        "site": site_of(compilation, id).map(|s| registry.qualified_name(s)),
        "system": compilation.model.system_of(registry, id).map(|s| registry.qualified_name(s)),
        "incomplete": compilation.model.incomplete.contains(&id),
        "missing": missing(compilation, id),
        "declared": place(files, resource.name_span),
    })
}

/// A link as drawn from a resource: its other end and what it carries.
fn link_record(compilation: &Compilation, files: &[SourceFile], index: usize, other: ResourceId) -> Value {
    let registry = &compilation.registry;
    let written = &registry.links[index];
    let resolved = |key: &str| -> Vec<String> {
        compilation.model.reference(Holder::Link(index), key).iter().map(|&r| registry.qualified_name(r)).collect()
    };
    json!({
        "with": registry.qualified_name(other),
        "port": written.port.as_ref().map(|p| match &p.port {
            Some(port) => format!("{}/{port}", p.protocol),
            None => p.protocol.clone(),
        }),
        "layers": written.layers.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>(),
        "carries": resolved("carries"),
        "through": resolved("through"),
        "through_steps": match compilation.model.through_steps.get(&index) {
            Some(steps) => steps.iter().map(|step| step.iter().map(|&t| registry.qualified_name(t)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            None => resolved("through").into_iter().map(|t| vec![t]).collect(),
        },
        "through_deduced": compilation.model.deduced_through.contains_key(&index)
            || compilation.model.through_steps.contains_key(&index),
        "attributes": attributes(&written.attributes),
        "declared": place(files, written.span),
    })
}

/// The full record of a resource: what the site shows on its page.
pub fn resource(compilation: &Compilation, files: &[SourceFile], id: ResourceId) -> Value {
    let registry = &compilation.registry;
    let model = &compilation.model;
    let resource = registry.resource(id);
    let name = |i: ResourceId| registry.qualified_name(i);
    let names = |ids: &[ResourceId]| ids.iter().map(|&i| name(i)).collect::<Vec<_>>();
    let mut references = BTreeMap::new();
    for ((holder, key), targets) in &model.references {
        if *holder == Holder::Resource(id) {
            references.insert(key.clone(), names(targets));
        }
    }
    let (mut outgoing, mut incoming) = (Vec::new(), Vec::new());
    for (index, ends) in model.link_ends.iter().enumerate() {
        match ends {
            Some((source, target)) if *source == id => outgoing.push(link_record(compilation, files, index, *target)),
            Some((source, target)) if *target == id => incoming.push(link_record(compilation, files, index, *source)),
            _ => {}
        }
    }
    let path: Vec<String> = {
        let mut chain: Vec<ResourceId> = registry.ancestors(id).collect();
        chain.reverse();
        names(&chain)
    };
    let questions: Vec<String> = resource
        .attribute("question")
        .map(|a| a.value.items().into_iter().map(crate::query::value_text).collect())
        .unwrap_or_default();
    // The members of a pool or a cluster: the resources whose `pool` or `cluster` designates it.
    let mut members: Vec<ResourceId> = Vec::new();
    for ((holder, key), targets) in &model.references {
        if let Holder::Resource(member) = holder
            && matches!(key.as_str(), "pool" | "cluster" | "members")
            && targets.contains(&id)
            && !members.contains(member)
        {
            members.push(*member);
        }
    }
    members.sort_by_key(|&m| registry.qualified_name(m));
    // What runs here: on what it contains, on its members, and on what runs on those.
    let mut inside: Vec<ResourceId> = registry.descendants(id).collect();
    inside.extend(members.iter().copied());
    let mut runs_here: Vec<ResourceId> = Vec::new();
    let mut queue = inside.clone();
    while let Some(place) = queue.pop() {
        for &hosted in model.hosts.get(&place).into_iter().flatten() {
            if !runs_here.contains(&hosted) && !inside.contains(&hosted) {
                runs_here.push(hosted);
                queue.push(hosted);
            }
        }
    }
    runs_here.sort_by_key(|&m| registry.qualified_name(m));
    let mut record = summary(compilation, files, id);
    let extra = json!({
        "virtual": resource.is_virtual(),
        "description": text_attribute(registry, id, "description"),
        "parent": resource.parent.map(name),
        "path": path,
        "module": compilation.module_of(resource.name_span.file),
        "attributes": attributes(&resource.attributes),
        "references": references,
        "children": names(&resource.children),
        "hosts": model.hosts.get(&id).map(|h| names(h)).unwrap_or_default(),
        "members": names(&members),
        "runs_here": names(&runs_here),
        "failures": names(model.failures_of(registry, id)),
        "failure_pairs": model
            .failure_pairs_of(registry, id)
            .iter()
            .map(|&(a, b)| [registry.qualified_name(a), registry.qualified_name(b)])
            .collect::<Vec<_>>(),
        "questions": questions,
        "outgoing": outgoing,
        "incoming": incoming,
    });
    if let (Value::Object(record), Value::Object(extra)) = (&mut record, extra) {
        record.extend(extra);
    }
    record
}

/// Every resource, as its short record, in the order of the registry.
pub fn index(compilation: &Compilation, files: &[SourceFile]) -> Value {
    Value::Array(compilation.registry.ids().map(|id| summary(compilation, files, id)).collect())
}

/// Every link of the registry, both ends named.
pub fn links(compilation: &Compilation, files: &[SourceFile]) -> Value {
    let registry = &compilation.registry;
    let records = compilation.model.link_ends.iter().enumerate().filter_map(|(index, ends)| {
        let (source, target) = (*ends)?;
        let mut record = link_record(compilation, files, index, target);
        if let Value::Object(map) = &mut record {
            map.shift_remove("with");
            let mut ordered = Map::new();
            ordered.insert("source".to_owned(), Value::from(registry.qualified_name(source)));
            ordered.insert("target".to_owned(), Value::from(registry.qualified_name(target)));
            ordered.extend(std::mem::take(map));
            *map = ordered;
        }
        Some(record)
    });
    Value::Array(records.collect())
}

/// The record of a view, declared: name, title, description, formats, where.
pub fn view_summary(files: &[SourceFile], view: &View) -> Value {
    let formats: Vec<String> = view.render.iter().flat_map(|r| r.formats.iter().map(|f| f.text.clone())).collect();
    json!({
        "name": view.name,
        "title": view.title(),
        "description": view.description(),
        "formats": formats,
        "declared": place(files, view.span),
    })
}

/// A view evaluated: its nodes, frames and edges; `Err` with the errors of the project when
/// views cannot be evaluated.
pub fn view(compilation: &Compilation, files: &[SourceFile], view: &View) -> Result<Value, Vec<String>> {
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.render_in(files, compilation.language))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let registry = &compilation.registry;
    let mut diagnostics = Vec::new();
    let graph = crate::view::evaluate(view, registry, &compilation.model, &mut diagnostics);
    let frame_id = |index: usize| -> String {
        let group = &graph.groups[index];
        match (&group.resource, &group.zone, &group.value) {
            (Some(r), _, _) => registry.qualified_name(*r),
            (_, Some(zone), _) => format!("zone:{}", zone.name),
            (_, _, Some(value)) => format!("value:{value}"),
            _ => format!("frame:{index}"),
        }
    };
    let frames: Vec<Value> = (0..graph.groups.len())
        .map(|i| {
            let group = &graph.groups[i];
            let title = match (&group.resource, &group.zone, &group.value) {
                (Some(r), _, _) => label(registry, *r).unwrap_or_else(|| registry.resource(*r).name.clone()),
                (_, Some(zone), _) => zone.label.clone(),
                (_, _, Some(value)) => value.clone(),
                _ => String::new(),
            };
            json!({
                "id": frame_id(i),
                "title": title,
                "resource": group.resource.map(|r| registry.qualified_name(r)),
                "in": group.parent.map(frame_id),
            })
        })
        .collect();
    let nodes: Vec<Value> = graph
        .nodes
        .iter()
        .map(|&id| {
            json!({
                "id": registry.qualified_name(id),
                "kind": registry.resource(id).kind,
                "label": label(registry, id).unwrap_or_else(|| registry.resource(id).name.clone()),
                "frame": graph.node_group.get(&id).map(|&g| frame_id(g)),
            })
        })
        .collect();
    let edges: Vec<Value> = graph
        .edges
        .iter()
        .map(|e| {
            json!({
                "source": registry.qualified_name(e.source),
                "target": registry.qualified_name(e.target),
                "relation": e.relation,
                "ports": e.ports,
                "layers": e.layers,
                "carries": e.carries.iter().map(|&c| registry.qualified_name(c)).collect::<Vec<_>>(),
                "through": e.through.iter().map(|&t| registry.qualified_name(t)).collect::<Vec<_>>(),
                "through_steps": through_steps(registry, e),
                "step": e.step,
            })
        })
        .collect();
    let warnings: Vec<Value> = diagnostics
        .iter()
        .map(|d| d.in_language(compilation.language))
        .map(|d| json!({ "severity": severity(d.severity), "message": d.message, "help": d.help }))
        .collect();
    let mut record = view_summary(files, view);
    if let Value::Object(map) = &mut record {
        map.insert("nodes".to_owned(), Value::Array(nodes));
        map.insert("frames".to_owned(), Value::Array(frames));
        map.insert("edges".to_owned(), Value::Array(edges));
        map.insert("diagnostics".to_owned(), Value::Array(warnings));
    }
    Ok(record)
}

/// The way of an edge as steps, each with its variants (`[["sw-01", "sw-02"], ["fw"]]`): the
/// steps deduced from the cabling, the path of a `through` written, or each device as a step.
fn through_steps(registry: &Registry, e: &crate::view::GraphEdge) -> Vec<Vec<String>> {
    let names = |ids: &[ResourceId]| ids.iter().map(|&t| registry.qualified_name(t)).collect::<Vec<_>>();
    if e.provenance.len() == 1 && !e.through_steps.is_empty() {
        e.through_steps.iter().map(|step| names(step)).collect()
    } else if e.provenance.len() == 1 && !e.through_path.is_empty() {
        e.through_path.iter().map(|&t| vec![registry.qualified_name(t)]).collect()
    } else {
        e.through.iter().map(|&t| vec![registry.qualified_name(t)]).collect()
    }
}

/// The result of a check: passed or failed, what was selected, what fails it; `None` when the
/// project has errors (checks run on a valid model only).
pub fn check(compilation: &Compilation, files: &[SourceFile], check: &Check) -> Value {
    let base = json!({
        "name": check.name,
        "severity": severity(check.severity()),
        "message": check.message(),
        "declared": place(files, check.span),
    });
    let mut record = base;
    let has_errors = compilation.diagnostics.iter().any(|d| d.severity == Severity::Error);
    let outcome = if has_errors { None } else { crate::rules::outcome(compilation, check) };
    let extra = match (has_errors, outcome) {
        (true, _) => json!({ "result": "not run", "summary": compilation.language.message("the project has errors") }),
        (false, None) => json!({ "result": "nothing to run" }),
        (false, Some(outcome)) => {
            let items = |items: &[crate::rules::Item]| {
                items.iter().map(|i| json!({ "item": i.text, "declared": place(files, i.span) })).collect::<Vec<_>>()
            };
            json!({
                "result": if outcome.passed { "passed" } else if outcome.error.is_some() { "cannot be evaluated" } else { "failed" },
                "summary": compilation.language.message(&outcome.summary),
                "selected": outcome.selected.len(),
                "failing": items(&outcome.offending),
            })
        }
    };
    if let (Value::Object(record), Value::Object(extra)) = (&mut record, extra) {
        record.extend(extra);
    }
    record
}

pub fn checks(compilation: &Compilation, files: &[SourceFile]) -> Value {
    Value::Array(compilation.plans.checks.iter().map(|c| check(compilation, files, c)).collect())
}

/// The summary of the project: its name, the version of the model, its counts, its views and
/// checks, its modules, its diagnostics.
pub fn project(compilation: &Compilation, files: &[SourceFile], name: &str) -> Value {
    let registry = &compilation.registry;
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for id in registry.ids() {
        *kinds.entry(registry.resource(id).kind.as_str()).or_default() += 1;
    }
    let diagnostics: Vec<Value> = compilation
        .diagnostics
        .iter()
        .map(|d| d.in_language(compilation.language))
        .map(|d| {
            json!({
                "severity": severity(d.severity),
                "message": d.message,
                "help": d.help,
                "at": d.spans.first().map(|s| place(files, *s)),
            })
        })
        .collect();
    json!({
        "format": "cadastre-model",
        "version": VERSION,
        "cadastre": env!("CARGO_PKG_VERSION"),
        "name": name,
        "resources": registry.resources.len(),
        "links": compilation.model.link_ends.iter().flatten().count(),
        "kinds": kinds,
        "views": compilation.plans.views.iter().map(|v| view_summary(files, v)).collect::<Vec<_>>(),
        "checks": checks(compilation, files),
        "modules": compilation.modules.iter().map(|m| json!({ "name": m.name, "files": [m.files.start, m.files.end] })).collect::<Vec<_>>(),
        "files": files.iter().map(|f| &f.path).collect::<Vec<_>>(),
        "diagnostics": diagnostics,
    })
}

/// The index of the search of the site: id, kind, label and the words to match.
pub fn search(compilation: &Compilation) -> Value {
    let registry = &compilation.registry;
    let entries = registry.ids().map(|id| {
        let resource = registry.resource(id);
        let mut text = vec![resource.name.clone()];
        text.extend(label(registry, id));
        text.extend(text_attribute(registry, id, "description"));
        json!({
            "id": registry.qualified_name(id),
            "kind": resource.kind,
            "label": label(registry, id).unwrap_or_else(|| resource.name.clone()),
            "text": text.join(" "),
        })
    });
    Value::Array(entries.collect())
}

/// The whole model in one document: `cadastre export json`.
pub fn document(compilation: &Compilation, files: &[SourceFile], name: &str) -> Value {
    let mut document = project(compilation, files, name);
    if let Value::Object(map) = &mut document {
        map.insert(
            "resources".to_owned(),
            Value::Array(compilation.registry.ids().map(|id| resource(compilation, files, id)).collect()),
        );
        map.insert("links".to_owned(), links(compilation, files));
    }
    document
}
