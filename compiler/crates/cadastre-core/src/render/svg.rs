//! Icons and edge labels in the SVG laid out by Graphviz.
//!
//! The DOT gives each node and frame with an icon a `kind_<kind>` class and keeps an empty
//! square left of its text ([`node_label`], [`frame_label`]). Once Graphviz has laid the graph
//! out, [`with_icons`] defines each icon used once and draws it in that square: the SVG stays a
//! single file, without external images.
//!
//! The labels placed after the layout (`xlabel`: free edges, `edge_labels_layout: free`) are put
//! by Graphviz beside their edge; [`with_icons`] moves each one onto its drawn edge, its letters
//! outlined by a white halo the edge passes under.

use std::collections::{BTreeSet, HashMap};

use super::icons::icon;

/// Side of the icon and space before the text, in points: nodes, then frames.
pub(super) const NODE_ICON: f64 = 16.0;
pub(super) const NODE_GAP: f64 = 5.0;
pub(super) const FRAME_ICON: f64 = 14.0;
pub(super) const FRAME_GAP: f64 = 4.0;
/// Color of frame icons, darker than the frame border.
pub(super) const FRAME_COLOR: &str = "#666666";

/// The dark theme of the drawing, applied by the browser or the editor showing it in dark mode:
/// the background, the default strokes, arrowheads and texts, the frame borders, the halo of the
/// labels on edges and the icons turn light; the colors of the layers and of the style rules stay,
/// and a text on a light fill of a style rule stays dark.
const DARK_STYLE: &str = "\n<style>@media (prefers-color-scheme: dark) { \
.graph > polygon, [fill=\"white\"] { fill: #1b1f23 } \
[stroke=\"black\"] { stroke: #d1d5db } [fill=\"black\"] { fill: #d1d5db } \
[stroke=\"#999999\"] { stroke: #6b7280 } [style=\"color:black\"] { color: #d1d5db } \
text:not([fill]), text[fill=\"black\"] { fill: #d1d5db } text[stroke=\"white\"] { stroke: #1b1f23 } \
a:has(> [fill]:not([fill=\"none\"]):not([fill=\"white\"]):not([fill=\"black\"])) > text:not([fill]) { fill: #111827 } \
}</style>";

/// The class of an edge whose label is to be centered on it.
pub const ON_EDGE: &str = "on_edge";

/// Class of the edges redrawn as a straight line (`routing: straight` in a style rule).
pub const STRAIGHT: &str = "straight";

/// The class carrying the kind of a node or a frame, when it has an icon.
pub fn class(kind: &str) -> Option<String> {
    icon(kind).map(|_| format!("kind_{kind}"))
}

/// HTML-like label of a node with an icon: the icon square, a gap, then the lines.
pub fn node_label(lines: &[String]) -> String {
    label(lines, NODE_ICON, NODE_GAP)
}

/// HTML-like label of a frame with an icon.
pub fn frame_label(lines: &[String]) -> String {
    label(lines, FRAME_ICON, FRAME_GAP)
}

fn label(lines: &[String], icon: f64, gap: f64) -> String {
    let text: Vec<String> = lines.iter().map(|l| super::html(l)).collect();
    format!(
        "<<table border=\"0\" cellborder=\"0\" cellspacing=\"0\" cellpadding=\"0\"><tr>\
         <td fixedsize=\"true\" width=\"{icon}\" height=\"{icon}\"></td><td width=\"{gap}\"></td>\
         <td>{}</td></tr></table>>",
        text.join("<br/>")
    )
}

/// Draws the icons of the nodes and frames carrying a `kind_<kind>` class, and centers the labels
/// of the edges carrying the `on_edge` class on them.
pub fn with_icons(svg: &str) -> String {
    let centered = cleared_titles(&centered_labels(&straightened(svg)));
    let svg = centered.as_str();
    let mut out = String::with_capacity(svg.len() + 4096);
    let mut used = BTreeSet::new();
    let mut position = 0;
    while let Some(found) = svg[position..].find("<g id=\"") {
        let start = position + found;
        let Some(tag_end) = svg[start..].find('>').map(|i| start + i + 1) else { break };
        let tag = &svg[start..tag_end];
        let class = attribute(tag, "class").unwrap_or_default().replace("&#45;", "-");
        let kind = class.split(' ').find_map(|c| c.strip_prefix("kind_"));
        let frame = class.split(' ').any(|c| c == "cluster");
        let twins = class.split(' ').any(|c| c == "twins");
        let (Some(kind), Some(end)) = (kind.filter(|k| icon(k).is_some()), group_end(svg, start)) else {
            out.push_str(&svg[position..tag_end]);
            position = tag_end;
            continue;
        };
        out.push_str(&svg[position..end]);
        if let Some(drawn) = placed(&svg[tag_end..end], kind, frame, twins) {
            out.push_str(&drawn);
            used.insert(kind.to_owned());
        }
        position = end;
    }
    out.push_str(&svg[position..]);

    // At the start of the drawing: the icons used, once, and the dark theme (once too).
    let mut head = String::new();
    if !used.is_empty() {
        head.push_str(&format!("\n<defs>\n{}</defs>", icon_symbols(&used)));
    }
    if !out.contains("prefers-color-scheme") {
        head.push_str(DARK_STYLE);
    }
    if let Some(at) = out.find("<svg").and_then(|s| out[s..].find('>').map(|e| s + e + 1)) {
        out.insert_str(at, &head);
    }
    out
}

/// The icons of these kinds as SVG symbols (`icon-<kind>`), drawn in the current color.
pub(super) fn icon_symbols(kinds: &BTreeSet<String>) -> String {
    kinds
        .iter()
        .map(|kind| {
            format!(
                "<symbol id=\"icon-{kind}\" viewBox=\"0 0 24 24\"><g fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" \
                 stroke-linecap=\"round\" stroke-linejoin=\"round\">{}</g></symbol>\n",
                icon(kind).unwrap_or_default()
            )
        })
        .collect()
}

