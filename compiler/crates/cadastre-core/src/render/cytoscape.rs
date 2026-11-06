//! A view to move by hand: the nodes, frames and edges of a view as Cytoscape.js elements, placed
//! where Graphviz puts them in the SVG, for a page where they can be dragged and the edges bent.
//! Nothing comes back to the model: the positions live in the page.
//!
//! - Nodes carry their position (the center of their box), size, shape, colors, line and icon.
//! - Frames are compound nodes, the parents of their nodes and frames: moving a frame moves its
//!   content. A node drawn as a frame is that frame.
//! - Edges carry their color, line, arrow and label; when Graphviz's layout gives their route,
//!   the points of its curve, to start from the drawing of the SVG.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::drawio::{Bounds, from_cells, from_graphviz};
use super::{Arrow, Line, Scene, base64, icons::icon, is_dark};
use crate::registry::ResourceId;

/// The elements of a view, as JSON: `{ "nodes": […], "edges": […] }`; `layout` is Graphviz's
/// `-Tjson` output for the DOT of the view, the layout of the compiler without it.
pub fn cytoscape(scene: &Scene, layout: Option<&str>) -> String {
    let placement = layout.and_then(|json| from_graphviz(scene, json)).unwrap_or_else(|| from_cells(scene));
    let routes = layout.and_then(graphviz_routes).unwrap_or_default();
    let graph = scene.graph;
    let mut nodes = Vec::new();

    for (group, bounds) in sorted(scene, &placement.frames) {
        let parent = graph.groups[group].parent.filter(|p| placement.frames.contains_key(p));
        let resource = graph.groups[group].resource;
        let style = scene.frame_style(group);
        let kind = resource.map(|r| scene.registry.schema.base(&scene.registry.resource(r).kind).to_owned());
        let mut data = json!({
            "id": format!("f{group}"),
            "frame": true,
            "label": scene.frame_lines(group).join("\n"),
            "color": style.color.clone().unwrap_or_else(|| if resource.is_some_and(|r| scene.node_frame(r).is_some()) { "#000000" } else { "#999999" }.to_owned()),
            "fill": style.fill.clone().unwrap_or_else(|| "none".to_owned()),
            "rounded": style.shape == "rounded",
            "line": line(style.line),
            "width": bounds.width,
            "height": bounds.height,
        });
        if let Some(parent) = parent {
            data["parent"] = json!(format!("f{parent}"));
        }
        if let Some(r) = resource {
            data["name"] = json!(scene.registry.qualified_name(r));
            data["tooltip"] = json!(scene.node_tooltip(r));
        }
        if let Some(svg) = kind.as_deref().and_then(icon) {
            data["icon"] = json!(data_uri(svg, "#666666"));
        }
        nodes.push(json!({ "data": data }));
    }

    for &id in &graph.nodes {
        if scene.node_frame(id).is_some() {
            continue;
        }
        let Some(&b) = placement.nodes.get(&id) else { continue };
        let style = scene.node_style(id);
        let kind = scene.registry.schema.base(&scene.registry.resource(id).kind);
        let color = style.color.clone().unwrap_or_else(|| "#000000".to_owned());
        let fill = style.fill.clone().unwrap_or_else(|| "#ffffff".to_owned());
        let mut data = json!({
            "id": node_id(scene, id),
            "label": scene.node_lines(id).join("\n"),
            "name": scene.registry.qualified_name(id),
            "kind": kind,
            "tooltip": scene.node_tooltip(id),
            "shape": shape(&style.shape),
            "color": color,
            "fill": fill,
            "text": if is_dark(&fill) { "#ffffff" } else { "#000000" },
            "line": line(style.line),
            "width": b.width,
            "height": b.height,
        });
        if let Some(group) = graph.node_group.get(&id).copied().filter(|g| placement.frames.contains_key(g)) {
            data["parent"] = json!(format!("f{group}"));
        }
        if matches!(style.shape.as_str(), "box" | "rounded")
            && let Some(svg) = icon(kind)
        {
            data["icon"] = json!(data_uri(svg, &color));
        }
        nodes.push(json!({ "data": data, "position": center(b) }));
    }

    let mut taken = vec![false; routes.routes.len()];
    let edges: Vec<Value> = graph
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let (from, to, arrow) = scene.drawn(edge);
            let style = scene.edge_style(edge);
            let color = style.color.clone().unwrap_or_else(|| "#333333".to_owned());
            let (source, target) = (node_id(scene, from), node_id(scene, to));
            let mut data = json!({
                "id": format!("e{index}"),
                "source": source,
                "target": target,
                "label": scene.diagram_label_line(edge),
                "tooltip": scene.edge_tooltip(edge),
                "color": color,
                "line": line(style.line),
                "head": if matches!(arrow, Arrow::None) { "none" } else if style.hollow_arrow { "hollow" } else { "filled" },
                "tail": if matches!(arrow, Arrow::Both) { if style.hollow_arrow { "hollow" } else { "filled" } } else { "none" },
            });
            // The route of Graphviz: straight between the two nodes, or through the node of the
            // label of the edge (`e<index>`), in two halves.
            let (a, b) = (format!("n{}", from.0), format!("n{}", to.0));
            let label = format!("e{index}");
            let points = match (routes.labels.get(&label), take(&routes.routes, &mut taken, &a, &b)) {
                (_, Some(points)) => Some(points),
                (Some(&middle), None) => {
                    let first = take(&routes.routes, &mut taken, &a, &label);
                    let second = take(&routes.routes, &mut taken, &label, &b);
                    first.zip(second).map(|(first, second)| [first, vec![middle], second].concat())
                }
                (None, None) => None,
            };
            if let Some(points) = points.filter(|p| !p.is_empty()) {
                data["bendPointPositions"] =
                    json!(points.iter().map(|&(x, y)| json!({ "x": x, "y": y })).collect::<Vec<_>>());
            }
            json!({ "data": data })
        })
        .collect();

    json!({ "view": graph.view, "nodes": nodes, "edges": edges }).to_string()
}

