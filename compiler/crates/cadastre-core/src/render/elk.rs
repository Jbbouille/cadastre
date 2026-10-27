//! `layout: elk`: a view laid out by Eclipse ELK (its layered algorithm, ported to Rust by
//! `merman-layout-elk`) and drawn by the compiler, as the SVG laid out by Graphviz is: the shapes
//! and the icons of the kinds, the styles, the links (`url`, `console`) and tooltips of the nodes
//! and frames, the tooltips of the edges, the legend.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;

use merman_layout_elk as elk;

use super::draw::{
    CHAR_WIDTH, CYLINDER_CAP, LINE_HEIGHT, PADDING, dash, icon_use, kind_of, marker_id, outer_size, shape, text,
    text_size,
};
use super::svg::{FRAME_COLOR, FRAME_GAP, FRAME_ICON, NODE_GAP, NODE_ICON, icon_symbols};
use super::{Arrow, LegendEntry, Line, NodeStyle, Placed, Scene, html, is_dark, node_id};
use crate::registry::ResourceId;
use crate::view::{Direction, GraphEdge};

const MARGIN: f64 = 12.0;
/// Prefix of the nodes and edges given to ELK for the layout hints only, not drawn.
const HIDDEN: &str = "hidden-";

/// A node or a frame of the drawing: its text, its icon, its style.
struct Item {
    lines: Vec<String>,
    kind: Option<String>,
    style: NodeStyle,
    tooltip: String,
    link: Option<String>,
}

/// An edge of the drawing: parallel edges drawn alike are one.
struct DrawnEdge<'a> {
    id: String,
    source: String,
    target: String,
    edges: Vec<&'a GraphEdge>,
    arrow: Arrow,
    label: Vec<String>,
}

