//! Traversals (`render traversal`): an SVG with a row per step of the processes the view selects
//! (`edges process(backup)`), then a row per path between two resources (`edges path(proxy,
//! esx02)`), from the source to the target through the devices traversed, a box at each
//! traversal: a switch crossed by two steps appears in both rows.
//!
//! The devices of a step are those of the `through` of its link (written, or deduced from the
//! cabling), in the other order for a reply; without a link, and for a path, those of the
//! cabling between its ends. Redundant devices make one box (`col-sw10-01`, `or col-sw10-02`).

use std::collections::BTreeSet;
use std::fmt::Write;

use super::draw::{
    CHAR_WIDTH, CYLINDER_CAP, LINE_HEIGHT, PADDING, dash, icon_use, kind_of, marker_id, outer_size, shape, text,
    text_size,
};
use super::svg::{NODE_GAP, NODE_ICON, icon_symbols};
use super::{Line, NodeStyle, Scene, html, is_dark};
use crate::check::Holder;
use crate::path::{Network, over_internet};
use crate::process::{Event, ResolvedStep};
use crate::registry::ResourceId;

const MARGIN: f64 = 10.0;
/// Space between the header of a row and its boxes, and between rows.
const ROW_GAP: (f64, f64) = (8.0, 26.0);
/// The shortest arrow between two boxes.
const ARROW: f64 = 44.0;

/// A box of a row: a device (with its variants), or an end of the step.
struct Hop {
    /// What it stands for: the first one gives its style, its icon, its tooltip.
    ids: Vec<ResourceId>,
    /// The devices whose cables lead to the next box: the hosts of a machine for an end.
    devices: Vec<ResourceId>,
    lines: Vec<String>,
}

/// A step or a path: its header, its boxes, the labels of the arrows between them.
struct Row {
    header: String,
    hops: Vec<Hop>,
    labels: Vec<Vec<String>>,
    /// Over the internet: the first arrow, the last one.
    internet: (bool, bool),
    reply: bool,
}