/// The `<use>` drawing the icon of a node or a frame, left of its first line of text.
fn placed(body: &str, kind: &str, frame: bool, twins: bool) -> Option<String> {
    let texts: Vec<(f64, f64, f64)> = body
        .match_indices("<text ")
        .filter_map(|(i, _)| {
            let tag = &body[i..i + body[i..].find('>')?];
            let number = |name: &str| attribute(tag, name)?.parse::<f64>().ok();
            Some((number("x")?, number("y")?, number("font-size").unwrap_or(10.0)))
        })
        .collect();
    let (_, first_y, size) = *texts.first()?;
    // A box of twins: the icon beside its first line, not in the middle of the names.
    let last_y = if twins { first_y } else { texts.last()?.1 };
    let left = texts.iter().map(|t| t.0).fold(f64::INFINITY, f64::min);
    // Middle of the text block: the baselines, raised by about a third of the font size.
    let middle = (first_y + last_y) / 2.0 - size * 0.37;
    let (side, gap) = if frame { (FRAME_ICON, FRAME_GAP) } else { (NODE_ICON, NODE_GAP) };
    // The color of the text when it has one (white on a dark fill), otherwise of the outline;
    // the default frame border is too light for an icon.
    let first = |name: &str| {
        let key = format!("{name}=\"");
        body.match_indices(key.as_str())
            .map(|(i, m)| &body[i + m.len()..i + m.len() + body[i + m.len()..].find('"').unwrap_or(0)])
            .find(|c| *c != "none" && *c != "transparent")
    };
    let text_color = body.find("<text ").and_then(|i| {
        let tag = &body[i..i + body[i..].find('>')?];
        attribute(tag, "fill")
    });
    let color = match (text_color, first("stroke")) {
        (Some(text), _) => text,
        (None, Some(stroke)) if !(frame && stroke == "#999999") => stroke,
        _ if frame => FRAME_COLOR,
        _ => "black",
    };
    Some(format!(
        "<use xlink:href=\"#icon-{kind}\" x=\"{:.2}\" y=\"{:.2}\" width=\"{side}\" height=\"{side}\" style=\"color:{color}\"/>\n",
        left - gap - side,
        middle - side / 2.0
    ))
}

/// A rectangle: left, top, right, bottom.
type Rect = (f64, f64, f64, f64);

/// The quality of a drawing laid out by Graphviz, lower being better: the pairs of frames that
/// overlap without one holding the other, the crossings between edges, the total length of the
/// edges in points.
pub fn quality(svg: &str) -> (usize, usize, u64) {
    let frames: Vec<Rect> = bodies(svg, "cluster").iter().filter_map(|body| outline(body)).collect();
    let mut overlaps = 0;
    for (i, a) in frames.iter().enumerate() {
        for b in &frames[i + 1..] {
            let apart = a.2 <= b.0 || b.2 <= a.0 || a.3 <= b.1 || b.3 <= a.1;
            let holds = |o: &Rect, i: &Rect| o.0 <= i.0 && o.1 <= i.1 && o.2 >= i.2 && o.3 >= i.3;
            if !apart && !holds(a, b) && !holds(b, a) {
                overlaps += 1;
            }
        }
    }
    let edges: Vec<Vec<(f64, f64)>> = bodies(svg, "edge")
        .iter()
        .filter_map(|body| {
            let i = body.find("<path ")?;
            let tag = &body[i..i + body[i..].find('>')?];
            // Every fourth sample, and the last: enough to find the crossings, four times fewer
            // segments.
            let all = samples(attribute(tag, "d")?);
            let mut kept: Vec<(f64, f64)> = all.iter().copied().step_by(4).collect();
            if let Some(&last) = all.last()
                && kept.last() != Some(&last)
            {
                kept.push(last);
            }
            Some(kept)
        })
        .collect();
    let distance = |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let length: f64 = edges.iter().map(|e| e.windows(2).map(|w| distance(w[0], w[1])).sum::<f64>()).sum();
    let boxes: Vec<Rect> = edges.iter().map(|e| bounds(e)).collect();
    let mut crossings = 0;
    for (i, a) in edges.iter().enumerate() {
        for (j, b) in edges.iter().enumerate().skip(i + 1) {
            let (p, q) = (&boxes[i], &boxes[j]);
            if p.2 < q.0 || q.2 < p.0 || p.3 < q.1 || q.3 < p.1 {
                continue;
            }
            for s in a.windows(2) {
                for t in b.windows(2) {
                    if crosses(s[0], s[1], t[0], t[1]) {
                        crossings += 1;
                    }
                }
            }
        }
    }
    (overlaps, crossings, length.round() as u64)
}

/// How many average edges a crossing is worth when choosing among drawings: fewer crossings
/// read better, but not at any cost in the length of the edges (a drawing with one crossing less
/// and edges 75% longer reads worse).
const CROSSING_WEIGHT: f64 = 3.0;

/// Among several drawings of a view, the index of the one that reads best: no frames
/// overlapping first, then the least of the crossings, each worth three average edges, plus the
/// length of the edges; the first on a tie.
pub fn best_drawing<S: AsRef<str>>(svgs: &[S]) -> usize {
    let scores: Vec<(usize, usize, u64)> = svgs.iter().map(|s| quality(s.as_ref())).collect();
    let Some(fewest) = scores.iter().map(|s| s.0).min() else { return 0 };
    let edges = svgs.iter().map(|s| bodies(s.as_ref(), "edge").len()).max().unwrap_or(1).max(1);
    let shortest = scores.iter().filter(|s| s.0 == fewest).map(|s| s.2).min().unwrap_or(0);
    let unit = CROSSING_WEIGHT * shortest as f64 / edges as f64;
    let cost = |s: &(usize, usize, u64)| s.1 as f64 * unit + s.2 as f64;
    (0..svgs.len())
        .filter(|&i| scores[i].0 == fewest)
        .min_by(|&a, &b| cost(&scores[a]).total_cmp(&cost(&scores[b])).then(a.cmp(&b)))
        .unwrap_or(0)
}

