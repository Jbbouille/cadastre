//! draw.io: an editable diagram of a view (`.drawio`, opened by diagrams.net and the draw.io
//! desktop and VS Code apps).
//!
//! - Positions: from Graphviz, the `-Tjson` output of the DOT of the view, so that the diagram
//!   looks like the SVG; without it, from the layout of the ASCII art, scaled.
//! - Frames are draw.io containers holding their nodes: moving a frame moves its content.
//! - Edges are attached to their ends and routed by draw.io (orthogonal), without fixed points:
//!   moving a node pulls its edges.
//! - Shapes, colors, lines and icons are those of the SVG; each node and edge carries its data
//!   (qualified name, kind, attributes) as draw.io properties, shown by *Edit Data*.

use std::collections::HashMap;
use std::fmt::Write;

use super::{Arrow, Line, NodeStyle, Scene, ascii, base64, icons::icon, is_dark};
use crate::registry::ResourceId;

/// A box, in pixels, from the top left corner of the diagram.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Where the nodes and the frames are.
struct Placement {
    nodes: HashMap<ResourceId, Bounds>,
    frames: HashMap<usize, Bounds>,
}

/// Margin around the diagram, and height of the title band of a frame.
const MARGIN: f64 = 20.0;
const ICON: f64 = 16.0;

/// The draw.io file of a view; `layout` is Graphviz's `-Tjson` output for the DOT of the view.
pub fn drawio(scene: &Scene, layout: Option<&str>) -> String {
    let placement = layout.and_then(|json| from_graphviz(scene, json)).unwrap_or_else(|| from_cells(scene));
    write(scene, &placement)
}

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// Positions from Graphviz: nodes `n<id>` (`pos`, `width` and `height` in inches), frames
/// `cluster_<group>` (`bb`), in points with the origin at the bottom left.
fn from_graphviz(scene: &Scene, json: &str) -> Option<Placement> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let numbers = |text: &str| -> Vec<f64> { text.split(',').filter_map(|n| n.trim().parse().ok()).collect() };
    let bb = numbers(value.get("bb")?.as_str()?);
    let height = *bb.get(3)?;
    let mut placement = Placement { nodes: HashMap::new(), frames: HashMap::new() };
    for object in value.get("objects")?.as_array()? {
        let Some(name) = object.get("name").and_then(|n| n.as_str()) else { continue };
        if let Some(group) = name.strip_prefix("cluster_").and_then(|g| g.parse::<usize>().ok()) {
            let b = numbers(object.get("bb").and_then(|b| b.as_str()).unwrap_or_default());
            if let [x0, y0, x1, y1] = b[..] {
                placement.frames.insert(group, Bounds { x: x0, y: height - y1, width: x1 - x0, height: y1 - y0 });
            }
        } else if let Some(id) = name.strip_prefix('n').and_then(|n| n.parse::<u32>().ok()) {
            let id = ResourceId(id);
            let pos = numbers(object.get("pos").and_then(|p| p.as_str()).unwrap_or_default());
            let inches = |key: &str| object.get(key).and_then(|v| v.as_str()).and_then(|v| v.parse::<f64>().ok());
            if let ([x, y], Some(w), Some(h)) = (&pos[..], inches("width"), inches("height")) {
                let (w, h) = (w * 72.0, h * 72.0);
                placement.nodes.insert(id, Bounds { x: x - w / 2.0, y: height - y - h / 2.0, width: w, height: h });
            }
        }
    }
    // Every node must be placed: otherwise the layout is not that of this view.
    scene
        .graph
        .nodes
        .iter()
        .all(|id| placement.nodes.contains_key(id) || scene.node_frame(*id).is_some())
        .then_some(placement)
}

