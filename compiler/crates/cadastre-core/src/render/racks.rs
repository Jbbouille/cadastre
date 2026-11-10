//! Rack elevations (`render racks`): an SVG with a column per rack, a box per device at its
//! height, the racks of a site side by side under its name.
//!
//! The racks drawn are those the view selects, with everything placed in them (`rack: r12`), and
//! those of the devices it selects, with these devices only. A device is placed by `u`, its
//! lowest unit, numbered from 1 at the bottom, and `height`, its number of units (1 by default);
//! a rack has `units` units (42 by default). A device without `u` is listed under its rack.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::draw::{CHAR_WIDTH, LINE_HEIGHT};
use super::{Scene, html, is_dark};
use crate::check::Holder;
use crate::registry::ResourceId;

const MARGIN: f64 = 10.0;
/// Height of a unit, width of a rack inside its frame, of the column of unit numbers.
const UNIT: f64 = 16.0;
const WIDTH: f64 = 190.0;
const NUMBERS: f64 = 22.0;
/// Space between two racks, between a site and the next.
const GAP: (f64, f64) = (28.0, 34.0);
const UNITS: u32 = 42;

/// A rack and what is drawn in it.
struct Rack {
    id: ResourceId,
    units: u32,
    /// Placed: the device, its lowest unit, its height.
    placed: Vec<(ResourceId, u32, u32)>,
    unplaced: Vec<ResourceId>,
}

