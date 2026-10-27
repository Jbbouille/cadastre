//! `parent:`: a resource declared at the root placed in another resource, as if it were written
//! in its block. Placement happens before the checks, so that qualified names, lexical scope,
//! inheritance and consistency checks see the resource where it belongs.

use crate::diagnostic::Diagnostic;
use crate::registry::{Registry, ResourceId};
use crate::resolve::{Resolution, Resolver};
use crate::schema;
use crate::source::Span;

/// Places the resources carrying `parent:`; returns the names of their parents, for the editors.
pub fn place(registry: &mut Registry, diagnostics: &mut Vec<Diagnostic>) -> Vec<(Span, Vec<ResourceId>)> {
    let mut names = Vec::new();
    let mut pending: Vec<ResourceId> = Vec::new();
    for id in registry.ids() {
        let resource = registry.resource(id);
        let Some(attribute) = resource.attribute("parent") else { continue };
        if let Some(parent) = resource.parent {
            let parent = registry.resource(parent);
            diagnostics.push(
                Diagnostic::error(
                    format!(
                        "{} {} is already placed in {} {} by nesting",
                        resource.kind, resource.name, parent.kind, parent.name
                    ),
                    attribute.key_span,
                )
                .with_help("write `parent:` on a resource declared at the root, or nest it, not both"),
            );
        } else if registry.schema.kind_def(&resource.kind).is_some_and(|def| def.parents == ["none"]) {
            diagnostics.push(Diagnostic::error(
                format!("a {} is placed at the root: it has no parent", resource.kind),
                attribute.key_span,
            ));
        } else if registry.schema.kind_def(&resource.kind).is_some() {
            pending.push(id);
        }
    }

    // From one pass to the next: a parent may itself be placed by `parent:` (`lan.servers` is
    // only found once `servers` is in `lan`).
    let mut last: Vec<(ResourceId, Resolution)> = Vec::new();
    while !pending.is_empty() {
        let resolver = Resolver::new(registry);
        let mut placed = Vec::new();
        last.clear();
        for &id in &pending {
            let resource = registry.resource(id);
            let value = &resource.attribute("parent").expect("pending resources carry parent").value;
            let Some(reference) = value.as_reference() else {
                last.push((id, Resolution::Dangling));
                continue;
            };
            let expected = possible_parents(&registry.schema, &resource.kind);
            match resolver.resolve(&reference, None, &expected) {
                Resolution::Resolved(target) => placed.push((id, target, reference.span)),
                other => last.push((id, other)),
            }
        }
        if placed.is_empty() {
            break;
        }
        for (id, target, span) in placed {
            if target == id || registry.ancestors(target).any(|a| a == id) {
                diagnostics.push(Diagnostic::error(
                    format!("{} cannot be placed in itself", registry.resource(id).name),
                    span,
                ));
            } else {
                registry.resources[id.0 as usize].parent = Some(target);
                registry.resources[target.0 as usize].children.push(id);
            }
            names.push((span, vec![target]));
            pending.retain(|&p| p != id);
        }
    }

    // What could not be placed.
    for (id, resolution) in last {
        let resource = registry.resource(id);
        let value = &resource.attribute("parent").expect("pending resources carry parent").value;
        let kinds = possible_parents(&registry.schema, &resource.kind);
        let message = match &resolution {
            Resolution::Ambiguous(candidates) => {
                names.push((value.span, candidates.clone()));
                let list: Vec<String> = candidates.iter().map(|&c| registry.qualified_name(c)).collect();
                format!("ambiguous parent: {} ({})", value.raw, list.join(", "))
            }
            Resolution::WrongType(found) => {
                names.push((value.span, found.clone()));
                format!("{} cannot be placed in {} {}", resource.kind, registry.resource(found[0]).kind, value.raw)
            }
            _ => format!("unknown parent: {}", value.raw),
        };
        let parents = registry.schema.kind_def(&resource.kind).map_or(Vec::new(), |d| d.parents.to_vec());
        let help = format!(
            "parent of {}: {}",
            resource.kind,
            parents.iter().map(|p| if *p == "none" { "root" } else { p }).collect::<Vec<_>>().join(", ")
        );
        let mut d = Diagnostic::error(message, value.span).with_help(help);
        if matches!(resolution, Resolution::Dangling) {
            let candidates =
                registry.resources.iter().filter(|r| kinds.contains(&r.kind.as_str())).map(|r| r.name.as_str());
            if let Some(s) = crate::suggest::closest(&value.raw, candidates) {
                d = d.with_help(format!("did you mean `{s}`?"));
            }
        }
        diagnostics.push(d);
    }
    names
}

/// The kinds a resource of this kind can be placed in: its parents and their possible ancestors
/// (an approximate placement makes it incomplete, as when nested).
pub fn possible_parents(schema: &schema::Schema, kind: &str) -> Vec<&'static str> {
    schema
        .kind_names()
        .into_iter()
        .filter(|&parent| {
            schema
                .kind_def(kind)
                .is_some_and(|def| !matches!(schema.placement(def, Some(parent)), schema::Placement::Impossible))
        })
        .collect()
}
