//! Exports of the whole registry, for other tools: ArchiMate (Open Exchange Format), GraphML,
//! and JSON. Views stay the business of the renderings; an export carries every resource, its
//! containment, its references and its links.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::check::Holder;
use crate::project::Compilation;
use crate::registry::{Registry, ResourceId};

/// The export formats, with what they are for.
pub static FORMATS: &[(&str, &str)] = &[
    ("archimate", "ArchiMate model (Open Exchange Format), for Archi, Sparx EA and other modeling tools"),
    ("graphml", "graph of resources and relations, for yEd, Gephi, Cytoscape, NetworkX"),
    ("json", "the model as JSON: resources, containment, references and links, for scripts"),
];

/// The export of a compiled project in a format; `None` for an unknown format.
pub fn export(compilation: &Compilation, format: &str) -> Option<String> {
    export_with(compilation, &[], "", format)
}

/// Like [`export`], with the files of the project (for the places of the JSON records) and its
/// name.
pub fn export_with(
    compilation: &Compilation,
    files: &[crate::source::SourceFile],
    name: &str,
    format: &str,
) -> Option<String> {
    let model = Model::of(compilation);
    Some(match format {
        "archimate" => archimate(compilation, &model),
        "graphml" => graphml(compilation, &model),
        "json" => {
            serde_json::to_string_pretty(&crate::api::document(compilation, files, name)).expect("serializable") + "\n"
        }
        _ => return None,
    })
}

/// The relations of the registry, once: containment, references, links.
struct Model {
    /// (child, parent).
    containment: Vec<(ResourceId, ResourceId)>,
    /// (holder, key, target), in registry order.
    references: Vec<(ResourceId, String, ResourceId)>,
    links: Vec<Link>,
}

struct Link {
    index: usize,
    source: ResourceId,
    target: ResourceId,
    port: Option<String>,
    layers: Vec<String>,
}

impl Model {
    fn of(compilation: &Compilation) -> Self {
        let registry = &compilation.registry;
        let containment =
            registry.ids().filter_map(|id| registry.resource(id).parent.map(|parent| (id, parent))).collect();
        let mut references = Vec::new();
        for id in registry.ids() {
            for attribute in &registry.resource(id).attributes {
                for &target in compilation.model.reference(Holder::Resource(id), &attribute.key) {
                    references.push((id, attribute.key.clone(), target));
                }
            }
        }
        let links = registry
            .links
            .iter()
            .enumerate()
            .filter_map(|(index, link)| {
                let (source, target) = compilation.model.link_ends.get(index).copied().flatten()?;
                Some(Link {
                    index,
                    source,
                    target,
                    port: link.port.as_ref().map(|p| p.normalized()),
                    layers: link.layers.iter().map(|(l, _)| l.clone()).collect(),
                })
            })
            .collect();
        Model { containment, references, links }
    }
}

/// The raw text of the values of an attribute.
fn values(value: &crate::registry::Value) -> Vec<String> {
    value.items().into_iter().map(crate::query::value_text).collect()
}

/// Stable identifiers, valid as XML IDs: `id-` and the qualified name, its other characters as `_`.
fn identifiers(registry: &Registry) -> HashMap<ResourceId, String> {
    let mut used = HashSet::new();
    registry
        .ids()
        .map(|id| {
            let base: String = registry
                .qualified_name(id)
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_') { c } else { '_' })
                .collect();
            let mut identifier = format!("id-{base}");
            let mut n = 2;
            while !used.insert(identifier.clone()) {
                identifier = format!("id-{base}-{n}");
                n += 1;
            }
            (id, identifier)
        })
        .collect()
}

fn xml(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// ArchiMate
// ---------------------------------------------------------------------------

/// The ArchiMate element of a resource: a virtual machine is a node, whatever its kind.
fn element_of(registry: &Registry, id: ResourceId) -> &'static str {
    let resource = registry.resource(id);
    if resource.is_virtual() { "Node" } else { element_type(registry.schema.base(&resource.kind)) }
}