pub fn traversal(scene: &Scene) -> String {
    let options = &scene.graph.options;
    let (node_size, edge_size) = (options.node_label_size, options.edge_label_size);
    let network = Network::new(scene.registry, scene.model);
    let interfaces = options.edge_labels.iter().any(|k| k == "interface");

    // The steps, numbered as in a sequence diagram: a processing has a number, but no row.
    let mut rows = Vec::new();
    let mut number = 0;
    for event in scene.graph.processes.iter().flat_map(|p| scene.model.processes.get(p).into_iter().flatten()) {
        let Event::Step(step) = event else { continue };
        number += 1;
        let Some(target) = step.target else { continue };
        let message = super::sequence::message(scene, step);
        let header = if message.is_empty() { format!("{number}.") } else { format!("{number}. {message}") };
        let steps = devices(scene, &network, step);
        rows.push(row(scene, &network, interfaces, header, (step.source, target), steps, step.reply));
    }
    // The paths: their ends, named as in the boxes; why there is none.
    for &(a, b) in &scene.graph.paths {
        let name = |id: ResourceId| scene.node_lines(id).into_iter().next().unwrap_or_default();
        let header = format!("{} → {}", name(a), name(b));
        let (header, steps) = match network.between(a, b) {
            Ok(path) => (header, path.steps),
            Err(error) => (format!("{header}: no path, {}", network.explain(&error)), Vec::new()),
        };
        rows.push(row(scene, &network, interfaces, header, (a, b), steps, false));
    }

    let mut body = String::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut markers: Vec<(String, bool)> = Vec::new();
    let mut top = 0.0;
    let mut right: f64 = 0.0;
    if let [process] = scene.graph.processes.as_slice()
        && options.title
    {
        let title = scene.node_lines(*process).join(" ");
        let _ = writeln!(
            body,
            "<text x=\"0\" y=\"{:.1}\" font-size=\"{:.1}\" font-weight=\"bold\">{}</text>",
            node_size * 1.3,
            node_size * 1.3,
            html(&title)
        );
        right = right.max(title.chars().count() as f64 * node_size * 1.3 * CHAR_WIDTH);
        top += node_size * 1.3 * LINE_HEIGHT + ROW_GAP.1 / 2.0;
    }
    if rows.is_empty() {
        let note = if scene.graph.processes.is_empty() && scene.graph.paths.is_empty() {
            "nothing to traverse: the view selects no process (edges process(…)) and no path (edges path(a, b))"
        } else {
            "no step between two participants"
        };
        let _ = writeln!(
            body,
            "<text x=\"0\" y=\"{:.1}\" font-size=\"{node_size}\">{}</text>",
            top + node_size,
            html(note)
        );
        right = right.max(note.chars().count() as f64 * node_size * CHAR_WIDTH);
        top += node_size * LINE_HEIGHT;
    }
    for row in &rows {
        let _ = writeln!(
            body,
            "<text x=\"0\" y=\"{:.1}\" font-size=\"{node_size}\" font-weight=\"bold\">{}</text>",
            top + node_size,
            html(&row.header)
        );
        right = right.max(row.header.chars().count() as f64 * node_size * CHAR_WIDTH);
        top += node_size * LINE_HEIGHT + ROW_GAP.0;

        let styles: Vec<NodeStyle> = row.hops.iter().map(|h| scene.node_style(h.ids[0])).collect();
        let kinds: Vec<Option<String>> = row.hops.iter().map(|h| kind_of(scene, h.ids[0])).collect();
        let sizes: Vec<(f64, f64)> = row
            .hops
            .iter()
            .zip(&styles)
            .zip(&kinds)
            .map(|((hop, style), kind)| {
                let (w, h) = text_size(&hop.lines, node_size);
                let icon = kind.as_ref().map_or(0.0, |_| NODE_ICON + NODE_GAP);
                outer_size(&style.shape, w + icon + 2.0 * PADDING.0, h.max(NODE_ICON) + 2.0 * PADDING.1)
            })
            .collect();
        let widths: Vec<f64> = row
            .labels
            .iter()
            .map(|lines| {
                let (w, _) = text_size(lines, edge_size);
                (w + 16.0).max(ARROW)
            })
            .collect();
        // The arrows above the middle of the row leave room for their labels.
        let above = row.labels.iter().map(|l| l.len()).max().unwrap_or(0) as f64 * edge_size * LINE_HEIGHT;
        let height = sizes.iter().map(|s| s.1).fold(0.0, f64::max).max(2.0 * above + 8.0);
        let middle = top + height / 2.0;

        let mut x = 0.0;
        for (i, hop) in row.hops.iter().enumerate() {
            let (w, h) = sizes[i];
            let style = &styles[i];
            let (bx, by) = (x, middle - h / 2.0);
            let link = scene.node_link(hop.ids[0]);
            if let Some(link) = &link {
                let _ = writeln!(body, "<a xlink:href=\"{}\" href=\"{0}\" target=\"_blank\">", html(link));
            }
            let _ = writeln!(body, "<g><title>{}</title>", html(&scene.node_tooltip(hop.ids[0])));
            let stroke = style.color.as_deref().unwrap_or("black");
            shape(&mut body, &style.shape, bx, by, w, h, style.fill.as_deref().unwrap_or("white"), stroke, style.line);
            let text_color = if style.fill.as_deref().is_some_and(is_dark) { "white" } else { "black" };
            let (text_width, text_height) = text_size(&hop.lines, node_size);
            let icon = kinds[i].as_ref().map_or(0.0, |_| NODE_ICON + NODE_GAP);
            let left = bx + (w - text_width - icon) / 2.0;
            let center = if style.shape == "cylinder" { middle + CYLINDER_CAP / 2.0 } else { middle };
            if let Some(kind) = &kinds[i] {
                let color = if text_color == "white" { "white" } else { stroke };
                icon_use(&mut body, kind, left, center - NODE_ICON / 2.0, NODE_ICON, color);
                used.insert(kind.clone());
            }
            text(
                &mut body,
                &hop.lines,
                left + icon + text_width / 2.0,
                center - text_height / 2.0 + node_size,
                node_size,
                text_color,
            );
            body.push_str("</g>\n");
            if link.is_some() {
                body.push_str("</a>\n");
            }
            x += w;
            let Some(&gap) = widths.get(i) else { break };
            // The arrow to the next box; over the internet, dotted; a reply, dashed.
            let internet = (i == 0 && row.internet.0) || (i + 2 == row.hops.len() && row.internet.1);
            let line = if internet {
                Line::Dotted
            } else if row.reply {
                Line::Dashed
            } else {
                Line::Solid
            };
            let marker = marker_id(&mut markers, "black", false);
            let _ = writeln!(
                body,
                "<line x1=\"{x:.1}\" y1=\"{middle:.1}\" x2=\"{:.1}\" y2=\"{middle:.1}\" stroke=\"black\"{} marker-end=\"url(#{marker})\"/>",
                x + gap,
                dash(line)
            );
            let labels = &row.labels[i];
            let shown: Vec<String> =
                if internet && labels.is_empty() { vec!["internet".to_owned()] } else { labels.clone() };
            let first = middle - 4.0 - (shown.len() as f64 - 1.0) * edge_size * LINE_HEIGHT;
            text(&mut body, &shown, x + gap / 2.0, first, edge_size, "#444444");
            x += gap;
        }
        right = right.max(x);
        top += height + ROW_GAP.1;
    }

    let (width, height) = (right + 2.0 * MARGIN, top - ROW_GAP.1 + 2.0 * MARGIN);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width:.0}pt\" height=\"{height:.0}pt\" \
         viewBox=\"{m} {m} {width:.0} {height:.0}\" font-family=\"Helvetica, Arial, sans-serif\">\n<title>{}</title>\n<defs>\n",
        html(&scene.graph.view),
        m = -MARGIN,
    );
    out.push_str(&icon_symbols(&used));
    for (index, (color, hollow)) in markers.iter().enumerate() {
        let fill = if *hollow { "white" } else { color.as_str() };
        let _ = writeln!(
            out,
            "<marker id=\"arrow{index}\" viewBox=\"0 0 10 10\" refX=\"10\" refY=\"5\" markerWidth=\"7\" markerHeight=\"7\" \
             orient=\"auto-start-reverse\"><path d=\"M0,0 L10,5 L0,10 z\" fill=\"{fill}\" stroke=\"{color}\"/></marker>"
        );
    }
    let _ = writeln!(
        out,
        "</defs>\n<rect x=\"{m}\" y=\"{m}\" width=\"{width:.0}\" height=\"{height:.0}\" fill=\"white\"/>",
        m = -MARGIN
    );
    out.push_str(&body);
    out.push_str("</svg>\n");
    out
}