/// Several drawings in one text, each a whole SVG document: the best of them (`best_drawing`).
pub fn best_of(joined: &str) -> String {
    let mut parts = Vec::new();
    let mut rest = joined;
    while let Some(end) = rest.find("</svg>") {
        parts.push(rest[..end + 6].trim_start());
        rest = &rest[end + 6..];
    }
    match parts.len() {
        0 => joined.to_owned(),
        _ => parts[best_drawing(&parts)].to_owned(),
    }
}

/// The bodies of the groups of a class (`cluster`, `node`, `edge`).
fn bodies<'s>(svg: &'s str, class: &str) -> Vec<&'s str> {
    let mut out = Vec::new();
    let mut position = 0;
    while let Some(found) = svg[position..].find("<g id=\"") {
        let start = position + found;
        let Some(tag_end) = svg[start..].find('>').map(|i| start + i + 1) else { break };
        let first = attribute(&svg[start..tag_end], "class").unwrap_or_default().split(' ').next().unwrap_or_default();
        match group_end(svg, start) {
            Some(end) if first == class => {
                out.push(&svg[tag_end..end]);
                position = end;
            }
            _ => position = tag_end,
        }
    }
    out
}

fn bounds(points: &[(f64, f64)]) -> Rect {
    points.iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |r, p| {
        (r.0.min(p.0), r.1.min(p.1), r.2.max(p.0), r.3.max(p.1))
    })
}

/// Whether two segments cross properly: neither touching at an end nor along each other.
fn crosses(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let side = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let (s1, s2) = (side(a, b, c), side(a, b, d));
    let (s3, s4) = (side(c, d, a), side(c, d, b));
    (s1 * s2 < 0.0) && (s3 * s4 < 0.0)
}

/// Fractions of the length of an edge where its label may go, from the preferred one: the middle,
/// then further and further from it.
const POSITIONS: &[f64] = &[0.5, 0.4, 0.6, 0.3, 0.7, 0.25, 0.75, 0.2, 0.8, 0.15, 0.85];

/// The halo of a moved label: a white outline of its letters, under them.
const HALO: &str = "stroke=\"white\" stroke-width=\"3\" stroke-linejoin=\"round\" paint-order=\"stroke\"";

/// Free space kept around a moved label, in points.
const MARGIN: f64 = 3.0;

/// Moves the label of each `on_edge` edge onto its drawn edge, at the position along it closest
/// to the middle where the label covers no node and no other label.
fn centered_labels(svg: &str) -> String {
    // The groups of the drawing: nodes and edges, those to move apart.
    let mut groups = Vec::new();
    let mut position = 0;
    while let Some(found) = svg[position..].find("<g id=\"") {
        let start = position + found;
        let Some(tag_end) = svg[start..].find('>').map(|i| start + i + 1) else { break };
        let class = attribute(&svg[start..tag_end], "class").unwrap_or_default();
        let kind = class.split(' ').next().unwrap_or_default();
        match (kind, group_end(svg, start)) {
            ("node" | "edge", Some(end)) => {
                groups.push((start, tag_end, end, kind == "node", class.split(' ').any(|c| c == ON_EDGE)));
                position = end;
            }
            _ => position = tag_end,
        }
    }
    if !groups.iter().any(|g| g.4) {
        return svg.to_owned();
    }
    // Obstacles: the outline of every node, the text of every label not to move.
    let mut obstacles: Vec<Rect> = Vec::new();
    for &(_, tag_end, end, node, on_edge) in &groups {
        let body = &svg[tag_end..end];
        if node {
            obstacles.extend(outline(body));
        }
        if !on_edge {
            obstacles.extend(text_lines(body).iter().map(|line| line.rect()));
        }
    }

    let mut out = String::with_capacity(svg.len() + 1024);
    let mut position = 0;
    for &(_, tag_end, end, _, on_edge) in groups.iter().filter(|g| g.4) {
        let _ = on_edge;
        out.push_str(&svg[position..tag_end]);
        let (body, placed) = moved(&svg[tag_end..end], &obstacles);
        out.push_str(&body);
        obstacles.extend(placed);
        position = end;
    }
    out.push_str(&svg[position..]);
    out
}

/// Length and half width of an arrowhead, as Graphviz draws them.
const ARROW: (f64, f64) = (10.0, 3.5);

/// Redraws the `straight` edges as a straight line between the borders of their nodes, the
/// arrowheads at the new ends; the other edges are left as Graphviz drew them. An end that is
/// not a visible node (a frame) keeps the end of the drawn edge.
fn straightened(svg: &str) -> String {
    if !svg.contains(STRAIGHT) {
        return svg.to_owned();
    }
    let mut nodes: HashMap<String, Rect> = HashMap::new();
    let mut edges = Vec::new();
    let mut position = 0;
    while let Some(found) = svg[position..].find("<g id=\"") {
        let start = position + found;
        let Some(tag_end) = svg[start..].find('>').map(|i| start + i + 1) else { break };
        let class = attribute(&svg[start..tag_end], "class").unwrap_or_default();
        let mut words = class.split(' ');
        match (words.next(), group_end(svg, start)) {
            (Some("node"), Some(end)) => {
                let body = &svg[tag_end..end];
                if let (Some(title), Some(rect)) = (title(body), outline(body)) {
                    nodes.insert(title, rect);
                }
                position = end;
            }
            (Some("edge"), Some(end)) => {
                if words.any(|c| c == STRAIGHT) {
                    edges.push((tag_end, end));
                }
                position = end;
            }
            _ => position = tag_end,
        }
    }
    let mut out = String::with_capacity(svg.len());
    let mut position = 0;
    for (tag_end, end) in edges {
        out.push_str(&svg[position..tag_end]);
        let body = &svg[tag_end..end];
        out.push_str(&straight_edge(body, &nodes).unwrap_or_else(|| body.to_owned()));
        position = end;
    }
    out.push_str(&svg[position..]);
    out
}