/// The SVG of the racks of a view, and what is wrong in them: two devices on one unit, a device
/// beyond the top of its rack.
pub fn racks(scene: &Scene) -> (String, Vec<String>) {
    let registry = scene.registry;
    let rack_kinds = registry.schema.expand("rack");
    let rack_of = |id: ResourceId| scene.model.reference(Holder::Resource(id), "rack").first().copied();
    let number = |id: ResourceId, key: &str| -> Option<u32> {
        let value = registry.resource(id).attribute(key)?;
        crate::query::value_text(&value.value).trim().parse().ok()
    };

    // The racks, in the order of the registry; what each holds.
    let mut contents: BTreeMap<ResourceId, Vec<ResourceId>> = BTreeMap::new();
    for &id in &scene.graph.nodes {
        if rack_kinds.contains(&registry.resource(id).kind.as_str()) {
            let mut held: Vec<ResourceId> = scene
                .model
                .references
                .iter()
                .filter_map(|((holder, key), targets)| match holder {
                    Holder::Resource(r) if key == "rack" && targets.contains(&id) => Some(*r),
                    _ => None,
                })
                .collect();
            held.sort();
            contents.insert(id, held);
        }
    }
    for &id in &scene.graph.nodes {
        if let Some(rack) = rack_of(id)
            && !scene.graph.nodes.contains(&rack)
        {
            contents.entry(rack).or_default().push(id);
        }
    }

    let mut problems = Vec::new();
    let name = |id: ResourceId| registry.resource(id).name.clone();
    let racks: Vec<Rack> = contents
        .into_iter()
        .map(|(id, held)| {
            let mut rack = Rack {
                id,
                units: number(id, "units").filter(|&u| u > 0).unwrap_or(UNITS),
                placed: Vec::new(),
                unplaced: Vec::new(),
            };
            for device in held {
                match number(device, "u").filter(|&u| u > 0) {
                    Some(u) => rack.placed.push((device, u, number(device, "height").filter(|&h| h > 0).unwrap_or(1))),
                    None => rack.unplaced.push(device),
                }
            }
            rack.placed.sort_by_key(|&(device, u, _)| (u, device));
            for (i, &(a, u, h)) in rack.placed.iter().enumerate() {
                if u + h - 1 > rack.units {
                    problems.push(format!(
                        "{} in {}: units {u} to {} beyond the {} units of the rack",
                        name(a),
                        name(id),
                        u + h - 1,
                        rack.units
                    ));
                }
                for &(b, v, k) in &rack.placed[i + 1..] {
                    if overlap((u, h), (v, k)) {
                        problems.push(format!("{} and {} in {}: on the same units", name(a), name(b), name(id)));
                    }
                }
            }
            rack
        })
        .collect();

    // The racks of a site side by side, the sites one under the other.
    let mut sites: Vec<(Option<ResourceId>, Vec<&Rack>)> = Vec::new();
    for rack in &racks {
        let site = registry.ancestors(rack.id).into_iter().find(|&a| registry.resource(a).kind == "site");
        match sites.iter_mut().find(|(s, _)| *s == site) {
            Some((_, list)) => list.push(rack),
            None => sites.push((site, vec![rack])),
        }
    }

    let size = scene.graph.options.node_label_size;
    let overlapping: Vec<ResourceId> = racks
        .iter()
        .flat_map(|rack| {
            rack.placed.iter().filter(|&&(a, u, h)| {
                rack.placed.iter().any(|&(b, v, k)| a != b && overlap((u, h), (v, k))) || u + h - 1 > rack.units
            })
        })
        .map(|&(a, _, _)| a)
        .collect();
    let mut body = String::new();
    let mut top = 0.0;
    let mut right: f64 = 0.0;
    if racks.is_empty() {
        let note = "no rack: the view selects no rack (select rack) and no device placed in one (rack: r12)";
        let _ = writeln!(body, "<text x=\"0\" y=\"{:.1}\" font-size=\"{size}\">{}</text>", top + size, html(note));
        right = right.max(note.chars().count() as f64 * size * CHAR_WIDTH);
        top += size * LINE_HEIGHT;
    }
    for (site, list) in &sites {
        if let Some(site) = site {
            let label = scene.node_lines(*site).join(" ");
            let _ = writeln!(
                body,
                "<text x=\"0\" y=\"{:.1}\" font-size=\"{:.1}\" font-weight=\"bold\" fill=\"#444444\">{}</text>",
                top + size * 1.15,
                size * 1.15,
                html(&label)
            );
            top += size * 1.15 * LINE_HEIGHT + 6.0;
        }
        let mut x = 0.0;
        let mut bottom: f64 = top;
        for rack in list {
            let y = draw_rack(&mut body, scene, rack, x, top, size, &overlapping);
            bottom = bottom.max(y);
            x += NUMBERS + WIDTH + GAP.0;
        }
        right = right.max(x - GAP.0);
        top = bottom + GAP.1;
    }

    // The gap after the last site is not drawn.
    let bottom = if sites.is_empty() { top } else { top - GAP.1 };
    let (width, height) = (right + 2.0 * MARGIN, bottom + 2.0 * MARGIN);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width:.0}pt\" height=\"{height:.0}pt\" \
         viewBox=\"{m} {m} {width:.0} {height:.0}\" font-family=\"Helvetica, Arial, sans-serif\">\n<title>{}</title>\n\
         <rect x=\"{m}\" y=\"{m}\" width=\"{width:.0}\" height=\"{height:.0}\" fill=\"white\"/>\n",
        html(&scene.graph.view),
        m = -MARGIN,
    );
    out.push_str(&body);
    // Dark where it is shown dark, as the drawings of Graphviz.
    out.push_str(super::svg::DARK_STYLE);
    out.push_str("\n</svg>\n");
    (out, problems)
}