/// A row from a source to a target through the devices traversed.
fn row(
    scene: &Scene,
    network: &Network,
    interfaces: bool,
    header: String,
    (source, target): (ResourceId, ResourceId),
    steps: Vec<Vec<ResourceId>>,
    reply: bool,
) -> Row {
    let mut hops = vec![end(scene, network, source)];
    hops.extend(steps.into_iter().map(|ids| device(scene, ids)));
    hops.push(end(scene, network, target));
    let labels = hops
        .windows(2)
        .map(|pair| if interfaces { cables(scene, network, &pair[0], &pair[1]) } else { Vec::new() })
        .collect();
    let internet =
        (over_internet(scene.registry, scene.model, source), over_internet(scene.registry, scene.model, target));
    Row { header, hops, labels, internet, reply }
}

/// The devices a step goes through, each with its variants.
fn devices(scene: &Scene, network: &Network, step: &ResolvedStep) -> Vec<Vec<ResourceId>> {
    // The links of a step to a participant standing for several resources are alike: the first.
    let mut steps = match step.links.first() {
        Some(&index) => {
            let flat = || scene.model.reference(Holder::Link(index), "through").iter().map(|&d| vec![d]).collect();
            if scene.registry.links[index].attribute("through").is_some() {
                flat()
            } else {
                scene.model.through_steps.get(&index).cloned().unwrap_or_else(flat)
            }
        }
        None => step.target.and_then(|t| network.between(step.source, t).ok()).map(|p| p.steps).unwrap_or_default(),
    };
    // A reply goes back the way of its call.
    if step.reply {
        steps.reverse();
    }
    steps
}

