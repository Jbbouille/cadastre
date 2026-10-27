//! Hover: a card describing the resource a name designates.

use std::collections::BTreeMap;

use cadastre_core::Compilation;
use cadastre_core::check::Holder;
use cadastre_core::registry::{ResourceId, Value, ValueKind};
use cadastre_core::source::SourceFile;

/// Markdown description of the resources designated at a position (several for an ambiguous name).
pub fn describe(compilation: &Compilation, files: &[SourceFile], targets: &[ResourceId]) -> Option<String> {
    match targets {
        [] => None,
        [id] => Some(resource(compilation, files, *id)),
        candidates => {
            let names: Vec<String> = candidates
                .iter()
                .map(|&id| {
                    let r = compilation.registry.resource(id);
                    format!("- {} `{}`", r.kind, compilation.registry.qualified_name(id))
                })
                .collect();
            Some(format!("**Ambiguous name**, candidates:\n\n{}", names.join("\n")))
        }
    }
}

fn resource(compilation: &Compilation, files: &[SourceFile], id: ResourceId) -> String {
    let registry = &compilation.registry;
    let r = registry.resource(id);
    let virtual_ = if r.is_virtual() { "virtual " } else { "" };
    let mut out = format!("**{virtual_}{}** `{}`", r.kind, registry.qualified_name(id));
    if compilation.model.incomplete.contains(&id) {
        out.push_str(" · *incomplete*");
    }

    if let Some(description) = r.attribute("description")
        && let ValueKind::String(text) = &description.value.kind
    {
        out.push_str(&format!("\n\n{text}"));
    }

    let attributes: Vec<String> = r
        .attributes
        .iter()
        .filter(|a| a.key != "description")
        .map(|a| {
            // A cron schedule, in plain words.
            let plain = (a.key == "schedule")
                .then(|| cadastre_core::schedule::plain(registry, &compilation.model, id))
                .flatten()
                .map(|p| format!(" *({p})*"))
                .unwrap_or_default();
            format!("- `{}`: {}{plain}", a.key, value(&a.value))
        })
        .collect();
    if !attributes.is_empty() {
        out.push_str(&format!("\n\n{}", attributes.join("\n")));
    }
    for key in ["owner", "operator"] {
        if r.attribute(key).is_none()
            && let Some(inherited) = compilation.model.inherited(registry, id, key)
        {
            out.push_str(&format!("\n- `{key}`: {} *(inherited)*", registry.resource(inherited).name));
        }
    }

    if !r.children.is_empty() {
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for &child in &r.children {
            *kinds.entry(registry.resource(child).kind.as_str()).or_default() += 1;
        }
        let kinds: Vec<String> = kinds.into_iter().map(|(kind, n)| format!("{n} {kind}")).collect();
        out.push_str(&format!("\n\nContains {}", kinds.join(", ")));
    }

    // What runs on it (`on`), and the systems it serves.
    if let Some(hosted) = compilation.model.hosts.get(&id) {
        let names: Vec<String> = hosted.iter().map(|&h| format!("`{}`", registry.qualified_name(h))).collect();
        out.push_str(&format!("\n\n**Hosts** {}", names.join(", ")));
        let systems = compilation.model.hosted_systems(registry, id);
        if systems.len() > 1 {
            let systems: Vec<&str> = systems.iter().map(|&s| registry.resource(s).name.as_str()).collect();
            out.push_str(&format!(" · *shared by {} systems: {}*", systems.len(), systems.join(", ")));
        }
    }

    let (mut outgoing, mut incoming) = (Vec::new(), Vec::new());
    for (index, ends) in compilation.model.link_ends.iter().enumerate() {
        let Some((source, target)) = *ends else { continue };
        let link = &registry.links[index];
        let port = link.port.as_ref().map(|p| format!(" : {}", p.normalized())).unwrap_or_default();
        let carries: Vec<&str> = compilation
            .model
            .reference(Holder::Link(index), "carries")
            .iter()
            .map(|&d| registry.resource(d).name.as_str())
            .collect();
        let carries = if carries.is_empty() { String::new() } else { format!(" ({})", carries.join(", ")) };
        if source == id {
            outgoing.push(format!("- → `{}`{port}{carries}", registry.resource(target).name));
        }
        if target == id {
            incoming.push(format!("- ← `{}`{port}{carries}", registry.resource(source).name));
        }
    }
    if !outgoing.is_empty() || !incoming.is_empty() {
        out.push_str(&format!(
            "\n\n**Flows**\n{}",
            outgoing.into_iter().chain(incoming).collect::<Vec<_>>().join("\n")
        ));
    }

    let span = r.name_span;
    if let Some(file) = files.get(span.file.0 as usize) {
        out.push_str(&format!("\n\n*{}:{}*", file.path, span.line + 1));
    }
    out
}