/// The body of an edge group redrawn as a straight line.
fn straight_edge(body: &str, nodes: &HashMap<String, Rect>) -> Option<String> {
    let title = title(body)?.replace("&#45;", "-").replace("&gt;", ">");
    let (source, target) = title.split_once("->").or_else(|| title.split_once("--"))?;
    let path_start = body.find("<path ")?;
    let path_tag = &body[path_start..path_start + body[path_start..].find('>')?];
    let drawn: Vec<(f64, f64)> = numbers(attribute(path_tag, "d")?).chunks(2).map(|c| (c[0], c[1])).collect();
    let (&first, &last) = (drawn.first()?, drawn.last()?);
    let center = |r: Rect| ((r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0);
    let (from, to) = (nodes.get(source).copied(), nodes.get(target).copied());
    let a = from.map_or(first, center);
    let b = to.map_or(last, center);
    // Each end on the border of its node, or where the drawn edge ended.
    let start = from.map_or(first, |r| border(r, a, b));
    let finish = to.map_or(last, |r| border(r, b, a));
    let length = ((finish.0 - start.0).powi(2) + (finish.1 - start.1).powi(2)).sqrt();
    if length < 1.0 {
        return None;
    }
    let unit = ((finish.0 - start.0) / length, (finish.1 - start.1) / length);

    // The arrowheads: polygons near the end (head) or the start (tail) of the drawn edge.
    let mut out = String::with_capacity(body.len());
    let (mut line_start, mut line_end) = (start, finish);
    let mut polygons = Vec::new();
    let mut from_index = 0;
    while let Some(i) = body[from_index..].find("<polygon ").map(|i| from_index + i) {
        let close = i + body[i..].find('>')?;
        polygons.push((i, close));
        from_index = close;
    }
    let mut shapes = Vec::new();
    for &(i, close) in &polygons {
        let tag = &body[i..close];
        let points: Vec<(f64, f64)> =
            numbers(attribute(tag, "points").unwrap_or_default()).chunks(2).map(|c| (c[0], c[1])).collect();
        let count = points.len().max(1) as f64;
        let middle = points.iter().fold((0.0, 0.0), |m, p| (m.0 + p.0 / count, m.1 + p.1 / count));
        let distance = |p: (f64, f64)| (p.0 - middle.0).powi(2) + (p.1 - middle.1).powi(2);
        let head = distance(last) <= distance(first);
        let (tip, direction) = if head { (finish, unit) } else { (start, (-unit.0, -unit.1)) };
        let base = (tip.0 - direction.0 * ARROW.0, tip.1 - direction.1 * ARROW.0);
        if head {
            line_end = base;
        } else {
            line_start = base;
        }
        let side = (-direction.1 * ARROW.1, direction.0 * ARROW.1);
        let shape = [(base.0 + side.0, base.1 + side.1), tip, (base.0 - side.0, base.1 - side.1)];
        let mut points: Vec<String> = shape.iter().map(|p| format!("{},{}", fmt(p.0), fmt(p.1))).collect();
        points.push(points[0].clone());
        shapes.push(replaced(tag, "points", &points.join(" ")));
    }

    // A cubic curve with its control points on the line: the labels are centered along it.
    let third =
        |t: f64| (line_start.0 + (line_end.0 - line_start.0) * t, line_start.1 + (line_end.1 - line_start.1) * t);
    let (c1, c2) = (third(1.0 / 3.0), third(2.0 / 3.0));
    let d = format!(
        "M{},{}C{},{} {},{} {},{}",
        fmt(line_start.0),
        fmt(line_start.1),
        fmt(c1.0),
        fmt(c1.1),
        fmt(c2.0),
        fmt(c2.1),
        fmt(line_end.0),
        fmt(line_end.1)
    );
    let mut at = 0;
    let path_end = path_start + path_tag.len();
    out.push_str(&body[at..path_start]);
    out.push_str(&replaced(path_tag, "d", &d));
    at = path_end;
    for (&(i, close), shape) in polygons.iter().zip(&shapes) {
        out.push_str(&body[at..i]);
        out.push_str(shape);
        at = close;
    }
    out.push_str(&body[at..]);
    Some(out)
}

/// Where the segment from `inside` (in the rectangle) towards `outside` leaves the rectangle.
fn border(r: Rect, inside: (f64, f64), outside: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (outside.0 - inside.0, outside.1 - inside.1);
    let along = |low: f64, high: f64, origin: f64, delta: f64| {
        if delta > 0.0 {
            (high - origin) / delta
        } else if delta < 0.0 {
            (low - origin) / delta
        } else {
            f64::INFINITY
        }
    };
    let t = along(r.0, r.2, inside.0, dx).min(along(r.1, r.3, inside.1, dy)).clamp(0.0, 1.0);
    (inside.0 + dx * t, inside.1 + dy * t)
}

/// The numbers of an attribute: the coordinates of a path or of the points of a polygon.
fn numbers(value: &str) -> Vec<f64> {
    value.split(|c: char| c == ',' || c == ' ' || c.is_ascii_alphabetic()).filter_map(|n| n.parse().ok()).collect()
}

/// A start tag with the value of an attribute replaced.
fn replaced(tag: &str, name: &str, value: &str) -> String {
    let key = format!(" {name}=\"");
    let Some(start) = tag.find(&key).map(|s| s + key.len()) else { return tag.to_owned() };
    let Some(end) = tag[start..].find('"').map(|e| start + e) else { return tag.to_owned() };
    format!("{}{value}{}", &tag[..start], &tag[end..])
}

/// The text of the `<title>` of a group.
fn title(body: &str) -> Option<String> {
    let start = body.find("<title>")? + "<title>".len();
    let end = start + body[start..].find("</title>")?;
    Some(body[start..end].to_owned())
}

/// A line of text: where its tag starts and ends in the group, its anchor, its size.
struct TextLine {
    start: usize,
    end: usize,
    x: f64,
    y: f64,
    size: f64,
    chars: usize,
    anchor_start: bool,
}

impl TextLine {
    /// Width of the line, from its number of characters (Helvetica, about 0.55 em each).
    fn width(&self) -> f64 {
        self.chars as f64 * self.size * 0.55
    }

    fn rect(&self) -> Rect {
        let left = if self.anchor_start { self.x } else { self.x - self.width() / 2.0 };
        (left, self.y - self.size * 0.8, left + self.width(), self.y + self.size * 0.25)
    }
}

fn text_lines(body: &str) -> Vec<TextLine> {
    let mut lines = Vec::new();
    let mut from = 0;
    while let Some(i) = body[from..].find("<text ").map(|i| from + i) {
        let Some(close) = body[i..].find("</text>").map(|c| i + c + "</text>".len()) else { break };
        let tag = &body[i..i + body[i..].find('>').unwrap_or(0)];
        let number = |name: &str| attribute(tag, name).and_then(|v| v.parse::<f64>().ok());
        let text = &body[i + tag.len() + 1..close - "</text>".len()];
        let chars =
            text.replace("&#45;", "-").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").chars().count();
        if let (Some(x), Some(y)) = (number("x"), number("y")) {
            lines.push(TextLine {
                start: i,
                end: close,
                x,
                y,
                size: number("font-size").unwrap_or(9.0),
                chars,
                anchor_start: attribute(tag, "text-anchor") == Some("start"),
            });
        }
        from = close;
    }
    lines
}

/// The rectangle around the shapes of a node: polygons, paths, ellipses.
fn outline(body: &str) -> Option<Rect> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for (i, _) in body.match_indices('<') {
        let Some(tag) = body[i..].find('>').map(|e| &body[i..i + e]) else { continue };
        let numbers = |value: &str| -> Vec<f64> {
            value
                .split(|c: char| c == ',' || c == ' ' || c.is_ascii_alphabetic())
                .filter_map(|n| n.parse().ok())
                .collect()
        };
        if tag.starts_with("<polygon ") {
            points.extend(
                numbers(attribute(tag, "points").unwrap_or_default())
                    .chunks(2)
                    .filter(|c| c.len() == 2)
                    .map(|c| (c[0], c[1])),
            );
        } else if tag.starts_with("<path ") {
            points.extend(
                numbers(attribute(tag, "d").unwrap_or_default())
                    .chunks(2)
                    .filter(|c| c.len() == 2)
                    .map(|c| (c[0], c[1])),
            );
        } else if tag.starts_with("<ellipse ") {
            let number = |name: &str| attribute(tag, name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            let (cx, cy, rx, ry) = (number("cx"), number("cy"), number("rx"), number("ry"));
            points.extend([(cx - rx, cy - ry), (cx + rx, cy + ry)]);
        }
    }
    let (first, rest) = points.split_first()?;
    Some(
        rest.iter().fold((first.0, first.1, first.0, first.1), |r, p| {
            (r.0.min(p.0), r.1.min(p.1), r.2.max(p.0), r.3.max(p.1))
        }),
    )
}

fn overlap(a: Rect, b: Rect) -> f64 {
    let width = a.2.min(b.2) - a.0.max(b.0);
    let height = a.3.min(b.3) - a.1.max(b.1);
    if width > 0.0 && height > 0.0 { width * height } else { 0.0 }
}

/// The body of an edge group, its texts moved onto the path, at the first position along it
/// where they cover no obstacle, or where they cover the least, outlined by a white halo; and
/// the rectangle they now take.
fn moved(body: &str, obstacles: &[Rect]) -> (String, Option<Rect>) {
    let Some(d) = body.find("<path ").and_then(|i| attribute(&body[i..i + body[i..].find('>')?], "d")) else {
        return (body.to_owned(), None);
    };
    let lines = text_lines(body);
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else { return (body.to_owned(), None) };
    let size = first.size;
    // The block: its width from the longest line, its height from the first baseline, raised by
    // the ascent, to the last one, lowered by the descent.
    let width = lines.iter().map(TextLine::width).fold(0.0, f64::max);
    let (top, bottom) = (first.y - size * 0.8, last.y + size * 0.25);
    let center = (first.x, (top + bottom) / 2.0);
    let block = |at: (f64, f64)| -> Rect {
        let dy = at.1 - center.1;
        (at.0 - width / 2.0 - 2.0, top + dy - 1.0, at.0 + width / 2.0 + 2.0, bottom + dy + 1.0)
    };
    // A margin around the block, the widths of the texts being estimated.
    let covered = |at: (f64, f64)| {
        let (l, t, r, b) = block(at);
        let around = (l - MARGIN, t - MARGIN, r + MARGIN, b + MARGIN);
        obstacles.iter().map(|&o| overlap(around, o)).sum::<f64>()
    };
    let candidates: Vec<(f64, f64)> = POSITIONS.iter().filter_map(|&f| point_at(d, f)).collect();
    let Some(&best) = candidates
        .iter()
        .find(|&&at| covered(at) == 0.0)
        .or_else(|| candidates.iter().min_by(|a, b| covered(**a).total_cmp(&covered(**b))))
    else {
        return (body.to_owned(), None);
    };
    let (dx, dy) = (best.0 - center.0, best.1 - center.1);
    let rect = block(best);

    let mut out = String::with_capacity(body.len() + 128);
    out.push_str(&body[..first.start]);
    let mut previous = first.start;
    for line in &lines {
        out.push_str(&body[previous..line.start]);
        let text = &body[line.start..line.end];
        let text = text.replacen(&format!(" x=\"{}\"", fmt(line.x)), &format!(" x=\"{:.2}\"", line.x + dx), 1);
        let text = text.replacen(&format!(" y=\"{}\"", fmt(line.y)), &format!(" y=\"{:.2}\"", line.y + dy), 1);
        // A white halo around the letters, drawn under them: the edge stays visible between the
        // letters and the lines, but does not cross the characters.
        let text = text.replacen("<text ", &format!("<text {HALO} "), 1);
        out.push_str(&text);
        previous = line.end;
    }
    out.push_str(&body[previous..]);
    (out, Some(rect))
}

/// A number as Graphviz writes it, to find it back in a tag: `202.99`, `-472.3`, `48`.
fn fmt(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_owned() } else { text.to_owned() }
}

