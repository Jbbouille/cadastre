//! Sequence diagrams of the processes of a view (`render sequence`): a Mermaid `sequenceDiagram`
//! from the steps of each process the view selects (`edges process(orders)`).

use std::fmt::Write;

use super::{Scene, node_id};
use crate::check::Holder;
use crate::process::{Event, ResolvedStep};
use crate::registry::ResourceId;

pub fn sequence(scene: &Scene) -> String {
    let mut out = String::from("sequenceDiagram\n  autonumber\n");
    if scene.graph.processes.is_empty() {
        out.push_str("  %% no process: the view selects none (edges process(…))\n");
        return out;
    }
    let events: Vec<&Event> = scene
        .graph
        .processes
        .iter()
        .flat_map(|p| scene.model.processes.get(p).map(Vec::as_slice).unwrap_or_default())
        .collect();

    // The participants: those the processes list (`participants`), in this order, then the
    // others in their order of appearance.
    let mut participants: Vec<ResourceId> = Vec::new();
    let listed = scene.graph.processes.iter().flat_map(|&p| scene.model.reference(Holder::Resource(p), "participants"));
    let appearing =
        events.iter().filter_map(|e| e.step()).flat_map(|step| std::iter::once(step.source).chain(step.target));
    for id in listed.copied().chain(appearing) {
        if !participants.contains(&id) {
            participants.push(id);
        }
    }
    if let [process] = scene.graph.processes.as_slice()
        && scene.graph.options.title
    {
        let _ = writeln!(out, "  title {}", text(&scene.node_lines(*process).join(" ")));
    }
    for &id in &participants {
        let shown = crate::view::attribute_text(&scene.ctx(), id, "sequence_label").into_iter().next();
        let shown = shown.or_else(|| participant_text(scene, id)).unwrap_or_else(|| scene.node_lines(id).join("<br/>"));
        let _ = writeln!(out, "  participant {} as {}", node_id(id), text(&shown));
        // On the site: the page of the resource, in the menu of its participant.
        if let Some(link) = scene.node_link(id) {
            let _ = writeln!(out, "  link {}: {} @ {link}", node_id(id), scene.t("Page"));
        }
    }

    let mut depth = 1;
    for event in events {
        let indent = "  ".repeat(depth);
        match event {
            Event::LoopStart(label) => {
                let _ = writeln!(out, "{indent}loop {}", text(label));
                depth += 1;
            }
            Event::LoopEnd => {
                depth = depth.saturating_sub(1).max(1);
                let _ = writeln!(out, "{}end", "  ".repeat(depth));
            }
            Event::Step(step) => {
                let from = node_id(step.source);
                let to = step.target.map_or_else(|| from.clone(), node_id);
                let arrow = if step.reply {
                    "-->>"
                } else if step.asynchronous {
                    "-)"
                } else {
                    "->>"
                };
                let _ = writeln!(out, "{indent}{from}{arrow}{to}: {}", text(&message(scene, step)));
                if let Some(note) = &step.note {
                    let place = if from == to { format!("right of {from}") } else { format!("over {from},{to}") };
                    let _ = writeln!(out, "{indent}Note {place}: {}", text(note));
                }
            }
        }
    }
    out
}

/// The text of a participant of a process standing for several resources: its `label`, otherwise
/// its members as written (`esx1, esx2`); `None` for a resource.
pub(super) fn participant_text(scene: &Scene, id: ResourceId) -> Option<String> {
    if scene.registry.resource(id).kind != "participant" {
        return None;
    }
    if let Some(label) = crate::view::attribute_text(&scene.ctx(), id, "label").into_iter().next() {
        return Some(label);
    }
    let members = scene.model.reference(Holder::Resource(id), "members");
    Some(members.iter().map(|&m| scene.registry.resource(m).name.as_str()).collect::<Vec<_>>().join(", "))
}