/// The SVG of a view laid out by ELK, or why it could not be.
pub fn svg(scene: &Scene) -> Result<String, String> {
    let options = &scene.graph.options;
    let (node_size, edge_size, frame_size) =
        (options.node_label_size, options.edge_label_size, options.frame_label_size);
    let mut graph = elk::Graph {
        id: scene.graph.view.clone(),
        direction: match options.direction {
            Direction::LeftRight => elk::Direction::Right,
            Direction::RightLeft => elk::Direction::Left,
            Direction::TopBottom => elk::Direction::Down,
            Direction::BottomTop => elk::Direction::Up,
        },
        ..Default::default()
    };
    graph.spacing.node_node *= options.spacing.0;
    graph.spacing.layer_layer *= options.spacing.1;

    // The frames, then the nodes, each in its frame (a node drawn as a frame is the frame).
    let mut frames = Vec::new();
    let mut nodes = Vec::new();
    collect(scene, None, &mut frames, &mut nodes);
    let mut items: HashMap<String, Item> = HashMap::new();
    for &(group, parent) in &frames {
        let resource = scene.graph.groups[group].resource;
        let lines = scene.frame_lines(group);
        let kind = resource.and_then(|r| kind_of(scene, r));
        let (width, height) = text_size(&lines, frame_size);
        let width = width + kind.as_ref().map_or(0.0, |_| FRAME_ICON + FRAME_GAP);
        let id = format!("g{group}");
        graph.nodes.push(node(
            &id,
            elk::NodeKind::Group,
            parent.map(|p| format!("g{p}")),
            0.0,
            0.0,
            Some((lines.join(" "), width, height)),
        ));
        let item = Item {
            tooltip: resource.map_or_else(|| lines.join("\n"), |r| scene.node_tooltip(r)),
            link: resource.and_then(|r| scene.node_link(r)),
            style: scene.frame_style(group),
            lines,
            kind,
        };
        items.insert(id, item);
    }
    for &(id, parent) in &nodes {
        let lines = scene.node_lines(id);
        let kind = kind_of(scene, id);
        let style = scene.node_style(id);
        let (text_width, text_height) = text_size(&lines, node_size);
        let icon = kind.as_ref().map_or(0.0, |_| NODE_ICON + NODE_GAP);
        let (width, height) =
            outer_size(&style.shape, text_width + icon + 2.0 * PADDING.0, text_height.max(NODE_ICON) + 2.0 * PADDING.1);
        let name = node_id(id);
        let mut leaf = node(&name, elk::NodeKind::Leaf, parent.map(|p| format!("g{p}")), width, height, None);
        leaf.layer_constraint = layer_constraint(scene, id);
        graph.nodes.push(leaf);
        items.insert(name, Item { tooltip: scene.node_tooltip(id), link: scene.node_link(id), style, lines, kind });
    }
    // `cross`: the frames and nodes are given in its order; ELK keeps it within each layer.
    graph.options.layered.force_node_model_order = !options.cross.is_empty();

    // The edges, from the end the arrow leaves. Parallel edges drawn alike (same ends, arrow,
    // line and color) become one, with the labels of all of them, as in the Mermaid output.
    let end = |id: ResourceId| scene.node_frame(id).map_or_else(|| node_id(id), |g| format!("g{g}"));
    let mut drawn: Vec<DrawnEdge> = Vec::new();
    for edge in &scene.graph.edges {
        let (from, to, arrow) = scene.drawn(edge);
        let (source, target) = (end(from), end(to));
        let style = scene.edge_style(edge);
        let label: Vec<String> = scene.diagram_label(edge).lines().map(str::to_owned).collect();
        let same = drawn.iter_mut().find(|d| {
            let other = scene.edge_style(d.edges[0]);
            d.source == source
                && d.target == target
                && d.arrow == arrow
                && other.line == style.line
                && other.color == style.color
        });
        match same {
            Some(d) => {
                d.edges.push(edge);
                for line in label {
                    if !d.label.contains(&line) {
                        d.label.push(line);
                    }
                }
            }
            None => {
                let id = format!("e{}", drawn.len());
                drawn.push(DrawnEdge { id, source, target, edges: vec![edge], arrow, label });
            }
        }
    }
    for d in &drawn {
        let size = (!d.label.is_empty()).then(|| text_size(&d.label, edge_size));
        graph.edges.push(elk::Edge {
            id: d.id.clone(),
            source: d.source.clone(),
            target: d.target.clone(),
            label: size.map(|(width, height)| elk::Label { width, height }),
            terminal_labels: Vec::new(),
            minlen: 1,
            inside_self_loops_yo: false,
        });
    }
    // `stack`, `first` and `last`: invisible edges from an item to the next one, as in the DOT; a
    // level of several items, pulled to one layer by an invisible node before them. Neither is
    // drawn.
    let hidden = |source: String, target: String, graph: &mut elk::Graph| {
        let id = format!("{HIDDEN}{}", graph.edges.len());
        graph.edges.push(elk::Edge {
            id,
            source,
            target,
            label: None,
            terminal_labels: Vec::new(),
            minlen: 1,
            inside_self_loops_yo: false,
        });
    };
    // A link between the first anchors of two items is enough: one between every pair of their
    // anchors stretches the layout of ELK far more than that of Graphviz.
    for (first, second) in scene.stack_pairs() {
        if let (Some(&a), Some(&b)) = (scene.anchors(first, true).first(), scene.anchors(second, false).first()) {
            hidden(end(a), end(b), &mut graph);
        }
    }
    for (index, level) in scene.stack().iter().enumerate().filter(|(_, level)| level.len() > 1) {
        let id = format!("{HIDDEN}level{index}");
        graph.nodes.push(node(&id, elk::NodeKind::Leaf, None, 1.0, 1.0, None));
        for &item in level {
            if let Some(&a) = scene.anchors(item, false).first() {
                hidden(id.clone(), end(a), &mut graph);
            }
        }
    }

    let layout = elk::layout(&graph).map_err(|error| format!("ELK: {error}"))?;
    let boxes: HashMap<&str, &elk::NodeLayout> = layout.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let routes: HashMap<&str, &elk::EdgeLayout> = layout.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let spread = spread_ends(&drawn, &routes, &boxes);

    // The extent of the drawing, then the legend below it.
    let (mut right, mut bottom) = (0.0_f64, 0.0_f64);
    for n in layout.nodes.iter().filter(|n| !n.id.starts_with(HIDDEN)) {
        right = right.max(n.x + n.width / 2.0);
        bottom = bottom.max(n.y + n.height / 2.0);
    }
    for e in layout.edges.iter().filter(|e| !e.id.starts_with(HIDDEN)) {
        for p in &e.points {
            right = right.max(p.x);
            bottom = bottom.max(p.y);
        }
        for l in &e.labels {
            right = right.max(l.x + l.width);
            bottom = bottom.max(l.y + l.height);
        }
    }
    let legend = if options.legend { scene.legend() } else { Vec::new() };
    let (legend_width, legend_height) = legend_size(&legend, node_size);
    let legend_top = bottom + 2.0 * MARGIN;
    if !legend.is_empty() {
        right = right.max(legend_width);
        bottom = legend_top + legend_height;
    }

    let mut body = String::new();
    let mut markers: Vec<(String, bool)> = Vec::new();
    let mut used = BTreeSet::new();
    // Frames first, under the edges and the nodes.
    for &(group, _) in &frames {
        let id = format!("g{group}");
        let (Some(b), Some(item)) = (boxes.get(id.as_str()), items.get(&id)) else { continue };
        let node = scene.graph.groups[group].resource.is_some_and(|r| scene.node_frame(r).is_some());
        let style = &item.style;
        let border = style.color.as_deref().unwrap_or(if node { "black" } else { "#999999" });
        let rounded = !node || style.shape == "rounded";
        let (x, y) = (b.x - b.width / 2.0, b.y - b.height / 2.0);
        open(&mut body, item);
        let _ = writeln!(
            body,
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"{}\" fill=\"{}\" stroke=\"{border}\"{}/>",
            b.width,
            b.height,
            if rounded { 8 } else { 0 },
            style.fill.as_deref().unwrap_or("none"),
            dash(style.line),
        );
        // The title at the top, centered, after the icon of the kind.
        let text_color = if style.fill.as_deref().is_some_and(is_dark) { "white" } else { "black" };
        let (text_width, _) = text_size(&item.lines, frame_size);
        let icon = item.kind.as_ref().map_or(0.0, |_| FRAME_ICON + FRAME_GAP);
        let left = b.x - (text_width + icon) / 2.0;
        let top = y + 6.0;
        if let Some(kind) = &item.kind {
            let color = if text_color == "white" {
                "white"
            } else if style.color.is_some() {
                border
            } else {
                FRAME_COLOR
            };
            icon_use(&mut body, kind, left, top + (frame_size * LINE_HEIGHT - FRAME_ICON) / 2.0, FRAME_ICON, color);
            used.insert(kind.clone());
        }
        text(&mut body, &item.lines, left + icon + text_width / 2.0, top + frame_size, frame_size, text_color);
        close(&mut body, item);
    }
    // Edges, under the nodes; their labels on them.
    for d in &drawn {
        let Some(route) = routes.get(d.id.as_str()) else { continue };
        if route.points.len() < 2 {
            continue;
        }
        let style = scene.edge_style(d.edges[0]);
        let color = style.color.clone().unwrap_or_else(|| "black".to_owned());
        let marker = marker_id(&mut markers, &color, style.hollow_arrow);
        let route_points = spread.get(&d.id).unwrap_or(&route.points);
        let points: Vec<String> = route_points.iter().map(|p| format!("{:.1},{:.1}", p.x, p.y)).collect();
        let (start, end) = match d.arrow {
            Arrow::Forward | Arrow::Back => (String::new(), format!(" marker-end=\"url(#{marker})\"")),
            Arrow::Both => (format!(" marker-start=\"url(#{marker})\""), format!(" marker-end=\"url(#{marker})\"")),
            Arrow::None => (String::new(), String::new()),
        };
        let tooltips: Vec<String> = d.edges.iter().map(|e| scene.edge_tooltip(e)).collect();
        let _ = writeln!(body, "<g class=\"edge\"><title>{}</title>", html(&tooltips.join("\n")));
        let _ = writeln!(
            body,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"{}{start}{end}/>",
            points.join(" "),
            if style.thin { 0.6 } else { 1.0 },
            dash(style.line),
        );
        if let Some(place) = route.labels.iter().find(|l| l.terminal.is_none())
            && !d.label.is_empty()
        {
            let _ = writeln!(
                body,
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"white\" fill-opacity=\"0.85\"/>",
                place.x, place.y, place.width, place.height
            );
            let text_color = style.color.as_deref().unwrap_or("black");
            text(&mut body, &d.label, place.x + place.width / 2.0, place.y + edge_size, edge_size, text_color);
        }
        body.push_str("</g>\n");
    }
    // Nodes, on top.
    for &(id, _) in &nodes {
        let name = node_id(id);
        let (Some(b), Some(item)) = (boxes.get(name.as_str()), items.get(&name)) else { continue };
        let style = &item.style;
        let stroke = style.color.as_deref().unwrap_or("black");
        let fill = style.fill.as_deref().unwrap_or("white");
        open(&mut body, item);
        shape(
            &mut body,
            &style.shape,
            b.x - b.width / 2.0,
            b.y - b.height / 2.0,
            b.width,
            b.height,
            fill,
            stroke,
            style.line,
        );
        // The text block, centered, after the icon of the kind.
        let text_color = if style.fill.as_deref().is_some_and(is_dark) { "white" } else { "black" };
        let (text_width, text_height) = text_size(&item.lines, node_size);
        let icon = item.kind.as_ref().map_or(0.0, |_| NODE_ICON + NODE_GAP);
        let left = b.x - (text_width + icon) / 2.0;
        // A cylinder draws its text below its top cap.
        let middle = if style.shape == "cylinder" { b.y + CYLINDER_CAP / 2.0 } else { b.y };
        if let Some(kind) = &item.kind {
            let color = if text_color == "white" { "white" } else { stroke };
            icon_use(&mut body, kind, left, middle - NODE_ICON / 2.0, NODE_ICON, color);
            used.insert(kind.clone());
        }
        text(
            &mut body,
            &item.lines,
            left + icon + text_width / 2.0,
            middle - text_height / 2.0 + node_size,
            node_size,
            text_color,
        );
        close(&mut body, item);
    }
    if !legend.is_empty() {
        draw_legend(&mut body, &legend, 0.0, legend_top, legend_width, legend_height, node_size, &mut markers);
    }

    let (width, height) = (right + 2.0 * MARGIN, bottom + 2.0 * MARGIN);
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
    Ok(out)
}