/// The frames, the outer ones first: Cytoscape wants a parent before its children.
fn sorted(scene: &Scene, frames: &HashMap<usize, Bounds>) -> Vec<(usize, Bounds)> {
    let groups = &scene.graph.groups;
    let depth = |g: usize| std::iter::successors(Some(g), |&g| groups[g].parent).count();
    let mut frames: Vec<(usize, Bounds)> = frames.iter().map(|(&g, &b)| (g, b)).collect();
    frames.sort_by_key(|&(g, _)| (depth(g), g));
    frames
}

/// The points of the first route not taken from `a` to `b`, or from `b` to `a` reversed.
fn take(routes: &[Route], taken: &mut [bool], a: &str, b: &str) -> Option<Vec<(f64, f64)>> {
    let i = routes
        .iter()
        .enumerate()
        .position(|(i, r)| !taken[i] && ((r.tail == a && r.head == b) || (r.tail == b && r.head == a)))?;
    taken[i] = true;
    let mut points = routes[i].points.clone();
    if routes[i].tail != a {
        points.reverse();
    }
    Some(points)
}

/// The element of a node: itself, or the frame it is drawn as.
fn node_id(scene: &Scene, id: ResourceId) -> String {
    match scene.node_frame(id) {
        Some(group) => format!("f{group}"),
        None => format!("n{}", id.0),
    }
}

fn center(b: Bounds) -> Value {
    json!({ "x": b.x + b.width / 2.0, "y": b.y + b.height / 2.0 })
}

/// The Cytoscape shape of a shape of the SVG.
fn shape(shape: &str) -> &'static str {
    match shape {
        "rounded" => "round-rectangle",
        "ellipse" => "ellipse",
        "cylinder" => "barrel",
        "hexagon" => "hexagon",
        "note" => "tag",
        _ => "rectangle",
    }
}