/// The text of a step: the text of a processing; its `label`, followed by the protocol of its
/// link when the view shows the ports (`edge_labels: [port]`, the default); otherwise the protocol
/// of its link and the data it carries.
pub(super) fn message(scene: &Scene, step: &ResolvedStep) -> String {
    if let Some(text) = &step.text {
        return text.clone();
    }
    // The links of a step to a participant standing for several resources are alike: the first.
    let index = step.links.first().copied();
    let link = index.map(|index| &scene.registry.links[index]);
    // A reply: what it brings back, the port being that of the call.
    let port = link.and_then(|l| l.port.as_ref()).filter(|_| !step.reply).map(|p| protocol(&p.normalized()));
    if let Some(label) = &step.label {
        let shown = port.filter(|_| scene.graph.options.edge_labels.iter().any(|k| k == "port"));
        return match shown {
            Some(port) => format!("{label} ({port})"),
            None => label.clone(),
        };
    }
    let (Some(index), Some(link)) = (index, link) else { return String::new() };
    let carries: Vec<&str> = scene
        .model
        .reference(Holder::Link(index), "carries")
        .iter()
        .map(|&d| scene.registry.resource(d).name.as_str())
        .collect();
    let carries = (!carries.is_empty()).then(|| carries.join(", "));
    let parts: Vec<String> = port.into_iter().chain(carries).collect();
    match (parts.is_empty(), step.reply) {
        (false, _) => parts.join(" · "),
        (true, true) => "reply".to_owned(),
        // Nothing else to say: its layers (`trigger`).
        (true, false) => link.layers.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>().join(", "),
    }
}

/// The protocol of a link as a sequence diagram shows it: an acronym in capitals, a name as it is
/// usually written, followed by its port unless it is the usual one of the protocol: `JDBC`,
/// `HTTPS`, `HTTP 8080`, `OpenEdge`.
fn protocol(normalized: &str) -> String {
    const ACRONYMS: &[&str] = &[
        "http", "https", "jdbc", "odbc", "ssh", "sftp", "ftp", "ftps", "smtp", "ldap", "ldaps", "tcp", "udp", "amqp",
        "mqtt", "snmp", "rdp", "smb", "nfs", "dns", "ntp",
    ];
    const NAMES: &[(&str, &str)] =
        &[("openedge", "OpenEdge"), ("hessian", "Hessian"), ("kafka", "Kafka"), ("grpc", "gRPC"), ("oracle", "Oracle")];
    const USUAL: &[(&str, &str)] = &[
        ("http", "80"),
        ("https", "443"),
        ("ssh", "22"),
        ("sftp", "22"),
        ("ftp", "21"),
        ("smtp", "25"),
        ("ldap", "389"),
        ("ldaps", "636"),
        ("jdbc", "1521"),
        ("kafka", "9092"),
    ];
    let (name, number) = normalized.split_once('/').map_or((normalized, None), |(n, p)| (n, Some(p)));
    let shown = match NAMES.iter().find(|(n, _)| *n == name) {
        Some((_, written)) => (*written).to_owned(),
        None if ACRONYMS.contains(&name) => name.to_uppercase(),
        None => name.to_owned(),
    };
    match number.filter(|&p| !USUAL.contains(&(name, p))) {
        Some(number) => format!("{shown} {number}"),
        None => shown,
    }
}

/// Text in a Mermaid sequence diagram: no line break, no `;` nor `#` (its separators and escapes).
fn text(value: &str) -> String {
    value.replace('\n', "<br/>").replace(';', ",").replace('#', "")
}

#[cfg(test)]
mod tests {
    use super::{protocol, text};

    #[test]
    fn protocols_hide_their_usual_port() {
        assert_eq!(protocol("jdbc"), "JDBC");
        assert_eq!(protocol("jdbc/1521"), "JDBC");
        assert_eq!(protocol("https/443"), "HTTPS");
        assert_eq!(protocol("http/8080"), "HTTP 8080");
        assert_eq!(protocol("openedge/15008"), "OpenEdge 15008");
        assert_eq!(protocol("s3"), "s3");
    }

    #[test]
    fn texts_are_escaped() {
        assert_eq!(text("a;b\n#c"), "a,b<br/>c");
    }
}