/// Space between the ends of edges on a same side of a node: ELK puts them a pixel or two apart.
const END_GAP: f64 = 8.0;

/// The routes of the edges, their ends on a same side of a node spread along it, at most
/// [`END_GAP`] apart and within its middle part: the last segment, orthogonal to the side, slides
/// along it, the route staying orthogonal.
fn spread_ends(
    drawn: &[DrawnEdge],
    routes: &HashMap<&str, &elk::EdgeLayout>,
    boxes: &HashMap<&str, &elk::NodeLayout>,
) -> HashMap<String, Vec<elk::Point>> {
    let mut points: HashMap<String, Vec<elk::Point>> = drawn
        .iter()
        .filter_map(|d| routes.get(d.id.as_str()).map(|r| (d.id.clone(), r.points.clone())))
        .filter(|(_, p)| p.len() >= 2)
        .collect();
    // The ends on each side of each node: (node, side) -> [(edge, its last point?)].
    let mut sides: HashMap<(&str, Side), Vec<(String, bool)>> = HashMap::new();
    for d in drawn {
        let Some(route) = points.get(&d.id) else { continue };
        for (node, last) in [(d.target.as_str(), true), (d.source.as_str(), false)] {
            let (Some(b), Some(point)) = (boxes.get(node), if last { route.last() } else { route.first() }) else {
                continue;
            };
            if let Some(side) = side_of(point, b) {
                sides.entry((node, side)).or_default().push((d.id.clone(), last));
            }
        }
    }
    for ((node, side), mut ends) in sides {
        if ends.len() < 2 {
            continue;
        }
        let b = boxes[node];
        let vertical = matches!(side, Side::Left | Side::Right);
        let along = |p: &elk::Point| if vertical { p.y } else { p.x };
        let at = |points: &HashMap<String, Vec<elk::Point>>, (id, last): &(String, bool)| {
            let route = &points[id];
            along(if *last { route.last().unwrap() } else { route.first().unwrap() })
        };
        ends.sort_by(|a, b| at(&points, a).total_cmp(&at(&points, b)));
        let length = if vertical { b.height } else { b.width };
        let gap = END_GAP.min(length * 0.8 / (ends.len() - 1) as f64);
        let current = ends.windows(2).map(|w| at(&points, &w[1]) - at(&points, &w[0])).fold(f64::INFINITY, f64::min);
        if current >= gap * 0.9 {
            continue;
        }
        let middle = if vertical { b.y } else { b.x };
        let first = middle - gap * (ends.len() - 1) as f64 / 2.0;
        for (k, (id, last)) in ends.iter().enumerate() {
            let route = points.get_mut(id).unwrap();
            slide(route, *last, vertical, first + k as f64 * gap);
        }
    }
    points
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// The side of a node a point lies on, if any.
fn side_of(point: &elk::Point, b: &elk::NodeLayout) -> Option<Side> {
    let (left, right, top, bottom) =
        (b.x - b.width / 2.0, b.x + b.width / 2.0, b.y - b.height / 2.0, b.y + b.height / 2.0);
    let near = |a: f64, b: f64| (a - b).abs() < 2.0;
    let within = |v: f64, low: f64, high: f64| v >= low - 2.0 && v <= high + 2.0;
    match () {
        _ if near(point.x, left) && within(point.y, top, bottom) => Some(Side::Left),
        _ if near(point.x, right) && within(point.y, top, bottom) => Some(Side::Right),
        _ if near(point.y, top) && within(point.x, left, right) => Some(Side::Top),
        _ if near(point.y, bottom) && within(point.x, left, right) => Some(Side::Bottom),
        _ => None,
    }
}

/// Moves an end of a route along the side it lies on (`vertical`: a left or right side) to
/// `to`, with the segment leading to it; a straight route gets a bend in its middle.
fn slide(route: &mut Vec<elk::Point>, last: bool, vertical: bool, to: f64) {
    if !last {
        route.reverse();
    }
    let n = route.len();
    let (end, before) = (route[n - 1], route[n - 2]);
    let perpendicular = if vertical { (end.y - before.y).abs() < 0.5 } else { (end.x - before.x).abs() < 0.5 };
    if perpendicular {
        if n == 2 {
            // A straight edge: a bend halfway, then the end moved.
            let half = if vertical { (end.x + before.x) / 2.0 } else { (end.y + before.y) / 2.0 };
            let (a, b) = if vertical {
                (elk::Point { x: half, y: before.y }, elk::Point { x: half, y: to })
            } else {
                (elk::Point { x: before.x, y: half }, elk::Point { x: to, y: half })
            };
            route.insert(1, b);
            route.insert(1, a);
        } else {
            let set = |p: &mut elk::Point| if vertical { p.y = to } else { p.x = to };
            set(&mut route[n - 2]);
        }
        let n = route.len();
        if vertical { route[n - 1].y = to } else { route[n - 1].x = to }
    }
    if !last {
        route.reverse();
    }
}

/// The frames (with their parent) and the nodes (with their frame) of a frame, depth first.
fn collect(
    scene: &Scene,
    frame: Option<usize>,
    frames: &mut Vec<(usize, Option<usize>)>,
    nodes: &mut Vec<(ResourceId, Option<usize>)>,
) {
    for placed in scene.ordered_content(frame, false) {
        match placed {
            Placed::Frame(group) => {
                frames.push((group, frame));
                collect(scene, Some(group), frames, nodes);
            }
            Placed::Node(node) => nodes.push((node, frame)),
        }
    }
}

/// `first` and `last` naming a node: the first or the last layer of its frame, in the order of
/// the drawing (the top or the left), whatever the direction of the edges.
fn layer_constraint(scene: &Scene, id: ResourceId) -> Option<elk::LayerConstraint> {
    let options = &scene.graph.options;
    let names = |names: &[String]| names.iter().any(|n| scene.placed(n) == Some(Placed::Node(id)));
    let (first, last) = (names(&options.first), names(&options.last));
    let reversed = options.direction.reversed();
    match (first, last) {
        (true, false) => Some(if reversed { elk::LayerConstraint::Last } else { elk::LayerConstraint::First }),
        (false, true) => Some(if reversed { elk::LayerConstraint::First } else { elk::LayerConstraint::Last }),
        _ => None,
    }
}

fn node(
    id: &str,
    kind: elk::NodeKind,
    parent: Option<String>,
    width: f64,
    height: f64,
    title: Option<(String, f64, f64)>,
) -> elk::Node {
    elk::Node {
        id: id.to_owned(),
        kind,
        label_text: title.as_ref().map(|t| t.0.clone()),
        container: elk::ContainerNodeOptions::default(),
        width,
        height,
        parent,
        direction: None,
        hierarchy_handling: None,
        layer_constraint: None,
        port_alignment: None,
        label: title.map(|(_, width, height)| elk::Label { width, height }),
    }
}

/// Opens the group of a node or a frame: its tooltip, and its link opened in a new tab.
fn open(out: &mut String, item: &Item) {
    if let Some(link) = &item.link {
        let _ = writeln!(out, "<a xlink:href=\"{}\" href=\"{0}\" target=\"_blank\">", html(link));
    }
    let _ = writeln!(out, "<g><title>{}</title>", html(&item.tooltip));
}

fn close(out: &mut String, item: &Item) {
    out.push_str("</g>\n");
    if item.link.is_some() {
        out.push_str("</a>\n");
    }
}

/// Height of a line of the legend, and width of its sample.
const LEGEND_ROW: f64 = 18.0;
const LEGEND_SAMPLE: f64 = 28.0;

fn legend_size(legend: &[LegendEntry], size: f64) -> (f64, f64) {
    let longest = legend.iter().map(|e| e.label.chars().count()).max().unwrap_or(0) as f64;
    (LEGEND_SAMPLE + 24.0 + longest * size * CHAR_WIDTH, 28.0 + legend.len() as f64 * LEGEND_ROW)
}

/// The legend: a frame titled "Legend", a sample of each style and its label.
#[allow(clippy::too_many_arguments)]
fn draw_legend(
    out: &mut String,
    legend: &[LegendEntry],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    size: f64,
    markers: &mut Vec<(String, bool)>,
) {
    let _ = writeln!(
        out,
        "<g class=\"legend\"><rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"8\" fill=\"none\" stroke=\"#bbbbbb\"/>"
    );
    text(out, &["Legend".to_owned()], x + w / 2.0, y + 6.0 + size, size, "black");
    for (i, entry) in legend.iter().enumerate() {
        let row = y + 26.0 + i as f64 * LEGEND_ROW;
        let color = entry.color.as_deref().unwrap_or("black");
        if entry.edges {
            let marker = marker_id(markers, color, false);
            let middle = row + LEGEND_ROW / 2.0;
            let _ = writeln!(
                out,
                "<line x1=\"{:.1}\" y1=\"{middle:.1}\" x2=\"{:.1}\" y2=\"{middle:.1}\" stroke=\"{color}\"{} marker-end=\"url(#{marker})\"/>",
                x + 10.0,
                x + 10.0 + LEGEND_SAMPLE,
                dash(entry.line.unwrap_or(Line::Solid)),
            );
        } else {
            let _ = writeln!(
                out,
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"12\" height=\"12\" fill=\"{}\" stroke=\"{color}\"{}/>",
                x + 10.0 + (LEGEND_SAMPLE - 12.0) / 2.0,
                row + (LEGEND_ROW - 12.0) / 2.0,
                entry.fill.as_deref().unwrap_or("white"),
                dash(entry.line.unwrap_or(Line::Solid)),
            );
        }
        let _ = writeln!(
            out,
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{size}\" fill=\"black\">{}</text>",
            x + 18.0 + LEGEND_SAMPLE,
            row + LEGEND_ROW / 2.0 + size * 0.35,
            html(&entry.label),
        );
    }
    out.push_str("</g>\n");
}