/// The ArchiMate element of a resource kind.
fn element_type(kind: &str) -> &'static str {
    match kind {
        "site" => "Location",
        "rack" => "Equipment",
        "vpc" | "network" | "subnet" | "internet" => "CommunicationNetwork",
        // A physical server is a device; a cluster, a pool, a fleet, nodes.
        "server" => "Device",
        "cluster" | "pool" | "fleet" => "Node",
        "router" | "switch" | "firewall" | "forward_proxy" | "load_balancer" | "proxy" | "vpn_gateway" | "device" => {
            "Device"
        }
        "vip" | "interface" => "TechnologyInterface",
        "vpn_tunnel" | "vpc_peering" | "wan_link" => "Path",
        "system" => "Grouping",
        "app" | "saas" | "component" => "ApplicationComponent",
        "job" => "ApplicationProcess",
        "database" => "SystemSoftware",
        "channel" => "ApplicationInterface",
        "data" => "DataObject",
        "team" | "actor" => "BusinessActor",
        _ => "Grouping",
    }
}

/// The ArchiMate relationship between two elements, among those the language allows for their
/// types; an association, allowed between any two elements, otherwise.
fn relationship_type(relation: Relation, source: &str, target: &str) -> &'static str {
    let application = |t: &str| matches!(t, "ApplicationComponent" | "ApplicationProcess" | "ApplicationInterface");
    let technology = |t: &str| matches!(t, "Node" | "Device" | "SystemSoftware" | "TechnologyInterface");
    match relation {
        // The parent aggregates or is composed of the child.
        Relation::Containment => match (source, target) {
            ("Location" | "Grouping", _) => "Aggregation",
            ("CommunicationNetwork", "CommunicationNetwork") | ("BusinessActor", "BusinessActor") => "Aggregation",
            ("Node" | "Device", "Node" | "Device" | "SystemSoftware" | "TechnologyInterface") => "Composition",
            ("ApplicationComponent", "ApplicationComponent" | "ApplicationInterface") => "Composition",
            _ => "Association",
        },
        // What runs on a host: system software assigned to its node, an application served by it.
        Relation::On => match (source, target) {
            ("Node" | "Device", "SystemSoftware") => "Assignment",
            (s, t) if technology(s) && application(t) => "Serving",
            _ => "Association",
        },
        Relation::Flow => match (source, target) {
            (s, t) if application(s) && application(t) => "Flow",
            (s, t) if technology(s) && technology(t) => "Flow",
            ("BusinessActor", "BusinessActor") => "Flow",
            _ => "Association",
        },
        Relation::Other => "Association",
    }
}

#[derive(Clone, Copy)]
enum Relation {
    Containment,
    On,
    Flow,
    Other,
}