/// The box of an end of a step: its text, then the machines it runs on (`on proxy01`,
/// `on esx-01` for a virtual machine on its host).
fn end(scene: &Scene, network: &Network, id: ResourceId) -> Hop {
    let mut lines = super::sequence::participant_text(scene, id).map_or_else(|| scene.node_lines(id), |t| vec![t]);
    let participant = scene.registry.resource(id).kind == "participant";
    let machines = network.machines(id);
    if !participant {
        let mut current = id;
        for _ in 0..4 {
            let on = scene.model.reference(Holder::Resource(current), "on");
            let [host] = on else { break };
            lines.push(format!("on {}", scene.registry.resource(*host).name));
            current = *host;
        }
    }
    let devices = machines.iter().flat_map(|&m| network.cabled(m)).collect();
    Hop { ids: vec![id], devices, lines }
}

/// The box of a device with its variants: `col-sw10-01`, `or col-sw10-02`.
fn device(scene: &Scene, ids: Vec<ResourceId>) -> Hop {
    let mut lines = scene.node_lines(ids[0]);
    lines.extend(ids[1..].iter().map(|&v| format!("or {}", scene.registry.resource(v).name)));
    Hop { devices: ids.clone(), ids, lines }
}

/// The interfaces of the cables between two boxes (`edge_labels: [interface]`): `ge-0/0/1 → eth0`,
/// a server without interfaces counting for its own; the interfaces of redundant cables to a
/// same one together, `p1, p2 → eth1-11` (or from a same one, `eth1-12 → p1, p2`).
fn cables(scene: &Scene, network: &Network, from: &Hop, to: &Hop) -> Vec<String> {
    let plug = |p: ResourceId| {
        let resource = scene.registry.resource(p);
        (scene.registry.schema.base(&resource.kind) == "interface").then(|| resource.name.clone())
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (a, b) in network.cables_between(&from.devices, &to.devices) {
        let (a, b) = (plug(a).unwrap_or_default(), plug(b).unwrap_or_default());
        if !(a.is_empty() && b.is_empty()) && !pairs.contains(&(a.clone(), b.clone())) {
            pairs.push((a, b));
        }
    }
    // Grouped by the interface reached, or by the one left: the shorter.
    let grouped = |by_target: bool| {
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for (a, b) in &pairs {
            let (key, other) = if by_target { (b, a) } else { (a, b) };
            match groups.iter_mut().find(|(k, _)| k == key) {
                Some((_, others)) => others.push(other.clone()),
                None => groups.push((key.clone(), vec![other.clone()])),
            }
        }
        groups
            .into_iter()
            .map(|(key, others)| {
                let others = others.join(", ");
                let (a, b) = if by_target { (others, key) } else { (key, others) };
                match (a.is_empty(), b.is_empty()) {
                    (false, false) => format!("{a} → {b}"),
                    (false, true) => a,
                    _ => format!("→ {b}"),
                }
            })
            .collect::<Vec<String>>()
    };
    let (to_one, from_one) = (grouped(true), grouped(false));
    if from_one.len() < to_one.len() { from_one } else { to_one }
}