/// Positions from the layout of the ASCII art: a cell is 8 pixels wide and 18 high.
fn from_cells(scene: &Scene) -> Placement {
    let (boxes, frames) = ascii::cells(scene);
    let (cw, ch) = (8.0, 18.0);
    let bounds = |r: &ascii::Rect| Bounds {
        x: r.x0 as f64 * cw,
        y: r.y0 as f64 * ch,
        width: (r.x1 + 1 - r.x0) as f64 * cw,
        height: (r.y1 + 1 - r.y0) as f64 * ch,
    };
    Placement {
        nodes: scene.graph.nodes.iter().zip(&boxes).map(|(&id, r)| (id, bounds(r))).collect(),
        frames: frames.iter().enumerate().map(|(g, r)| (g, bounds(r))).collect(),
    }
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

fn write(scene: &Scene, placement: &Placement) -> String {
    let graph = scene.graph;
    // The diagram starts at the margin.
    let all = placement.nodes.values().chain(placement.frames.values());
    let (min_x, min_y) = all.fold((f64::MAX, f64::MAX), |(x, y), b| (x.min(b.x), y.min(b.y)));
    let (dx, dy) = if min_x == f64::MAX { (0.0, 0.0) } else { (MARGIN - min_x, MARGIN - min_y) };
    let absolute = |b: Bounds| Bounds { x: b.x + dx, y: b.y + dy, ..b };

    let mut out = String::new();
    let _ = write!(
        out,
        "<mxfile host=\"cadastre\">\n<diagram id=\"{0}\" name=\"{0}\">\n\
         <mxGraphModel grid=\"1\" gridSize=\"10\" guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" \
         fold=\"1\" page=\"0\" math=\"0\" shadow=\"0\">\n<root>\n<mxCell id=\"0\"/>\n<mxCell id=\"1\" parent=\"0\"/>\n",
        xml(&graph.view)
    );

    // Frames, the outer ones first: a cell comes after its container.
    let depth = |g: usize| std::iter::successors(Some(g), |&g| graph.groups[g].parent).count();
    let mut frames: Vec<usize> = (0..graph.groups.len()).filter(|g| placement.frames.contains_key(g)).collect();
    frames.sort_by_key(|&g| depth(g));
    let parent_of_frame = |g: usize| graph.groups[g].parent.filter(|p| placement.frames.contains_key(p));
    let origin = |container: Option<usize>| {
        container.and_then(|c| placement.frames.get(&c)).map_or((0.0, 0.0), |b| (b.x + dx, b.y + dy))
    };
    for &group in &frames {
        let bounds = absolute(placement.frames[&group]);
        let parent = parent_of_frame(group);
        let (ox, oy) = origin(parent);
        let resource = graph.groups[group].resource;
        let style = frame_style(
            &scene.frame_style(group),
            resource.map(|r| scene.registry.schema.base(&scene.registry.resource(r).kind)),
            scene.graph.options.frame_label_size,
        );
        let mut data = vec![("label".to_owned(), html_lines(&scene.frame_lines(group)))];
        if let Some(r) = resource {
            data.extend(resource_data(scene, r));
        }
        cell(
            &mut out,
            &format!("f{group}"),
            &data,
            &style,
            "vertex=\"1\"",
            &parent.map_or("1".to_owned(), |p| format!("f{p}")),
            Bounds { x: bounds.x - ox, y: bounds.y - oy, ..bounds },
        );
    }

    // Nodes, in their frame; a node drawn as a frame is that frame.
    for &id in &graph.nodes {
        if scene.node_frame(id).is_some() {
            continue;
        }
        let Some(&bounds) = placement.nodes.get(&id) else { continue };
        let bounds = absolute(bounds);
        let frame = graph.node_group.get(&id).copied().filter(|g| placement.frames.contains_key(g));
        let (ox, oy) = origin(frame);
        let kind = scene.registry.schema.base(&scene.registry.resource(id).kind);
        let mut data = vec![("label".to_owned(), html_lines(&scene.node_lines(id)))];
        data.extend(resource_data(scene, id));
        cell(
            &mut out,
            &cell_id(scene, id),
            &data,
            &node_style(&scene.node_style(id), kind, scene.graph.options.node_label_size),
            "vertex=\"1\"",
            &frame.map_or("1".to_owned(), |g| format!("f{g}")),
            Bounds { x: bounds.x - ox, y: bounds.y - oy, ..bounds },
        );
    }

    // Edges, attached to their ends. The edges leaving (or entering) a node are spread along its
    // side, in the order of their other end, so that draw.io routes them apart.
    let center = |id: ResourceId| -> (f64, f64) {
        let b = scene.node_frame(id).and_then(|g| placement.frames.get(&g)).or_else(|| placement.nodes.get(&id));
        b.map_or((0.0, 0.0), |b| (b.x + b.width / 2.0, b.y + b.height / 2.0))
    };
    let vertical = graph.options.direction.vertical();
    let reversed = graph.options.direction.reversed();
    let across = |id: ResourceId| if vertical { center(id).0 } else { center(id).1 };
    let drawn: Vec<(ResourceId, ResourceId)> = graph
        .edges
        .iter()
        .map(|e| {
            let (from, to, _) = scene.drawn(e);
            (from, to)
        })
        .collect();
    let spread = |end: fn(&(ResourceId, ResourceId)) -> ResourceId,
                  other: fn(&(ResourceId, ResourceId)) -> ResourceId| {
        let mut sides: HashMap<ResourceId, Vec<usize>> = HashMap::new();
        for (i, pair) in drawn.iter().enumerate() {
            sides.entry(end(pair)).or_default().push(i);
        }
        let mut fraction = vec![0.5; drawn.len()];
        for edges in sides.values_mut() {
            edges.sort_by(|&a, &b| across(other(&drawn[a])).total_cmp(&across(other(&drawn[b]))));
            for (k, &e) in edges.iter().enumerate() {
                fraction[e] = (k + 1) as f64 / (edges.len() + 1) as f64;
            }
        }
        fraction
    };
    let exits = spread(|p| p.0, |p| p.1);
    let entries = spread(|p| p.1, |p| p.0);
    // The side an edge leaves by, and the side it enters by, along the direction of the view.
    let (out_side, in_side) = if reversed { (0.0, 1.0) } else { (1.0, 0.0) };
    let port = |prefix: &str, side: f64, fraction: f64| {
        let (x, y) = if vertical { (fraction, side) } else { (side, fraction) };
        format!("{prefix}X={};{prefix}Y={};{prefix}Dx=0;{prefix}Dy=0", round_fraction(x), round_fraction(y))
    };

    for (index, edge) in graph.edges.iter().enumerate() {
        let (from, to, arrow) = scene.drawn(edge);
        let style = scene.edge_style(edge);
        let mut css = vec![
            "edgeStyle=orthogonalEdgeStyle".to_owned(),
            "rounded=1".to_owned(),
            "html=1".to_owned(),
            format!("fontSize={}", scene.graph.options.edge_label_size),
            "labelBackgroundColor=#ffffff".to_owned(),
        ];
        let head = if style.hollow_arrow { "block;endFill=0" } else { "block;endFill=1" };
        match arrow {
            Arrow::None => css.push("endArrow=none".to_owned()),
            Arrow::Both => {
                css.push(format!("endArrow={head}"));
                css.push(format!("startArrow={}", head.replace("endFill", "startFill")));
            }
            Arrow::Forward | Arrow::Back => css.push(format!("endArrow={head}")),
        }
        let color = style.color.clone().unwrap_or_else(|| "#333333".to_owned());
        css.push(format!("strokeColor={color}"));
        css.push(format!("fontColor={color}"));
        css.extend(dashes(style.line));
        if from != to {
            css.push(port("exit", out_side, exits[index]));
            css.push(port("entry", in_side, entries[index]));
        }
        if style.thin {
            css.push("strokeWidth=1".to_owned());
        }
        let mut data = vec![
            ("label".to_owned(), xml_text(&scene.diagram_label_line(edge))),
            ("tooltip".to_owned(), scene.edge_tooltip(edge)),
            ("source".to_owned(), scene.registry.qualified_name(edge.source)),
            ("target".to_owned(), scene.registry.qualified_name(edge.target)),
        ];
        if let Some(relation) = &edge.relation {
            data.push(("relation".to_owned(), relation.clone()));
        }
        let _ = writeln!(out, "<object id=\"e{index}\"{}>", attributes(&data));
        let _ = writeln!(
            out,
            "<mxCell style=\"{};\" edge=\"1\" parent=\"1\" source=\"{}\" target=\"{}\">\
             <mxGeometry relative=\"1\" as=\"geometry\"/></mxCell>\n</object>",
            xml(&css.join(";")),
            cell_id(scene, from),
            cell_id(scene, to)
        );
    }

    out.push_str("</root>\n</mxGraphModel>\n</diagram>\n</mxfile>\n");
    out
}

/// The cell of a node: itself, or the frame it is drawn as.
fn cell_id(scene: &Scene, id: ResourceId) -> String {
    match scene.node_frame(id) {
        Some(group) => format!("f{group}"),
        None => format!("n{}", id.0),
    }
}

fn cell(out: &mut String, id: &str, data: &[(String, String)], style: &str, kind: &str, parent: &str, b: Bounds) {
    let _ = writeln!(out, "<object id=\"{}\"{}>", xml(id), attributes(data));
    let _ = writeln!(
        out,
        "<mxCell style=\"{}\" {kind} parent=\"{}\"><mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
         as=\"geometry\"/></mxCell>\n</object>",
        xml(style),
        xml(parent),
        round(b.x),
        round(b.y),
        round(b.width),
        round(b.height)
    );
}

/// The data of a resource, as draw.io properties: its qualified name, its kind, its tooltip and
/// its attributes (a key named like a property draw.io reads is prefixed).
fn resource_data(scene: &Scene, id: ResourceId) -> Vec<(String, String)> {
    let resource = scene.registry.resource(id);
    let mut data = vec![
        ("tooltip".to_owned(), scene.node_tooltip(id)),
        ("name".to_owned(), scene.registry.qualified_name(id)),
        ("kind".to_owned(), resource.kind.clone()),
    ];
    if let Some(link) = scene.node_link(id) {
        data.push(("link".to_owned(), link));
    }
    for attribute in &resource.attributes {
        let values: Vec<String> = attribute.value.items().into_iter().map(crate::query::value_text).collect();
        let key = match attribute.key.as_str() {
            "id" | "label" | "tooltip" | "link" | "placeholders" | "name" | "kind" => {
                format!("cadastre_{}", attribute.key)
            }
            key => key.to_owned(),
        };
        data.push((key, values.join(", ")));
    }
    data
}

fn attributes(data: &[(String, String)]) -> String {
    data.iter().map(|(k, v)| format!(" {}=\"{}\"", k, xml(v))).collect()
}

/// The style of a node: the shape of the SVG, its colors and line; the icon of its kind in a
/// box (draw.io's `label` shape: an image beside the text).
fn node_style(style: &NodeStyle, kind: &str, size: f64) -> String {
    let mut css = vec!["whiteSpace=wrap".to_owned(), "html=1".to_owned(), format!("fontSize={size}")];
    let boxed = matches!(style.shape.as_str(), "box" | "rounded");
    match (style.shape.as_str(), icon(kind)) {
        ("box" | "rounded", Some(svg)) => {
            css.push("shape=label".to_owned());
            css.push(format!("image={}", data_uri(svg, style.color.as_deref().unwrap_or("#000000"))));
            css.extend(
                ["imageWidth=16", "imageHeight=16", "imageAlign=left", "imageVerticalAlign=middle", "spacingLeft=26"]
                    .map(str::to_owned),
            );
        }
        ("ellipse", _) => css.push("ellipse".to_owned()),
        ("cylinder", _) => css.extend(["shape=cylinder3", "boundedLbl=1", "size=6"].map(str::to_owned)),
        ("hexagon", _) => css.extend(["shape=hexagon", "perimeter=hexagonPerimeter2", "size=0.1"].map(str::to_owned)),
        ("note", _) => css.extend(["shape=note", "size=10"].map(str::to_owned)),
        _ => {}
    }
    if boxed {
        css.push(format!("rounded={}", u8::from(style.shape == "rounded")));
    }
    css.push(format!("strokeColor={}", style.color.as_deref().unwrap_or("#000000")));
    css.push(format!("fillColor={}", style.fill.as_deref().unwrap_or("#ffffff")));
    if style.fill.as_deref().is_some_and(is_dark) {
        css.push("fontColor=#ffffff".to_owned());
    }
    css.extend(dashes(style.line));
    css.join(";") + ";"
}

/// The style of a frame: a container, its title at the top left, beside the icon of its kind.
fn frame_style(style: &NodeStyle, kind: Option<&str>, size: f64) -> String {
    let mut css = vec![
        "container=1".to_owned(),
        "collapsible=0".to_owned(),
        "whiteSpace=wrap".to_owned(),
        "html=1".to_owned(),
        format!("fontSize={size}"),
        "verticalAlign=top".to_owned(),
        "align=left".to_owned(),
        "spacingTop=2".to_owned(),
        format!("rounded={}", u8::from(style.shape == "rounded")),
        "arcSize=4".to_owned(),
    ];
    match kind.and_then(icon) {
        Some(svg) => {
            css.push("shape=label".to_owned());
            css.push(format!("image={}", data_uri(svg, "#666666")));
            css.extend(
                ["imageWidth=14", "imageHeight=14", "imageAlign=left", "imageVerticalAlign=top", "spacingLeft=22"]
                    .map(str::to_owned),
            );
        }
        None => css.push("spacingLeft=6".to_owned()),
    }
    css.push(format!("strokeColor={}", style.color.as_deref().unwrap_or("#999999")));
    css.push(format!("fillColor={}", style.fill.as_deref().unwrap_or("none")));
    css.extend(dashes(style.line));
    css.join(";") + ";"
}

fn dashes(line: Line) -> Option<String> {
    match line {
        Line::Solid => None,
        Line::Dashed => Some("dashed=1".to_owned()),
        Line::Dotted => Some("dashed=1;dashPattern=1 3".to_owned()),
    }
}

/// An icon as an image of a draw.io style: `data:image/svg+xml,` then the SVG in base64 (the
/// `;base64` of a data URI would end the style property).
fn data_uri(icon: &str, color: &str) -> String {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{ICON}\" height=\"{ICON}\" viewBox=\"0 0 24 24\" \
         fill=\"none\" stroke=\"{color}\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{icon}</svg>"
    );
    format!("data:image/svg+xml,{}", base64(svg.as_bytes()))
}

/// Lines of text for an HTML label.
fn html_lines(lines: &[String]) -> String {
    lines.iter().map(|l| xml_text(l)).collect::<Vec<_>>().join("<br>")
}

/// Text in an HTML label: its special characters escaped.
fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A value in an XML attribute.
fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\n', "&#10;")
}

fn round_fraction(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    format!("{rounded}")
}

fn round(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 { format!("{}", rounded as i64) } else { format!("{rounded}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encodes() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"<svg/>"), "PHN2Zy8+");
    }
}