/// The point at a fraction of the length of a path made of cubic Bézier curves (`M x,y C x,y x,y
/// x,y …`), as Graphviz draws edges.
fn point_at(d: &str, fraction: f64) -> Option<(f64, f64)> {
    let samples = samples(d);
    if samples.len() < 2 {
        return samples.first().copied();
    }
    let length = |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let total: f64 = samples.windows(2).map(|w| length(w[0], w[1])).sum();
    let goal = total * fraction;
    let mut walked = 0.0;
    for w in samples.windows(2) {
        let step = length(w[0], w[1]);
        if walked + step >= goal && step > 0.0 {
            let t = (goal - walked) / step;
            return Some((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
        }
        walked += step;
    }
    samples.last().copied()
}

/// The points of a path of cubic curves (`M x,y C x,y x,y x,y …`), each curve sampled 20 times.
fn samples(d: &str) -> Vec<(f64, f64)> {
    let numbers: Vec<f64> = d
        .split(|c: char| c == ',' || c == ' ' || c.is_ascii_alphabetic())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let points: Vec<(f64, f64)> = numbers.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect();
    if points.len() < 4 {
        return points;
    }
    let mut samples = vec![points[0]];
    for curve in points[1..].chunks(3).filter(|c| c.len() == 3) {
        let start = *samples.last().expect("a first point");
        for step in 1..=20 {
            let t = step as f64 / 20.0;
            let u = 1.0 - t;
            let along = |a: f64, b: f64, c: f64, d: f64| {
                u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
            };
            samples.push((
                along(start.0, curve[0].0, curve[1].0, curve[2].0),
                along(start.1, curve[0].1, curve[1].1, curve[2].1),
            ));
        }
    }
    samples
}

/// Moves the title of a frame that an edge crosses along the top of the frame, to where no edge
/// passes (a quarter in, three quarters in, the left, the right): Graphviz routes the edges
/// entering a frame through its title as through anything else. A title no position frees stays.
fn cleared_titles(svg: &str) -> String {
    let mut edges: Vec<Vec<(f64, f64)>> = Vec::new();
    // Each frame: the start of its tag, the end of the tag, the end of the group.
    let mut frames: Vec<(usize, usize, usize)> = Vec::new();
    let mut position = 0;
    while let Some(found) = svg[position..].find("<g id=\"") {
        let start = position + found;
        let Some(tag_end) = svg[start..].find('>').map(|i| start + i + 1) else { break };
        let class = attribute(&svg[start..tag_end], "class").unwrap_or_default();
        match (class.split(' ').next().unwrap_or_default(), group_end(svg, start)) {
            ("edge", Some(end)) => {
                let body = &svg[tag_end..end];
                let mut from = 0;
                while let Some(i) = body[from..].find("<path ").map(|i| from + i) {
                    let Some(close) = body[i..].find('>').map(|c| i + c) else { break };
                    if let Some(d) = attribute(&body[i..close], "d") {
                        edges.push(samples(d));
                    }
                    from = close;
                }
                position = end;
            }
            ("cluster", Some(end)) => {
                frames.push((start, tag_end, end));
                position = tag_end;
            }
            _ => position = tag_end,
        }
    }
    if frames.is_empty() || edges.is_empty() {
        return svg.to_owned();
    }
    let crossed = |rect: Rect| {
        let around = (rect.0 - MARGIN, rect.1 - MARGIN, rect.2 + MARGIN, rect.3 + MARGIN);
        edges.iter().flatten().any(|&(x, y)| around.0 <= x && x <= around.2 && around.1 <= y && y <= around.3)
    };
    // The moves: the absolute range of a text tag, and its new x.
    let mut moves: Vec<(usize, usize, f64, f64)> = Vec::new();
    for &(_, tag_end, end) in &frames {
        let body = &svg[tag_end..end];
        let lines = text_lines(body);
        let (Some(first), Some(frame)) = (lines.first(), outline(body)) else { continue };
        let block = lines
            .iter()
            .map(TextLine::rect)
            .fold(first.rect(), |a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)));
        if !crossed(block) {
            continue;
        }
        let width = block.2 - block.0;
        let (left, right) = (frame.0 + 6.0, frame.2 - 6.0);
        if right - left <= width {
            continue;
        }
        let span = right - left - width;
        let candidates = [0.25, 0.75, 0.0, 1.0, 0.125, 0.875, 0.375, 0.625, 0.5].map(|f| left + span * f);
        let Some(new_left) = candidates.iter().copied().find(|&l| {
            let dx = l - block.0;
            !crossed((block.0 + dx, block.1, block.2 + dx, block.3))
        }) else {
            continue;
        };
        let dx = new_left - block.0;
        for line in &lines {
            moves.push((tag_end + line.start, tag_end + line.end, line.x, line.x + dx));
        }
    }
    if moves.is_empty() {
        return svg.to_owned();
    }
    moves.sort_by_key(|m| m.0);
    let mut out = String::with_capacity(svg.len() + 64);
    let mut position = 0;
    for (start, end, x, new_x) in moves {
        out.push_str(&svg[position..start]);
        out.push_str(&svg[start..end].replacen(&format!(" x=\"{}\"", fmt(x)), &format!(" x=\"{new_x:.2}\""), 1));
        position = end;
    }
    out.push_str(&svg[position..]);
    out
}