fn line(line: Line) -> &'static str {
    match line {
        Line::Solid => "solid",
        Line::Dashed => "dashed",
        Line::Dotted => "dotted",
    }
}

/// An icon as a data URI, drawn in a color.
fn data_uri(icon: &str, color: &str) -> String {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" fill=\"none\" \
         stroke=\"{color}\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{icon}</svg>"
    );
    format!("data:image/svg+xml;base64,{}", base64(svg.as_bytes()))
}

/// An edge laid out by Graphviz: its two ends (`n<id>` nodes, `e<index>` nodes of labels) and the
/// points of its curve between them, with the origin at the top left as the placed nodes.
struct Route {
    tail: String,
    head: String,
    points: Vec<(f64, f64)>,
}

/// The edges of Graphviz's layout, and the centers of the nodes of the labels of edges.
#[derive(Default)]
struct Routes {
    routes: Vec<Route>,
    labels: HashMap<String, (f64, f64)>,
}

/// The routes of the edges of Graphviz's `-Tjson` output: of each cubic Bézier segment of a curve,
/// the point it ends at, the ends of the curve left out (they are on the borders of the nodes).
fn graphviz_routes(json: &str) -> Option<Routes> {
    let value: Value = serde_json::from_str(json).ok()?;
    let numbers = |text: &str| -> Vec<f64> { text.split(',').filter_map(|n| n.trim().parse().ok()).collect() };
    let height = *numbers(value.get("bb")?.as_str()?).get(3)?;
    let mut names: HashMap<u64, String> = HashMap::new();
    let mut labels = HashMap::new();
    for object in value.get("objects")?.as_array()? {
        let (Some(gvid), Some(name)) =
            (object.get("_gvid").and_then(Value::as_u64), object.get("name").and_then(Value::as_str))
        else {
            continue;
        };
        names.insert(gvid, name.to_owned());
        let is_label = name.strip_prefix('e').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if is_label && let [x, y] = numbers(object.get("pos").and_then(Value::as_str).unwrap_or_default())[..] {
            labels.insert(name.to_owned(), (x, height - y));
        }
    }
    let mut routes = Vec::new();
    for edge in value.get("edges").and_then(Value::as_array).into_iter().flatten() {
        let end = |key: &str| edge.get(key).and_then(Value::as_u64).and_then(|g| names.get(&g)).cloned();
        let (Some(tail), Some(head)) = (end("tail"), end("head")) else { continue };
        let Some(pos) = edge.get("pos").and_then(Value::as_str) else { continue };
        // `e,x,y` and `s,x,y` are the tips of the arrows; the rest, the control points.
        let curve: Vec<(f64, f64)> = pos
            .split_whitespace()
            .filter(|p| !p.starts_with("e,") && !p.starts_with("s,"))
            .filter_map(|p| match numbers(p)[..] {
                [x, y] => Some((x, height - y)),
                _ => None,
            })
            .collect();
        let points: Vec<(f64, f64)> = curve
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 == 0 && *i > 0 && *i + 1 < curve.len())
            .map(|(_, &p)| p)
            .collect();
        routes.push(Route { tail, head, points });
    }
    Some(Routes { routes, labels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_keep_the_points_inside_the_curve() {
        let json = r#"{"bb":"0,0,100,100","objects":[{"_gvid":0,"name":"n1"},{"_gvid":1,"name":"n2"}],
            "edges":[{"tail":0,"head":1,"pos":"e,90,10 10,90 20,80 30,70 40,60 50,50 60,40 70,30"}]}"#;
        let routes = graphviz_routes(json).unwrap().routes;
        assert_eq!(routes.len(), 1);
        assert_eq!((routes[0].tail.as_str(), routes[0].head.as_str()), ("n1", "n2"));
        assert_eq!(routes[0].points, vec![(40.0, 40.0)]);
    }
}