/// A value as written, its URLs as links to open (`url`, `console`).
fn value(value: &Value) -> String {
    let url = |item: &Value| match &item.kind {
        ValueKind::String(text) if cadastre_core::net::check_url(text).is_ok() => Some(format!("[{text}]({text})")),
        _ => None,
    };
    let items = value.items();
    match items.iter().map(|item| url(item)).collect::<Option<Vec<_>>>() {
        Some(links) if !links.is_empty() => links.join(", "),
        _ => value.raw.clone(),
    }
}

#[cfg(test)]
mod tests {
    use cadastre_core::compile;
    use cadastre_core::source::FileId;

    use super::*;

    #[test]
    fn describes_a_resource() {
        let text = "team ops\nsystem billing { owner: ops\n  app api { description: \"Invoices\", version: 1.2.0\n    -> db : jdbc/5432 [flow] { carries: [invoice] } }\n  database db { stores: [invoice] } }\ndata invoice\n";
        let files = [SourceFile { path: "a.ca".into(), text: text.into() }];
        let c = compile(&files);
        let api = c.definitions(FileId(0), text.find("api").unwrap());
        let card = describe(&c, &files, &api).unwrap();
        assert!(card.starts_with("**app** `billing.api`"), "{card}");
        assert!(card.contains("\n\nInvoices\n"), "{card}");
        assert!(card.contains("- `version`: 1.2.0"), "{card}");
        assert!(card.contains("- `owner`: ops *(inherited)*"), "{card}");
        assert!(card.contains("- → `db` : jdbc/5432 (invoice)"), "{card}");
        assert!(card.ends_with("*a.ca:3*"), "{card}");

        let db = c.definitions(FileId(0), text.find("database db").unwrap() + 9);
        let card = describe(&c, &files, &db).unwrap();
        assert!(card.contains("- ← `api` : jdbc/5432 (invoice)"), "{card}");
        let system = c.definitions(FileId(0), text.find("billing").unwrap());
        assert!(describe(&c, &files, &system).unwrap().contains("Contains 1 app, 1 database"));
    }

    #[test]
    fn links_the_urls() {
        let text = "server web01 { console: [\"https://idrac-web01\", \"ssh://admin@web01\"] }
";
        let files = [SourceFile { path: "a.ca".into(), text: text.into() }];
        let c = compile(&files);
        let card = describe(&c, &files, &c.definitions(FileId(0), 8)).unwrap();
        assert!(
            card.contains(
                "- `console`: [https://idrac-web01](https://idrac-web01), [ssh://admin@web01](ssh://admin@web01)"
            ),
            "{card}"
        );
    }

    #[test]
    fn describes_what_a_server_hosts() {
        let text =
            "server mutu01\nsystem pricing { app ui { on: mutu01 } }\nsystem catalog { app api { on: mutu01 } }\n";
        let files = [SourceFile { path: "a.ca".into(), text: text.into() }];
        let c = compile(&files);
        let card = describe(&c, &files, &c.definitions(FileId(0), 8)).unwrap();
        assert!(
            card.contains("**Hosts** `pricing.ui`, `catalog.api` · *shared by 2 systems: pricing, catalog*"),
            "{card}"
        );
    }
}