/// A rack at `(x, top)`: its name, its frame with the numbers of its units, its devices, then the
/// devices not placed. Returns the bottom of what it drew.
fn draw_rack(
    out: &mut String,
    scene: &Scene,
    rack: &Rack,
    x: f64,
    top: f64,
    size: f64,
    overlapping: &[ResourceId],
) -> f64 {
    let registry = scene.registry;
    let resource = registry.resource(rack.id);
    let mut header = resource.name.clone();
    if let Some(room) = resource.attribute("room") {
        header.push_str(&format!(" · {}", crate::query::value_text(&room.value)));
    }
    let left = x + NUMBERS;
    let _ = writeln!(
        out,
        "<g><title>{}</title><text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{size}\" font-weight=\"bold\" text-anchor=\"middle\">{}</text></g>",
        html(&scene.node_tooltip(rack.id)),
        left + WIDTH / 2.0,
        top + size,
        html(&header)
    );
    let frame = top + size * LINE_HEIGHT + 4.0;
    let height = f64::from(rack.units) * UNIT;
    let _ = writeln!(
        out,
        "<rect x=\"{left:.1}\" y=\"{frame:.1}\" width=\"{WIDTH:.1}\" height=\"{height:.1}\" fill=\"#f6f8fa\" stroke=\"#555555\" stroke-width=\"2\"/>"
    );
    // The units, numbered from the bottom.
    for unit in 1..=rack.units {
        let y = frame + f64::from(rack.units - unit) * UNIT;
        if unit < rack.units {
            let _ = writeln!(
                out,
                "<line x1=\"{left:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" stroke=\"#d0d7de\" stroke-width=\"0.5\"/>",
                left + WIDTH
            );
        }
        let _ = writeln!(
            out,
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" text-anchor=\"end\" fill=\"#888888\">{unit}</text>",
            left - 4.0,
            y + UNIT / 2.0 + size * 0.3,
            size * 0.8
        );
    }
    for &(device, u, h) in &rack.placed {
        // Clipped to the rack when beyond its top, drawn in red.
        let highest = (u + h - 1).min(rack.units);
        if u > rack.units {
            continue;
        }
        let y = frame + f64::from(rack.units - highest) * UNIT + 1.0;
        let box_height = f64::from(highest - u + 1) * UNIT - 2.0;
        let style = scene.node_style(device);
        let fill = style.fill.clone().unwrap_or_else(|| "#ddeaf7".to_owned());
        let stroke = if overlapping.contains(&device) {
            "#d1242f".to_owned()
        } else {
            style.color.clone().unwrap_or_else(|| "black".to_owned())
        };
        let text_color = if is_dark(&fill) { "white" } else { "black" };
        let link = scene.node_link(device);
        if let Some(link) = &link {
            let _ = writeln!(out, "<a xlink:href=\"{}\" href=\"{0}\" target=\"_blank\">", html(link));
        }
        let _ = writeln!(out, "<g><title>{}</title>", html(&scene.node_tooltip(device)));
        let _ = writeln!(
            out,
            "<rect x=\"{:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{box_height:.1}\" rx=\"2\" fill=\"{}\" stroke=\"{}\"/>",
            left + 3.0,
            WIDTH - 6.0,
            html(&fill),
            html(&stroke)
        );
        // As many lines of the node as the box holds, cut to its width.
        let lines = scene.node_lines(device);
        let fits = ((box_height / (size * LINE_HEIGHT)).floor() as usize).max(1);
        let chars = ((WIDTH - 16.0) / (size * CHAR_WIDTH)).floor() as usize;
        let shown: Vec<String> = lines.iter().take(fits).map(|l| cut(l, chars)).collect();
        let first = y + box_height / 2.0 - (shown.len() as f64 - 1.0) * size * LINE_HEIGHT / 2.0 + size * 0.35;
        for (i, line) in shown.iter().enumerate() {
            let _ = writeln!(
                out,
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{size}\" text-anchor=\"middle\" fill=\"{text_color}\">{}</text>",
                left + WIDTH / 2.0,
                first + i as f64 * size * LINE_HEIGHT,
                html(line)
            );
        }
        out.push_str("</g>\n");
        if link.is_some() {
            out.push_str("</a>\n");
        }
    }
    let mut bottom = frame + height;
    if !rack.unplaced.is_empty() {
        let names: Vec<String> = rack.unplaced.iter().map(|&id| registry.resource(id).name.clone()).collect();
        let chars = ((WIDTH + NUMBERS) / (size * CHAR_WIDTH)).floor() as usize;
        let mut lines = vec!["Not placed (no u):".to_owned()];
        for name in names {
            let joins = lines.len() > 1;
            match lines.last_mut() {
                Some(last) if joins && last.chars().count() + name.chars().count() + 2 <= chars => {
                    last.push_str(&format!(", {name}"))
                }
                _ => lines.push(name),
            }
        }
        for line in lines {
            bottom += size * LINE_HEIGHT;
            let _ = writeln!(
                out,
                "<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" fill=\"#666666\">{}</text>",
                bottom,
                size * 0.9,
                html(&line)
            );
        }
    }
    bottom
}

/// A line of text cut to a number of characters, an ellipsis at its end.
fn cut(line: &str, chars: usize) -> String {
    if line.chars().count() <= chars {
        return line.to_owned();
    }
    let mut cut: String = line.chars().take(chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// Whether two devices, each its lowest unit and its height, share a unit.
fn overlap((u, h): (u32, u32), (v, k): (u32, u32)) -> bool {
    v < u + h && u < v + k
}