fn archimate(compilation: &Compilation, model: &Model) -> String {
    let registry = &compilation.registry;
    let ids = identifiers(registry);
    // Property definitions: the kind, the qualified name, every key written.
    let mut keys: Vec<String> = vec!["kind".to_owned(), "qualified_name".to_owned()];
    for resource in &registry.resources {
        for attribute in &resource.attributes {
            if attribute.key != "description" && !keys.contains(&attribute.key) {
                keys.push(attribute.key.clone());
            }
        }
    }
    for link in &registry.links {
        for attribute in &link.attributes {
            if attribute.key != "description" && !keys.contains(&attribute.key) {
                keys.push(attribute.key.clone());
            }
        }
    }
    for extra in ["port", "layers"] {
        if !keys.iter().any(|k| k == extra) {
            keys.push(extra.to_owned());
        }
    }
    let definition = |key: &str| format!("pd-{}", keys.iter().position(|k| k == key).unwrap_or(0) + 1);
    let properties = |pairs: &[(String, String)]| -> String {
        if pairs.is_empty() {
            return String::new();
        }
        let mut out = String::from("<properties>");
        for (key, value) in pairs {
            let _ = write!(
                out,
                "<property propertyDefinitionRef=\"{}\"><value xml:lang=\"en\">{}</value></property>",
                definition(key),
                xml(value)
            );
        }
        out.push_str("</properties>");
        out
    };

    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <model xmlns=\"http://www.opengroup.org/xsd/archimate/3.0/\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xsi:schemaLocation=\"http://www.opengroup.org/xsd/archimate/3.0/ http://www.opengroup.org/xsd/archimate/3.1/archimate3_Model.xsd\" \
         identifier=\"id-cadastre-model\">\n<name xml:lang=\"en\">Cadastre</name>\n<elements>\n",
    );
    for id in registry.ids() {
        let resource = registry.resource(id);
        let name = resource
            .attribute("label")
            .map(|a| crate::query::value_text(&a.value))
            .unwrap_or_else(|| resource.name.clone());
        let mut pairs = vec![
            ("kind".to_owned(), resource.kind.clone()),
            ("qualified_name".to_owned(), registry.qualified_name(id)),
        ];
        for attribute in resource.attributes.iter().filter(|a| a.key != "description") {
            pairs.push((attribute.key.clone(), values(&attribute.value).join(", ")));
        }
        let documentation = resource
            .attribute("description")
            .map(|a| {
                format!("<documentation xml:lang=\"en\">{}</documentation>", xml(&crate::query::value_text(&a.value)))
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "<element identifier=\"{}\" xsi:type=\"{}\"><name xml:lang=\"en\">{}</name>{documentation}{}</element>",
            ids[&id],
            element_of(registry, id),
            xml(&name),
            properties(&pairs)
        );
    }
    out.push_str("</elements>\n<relationships>\n");

    let mut count = 0;
    let mut relationship = |out: &mut String,
                            relation: Relation,
                            source: ResourceId,
                            target: ResourceId,
                            name: &str,
                            pairs: &[(String, String)],
                            documentation: Option<String>| {
        count += 1;
        let kind = relationship_type(relation, element_of(registry, source), element_of(registry, target));
        let name = if name.is_empty() { String::new() } else { format!("<name xml:lang=\"en\">{}</name>", xml(name)) };
        let documentation = documentation
            .map(|d| format!("<documentation xml:lang=\"en\">{}</documentation>", xml(&d)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "<relationship identifier=\"rel-{count}\" source=\"{}\" target=\"{}\" xsi:type=\"{kind}\">{name}{documentation}{}</relationship>",
            ids[&source],
            ids[&target],
            properties(pairs)
        );
    };
    for &(child, parent) in &model.containment {
        relationship(&mut out, Relation::Containment, parent, child, "", &[], None);
    }
    for (holder, key, target) in &model.references {
        // `on`: the host serves (or is assigned to) what runs on it.
        if key == "on" {
            relationship(&mut out, Relation::On, *target, *holder, "", &[], None);
        } else {
            relationship(&mut out, Relation::Other, *holder, *target, key, &[], None);
        }
    }
    for link in &model.links {
        let written = &registry.links[link.index];
        let mut pairs = Vec::new();
        if let Some(port) = &link.port {
            pairs.push(("port".to_owned(), port.clone()));
        }
        if !link.layers.is_empty() {
            pairs.push(("layers".to_owned(), link.layers.join(", ")));
        }
        for attribute in written.attributes.iter().filter(|a| a.key != "description") {
            pairs.push((attribute.key.clone(), values(&attribute.value).join(", ")));
        }
        let documentation = written.attribute("description").map(|a| crate::query::value_text(&a.value));
        let name = link.port.clone().unwrap_or_default();
        relationship(&mut out, Relation::Flow, link.source, link.target, &name, &pairs, documentation);
    }
    out.push_str("</relationships>\n<propertyDefinitions>\n");
    for (i, key) in keys.iter().enumerate() {
        let _ = writeln!(
            out,
            "<propertyDefinition identifier=\"pd-{}\" type=\"string\"><name xml:lang=\"en\">{}</name></propertyDefinition>",
            i + 1,
            xml(key)
        );
    }
    out.push_str("</propertyDefinitions>\n</model>\n");
    out
}

// ---------------------------------------------------------------------------
// GraphML
// ---------------------------------------------------------------------------

fn graphml(compilation: &Compilation, model: &Model) -> String {
    let registry = &compilation.registry;
    let ids = identifiers(registry);
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xsi:schemaLocation=\"http://graphml.graphdrawing.org/xmlns http://graphml.graphdrawing.org/xmlns/1.0/graphml.xsd\">\n\
         <key id=\"kind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/>\n\
         <key id=\"name\" for=\"node\" attr.name=\"name\" attr.type=\"string\"/>\n\
         <key id=\"label\" for=\"node\" attr.name=\"label\" attr.type=\"string\"/>\n\
         <key id=\"description\" for=\"all\" attr.name=\"description\" attr.type=\"string\"/>\n\
         <key id=\"relation\" for=\"edge\" attr.name=\"relation\" attr.type=\"string\"/>\n\
         <key id=\"port\" for=\"edge\" attr.name=\"port\" attr.type=\"string\"/>\n\
         <key id=\"layers\" for=\"edge\" attr.name=\"layers\" attr.type=\"string\"/>\n\
         <key id=\"carries\" for=\"edge\" attr.name=\"carries\" attr.type=\"string\"/>\n\
         <graph id=\"cadastre\" edgedefault=\"directed\">\n",
    );
    let text = |id: ResourceId, key: &str| {
        registry.resource(id).attribute(key).map(|a| values(&a.value).join(", ")).unwrap_or_default()
    };
    for id in registry.ids() {
        let resource = registry.resource(id);
        let _ = write!(
            out,
            "<node id=\"{}\"><data key=\"kind\">{}</data><data key=\"name\">{}</data><data key=\"label\">{}</data>",
            ids[&id],
            xml(&resource.kind),
            xml(&registry.qualified_name(id)),
            xml(&if text(id, "label").is_empty() { resource.name.clone() } else { text(id, "label") })
        );
        let description = text(id, "description");
        if !description.is_empty() {
            let _ = write!(out, "<data key=\"description\">{}</data>", xml(&description));
        }
        out.push_str("</node>\n");
    }
    let mut count = 0;
    let mut edge = |out: &mut String, source: ResourceId, target: ResourceId, data: &[(&str, String)]| {
        count += 1;
        let _ = write!(out, "<edge id=\"e{count}\" source=\"{}\" target=\"{}\">", ids[&source], ids[&target]);
        for (key, value) in data.iter().filter(|(_, v)| !v.is_empty()) {
            let _ = write!(out, "<data key=\"{key}\">{}</data>", xml(value));
        }
        out.push_str("</edge>\n");
    };
    for &(child, parent) in &model.containment {
        edge(&mut out, child, parent, &[("relation", "parent".to_owned())]);
    }
    for (holder, key, target) in &model.references {
        edge(&mut out, *holder, *target, &[("relation", key.clone())]);
    }
    for link in &model.links {
        let written = &registry.links[link.index];
        let carries: Vec<String> = compilation
            .model
            .reference(Holder::Link(link.index), "carries")
            .iter()
            .map(|&d| registry.qualified_name(d))
            .collect();
        let description =
            written.attribute("description").map(|a| crate::query::value_text(&a.value)).unwrap_or_default();
        edge(
            &mut out,
            link.source,
            link.target,
            &[
                ("relation", "link".to_owned()),
                ("port", link.port.clone().unwrap_or_default()),
                ("layers", link.layers.join(", ")),
                ("carries", carries.join(", ")),
                ("description", description),
            ],
        );
    }
    out.push_str("</graph>\n</graphml>\n");
    out
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------