/// Index of the `</g>` closing the group opening at `start`.
fn group_end(svg: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = start;
    loop {
        let open = svg[i..].find("<g").map(|o| i + o);
        let close = svg[i..].find("</g>").map(|c| i + c)?;
        match open {
            Some(o) if o < close && matches!(svg.as_bytes().get(o + 2), Some(b' ' | b'>')) => {
                depth += 1;
                i = o + 2;
            }
            Some(o) if o < close => i = o + 2,
            _ => {
                depth -= 1;
                if depth == 0 {
                    return Some(close);
                }
                i = close + 4;
            }
        }
    }
}

/// The value of an attribute in a start tag.
fn attribute<'t>(tag: &'t str, name: &str) -> Option<&'t str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = start + tag[start..].find('"')?;
    Some(&tag[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quality_counts_overlaps_and_crossings_and_the_best_is_chosen() {
        let frame = |id: &str, l: f64, t: f64, r: f64, b: f64| {
            format!(
                "<g id=\"{id}\" class=\"cluster\"><title>{id}</title><polygon points=\"{l},{t} {r},{t} {r},{b} {l},{b}\"/></g>
"
            )
        };
        let edge = |id: &str, d: &str| {
            format!(
                "<g id=\"{id}\" class=\"edge\"><title>{id}</title><path d=\"{d}\"/></g>
"
            )
        };
        // Two frames apart, two edges crossing like an X: no overlap, one crossing.
        let crossed = format!(
            "<svg>
{}{}{}{}</svg>",
            frame("c1", 0.0, 0.0, 100.0, 100.0),
            frame("c2", 200.0, 0.0, 300.0, 100.0),
            edge("e1", "M0,0 100,100"),
            edge("e2", "M0,100 100,0")
        );
        assert_eq!(quality(&crossed), (0, 1, 283));
        // Two frames overlapping, neither holding the other; two edges apart.
        let overlapping = format!(
            "<svg>
{}{}{}{}</svg>",
            frame("c1", 0.0, 0.0, 100.0, 100.0),
            frame("c2", 50.0, 50.0, 300.0, 100.0),
            edge("e1", "M0,0 100,0"),
            edge("e2", "M0,50 100,50")
        );
        assert_eq!(quality(&overlapping), (1, 0, 200));
        // A frame holding another is not an overlap.
        let nested = format!(
            "<svg>
{}{}</svg>",
            frame("c1", 0.0, 0.0, 100.0, 100.0),
            frame("c2", 10.0, 10.0, 50.0, 50.0)
        );
        assert_eq!(quality(&nested).0, 0);
        // No overlap beats a crossing; the best of a joined text is returned whole.
        assert_eq!(best_drawing(&[overlapping.as_str(), crossed.as_str()]), 1);
        assert_eq!(
            best_of(&format!(
                "{overlapping}
{crossed}"
            )),
            crossed
        );
    }

    #[test]
    fn a_frame_title_crossed_by_an_edge_moves_along_the_top() {
        // A frame 300 wide, its title centered at the top, an edge coming down through the title.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
<g id="graph0" class="graph">
<g id="clust1" class="cluster">
<title>cluster_0</title>
<g id="a_clust1"><a xlink:title="system sales">
<path fill="none" stroke="#999999" d="M12,-8C12,-8 312,-8 312,-8 318,-8 324,-14 324,-20 324,-20 324,-200 324,-200 324,-206 318,-212 312,-212 312,-212 12,-212 12,-212 6,-212 0,-206 0,-200 0,-200 0,-20 0,-20 0,-14 6,-8 12,-8"/>
<text xml:space="preserve" text-anchor="middle" x="162" y="-196.25" font-family="Helvetica,sans-Serif" font-size="11.00">sales</text>
</a>
</g>
</g>
<g id="edge1" class="edge">
<title>n1&#45;&gt;n2</title>
<path fill="none" stroke="black" d="M162,-240C162,-220 162,-180 162,-150"/>
</g>
</g>
</svg>"##;
        let out = cleared_titles(svg);
        assert!(!out.contains(r#"x="162" y="-196.25""#), "{out}");
        // A quarter in: 6 + (312 - 30) * 0.25, the width of the text being 5 × 11 × 0.55.
        assert!(out.contains(r#"x="91.56" y="-196.25""#), "{out}");
        // An edge elsewhere leaves the title alone.
        let elsewhere = svg.replace("M162,-240C162,-220 162,-180 162,-150", "M40,-240C40,-220 40,-180 40,-150");
        assert_eq!(cleared_titles(&elsewhere), elsewhere);
    }

    #[test]
    fn the_dark_theme_is_written_once() {
        let once = with_icons(SVG);
        assert_eq!(once.matches("prefers-color-scheme").count(), 1, "{once}");
        assert!(once.find("<style>").unwrap() < once.find("<g id=\"graph0\"").unwrap());
        let twice = with_icons(&once);
        assert_eq!(twice.matches("prefers-color-scheme").count(), 1);
    }

    /// A node and a frame as laid out by Graphviz (abridged).
    const SVG: &str = r##"<?xml version="1.0"?>
<svg width="200pt" height="90pt" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
<g id="graph0" class="graph" transform="translate(4 86)">
<g id="clust1" class="cluster kind_system">
<title>cluster_0</title>
<g id="a_clust1"><a xlink:title="system pricing">
<path fill="none" stroke="#999999" d="M20,-8"/>
<text xml:space="preserve" text-anchor="start" x="96.75" y="-67.25" font-family="Helvetica,sans-Serif" font-size="11.00">Pricing</text>
</a>
</g>
</g>
<g id="node1" class="node kind_app">
<title>n1</title>
<g id="a_node1"><a xlink:title="app pricing.ui">
<polygon fill="none" stroke="#1d4ed8" points="102.5,-52 16,-52 16,-16 102.5,-16 102.5,-52"/>
<text xml:space="preserve" text-anchor="start" x="48" y="-36.25" font-family="Helvetica,sans-Serif" font-size="10.00">Pricing UI</text>
<text xml:space="preserve" text-anchor="start" x="45" y="-24.25" font-family="Helvetica,sans-Serif" font-size="10.00">second line</text>
</a>
</g>
</g>
<g id="node2" class="node">
<title>e1</title>
<text x="1" y="1">jdbc</text>
</g>
</g>
</svg>
"##;

    #[test]
    fn draws_the_icons_in_their_squares() {
        let svg = with_icons(SVG);
        // One definition per kind used, right after the root element.
        assert!(svg.contains("xlink=\"http://www.w3.org/1999/xlink\">\n<defs>\n<symbol id=\"icon-app\""), "{svg}");
        assert_eq!(svg.matches("<symbol ").count(), 2, "{svg}");
        // The node icon: left of its widest line, centered on its two lines, in its color.
        assert!(
            svg.contains(
                r##"<use xlink:href="#icon-app" x="24.00" y="-41.95" width="16" height="16" style="color:#1d4ed8"/>"##
            ),
            "{svg}"
        );
        // Inside the node group, before it closes.
        let node = svg.find("<g id=\"node1\"").unwrap();
        let use_at = svg.find("#icon-app\" x").unwrap();
        let closing = node + svg[node..].find("</a>\n</g>\n").unwrap();
        assert!(use_at > closing && use_at < svg.find("<g id=\"node2\"").unwrap(), "{svg}");
        // The frame icon, in the frame color; nothing for a node without a kind class.
        assert!(svg.contains(r##"<use xlink:href="#icon-system" x="78.75" y="-78.32" width="14" height="14" style="color:#666666"/>"##), "{svg}");
        assert_eq!(svg.matches("<use ").count(), 2);
    }

    #[test]
    fn centers_the_labels_on_their_edges() {
        // A straight edge from (0, 0) to (100, -100), its label put beside it by Graphviz.
        let svg = r##"<svg>
<g id="edge1" class="edge on_edge">
<title>n1&#45;&gt;n2</title>
<g id="a_edge1"><a xlink:title="flow">
<path fill="none" stroke="black" d="M0,0C33.33,-33.33 66.67,-66.67 100,-100"/>
</a>
</g>
<text xml:space="preserve" text-anchor="middle" x="10" y="-80" font-family="Helvetica,sans-Serif" font-size="10.00">ab</text>
<text xml:space="preserve" text-anchor="middle" x="10" y="-70" font-family="Helvetica,sans-Serif" font-size="10.00">abcd</text>
</g>
</svg>"##;
        let out = with_icons(svg);
        // The middle of the edge is (50, -50); the block runs from -88 to -67.5, its center -77.75.
        assert!(out.contains(r#"x="50.00" y="-52.25""#) && out.contains(r#"x="50.00" y="-42.25""#), "{out}");
        // A halo around the letters, no background.
        assert!(
            out.contains(r#"<text stroke="white" stroke-width="3" stroke-linejoin="round" paint-order="stroke" "#),
            "{out}"
        );
        assert!(!out.contains("<rect"), "{out}");
        // Other edges are left alone.
        let plain = svg.replace(" on_edge", "");
        assert_eq!(with_icons(&plain).replace(DARK_STYLE, ""), plain);

        // A node on the middle of the edge: the label moves along it, to the first position where
        // it covers nothing, a quarter of its length.
        let crowded = svg.replace(
            "</svg>",
            "<g id=\"node1\" class=\"node\">\n<polygon fill=\"none\" stroke=\"black\" points=\"40,-40 60,-40 60,-60 40,-60 40,-40\"/>\n</g>\n</svg>",
        );
        let out = with_icons(&crowded);
        assert!(out.contains(r#"x="25.00" y="-27.25""#), "{out}");
    }

    #[test]
    fn straightens_the_straight_edges() {
        // Two nodes side by side, an edge curving over them, its arrowhead on the second one.
        let svg = r##"<svg>
<g id="node1" class="node">
<title>n1</title>
<polygon fill="none" stroke="black" points="0,0 20,0 20,20 0,20 0,0"/>
</g>
<g id="node2" class="node">
<title>n2</title>
<polygon fill="none" stroke="black" points="100,0 120,0 120,20 100,20 100,0"/>
</g>
<g id="edge1" class="edge straight">
<title>n1&#45;&gt;n2</title>
<path fill="none" stroke="black" d="M10,0C40,-60 70,-60 103,-1"/>
<polygon fill="black" stroke="black" points="99,-3 103,-1 101,2 99,-3"/>
</g>
</svg>"##;
        let out = with_icons(svg);
        // From the border of the first node to the base of the arrowhead, on the line between the
        // centers; the arrowhead at the border of the second node.
        assert!(out.contains(r#"d="M20,10C43.33,10 66.67,10 90,10""#), "{out}");
        assert!(out.contains(r#"points="90,13.5 100,10 90,6.5 90,13.5""#), "{out}");
        // Other edges are left alone.
        let plain = svg.replace(" straight", "");
        assert_eq!(with_icons(&plain).replace(DARK_STYLE, ""), plain);
    }

    #[test]
    fn leaves_an_svg_without_icons_alone() {
        let plain = SVG.replace(" kind_system", "").replace(" kind_app", "");
        assert_eq!(with_icons(&plain).replace(DARK_STYLE, ""), plain);
    }
}
