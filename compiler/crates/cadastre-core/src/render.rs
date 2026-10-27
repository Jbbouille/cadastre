//! Renderings of an evaluated view: DOT (and SVG through Graphviz), text, Mermaid (spec § 6).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

use crate::check::Model;
use crate::plan::{Plans, StyleRule, StyleTarget};
use crate::query::{Ctx, Subject};
use crate::registry::{Registry, ResourceId};
use crate::view::{self, Confidence, Direction, Exchange, Graph, GraphEdge, Orientation, Routing, Wrap};

mod ascii;
mod draw;
pub mod drawio;
#[cfg(feature = "elk")]
pub mod elk;
pub mod icons;
pub mod sequence;
mod svg;
pub mod table;
pub mod traversal;
pub mod xlsx;

pub use ascii::ascii;
pub use svg::{best_drawing, best_of, quality, with_icons};

/// The DOT of a view and its variants, which Graphviz draws all, the drawing that reads best
/// being kept (`best_drawing`). Graphviz orders the frames and nodes from the order they are
/// written in, and computes the ranks over the whole view (`newrank`) or frame by frame: on real
/// views, each choice wins somewhere and no rule tells which beforehand (the backup view of the
/// `tangled` example: 25 crossings as written, 1 in another order). The variants: the items of
/// the root as written, reversed, by connectivity (the item most linked to those already placed
/// next) and that reversed, the inside of the frames as written; then each of the three orders
/// applied at every depth; each with the ranks over the view and frame by frame. With `stack`,
/// `first` or `last`, which fix the ranks, the ranks stay frame by frame (their DOT has no
/// `newrank`) and only the order across them is tried; with `cross`, which fixes that order too
/// (its DOT cuts the crossing minimization short), the DOT alone. Each Graphviz layout takes
/// milliseconds.
pub fn dot_variants(dot: &str) -> Vec<String> {
    if dot.contains("mclimit=") || dot.contains("remincross=") {
        return vec![dot.to_owned()];
    }
    let mut orders = vec![dot.to_owned()];
    if let Some(tree) = DotTree::parse(dot) {
        use Order::{AsWritten, Connected, ConnectedReversed, Reversed};
        let pairs = [
            (Reversed, AsWritten),
            (Connected, AsWritten),
            (ConnectedReversed, AsWritten),
            (Reversed, Reversed),
            (Connected, Connected),
            (ConnectedReversed, ConnectedReversed),
        ];
        orders.extend(pairs.map(|(root, inner)| tree.write(root, inner)));
    }
    let mut out: Vec<String> = Vec::new();
    for order in orders {
        let plain = order.replacen(", newrank=true", "", 1);
        for variant in [order, plain] {
            if !out.contains(&variant) {
                out.push(variant);
            }
        }
    }
    out
}

/// An order of the items of a level of the DOT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    AsWritten,
    Reversed,
    /// The most linked item first, then each time the item most linked to those already placed
    /// (its degree, then its order written, on a tie).
    Connected,
    ConnectedReversed,
}

/// An item of a level of the DOT: a node (one line), or a frame (its opening lines, its items,
/// its closing lines). Each keeps its text, newlines included.
enum DotItem<'d> {
    Line(&'d str),
    Frame { open: &'d str, items: Vec<DotItem<'d>>, close: &'d str },
}

impl<'d> DotItem<'d> {
    /// The ids of the nodes it holds, at any depth.
    fn ids(&self, out: &mut Vec<&'d str>) {
        match self {
            Self::Line(line) => out.extend(line.trim_start().split(' ').next()),
            Self::Frame { items, .. } => items.iter().for_each(|item| item.ids(out)),
        }
    }
}

/// The DOT of a view as a tree of items: what comes before the root items (the graph, node and
/// edge defaults), the items, and what comes after (the edges, the hints, the legend), with the
/// visible edges as pairs of ids, for the order by connectivity.
struct DotTree<'d> {
    head: &'d str,
    items: Vec<DotItem<'d>>,
    tail: &'d str,
    edges: Vec<(&'d str, &'d str)>,
}

impl<'d> DotTree<'d> {
    fn parse(dot: &'d str) -> Option<Self> {
        let lines: Vec<&str> = dot.split_inclusive('\n').collect();
        let mut offsets = Vec::with_capacity(lines.len() + 1);
        let mut at = 0;
        for line in &lines {
            offsets.push(at);
            at += line.len();
        }
        offsets.push(at);
        let mut i = lines.iter().position(|line| is_item(line, 2))?;
        let head = &dot[..offsets[i]];
        let items = parse_items(dot, &lines, &offsets, &mut i, 2);
        let tail = &dot[offsets[i]..];
        let edges = tail
            .lines()
            .filter(|line| line.contains(" -> ") && !line.contains("style=invis"))
            .filter_map(|line| {
                let mut ends = line.trim_start().split(" -> ");
                let a = ends.next()?;
                let b = ends.next()?.split([' ', ';']).next()?;
                Some((a, b))
            })
            .collect();
        Some(Self { head, items, tail, edges })
    }

    /// The DOT with the items of the root in one order, those inside the frames in another.
    fn write(&self, root: Order, inner: Order) -> String {
        let mut out = String::from(self.head);
        self.write_items(&self.items, root, inner, &mut out);
        out.push_str(self.tail);
        out
    }

    fn write_items(&self, items: &[DotItem<'d>], order: Order, inner: Order, out: &mut String) {
        for item in self.arrange(items, order) {
            match item {
                DotItem::Line(line) => out.push_str(line),
                DotItem::Frame { open, items, close } => {
                    out.push_str(open);
                    self.write_items(items, inner, inner, out);
                    out.push_str(close);
                }
            }
        }
    }

    fn arrange<'a>(&self, items: &'a [DotItem<'d>], order: Order) -> Vec<&'a DotItem<'d>> {
        let written: Vec<&DotItem> = items.iter().collect();
        let connected = |written: Vec<&'a DotItem<'d>>| {
            let mut owner: HashMap<&str, usize> = HashMap::new();
            for (index, item) in written.iter().enumerate() {
                let mut ids = Vec::new();
                item.ids(&mut ids);
                owner.extend(ids.into_iter().map(|id| (id, index)));
            }
            let mut weight: BTreeMap<(usize, usize), usize> = BTreeMap::new();
            for (a, b) in &self.edges {
                if let (Some(&x), Some(&y)) = (owner.get(a), owner.get(b))
                    && x != y
                {
                    *weight.entry((x.min(y), x.max(y))).or_default() += 1;
                }
            }
            let link = |a: usize, b: usize| weight.get(&(a.min(b), a.max(b))).copied().unwrap_or(0);
            let degree = |a: usize| (0..written.len()).map(|b| link(a, b)).sum::<usize>();
            let mut left: Vec<usize> = (0..written.len()).collect();
            let mut placed: Vec<usize> = Vec::new();
            while !left.is_empty() {
                let score =
                    |k: usize| (placed.iter().map(|&o| link(k, o)).sum::<usize>(), degree(k), std::cmp::Reverse(k));
                let (position, _) = left.iter().enumerate().max_by_key(|&(_, &k)| score(k)).expect("an item left");
                placed.push(left.remove(position));
            }
            placed.into_iter().map(|k| written[k]).collect::<Vec<_>>()
        };
        match order {
            Order::AsWritten => written,
            Order::Reversed => written.into_iter().rev().collect(),
            Order::Connected => connected(written),
            Order::ConnectedReversed => connected(written).into_iter().rev().collect(),
        }
    }
}

/// Whether a line of the DOT starts an item at this indentation: a frame, or a node whose id is
/// one the renderer writes (`n12`, `n12s3`, `e4`, `t0`).
fn is_item(line: &str, indent: usize) -> bool {
    let Some(rest) = line.get(indent..).filter(|_| line.as_bytes()[..indent].iter().all(|&b| b == b' ')) else {
        return false;
    };
    if rest.starts_with("subgraph cluster_") {
        return true;
    }
    let id = rest.split(' ').next().unwrap_or_default();
    id.len() > 1
        && matches!(id.as_bytes()[0], b'n' | b'e' | b't')
        && id.as_bytes()[1].is_ascii_digit()
        && rest[id.len()..].starts_with(" [")
}

/// The items of a level, from line `i` on, as long as lines start items at this indentation.
fn parse_items<'d>(dot: &'d str, lines: &[&str], offsets: &[usize], i: &mut usize, indent: usize) -> Vec<DotItem<'d>> {
    let mut items = Vec::new();
    while *i < lines.len() && is_item(lines[*i], indent) {
        let start = *i;
        if !lines[start][indent..].starts_with("subgraph cluster_") {
            *i += 1;
            items.push(DotItem::Line(&dot[offsets[start]..offsets[*i]]));
            continue;
        }
        let closing = format!("{}}}", " ".repeat(indent));
        // The opening: the subgraph line and its attributes, up to its first item.
        *i += 1;
        while *i < lines.len() && !is_item(lines[*i], indent + 2) && lines[*i].trim_end() != closing {
            *i += 1;
        }
        let open = &dot[offsets[start]..offsets[*i]];
        let inner = parse_items(dot, lines, offsets, i, indent + 2);
        // The closing: what is left up to the closing brace, included.
        let close_start = *i;
        while *i < lines.len() && lines[*i].trim_end() != closing {
            *i += 1;
        }
        *i = (*i + 1).min(lines.len());
        items.push(DotItem::Frame { open, items: inner, close: &dot[offsets[close_start]..offsets[*i]] });
    }
    items
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Solid,
    Dashed,
    Dotted,
}

impl Line {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "solid" => Some(Self::Solid),
            "dashed" => Some(Self::Dashed),
            "dotted" => Some(Self::Dotted),
            _ => None,
        }
    }

    fn dot(self) -> Option<&'static str> {
        match self {
            Self::Solid => None,
            Self::Dashed => Some("dashed"),
            Self::Dotted => Some("dotted"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NodeStyle {
    /// Outline (and, for frames, border).
    pub color: Option<String>,
    /// Background.
    pub fill: Option<String>,
    pub shape: String,
    pub line: Line,
}

#[derive(Clone, Debug)]
pub struct EdgeStyle {
    pub color: Option<String>,
    pub line: Line,
    /// `layout: free`: drawn, but left out of the layout (it places no node).
    pub free: bool,
    /// `routing: straight`: redrawn as a straight line after the layout (SVG).
    pub straight: bool,
    pub thin: bool,
    pub hollow_arrow: bool,
}

#[derive(Clone, Debug)]
pub struct LegendEntry {
    pub label: String,
    /// What the rule styles: a filled square for nodes and frames, a line for edges.
    pub edges: bool,
    pub color: Option<String>,
    pub fill: Option<String>,
    pub line: Option<Line>,
}

/// The nodes of a scenario of failures (`failing`): their border, the fill of those failing, of
/// those unavailable through them.
const DOWN_COLOR: &str = "#b91c1c";
const FAILING_FILL: &str = "#fca5a5";
const DOWN_FILL: &str = "#fee2e2";

/// A style of the default theme in the legend: its label, its line, the key a rule must be about
/// to explain it, and what it concerns.
type Theme<'a, T> = (&'static str, Line, &'static str, &'a dyn Fn(T) -> bool);

/// Everything a renderer needs about a view.
/// From how many edges a diagram leaves the labels of its edges to the tooltips, unless the view
/// writes `edge_labels`: beyond, the labels are what makes it unreadable.
pub const DENSE_EDGES: usize = 20;

pub struct Scene<'a> {
    pub graph: &'a Graph,
    pub registry: &'a Registry,
    pub model: &'a Model,
    pub rules: Vec<&'a StyleRule>,
    /// Where the nodes and frames link to, for the site: a prefix before the file name of the
    /// page of each resource (`../resources/`); `None`: their `url` or `console`.
    pub page_links: Option<String>,
    /// The language of the words the drawing adds: the legend, the tooltips.
    pub language: crate::i18n::Language,
    /// The satellites of the drawing (`satellites`).
    satellites: Satellites,
    /// The boxes of twins of the drawing: nodes of one kind with the same edges, drawn as one
    /// box listing them (`twins`); each box, its members in the order of their names, the first
    /// one standing for them all in the edges.
    pub twins: Vec<Vec<Drawn>>,
}

/// A node as drawn in one place: itself, or one of its satellite copies, by its DOT id.
#[derive(Clone, Debug)]
pub struct Drawn {
    pub node: ResourceId,
    pub id: String,
    /// The frame of a satellite copy.
    pub satellite: Option<usize>,
}

impl<'a> Scene<'a> {
    pub fn new(graph: &'a Graph, plans: &'a Plans, registry: &'a Registry, model: &'a Model) -> Self {
        let view = plans.views.iter().find(|v| v.name == graph.view);
        let rules = plans
            .project_styles
            .iter()
            .take(1)
            .flat_map(|b| b.rules.iter())
            .chain(view.into_iter().flat_map(|v| v.style.iter()))
            .collect();
        let mut scene = Self {
            graph,
            registry,
            model,
            rules,
            page_links: None,
            language: Default::default(),
            satellites: Satellites::new(),
            twins: Vec::new(),
        };
        scene.satellites = satellites(&scene);
        scene.twins = twins(&scene);
        scene
    }

    /// The box of twins a drawn node (by its DOT id) is in, when it is in one.
    fn twin_of(&self, id: &str) -> Option<usize> {
        self.twins.iter().position(|members| members.iter().any(|m| m.id == id))
    }

    /// The DOT id of a drawn node: that of its box of twins when it is in one.
    fn twin_id(&self, id: String) -> String {
        match self.twin_of(&id) {
            Some(t) => format!("t{t}"),
            None => id,
        }
    }

    /// The DOT ids of the two ends of an edge as drawn, before the twins: the node itself, or
    /// its satellite copy in the frame of the other end.
    fn end_ids(&self, edge: &GraphEdge) -> [String; 2] {
        let (replaced, _) = satellite_ends(self, &self.satellites, edge);
        [edge.source, edge.target].map(|end| match &replaced {
            Some((node, id)) if *node == end => id.clone(),
            _ => node_id(end),
        })
    }

    /// Whether an edge is drawn: the first member of a box of twins stands for the edges of all.
    fn edge_drawn(&self, edge: &GraphEdge) -> bool {
        self.end_ids(edge).iter().all(|id| self.twin_of(id).is_none_or(|t| self.twins[t][0].id == *id))
    }

    /// The DOT id of a node: that of its box of twins when it is in one.
    fn dot_id(&self, id: ResourceId) -> String {
        self.twin_id(node_id(id))
    }

    /// The words the drawing adds, in this language.
    pub fn with_language(mut self, language: crate::i18n::Language) -> Self {
        self.language = language;
        self
    }

    /// A word of the drawing in its language.
    fn t(&self, english: &'static str) -> &'static str {
        self.language.text(english)
    }

    /// The nodes and frames linking to the pages of their resources, under `prefix`.
    pub fn with_page_links(mut self, prefix: &str) -> Self {
        self.page_links = Some(prefix.to_owned());
        self
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx { registry: self.registry, model: self.model, down: &self.graph.down }
    }

    fn name(&self, id: ResourceId) -> &str {
        &self.registry.resource(id).name
    }

    /// Displayed node text: `label` (or the first host name of a virtual IP, or the name), then
    /// the extra `node_labels` attributes.
    fn node_lines(&self, id: ResourceId) -> Vec<String> {
        let ctx = self.ctx();
        let resource = self.registry.resource(id);
        let host = || {
            (resource.kind == "vip")
                .then(|| resource.attribute("host"))
                .flatten()
                .and_then(|hosts| hosts.value.items().into_iter().find_map(|v| v.as_host()))
        };
        let first = view::attribute_text(&ctx, id, "label")
            .into_iter()
            .next()
            .or_else(host)
            .unwrap_or_else(|| self.name(id).to_owned());
        self.with_node_labels(id, first)
    }

    /// Frame text: name and key attribute, `dmz (10.0.1.0/24)`, then the extra `node_labels` attributes.
    fn group_lines(&self, id: ResourceId) -> Vec<String> {
        let ctx = self.ctx();
        let resource = self.registry.resource(id);
        let base = view::attribute_text(&ctx, id, "label").into_iter().next().unwrap_or_else(|| resource.name.clone());
        let key = match resource.kind.as_str() {
            "subnet" => resource.attribute("cidr"),
            "site" => resource.attribute("hosting"),
            _ => None,
        };
        let first = match key {
            Some(attribute) => format!("{base} ({})", attribute.value.raw),
            None => base,
        };
        self.with_node_labels(id, first)
    }

    fn with_node_labels(&self, id: ResourceId, first: String) -> Vec<String> {
        let ctx = self.ctx();
        let mut lines = vec![first];
        for key in self.graph.options.node_labels.iter().filter(|k| *k != "label") {
            let values = view::attribute_text(&ctx, id, key);
            if !values.is_empty() {
                lines.push(values.join(", "));
            }
        }
        lines
    }

    /// The label of an edge in a diagram: none on a dense diagram, unless the view writes
    /// `edge_labels`; the text and table renderings keep every label.
    fn diagram_label(&self, edge: &GraphEdge) -> String {
        if self.labels_hidden() { String::new() } else { self.edge_label(edge) }
    }

    fn diagram_label_line(&self, edge: &GraphEdge) -> String {
        self.diagram_label(edge).replace('\n', ", ")
    }

    /// Whether the labels of the edges are left to the tooltips: a diagram of more than
    /// [`DENSE_EDGES`] edges whose view does not write `edge_labels`.
    pub fn labels_hidden(&self) -> bool {
        !self.graph.options.edge_labels_written && self.graph.edges.len() > DENSE_EDGES
    }

    fn edge_label(&self, edge: &GraphEdge) -> String {
        // An edge merging enough links: a summary, the detail being in the tooltip.
        let merged = edge.provenance.len();
        if self.graph.options.summary_from.is_some_and(|from| merged >= from) {
            let protocols: BTreeSet<&str> = edge.ports.iter().map(|p| p.split('/').next().unwrap_or(p)).collect();
            let mut summary = match edge.relation.as_ref() {
                Some(relation) => format!("{relation} ×{merged}"),
                None if !protocols.is_empty() && protocols.len() <= 3 => {
                    format!("{} ×{merged}", protocols.into_iter().collect::<Vec<_>>().join(", "))
                }
                None => format!("×{merged}"),
            };
            if edge.confidence == Confidence::Doubtful {
                summary.push_str(" ?");
            }
            return summary;
        }
        let relation = edge.relation.as_ref().map(|relation| {
            if edge.provenance.len() > 1 { format!("{relation} ×{}", edge.provenance.len()) } else { relation.clone() }
        });
        let mut parts = Vec::new();
        for kind in &self.graph.options.edge_labels {
            match kind.as_str() {
                "relation" => parts.extend(relation.clone()),
                "port" if edge.relation.is_none() => parts.extend(edge.ports.iter().cloned()),
                "carries" => parts.extend(edge.carries.iter().map(|&c| self.name(c).to_owned())),
                "through" => parts.extend(edge.through.iter().map(|&t| format!("via {}", self.name(t)))),
                "description" => parts.extend(edge.descriptions.iter().cloned()),
                "port" | "step" | "none" => {}
                key => {
                    // Any other key written on the links or the connectors of the edge
                    // (`circuit_id`), or on the ends of the references it draws, the end carrying
                    // the reference first (the interfaces of a `peer` cable); for that end, `label`
                    // falls back to its name.
                    let links = edge.links.iter().map(|&l| &self.registry.links[l].attributes);
                    let connectors = edge.connectors.iter().map(|&c| &self.registry.resource(c).attributes);
                    let mut texts: Vec<String> = links
                        .chain(connectors)
                        .flatten()
                        .filter(|a| a.key == key)
                        .flat_map(|a| a.value.items().into_iter().map(crate::query::value_text).collect::<Vec<_>>())
                        .collect();
                    for &(holder, designated) in &edge.declared {
                        for end in [holder, designated] {
                            let resource = self.registry.resource(end);
                            match resource.attribute(key) {
                                Some(a) => texts.extend(a.value.items().into_iter().map(crate::query::value_text)),
                                None if key == "label" && end == holder => texts.push(resource.name.clone()),
                                None => {}
                            }
                        }
                    }
                    for text in texts {
                        if !parts.contains(&text) {
                            parts.push(text);
                        }
                    }
                }
            }
        }
        if !self.graph.options.edge_labels.iter().any(|k| k == "through") {
            parts.extend(edge.via.iter().map(|&v| format!("via {}", self.name(v))));
        }
        // A relation edge (`peer`, a connector…) is named after it when no other label applies,
        // unless the view asks for keys of its own (`[label, circuit_id]`): a cable without them
        // stays bare.
        let own_keys = self
            .graph
            .options
            .edge_labels
            .iter()
            .any(|k| !crate::vocabulary::words(crate::vocabulary::EDGE_LABELS).any(|w| w == k));
        if parts.is_empty() && !own_keys {
            parts.extend(relation);
        }
        if let Some(step) = edge.step.filter(|_| self.graph.options.edge_labels.iter().any(|k| k == "step")) {
            match parts.first_mut() {
                Some(first) => *first = format!("{step}. {first}"),
                None => parts.push(format!("{step}.")),
            }
        }
        if edge.confidence == Confidence::Doubtful {
            match parts.last_mut() {
                Some(last) => last.push_str(" ?"),
                None => parts.push("?".to_owned()),
            }
        }
        wrap(&parts, self.graph.options.wrap)
    }

    /// The label of an edge on a single line, for the text output and ASCII art.
    fn edge_label_line(&self, edge: &GraphEdge) -> String {
        self.edge_label(edge).replace('\n', ", ")
    }

    fn node_tooltip(&self, id: ResourceId) -> String {
        let resource = self.registry.resource(id);
        let virtual_ = if resource.is_virtual() { "virtual " } else { "" };
        let mut lines = vec![format!("{virtual_}{} {}", resource.kind, self.registry.qualified_name(id))];
        for attribute in &resource.attributes {
            let values: Vec<String> = attribute.value.items().into_iter().map(crate::query::value_text).collect();
            let plain = (attribute.key == "schedule")
                .then(|| crate::schedule::plain(self.registry, self.model, id))
                .flatten()
                .map(|p| format!(" ({p})"))
                .unwrap_or_default();
            lines.push(format!("{}: {}{plain}", attribute.key, values.join(", ")));
        }
        lines.join("\n")
    }

    /// The link a click on a resource opens: its first `url`, otherwise its first `console`.
    fn node_link(&self, id: ResourceId) -> Option<String> {
        if let Some(prefix) = &self.page_links {
            return Some(format!("{prefix}{}.html", crate::site::file_name(&self.registry.qualified_name(id))));
        }
        let resource = self.registry.resource(id);
        ["url", "console"].into_iter().filter_map(|key| resource.attribute(key)).find_map(|attribute| {
            attribute.value.items().into_iter().find_map(|item| match &item.kind {
                crate::registry::ValueKind::String(text) if crate::net::check_url(text).is_ok() => Some(text.clone()),
                _ => None,
            })
        })
    }

    fn edge_tooltip(&self, edge: &GraphEdge) -> String {
        let mut lines = Vec::new();
        if let Some(step) = edge.step {
            lines.push(format!("step: {step}"));
        }
        lines.extend(edge.descriptions.iter().cloned());
        if !edge.carries.is_empty() {
            lines.push(format!("carries: {}", self.names(&edge.carries)));
        }
        let deduced = if edge.through_origins.is_empty() {
            String::new()
        } else {
            format!(" (deduced: {})", edge.through_origins.iter().cloned().collect::<Vec<_>>().join(", "))
        };
        if edge.provenance.len() == 1 && !edge.through_steps.is_empty() {
            // Deduced from the cabling: each step with its variants.
            let steps: Vec<String> = edge
                .through_steps
                .iter()
                .map(|step| step.iter().map(|&t| self.name(t)).collect::<Vec<_>>().join(" or "))
                .collect();
            lines.push(format!("through: {}{deduced}", steps.join(", ")));
        } else if edge.provenance.len() == 1 && !edge.through_path.is_empty() {
            let path: Vec<&str> = edge.through_path.iter().map(|&t| self.name(t)).collect();
            lines.push(format!("through: {}{deduced}", path.join(", ")));
        } else if !edge.through.is_empty() {
            lines.push(format!("through: {}{deduced}", self.names(&edge.through)));
        }
        lines.extend(edge.questions.iter().map(|q| format!("question: {q}")));
        lines.extend(edge.provenance.iter().map(|p| format!("from: {p}")));
        lines.join("\n")
    }

    fn names(&self, ids: &BTreeSet<ResourceId>) -> String {
        ids.iter().map(|&i| self.name(i)).collect::<Vec<_>>().join(", ")
    }

    /// The fill of a node in the scenario of the view and its label in the legend: failing, or
    /// unavailable through what fails.
    fn down_style(&self, id: ResourceId) -> Option<(&'static str, &'static str)> {
        if self.graph.failing.contains(&id) {
            Some((FAILING_FILL, "Failing"))
        } else if self.graph.down.contains(&id) {
            Some((DOWN_FILL, "Unavailable"))
        } else {
            None
        }
    }

    pub fn node_style(&self, id: ResourceId) -> NodeStyle {
        let resource = self.registry.resource(id);
        let attr = |key: &str| resource.attribute(key).map(|a| a.value.raw.as_str());
        let mut style = NodeStyle {
            color: None,
            fill: None,
            shape: match self.registry.schema.base(&resource.kind) {
                "database" => "cylinder",
                "saas" => "rounded",
                "actor" => "ellipse",
                "job" => "hexagon",
                _ => "box",
            }
            .to_owned(),
            line: match (attr("scope"), attr("confidence")) {
                (_, Some("doubtful")) => Line::Dotted,
                (Some("external"), _) | (_, Some("assumed")) => Line::Dashed,
                _ => Line::Solid,
            },
        };
        // A scenario of failures: what fails, and what is unavailable through it, in red.
        if let Some((fill, _)) = self.down_style(id) {
            style.color = Some(DOWN_COLOR.to_owned());
            style.fill = Some(fill.to_owned());
        }
        for rule in self.rules.iter().filter(|rule| self.node_rule_applies(rule, id)) {
            for property in &rule.properties {
                let value = crate::query::value_text(&property.value);
                match property.key.as_str() {
                    "color" => style.color = Some(value),
                    "fill" => style.fill = Some(value),
                    "shape" => style.shape = value,
                    "line" => style.line = Line::parse(&value).unwrap_or(style.line),
                    _ => {}
                }
            }
        }
        style
    }

    pub fn edge_style(&self, edge: &GraphEdge) -> EdgeStyle {
        let connector = edge
            .relation
            .as_deref()
            .is_some_and(|r| self.registry.schema.kind_def(r).is_some_and(|d| d.connector.is_some()));
        let mut style = EdgeStyle {
            color: None,
            line: if connector || edge.confidence == Confidence::Doubtful {
                Line::Dotted
            } else if edge.confidence == Confidence::Assumed || edge.layers.contains("trigger") {
                Line::Dashed
            } else {
                Line::Solid
            },
            thin: edge.layers.contains("auth"),
            hollow_arrow: edge.layers.contains("trigger"),
            free: false,
            straight: false,
        };
        for rule in self.rules.iter().filter(|rule| self.edge_rule_applies(rule, edge)) {
            for property in &rule.properties {
                let value = crate::query::value_text(&property.value);
                match property.key.as_str() {
                    "color" => style.color = Some(value),
                    "line" => style.line = Line::parse(&value).unwrap_or(style.line),
                    "layout" => style.free = value == "free",
                    "routing" => style.straight = value == "straight",
                    _ => {}
                }
            }
        }
        style
    }

    /// The `stack` render option: its levels of frames or nodes, in its order. A name designates
    /// a frame or a node of the view by a suffix of its qualified name; others are left out, and
    /// so are levels left empty.
    fn stack(&self) -> Vec<Vec<Placed>> {
        self.graph
            .options
            .stack
            .iter()
            .map(|level| level.iter().filter_map(|name| self.placed(name)).collect::<Vec<_>>())
            .filter(|level| !level.is_empty())
            .collect()
    }

    /// The frame or the node a name of a render option designates; a frame first, and a node
    /// drawn as a frame is that frame.
    fn placed(&self, name: &str) -> Option<Placed> {
        let frame = (0..self.graph.groups.len()).find(|&g| self.frame_designated(name, g));
        let node = || self.graph.nodes.iter().copied().find(|&n| self.designates(name, n));
        frame.map(Placed::Frame).or_else(|| {
            node().map(|n| match self.node_frame(n) {
                Some(f) => Placed::Frame(f),
                None => Placed::Node(n),
            })
        })
    }

    /// The frame holding an item (`None`: the top level).
    fn container(&self, placed: Placed) -> Option<usize> {
        match placed {
            Placed::Frame(g) => self.graph.groups[g].parent,
            Placed::Node(n) => self.graph.node_group.get(&n).copied(),
        }
    }

    /// Whether a name written in a render option designates a resource: a suffix of its
    /// qualified name.
    fn designates(&self, name: &str, id: ResourceId) -> bool {
        view::designates(self.registry, name, id)
    }

    /// Whether a name written in a render option designates a frame: the name of a zone, or a
    /// name designating the resource of the frame.
    fn frame_designated(&self, name: &str, group: usize) -> bool {
        let group = &self.graph.groups[group];
        if let Some(value) = &group.value {
            return value == name;
        }
        match (&group.zone, group.resource) {
            (Some(zone), _) => zone.name == name,
            (None, Some(resource)) => self.designates(name, resource),
            (None, None) => false,
        }
    }

    /// The text of a frame: a zone's label, a node drawn as a frame, or a group frame.
    pub(crate) fn frame_lines(&self, group: usize) -> Vec<String> {
        let group = &self.graph.groups[group];
        if let Some(value) = &group.value {
            return vec![value.clone()];
        }
        match (&group.zone, group.resource) {
            (Some(zone), _) => vec![zone.label.clone()],
            (None, Some(resource)) if self.node_frame(resource).is_some() => self.node_lines(resource),
            (None, Some(resource)) => self.group_lines(resource),
            (None, None) => Vec::new(),
        }
    }

    /// The style of a frame: a zone is dashed and grey unless its properties say otherwise.
    fn frame_style(&self, group: usize) -> NodeStyle {
        let group = &self.graph.groups[group];
        let Some(zone) = &group.zone else {
            // A frame of values: the default frame, rounded, its border grey.
            let plain = NodeStyle { color: None, fill: None, shape: "rounded".to_owned(), line: Line::Solid };
            return group.resource.map_or(plain, |r| self.node_style(r));
        };
        self.zone_style(&zone.properties)
    }

    fn zone_style(&self, properties: &[crate::registry::Attribute]) -> NodeStyle {
        let mut style = NodeStyle { color: None, fill: None, shape: "rounded".to_owned(), line: Line::Dashed };
        for property in properties {
            let value = crate::query::value_text(&property.value);
            match property.key.as_str() {
                "color" => style.color = Some(value),
                "fill" => style.fill = Some(value),
                "line" => style.line = Line::parse(&value).unwrap_or(style.line),
                _ => {}
            }
        }
        style
    }

    /// Whether a frame holds a node, itself or in a nested frame: a `group-by` frame whose
    /// members all went into zones is not drawn.
    fn has_nodes(&self, group: usize) -> bool {
        self.graph.groups[group].resource.is_some_and(|r| self.node_frame(r).is_some())
            || self.graph.nodes.iter().any(|&n| self.within(self.graph.node_group.get(&n).copied(), group))
    }

    /// The children of a frame (`None`: the root) in drawing order. By default, the frames then
    /// the nodes; with the `cross` render option, the ones it names first, in its order across
    /// the direction of the edges (top to bottom in `lr` / `rl`, left to right in `tb` / `bt`),
    /// the others after them.
    ///
    /// `dot`: the order to write them in DOT. Graphviz lays a graph out top to bottom, then turns
    /// it for `lr` / `rl`: the first frame or node written in a rank ends up at the bottom.
    fn ordered_content(&self, frame: Option<usize>, dot: bool) -> Vec<Placed> {
        let (groups, nodes) = self.frame_content(frame);
        let items: Vec<Placed> =
            groups.into_iter().map(Placed::Frame).chain(nodes.into_iter().map(Placed::Node)).collect();
        let cross = &self.graph.options.cross;
        if cross.is_empty() {
            return items;
        }
        let index = |placed: &Placed| match placed {
            Placed::Frame(g) => cross.iter().position(|name| self.frame_designated(name, *g)),
            Placed::Node(n) => cross.iter().position(|name| self.designates(name, *n)),
        };
        let (mut listed, others): (Vec<Placed>, Vec<Placed>) = items.into_iter().partition(|p| index(p).is_some());
        listed.sort_by_key(|p| index(p));
        if dot && !self.graph.options.direction.vertical() {
            listed.reverse();
            others.into_iter().chain(listed).collect()
        } else {
            listed.into_iter().chain(others).collect()
        }
    }

    /// The nodes to attach a layout hint to: the node itself, or the nodes of a frame at its
    /// last ranks (`last`: no edge inside the frame leaves them) or at its first ranks (no edge
    /// inside the frame reaches them). Hints from the last nodes of a frame to the first nodes of
    /// the next one place the whole next frame after it, whatever the depth of both.
    fn anchors(&self, placed: Placed, last: bool) -> Vec<ResourceId> {
        let group = match placed {
            Placed::Node(id) => return vec![id],
            Placed::Frame(group) => group,
        };
        let inside = |n: ResourceId| {
            std::iter::successors(self.graph.node_group.get(&n).copied(), |&g| self.graph.groups[g].parent)
                .any(|g| g == group)
        };
        let internal: Vec<&GraphEdge> = self
            .graph
            .edges
            .iter()
            .filter(|e| e.source != e.target && inside(e.source) && inside(e.target) && !self.edge_style(e).free)
            .collect();
        self.graph
            .nodes
            .iter()
            .copied()
            .filter(|&n| inside(n))
            .filter(|&n| !internal.iter().any(|e| if last { e.source == n } else { e.target == n }))
            .collect()
    }

    /// Pairs of items of consecutive `stack` levels, in the order of the layout ranks: an edge
    /// from the first to the second puts the first one higher (`tb`) or more to the left (`lr`);
    /// `bt` and `rl` reverse the ranks. Every item of a level comes before every item of the next
    /// one, so the items of a level share a rank.
    fn stack_pairs(&self) -> Vec<(Placed, Placed)> {
        let stack = self.stack();
        let mut pairs = Vec::new();
        for levels in stack.windows(2) {
            for &first in &levels[0] {
                for &second in &levels[1] {
                    pairs.push((first, second));
                }
            }
        }
        // `first` and `last`: an item before, or after, every other item of its frame.
        let options = &self.graph.options;
        let listed: Vec<Placed> = options.first.iter().chain(&options.last).filter_map(|n| self.placed(n)).collect();
        for (names, first) in [(&options.first, true), (&options.last, false)] {
            for item in names.iter().filter_map(|n| self.placed(n)) {
                let siblings = self.ordered_content(self.container(item), false);
                for sibling in siblings.into_iter().filter(|s| !listed.contains(s)) {
                    pairs.push(if first { (item, sibling) } else { (sibling, item) });
                }
            }
        }
        if self.graph.options.direction.reversed() {
            pairs = pairs.into_iter().map(|(a, b)| (b, a)).collect();
        }
        pairs
    }

    /// Whether a style rule applies to a node or a group frame. `nodes` rules leave frames out;
    /// `kind` rules apply to both.
    /// Whether a style rule applies to a node, a frame or an edge of the view.
    pub fn applies_somewhere(&self, rule: &StyleRule) -> bool {
        let frames = self.graph.groups.iter().filter_map(|g| g.resource);
        self.graph.nodes.iter().copied().chain(frames).any(|id| self.node_rule_applies(rule, id))
            || self.graph.edges.iter().any(|edge| self.edge_rule_applies(rule, edge))
    }

    fn node_rule_applies(&self, rule: &StyleRule, id: ResourceId) -> bool {
        let kind = self.registry.resource(id).kind.as_str();
        // A node drawn as a frame is still a node for the `nodes` rules.
        let frame = self.graph.groups.iter().any(|g| g.resource == Some(id)) && !self.graph.nodes.contains(&id);
        let target = match &rule.target {
            StyleTarget::Kind(k) => self.registry.schema.expand(k).contains(&kind),
            StyleTarget::Nodes => !frame,
            _ => false,
        };
        target && rule.filter.as_ref().is_none_or(|f| self.ctx().matches(&Subject::Node(id), f))
    }

    fn edge_rule_applies(&self, rule: &StyleRule, edge: &GraphEdge) -> bool {
        let target = match &rule.target {
            StyleTarget::Layer(l) => edge.layers.contains(l),
            StyleTarget::Kind(k) => {
                edge.relation.as_deref().is_some_and(|r| self.registry.schema.expand(k).contains(&r))
            }
            StyleTarget::Edges => true,
            StyleTarget::Nodes => false,
        };
        target && rule.filter.as_ref().is_none_or(|f| self.ctx().matches(&Subject::Edge(edge), f))
    }

    /// The rules with a `legend` whose style shows in the view: they apply to at least one of its
    /// nodes, frames or edges, and are not overridden there by a later rule, nor relabeled by a
    /// later rule with a `legend` setting the same properties (a view redefining a rule of the
    /// project).
    pub fn legend(&self) -> Vec<LegendEntry> {
        const STYLED: &[&str] = &["color", "fill", "line"];
        let sets = |rule: &StyleRule, key: &str| rule.properties.iter().any(|p| p.key == key);
        let mut entries: Vec<LegendEntry> = self
            .rules
            .iter()
            .enumerate()
            .filter_map(|(index, &rule)| {
                let label = rule.properties.iter().find(|p| p.key == "legend")?;
                let relabels: Vec<&StyleRule> = self.rules[index + 1..]
                    .iter()
                    .copied()
                    .filter(|later| sets(later, "legend") && STYLED.iter().all(|k| !sets(rule, k) || sets(later, k)))
                    .collect();
                let get = |key: &str| {
                    rule.properties.iter().find(|p| p.key == key).map(|p| crate::query::value_text(&p.value))
                };
                let (color, fill, line) = (get("color"), get("fill"), get("line").and_then(|l| Line::parse(&l)));
                let same = |wanted: &Option<String>, actual: &Option<String>| wanted.is_none() || wanted == actual;
                let shows_on_node = |id: ResourceId| {
                    self.node_rule_applies(rule, id)
                        && !relabels.iter().any(|later| self.node_rule_applies(later, id))
                        && {
                            let style = self.node_style(id);
                            same(&color, &style.color)
                                && same(&fill, &style.fill)
                                && line.is_none_or(|l| l == style.line)
                        }
                };
                let shows_on_edge = |edge: &GraphEdge| {
                    self.edge_rule_applies(rule, edge)
                        && !relabels.iter().any(|later| self.edge_rule_applies(later, edge))
                        && {
                            let style = self.edge_style(edge);
                            same(&color, &style.color) && line.is_none_or(|l| l == style.line)
                        }
                };
                let frames = self.graph.groups.iter().filter_map(|g| g.resource);
                let edges = if self.graph.nodes.iter().copied().chain(frames).any(shows_on_node) {
                    false
                } else if self.graph.edges.iter().any(shows_on_edge) {
                    true
                } else {
                    return None;
                };
                Some(LegendEntry { label: crate::query::value_text(&label.value), edges, color, fill, line })
            })
            .collect();

        // The lines of the default theme drawn in the view, unless a rule with a legend is about
        // the same thing: it sets the line, its filter reads the key (`confidence`, `scope`), or
        // it styles the layer.
        let about = |rule: &StyleRule, key: &str| {
            sets(rule, "line")
                || matches!(&rule.target, StyleTarget::Layer(layer) if layer == key)
                || rule.filter.as_ref().is_some_and(|filter| {
                    filter.paths().iter().any(|path| {
                        path.steps.first().is_some_and(|step| step.kind == crate::plan::StepKind::Name(key.to_owned()))
                    })
                })
        };
        let explained = |edge: &GraphEdge, key: &str| {
            self.rules.iter().any(|rule| sets(rule, "legend") && about(rule, key) && self.edge_rule_applies(rule, edge))
        };
        let connector = |edge: &GraphEdge| {
            edge.relation
                .as_deref()
                .is_some_and(|r| self.registry.schema.kind_def(r).is_some_and(|d| d.connector.is_some()))
        };
        let theme: [Theme<&GraphEdge>; 3] = [
            ("Assumed link", Line::Dashed, "confidence", &|e| e.confidence == Confidence::Assumed),
            ("Doubtful link", Line::Dotted, "confidence", &|e| e.confidence == Confidence::Doubtful && !connector(e)),
            ("Trigger", Line::Dashed, "trigger", &|e| e.layers.contains("trigger")),
        ];
        for (label, line, key, concerned) in theme {
            let drawn =
                self.graph.edges.iter().any(|e| concerned(e) && self.edge_style(e).line == line && !explained(e, key));
            if drawn {
                entries.push(LegendEntry {
                    label: self.t(label).to_owned(),
                    edges: true,
                    color: None,
                    fill: None,
                    line: Some(line),
                });
            }
        }
        // The same for the borders of the nodes: outside the perimeter, assumed, doubtful.
        let attribute = |id: ResourceId, key: &str| {
            self.registry.resource(id).attribute(key).map(|a| crate::query::value_text(&a.value))
        };
        let node_explained = |id: ResourceId, key: &str| {
            self.rules.iter().any(|rule| sets(rule, "legend") && about(rule, key) && self.node_rule_applies(rule, id))
        };
        let theme: [Theme<ResourceId>; 3] = [
            ("External", Line::Dashed, "scope", &|id| {
                attribute(id, "scope").as_deref() == Some("external")
                    && attribute(id, "confidence").as_deref() != Some("doubtful")
            }),
            ("Assumed resource", Line::Dashed, "confidence", &|id| {
                attribute(id, "confidence").as_deref() == Some("assumed")
                    && attribute(id, "scope").as_deref() != Some("external")
            }),
            ("Doubtful resource", Line::Dotted, "confidence", &|id| {
                attribute(id, "confidence").as_deref() == Some("doubtful")
            }),
        ];
        for (label, line, key, concerned) in theme {
            let drawn = self
                .graph
                .nodes
                .iter()
                .any(|&id| concerned(id) && self.node_style(id).line == line && !node_explained(id, key));
            if drawn {
                entries.push(LegendEntry {
                    label: self.t(label).to_owned(),
                    edges: false,
                    color: None,
                    fill: None,
                    line: Some(line),
                });
            }
        }
        // The fills of a scenario of failures, unless a rule restyles them.
        for (fill, label) in [(FAILING_FILL, "Failing"), (DOWN_FILL, "Unavailable")] {
            let drawn = self.graph.nodes.iter().any(|&id| {
                self.down_style(id).is_some_and(|(f, _)| f == fill)
                    && self.node_style(id).fill.as_deref() == Some(fill)
                    && !node_explained(id, "down")
            });
            if drawn {
                entries.push(LegendEntry {
                    label: self.t(label).to_owned(),
                    edges: false,
                    color: Some(DOWN_COLOR.to_owned()),
                    fill: Some(fill.to_owned()),
                    line: None,
                });
            }
        }
        entries
    }

    /// An edge as drawn: its ends in the order of the layout, and its arrow heads. With
    /// `orientation: data`, a `pull` edge is drawn from its target to its source, so that the
    /// layout follows the data as the arrow does.
    ///
    /// An undirected edge (a cable, a connector) has no direction to follow: its ends are drawn
    /// from a node listed in `first`, or towards a node listed in `last`; otherwise in the order
    /// of the `stack` levels when both are stacked, otherwise in the order of the registry,
    /// whichever end declares it.
    fn drawn(&self, edge: &GraphEdge) -> (ResourceId, ResourceId, Arrow) {
        match self.arrow(edge) {
            Arrow::Back => (edge.target, edge.source, Arrow::Forward),
            Arrow::None if edge.symmetric && self.undirected_order(edge.target, edge.source) => {
                (edge.target, edge.source, Arrow::None)
            }
            arrow => (edge.source, edge.target, arrow),
        }
    }

    /// Whether `a` comes before `b` for an undirected edge.
    fn undirected_order(&self, a: ResourceId, b: ResourceId) -> bool {
        // First, last, or neither: the items of `first` and `last` designate nodes or frames.
        let listed = |names: &[String], id: ResourceId| {
            names.iter().filter_map(|n| self.placed(n)).any(|placed| match placed {
                Placed::Node(n) => n == id,
                Placed::Frame(g) => {
                    self.node_frame(id) == Some(g) || self.within(self.graph.node_group.get(&id).copied(), g)
                }
            })
        };
        let options = &self.graph.options;
        let place = |id: ResourceId| match (listed(&options.first, id), listed(&options.last, id)) {
            (true, _) => 0,
            (_, true) => 2,
            _ => 1,
        };
        if place(a) != place(b) {
            return (place(a) < place(b)) != options.direction.reversed();
        }
        let stack = self.stack();
        let level = |id: ResourceId| {
            stack.iter().position(|level| {
                level.iter().any(|&placed| match placed {
                    Placed::Node(n) => n == id,
                    Placed::Frame(g) => {
                        self.node_frame(id) == Some(g) || self.within(self.graph.node_group.get(&id).copied(), g)
                    }
                })
            })
        };
        match (level(a), level(b)) {
            (Some(x), Some(y)) if x != y => (x < y) != self.graph.options.direction.reversed(),
            _ => a.0 < b.0,
        }
    }

    /// Direction of the arrow head for an edge, its ends in the connection order.
    fn arrow(&self, edge: &GraphEdge) -> Arrow {
        if edge.symmetric {
            return Arrow::None;
        }
        match self.graph.options.orientation {
            Orientation::Connection => Arrow::Forward,
            Orientation::Data => match edge.exchange {
                Some(Exchange::Push) => Arrow::Forward,
                Some(Exchange::Pull) => Arrow::Back,
                Some(Exchange::Both) => Arrow::Both,
                None => Arrow::None,
            },
        }
    }

    /// Children of a frame (`None` for the top level): nested frames and nodes. A node that is
    /// also a frame of the view (a server grouping what runs on it) is drawn as that frame only.
    fn frame_content(&self, frame: Option<usize>) -> (Vec<usize>, Vec<ResourceId>) {
        let groups = (0..self.graph.groups.len())
            .filter(|&g| self.graph.groups[g].parent == frame && self.has_nodes(g))
            .collect();
        let nodes = self
            .graph
            .nodes
            .iter()
            .copied()
            .filter(|&n| self.graph.node_group.get(&n).copied() == frame && self.node_frame(n).is_none())
            .collect();
        (groups, nodes)
    }

    /// The frame drawing a node, when the node is also a frame of the view: `group-by on` makes
    /// a frame of the server an app runs on, and the server is selected as a node too.
    fn node_frame(&self, id: ResourceId) -> Option<usize> {
        if !self.graph.nodes.contains(&id) {
            return None;
        }
        self.graph.groups.iter().position(|g| g.resource == Some(id))
    }

    /// Whether a frame (`None`: the root) lies inside `group`, or is it.
    /// Whether an edge links two items of a same level of `stack`: drawn without constraining the
    /// ranks, as a free edge, so that the items stay side by side.
    pub fn across_level(&self, edge: &GraphEdge) -> bool {
        let (from, to, _) = self.drawn(edge);
        self.stack().iter().any(|level| {
            let member = |end: ResourceId| level.iter().position(|&item| self.holds(item, end));
            matches!((member(from), member(to)), (Some(a), Some(b)) if a != b)
        })
    }

    /// Whether a `stack` item is a node or a frame holding it.
    fn holds(&self, item: Placed, end: ResourceId) -> bool {
        match item {
            Placed::Node(n) => n == end,
            Placed::Frame(g) => {
                let frame = self.node_frame(end).or_else(|| self.graph.node_group.get(&end).copied());
                self.within(frame, g)
            }
        }
    }

    /// The items of each level of `stack` that `cross` names, in its order across the direction
    /// of the layout; reversed for `lr` and `rl`, which Graphviz turns after the layout. The
    /// other items are left where Graphviz puts them.
    fn level_orders(&self) -> Vec<Vec<Placed>> {
        let cross = &self.graph.options.cross;
        let position = |item: &Placed| {
            cross.iter().position(|name| match item {
                Placed::Frame(g) => self.frame_designated(name, *g),
                Placed::Node(n) => self.designates(name, *n),
            })
        };
        self.stack()
            .into_iter()
            .filter_map(|level| {
                let mut named: Vec<(usize, Placed)> =
                    level.into_iter().filter_map(|item| position(&item).map(|p| (p, item))).collect();
                named.sort_by_key(|(p, _)| *p);
                let mut ordered: Vec<Placed> = named.into_iter().map(|(_, item)| item).collect();
                if !self.graph.options.direction.vertical() {
                    ordered.reverse();
                }
                (ordered.len() > 1).then_some(ordered)
            })
            .collect()
    }

    /// The level ordered by `cross` holding a node, and the place of its item in that level.
    fn level_place(&self, end: ResourceId) -> Option<(usize, usize)> {
        self.level_orders()
            .iter()
            .enumerate()
            .find_map(|(level, items)| items.iter().position(|&item| self.holds(item, end)).map(|at| (level, at)))
    }

    fn within(&self, frame: Option<usize>, group: usize) -> bool {
        std::iter::successors(frame, |&g| self.graph.groups[g].parent).any(|g| g == group)
    }

    /// Whether a `stack` item is drawn inside a frame, the frame itself excluded.
    fn placed_inside(&self, placed: Placed, group: usize) -> bool {
        // A node drawn as a frame counts as that frame.
        let frame = match placed {
            Placed::Node(n) if self.node_frame(n).is_none() => {
                return self.within(self.graph.node_group.get(&n).copied(), group);
            }
            Placed::Node(n) => self.node_frame(n),
            Placed::Frame(f) => Some(f),
        };
        frame != Some(group) && self.within(frame, group)
    }

    /// The contradictions between `stack` and the frames of the view: two items of a frame on
    /// levels separated by a level holding an item outside it. A frame is drawn in one piece, so
    /// it keeps its items together and the stack gives way.
    pub fn stack_conflicts(&self) -> Vec<String> {
        let levels = self.stack();
        let name = |placed: Placed| match placed {
            Placed::Frame(g) => self.frame_lines(g).first().cloned().unwrap_or_default(),
            Placed::Node(n) => self.name(n).to_owned(),
        };
        let mut conflicts = Vec::new();
        for group in 0..self.graph.groups.len() {
            let inside = |placed: &Placed| self.placed_inside(*placed, group);
            let rows: Vec<usize> = (0..levels.len()).filter(|&i| levels[i].iter().any(inside)).collect();
            let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else { continue };
            if last <= first + 1 {
                continue;
            }
            // An item between them, neither inside the frame nor containing it.
            let contains = |placed: &Placed| matches!(placed, Placed::Frame(f) if self.within(Some(group), *f));
            let between = levels[first + 1..last].iter().flatten().find(|p| !inside(p) && !contains(p));
            if let (Some(&between), Some(&a), Some(&b)) =
                (between, levels[first].iter().find(|p| inside(p)), levels[last].iter().find(|p| inside(p)))
            {
                conflicts.push(format!(
                    "stack separates {} and {} with {}, but they are in the frame {}, drawn in one piece",
                    name(a),
                    name(b),
                    name(between),
                    self.frame_lines(group).first().cloned().unwrap_or_default()
                ));
            }
        }
        conflicts
    }
}

/// A frame or a node named by the `stack` render option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placed {
    Frame(usize),
    Node(ResourceId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Arrow {
    Forward,
    Back,
    Both,
    None,
}

/// Whether the content of an `svg` output is already an SVG (`layout: elk`), rather than the DOT
/// source for the host to lay out with Graphviz.
pub fn is_drawn(svg_output: &str) -> bool {
    svg_output.trim_start().starts_with("<svg")
}

/// Bytes in base64: binary outputs (`xlsx`) carried as text, icons in data URIs.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn node_id(id: ResourceId) -> String {
    format!("n{}", id.0)
}

/// The satellites of a drawing: a node exchanging with two frames or more, other than its own
/// and those around or inside it, is drawn once in each of them, and the edges from that frame
/// go to the copy. Keyed by the node and the frame of the copy.
type Satellites = std::collections::BTreeSet<(ResourceId, usize)>;

fn satellites(scene: &Scene) -> Satellites {
    let mut out = Satellites::new();
    if !scene.graph.options.satellites {
        return out;
    }
    let graph = scene.graph;
    // The innermost frame of a plain node; a node drawn as a frame has none here.
    let frame_of = |id: ResourceId| {
        (scene.node_frame(id).is_none() && graph.nodes.contains(&id))
            .then(|| graph.node_group.get(&id).copied())
            .flatten()
    };
    for &node in &graph.nodes {
        if scene.node_frame(node).is_some() {
            continue;
        }
        let home = graph.node_group.get(&node).copied();
        let mut foreign = std::collections::BTreeSet::new();
        for edge in &graph.edges {
            let other = match (edge.source == node, edge.target == node) {
                (true, false) => edge.target,
                (false, true) => edge.source,
                _ => continue,
            };
            let Some(frame) = frame_of(other) else { continue };
            // Not the frame of the node, nor a frame around it or inside it.
            if Some(frame) == home || scene.within(home, frame) || home.is_some_and(|h| scene.within(Some(frame), h)) {
                continue;
            }
            foreign.insert(frame);
        }
        if foreign.len() >= 2 {
            out.extend(foreign.into_iter().map(|frame| (node, frame)));
        }
    }
    out
}

/// The nodes at the root of the view whose every edge goes to one of their satellites: not drawn
/// a second time, alone, where nothing reaches them.
fn hidden_by_satellites(scene: &Scene, satellites: &Satellites) -> std::collections::BTreeSet<ResourceId> {
    let graph = scene.graph;
    let frame_of =
        |id: ResourceId| scene.node_frame(id).is_none().then(|| graph.node_group.get(&id).copied()).flatten();
    satellites
        .iter()
        .map(|(node, _)| *node)
        .filter(|&node| !graph.node_group.contains_key(&node))
        .filter(|&node| {
            graph.edges.iter().filter(|e| e.source == node || e.target == node).all(|e| {
                let other = if e.source == node { e.target } else { e.source };
                frame_of(other).is_some_and(|f| satellites.contains(&(node, f)))
            })
        })
        .collect()
}

/// How many places a node is drawn in: its satellites, and itself unless it is hidden; `1` for
/// a node without satellites.
fn places(scene: &Scene, satellites: &Satellites, node: ResourceId) -> usize {
    let copies = satellites.iter().filter(|(n, _)| *n == node).count();
    if copies == 0 {
        return 1;
    }
    copies + usize::from(!hidden_by_satellites(scene, satellites).contains(&node))
}

/// The DOT id of a satellite copy.
fn satellite_id(node: ResourceId, frame: usize) -> String {
    format!("n{}s{frame}", node.0)
}

/// The ends of an edge as drawn: the target replaced by its satellite in the frame of the source
/// when there is one, otherwise the source by its satellite in the frame of the target; and the
/// frame both ends are then in, for the label of the edge.
fn satellite_ends(
    scene: &Scene,
    satellites: &Satellites,
    edge: &GraphEdge,
) -> (Option<(ResourceId, String)>, Option<usize>) {
    if satellites.is_empty() {
        return (None, common_frame(scene, edge.source, edge.target));
    }
    let graph = scene.graph;
    let frame_of =
        |id: ResourceId| scene.node_frame(id).is_none().then(|| graph.node_group.get(&id).copied()).flatten();
    if let Some(frame) = frame_of(edge.source)
        && satellites.contains(&(edge.target, frame))
    {
        return (Some((edge.target, satellite_id(edge.target, frame))), Some(frame));
    }
    if let Some(frame) = frame_of(edge.target)
        && satellites.contains(&(edge.source, frame))
    {
        return (Some((edge.source, satellite_id(edge.source, frame))), Some(frame));
    }
    (None, common_frame(scene, edge.source, edge.target))
}

/// The twins of a drawing: in a frame, the nodes of one kind, of the same look, with exactly the
/// same edges (same other ends as drawn, directions, labels, styles), drawn as one box listing
/// them, with one edge per shared exchange, rather than each with its own fan of edges. The
/// satellite copies of a frame are twins among themselves. A node drawn as a frame, named by a
/// layout hint, with a loop or without edges is never a twin. Each group, its members in the
/// order of their names.
fn twins(scene: &Scene) -> Vec<Vec<Drawn>> {
    if !scene.graph.options.twins {
        return Vec::new();
    }
    let graph = scene.graph;
    let copies = &scene.satellites;
    let o = &graph.options;
    let hinted: Vec<&String> = o.stack.iter().flatten().chain(&o.cross).chain(&o.first).chain(&o.last).collect();
    let hidden = hidden_by_satellites(scene, copies);
    // Every place a node is drawn in: itself, unless hidden, and its satellite copies.
    let drawn = graph
        .nodes
        .iter()
        .filter(|&&node| scene.node_frame(node).is_none() && !hidden.contains(&node))
        .map(|&node| Drawn { node, id: node_id(node), satellite: None })
        .chain(copies.iter().map(|&(node, frame)| Drawn {
            node,
            id: satellite_id(node, frame),
            satellite: Some(frame),
        }));
    let mut groups: BTreeMap<String, Vec<Drawn>> = BTreeMap::new();
    for place in drawn {
        let node = place.node;
        if hinted.iter().any(|name| scene.designates(name, node)) {
            continue;
        }
        let mut edges = Vec::new();
        let mut loops = false;
        for edge in graph.edges.iter().filter(|e| e.source == node || e.target == node) {
            if edge.source == edge.target {
                loops = true;
                break;
            }
            let ids = scene.end_ids(edge);
            let (mine, other_id) = if edge.source == node { (&ids[0], &ids[1]) } else { (&ids[1], &ids[0]) };
            // The edges of this place only: an edge going to a copy of the node is the copy's.
            if *mine != place.id {
                continue;
            }
            let (from, _, arrow) = scene.drawn(edge);
            edges.push(format!(
                "{other_id}|{}|{arrow:?}|{:?}|{}|{}",
                from == node,
                scene.edge_style(edge),
                scene.across_level(edge),
                scene.edge_label(edge)
            ));
        }
        if loops || edges.is_empty() {
            continue;
        }
        edges.sort();
        let lines = scene.node_lines(node);
        let frame = place.satellite.or_else(|| graph.node_group.get(&node).copied());
        let key = format!(
            "{frame:?}|{}|{}|{:?}|{:?}|{}",
            place.satellite.is_some(),
            scene.registry.resource(node).kind,
            scene.node_style(node),
            &lines[1..],
            edges.join(";")
        );
        groups.entry(key).or_default().push(place);
    }
    let mut out: Vec<Vec<Drawn>> = groups.into_values().filter(|members| members.len() > 1).collect();
    for members in &mut out {
        members.sort_by_cached_key(|m| scene.node_lines(m.node).into_iter().next().unwrap_or_default());
    }
    out.sort_by_key(|members| members[0].id.clone());
    out
}

/// The plural of a kind, for the first line of a box of twins and the counts of the site.
pub(crate) fn plural(kind: &str) -> String {
    let consonant_y = kind.ends_with('y') && !kind.ends_with("ay") && !kind.ends_with("ey") && !kind.ends_with("oy");
    if kind.ends_with('s') {
        kind.to_owned()
    } else if kind.ends_with('x') || kind.ends_with("ch") || kind.ends_with("sh") {
        format!("{kind}es")
    } else if consonant_y {
        format!("{}ies", &kind[..kind.len() - 1])
    } else {
        format!("{kind}s")
    }
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

fn html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// DOT
// ---------------------------------------------------------------------------

pub fn dot(scene: &Scene) -> String {
    let graph = scene.graph;
    let mut out = String::new();
    let rankdir = match graph.options.direction {
        Direction::LeftRight => "LR",
        Direction::RightLeft => "RL",
        Direction::TopBottom => "TB",
        Direction::BottomTop => "BT",
    };
    let _ = writeln!(out, "digraph {} {{", quote(&graph.view));
    let splines = match graph.options.routing {
        Routing::Curved => "",
        Routing::Polyline => ", splines=polyline",
        Routing::Straight => ", splines=line",
        Routing::Orthogonal => ", splines=ortho",
    };
    // Labels placed on the edges rather than as nodes: for free edges, or all of them.
    // A free edge (`layout: free`, or between two items of a level of `stack`): its label on it.
    let free = |edge: &GraphEdge| scene.edge_style(edge).free || scene.across_level(edge);
    // A straight edge too: a label laid out as a node would break the line in two.
    let on_edge = |edge: &GraphEdge| graph.options.free_labels || free(edge) || scene.edge_style(edge).straight;
    let any_on_edge = graph.edges.iter().any(|e| on_edge(e) && !scene.diagram_label(e).is_empty());
    let forcelabels = if any_on_edge { ", forcelabels=true" } else { "" };
    // With `cross`, the order the frames and nodes are written in is the order wanted: the
    // crossing minimization of Graphviz, which would undo it, is cut short.
    let mclimit = if graph.options.cross.is_empty() { "" } else { ", mclimit=0.01" };
    // Ranks computed over the whole graph rather than frame by frame (`newrank`): far fewer
    // crossings and shorter edges on a dense view with frames, the frames laid out as the flows
    // go. Not with the hints that order the ranks (`stack`, `first`, `last`, `cross`), which the
    // frame-by-frame ranking follows better.
    let newrank = {
        let o = &graph.options;
        if o.stack.is_empty() && o.first.is_empty() && o.last.is_empty() && o.cross.is_empty() {
            ", newrank=true"
        } else {
            ""
        }
    };
    // A level of `stack` ordered by `cross`: without the second crossing minimization of
    // Graphviz, which reorders the frames regardless of it (it helps `cross` otherwise).
    let ordered = !scene.level_orders().is_empty();
    let mclimit = if ordered { format!("{mclimit}, remincross=false") } else { mclimit.to_owned() };
    // `spacing`: factors of the defaults of Graphviz (0.25 inch between nodes, 0.5 between ranks).
    let options = &graph.options;
    let spacing = match options.spacing {
        (1.0, 1.0) => String::new(),
        (nodes, ranks) => format!(", nodesep={}, ranksep={}", 0.25 * nodes, 0.5 * ranks),
    };
    let _ = writeln!(
        out,
        "  graph [rankdir={rankdir}, fontname=\"Helvetica\", fontsize={}, compound=true{newrank}{splines}{forcelabels}{mclimit}{spacing}];",
        options.frame_label_size
    );
    let _ = writeln!(out, "  node [fontname=\"Helvetica\", fontsize={}];", options.node_label_size);
    let _ = writeln!(out, "  edge [fontname=\"Helvetica\", fontsize={}];", options.edge_label_size);

    // A labeled edge goes through a text node, `source -> label -> target`: the layout reserves
    // room for the text, which stays on its line. Edge labels (`label`, `xlabel`) drift away from
    // edges routed around clusters, or overlap nodes. The text node lies in the innermost frame
    // holding both ends, so that an edge inside a frame stays inside.
    let copies = &scene.satellites;
    let labels: Vec<EdgeLabel> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| scene.edge_drawn(edge))
        .filter_map(|(index, edge)| {
            let text = scene.diagram_label(edge);
            // A free edge keeps its label on the edge: a text node would place itself freely.
            (!text.is_empty() && !on_edge(edge)).then(|| EdgeLabel {
                index,
                frame: satellite_ends(scene, copies, edge).1,
                text,
                color: scene.edge_style(edge).color.clone(),
                tooltip: scene.edge_tooltip(edge),
            })
        })
        .collect();
    dot_frame(scene, None, 1, &labels, copies, &mut out);

    for (index, edge) in graph.edges.iter().enumerate() {
        if !scene.edge_drawn(edge) {
            continue;
        }
        let style = scene.edge_style(edge);
        let mut attrs = Vec::new();
        if let Some(line) = style.line.dot() {
            attrs.push(format!("style={line}"));
        }
        if let Some(color) = &style.color {
            attrs.push(format!("color={}", quote(color)));
        }
        if style.thin {
            attrs.push("penwidth=0.6".to_owned());
        }
        if style.hollow_arrow {
            attrs.push("arrowhead=empty, arrowtail=empty".to_owned());
        }
        if free(edge) {
            attrs.push("constraint=false".to_owned());
        }
        let mut classes = Vec::new();
        if style.straight {
            // Redrawn as a straight line afterwards (`svg::with_icons`).
            classes.push(svg::STRAIGHT);
        }
        if on_edge(edge) {
            // Placed after the edges are drawn, against the drawn edge: a `label` of a free edge
            // would be placed where the edge would have gone, away from the line.
            let label = scene.diagram_label(edge);
            if !label.is_empty() {
                // Centered on the drawn edge afterwards (`svg::with_icons`).
                attrs.push(format!("xlabel={}", quote(&label)));
                classes.push(svg::ON_EDGE);
                if let Some(color) = &style.color {
                    attrs.push(format!("fontcolor={}", quote(color)));
                }
            }
        }
        if !classes.is_empty() {
            attrs.push(format!("class=\"{}\"", classes.join(" ")));
        }
        let mut tooltip = scene.edge_tooltip(edge);
        let end_ids = scene.end_ids(edge);
        for (end, id) in [edge.source, edge.target].into_iter().zip(&end_ids) {
            if let Some(t) = scene.twin_of(id) {
                let members = &scene.twins[t];
                let note = scene
                    .t("(one edge for the {n} {kind} of the box of {name}: each has this exchange)")
                    .replace("{n}", &members.len().to_string())
                    .replace("{kind}", &plural(&scene.registry.resource(end).kind))
                    .replace("{name}", scene.name(end));
                let _ = write!(tooltip, "\n{note}");
            }
        }
        if !tooltip.is_empty() {
            attrs.push(format!("tooltip={}", quote(&tooltip)));
        }
        let attrs = attrs.join(", ");
        let (mut from, mut to, mut arrow) = scene.drawn(edge);
        // Between two items of a level, written in their order: Graphviz puts the tail of an edge
        // within a rank first; the arrow turned, the drawing is the same.
        if let (Some(a), Some(b)) = (scene.level_place(from), scene.level_place(to))
            && a.0 == b.0
            && a.1 > b.1
        {
            (from, to) = (to, from);
            arrow = match arrow {
                Arrow::Forward => Arrow::Back,
                Arrow::Back => Arrow::Forward,
                other => other,
            };
        }
        // An end drawn as its satellite in the frame of the other end.
        // Swapped with the ends when the arrow was turned.
        let id_of = |end: ResourceId| {
            let id = if end == edge.source { &end_ids[0] } else { &end_ids[1] };
            scene.twin_id(id.clone())
        };
        let (source, target) = (id_of(from), id_of(to));
        // An end drawn as a frame: the edge stops at its border, unless the other end is inside.
        let clip = |end: ResourceId, other: Option<usize>, key: &str| {
            scene
                .node_frame(end)
                .filter(|&g| !scene.within(other, g))
                .map(|g| format!(", {key}=cluster_{g}"))
                .unwrap_or_default()
        };
        let frame_of = |id: ResourceId| scene.node_frame(id).or_else(|| scene.graph.node_group.get(&id).copied());
        if let Some(label) = labels.iter().find(|l| l.index == index) {
            // The tail arrow is drawn on the first segment, the head arrow on the second.
            let (first, second) = match arrow {
                Arrow::Forward => ("none", "forward"),
                Arrow::Back => ("back", "none"),
                Arrow::Both => ("back", "forward"),
                Arrow::None => ("none", "none"),
            };
            let (tail, head) = (clip(from, label.frame, "ltail"), clip(to, label.frame, "lhead"));
            let _ = writeln!(out, "  {source} -> e{index} [dir={first}, {attrs}{tail}];");
            let _ = writeln!(out, "  e{index} -> {target} [dir={second}, {attrs}{head}];");
        } else {
            let dir = match arrow {
                Arrow::Forward => "forward",
                Arrow::Back => "back",
                Arrow::Both => "both",
                Arrow::None => "none",
            };
            let tail = clip(from, frame_of(to), "ltail");
            let head = clip(to, frame_of(from), "lhead");
            let _ = writeln!(out, "  {source} -> {target} [dir={dir}, {attrs}{tail}{head}];");
        }
    }

    // `stack`: invisible edges between consecutive items, from a node of each.
    for (first, second) in scene.stack_pairs() {
        let mut attrs = vec!["style=invis".to_owned()];
        if let Placed::Frame(g) = first {
            attrs.push(format!("ltail=cluster_{g}"));
        }
        if let Placed::Frame(g) = second {
            attrs.push(format!("lhead=cluster_{g}"));
        }
        let attrs = attrs.join(", ");
        for a in scene.anchors(first, true) {
            for b in scene.anchors(second, false) {
                let _ = writeln!(out, "  {} -> {} [{attrs}];", scene.dot_id(a), scene.dot_id(b));
            }
        }
    }
    // A level of several items: an invisible point before them, pulling them to the same rank,
    // so that they stay side by side whatever the edges between the levels (a label laid out as
    // a node takes a rank of its own).
    for (index, level) in scene.stack().iter().enumerate().filter(|(_, level)| level.len() > 1) {
        let _ = writeln!(out, "  level{index} [shape=point, style=invis, width=0, height=0, label=\"\"];");
        for &item in level {
            let head = match item {
                Placed::Frame(g) => format!(", lhead=cluster_{g}"),
                Placed::Node(_) => String::new(),
            };
            for a in scene.anchors(item, false) {
                let _ = writeln!(out, "  level{index} -> {} [style=invis, weight=100{head}];", scene.dot_id(a));
            }
        }
    }
    // The items of a level, in their order across the direction: an invisible edge of length
    // zero from each to the next, which Graphviz draws flat, its tail first.
    for ordered in scene.level_orders() {
        for pair in ordered.windows(2) {
            let first = scene.anchors(pair[0], false).first().copied();
            let second = scene.anchors(pair[1], false).first().copied();
            if let (Some(a), Some(b)) = (first, second) {
                let _ = writeln!(out, "  {} -> {} [style=invis, minlen=0];", scene.dot_id(a), scene.dot_id(b));
            }
        }
    }

    let legend = scene.legend();
    if graph.options.legend && !legend.is_empty() {
        let _ = writeln!(out, "  subgraph cluster_legend {{");
        let _ = writeln!(
            out,
            "    label={}; style=rounded; color=\"#bbbbbb\";{}",
            quote(scene.t("Legend")),
            title_on_top(scene)
        );
        let mut rows = String::new();
        for entry in &legend {
            let _ = write!(rows, "<tr><td>{}</td><td align=\"left\">{}</td></tr>", swatch(entry), html(&entry.label));
        }
        let _ = writeln!(
            out,
            "    legend [shape=plaintext, label=<<table border=\"0\" cellspacing=\"4\" cellpadding=\"0\">{rows}</table>>];"
        );
        let _ = writeln!(out, "  }}");
    }
    out.push_str("}\n");
    out
}

/// Graphviz puts frame titles at the bottom when the ranks go up (`bt`): `labelloc=b` brings
/// them back to the top.
fn title_on_top(scene: &Scene) -> &'static str {
    if scene.graph.options.direction == Direction::BottomTop { " labelloc=\"b\";" } else { "" }
}

/// The class of a node or a frame drawn with the icon of its kind (`icons` render option).
fn icon_class(scene: &Scene, id: ResourceId) -> Option<String> {
    scene
        .graph
        .options
        .icons
        .then(|| svg::class(scene.registry.schema.base(&scene.registry.resource(id).kind)))
        .flatten()
}

/// The text of a labeled edge, drawn as a node on the edge.
struct EdgeLabel {
    index: usize,
    frame: Option<usize>,
    text: String,
    color: Option<String>,
    tooltip: String,
}

/// The innermost frame holding both nodes (`None`: the root of the graph).
fn common_frame(scene: &Scene, a: ResourceId, b: ResourceId) -> Option<usize> {
    let groups = &scene.graph.groups;
    let chain = |id: ResourceId| {
        std::iter::successors(scene.graph.node_group.get(&id).copied(), |&g| groups[g].parent).collect::<Vec<_>>()
    };
    let of_b = chain(b);
    chain(a).into_iter().find(|g| of_b.contains(g))
}

fn dot_frame(
    scene: &Scene,
    frame: Option<usize>,
    depth: usize,
    labels: &[EdgeLabel],
    satellites: &Satellites,
    out: &mut String,
) {
    let indent = "  ".repeat(depth);
    // The satellites drawn in this frame, after its own content.
    if let Some(group) = frame {
        for &(node, _) in satellites.iter().filter(|(_, f)| *f == group) {
            let id = satellite_id(node, group);
            match scene.twin_of(&id) {
                Some(t) if scene.twins[t][0].id == id => dot_twins(scene, t, &indent, out),
                Some(_) => {}
                None => dot_node_as(scene, node, &id, true, places(scene, satellites, node), &indent, out),
            }
        }
    }
    let hidden = hidden_by_satellites(scene, satellites);
    for item in scene.ordered_content(frame, true) {
        let group = match item {
            Placed::Frame(group) => group,
            Placed::Node(node) => {
                match scene.twin_of(&node_id(node)) {
                    // The first member draws the box, the others are in it.
                    Some(t) if scene.twins[t][0].node == node => dot_twins(scene, t, &indent, out),
                    Some(_) => {}
                    None if !hidden.contains(&node) => {
                        dot_node_as(scene, node, &node_id(node), false, places(scene, satellites, node), &indent, out)
                    }
                    None => {}
                }
                continue;
            }
        };
        let resource = scene.graph.groups[group].resource;
        // A node drawn as a frame looks like the node: its text, a black outline by default,
        // square corners unless its shape is rounded.
        let node = resource.is_some_and(|r| scene.node_frame(r).is_some());
        let lines = scene.frame_lines(group);
        let (label, class) = match resource.and_then(|r| icon_class(scene, r)) {
            Some(class) => (svg::frame_label(&lines), format!(" class=\"{class}\";")),
            None => (quote(&lines.join("\n")), String::new()),
        };
        let style = scene.frame_style(group);
        let mut styles = Vec::new();
        if !node || style.shape == "rounded" {
            styles.push("rounded");
        }
        styles.extend(style.line.dot());
        let border = style.color.as_deref().unwrap_or(if node { "black" } else { "#999999" });
        let mut colors = format!("color={};", quote(border));
        if let Some(fill) = &style.fill {
            styles.push("filled");
            colors.push_str(&format!(" fillcolor={};", quote(fill)));
            if is_dark(fill) {
                colors.push_str(" fontcolor=\"white\";");
            }
        }
        let style = if styles.is_empty() { String::new() } else { format!(" style={};", quote(&styles.join(","))) };
        let _ = writeln!(out, "{indent}subgraph cluster_{group} {{");
        let _ = writeln!(
            out,
            "{indent}  label={label};{class}{style} {colors} tooltip={};{}{}",
            quote(&resource.map_or_else(|| lines.join("\n"), |r| scene.node_tooltip(r))),
            resource
                .and_then(|r| scene.node_link(r))
                .map(|link| format!(" URL={}; target=\"_blank\";", quote(&link)))
                .unwrap_or_default(),
            title_on_top(scene)
        );
        if let Some(resource) = resource.filter(|_| node) {
            // The node itself: an invisible point, for the edges and layout hints to attach to;
            // the edges are clipped at the frame border (`lhead`, `ltail`).
            let _ = writeln!(
                out,
                "{indent}  {} [shape=point, style=invis, width=0.01, height=0.01, label=\"\"];",
                node_id(resource)
            );
        }
        dot_frame(scene, Some(group), depth + 1, labels, satellites, out);
        let _ = writeln!(out, "{indent}}}");
    }
    for label in labels.iter().filter(|l| l.frame == frame) {
        // Without a frame but with a margin: the edge stops on each side of the text.
        let mut attrs = vec![
            format!("label={}", quote(&label.text)),
            format!(
                "shape=none, margin=\"0.03,0.01\", width=0, height=0, fontsize={}",
                scene.graph.options.edge_label_size
            ),
        ];
        if let Some(color) = &label.color {
            attrs.push(format!("fontcolor={}", quote(color)));
        }
        if !label.tooltip.is_empty() {
            attrs.push(format!("tooltip={}", quote(&label.tooltip)));
        }
        let _ = writeln!(out, "{indent}e{} [{}];", label.index, attrs.join(", "));
    }
}

/// How many names a column of a box of twins holds at most.
const TWINS_ROWS: usize = 12;

/// A box of twins: its first line says how many of what kind, then their names in columns, each
/// name leading where the node would.
fn dot_twins(scene: &Scene, index: usize, indent: &str, out: &mut String) {
    let members = &scene.twins[index];
    let satellite = members[0].satellite.is_some();
    let first = members[0].node;
    let style = scene.node_style(first);
    let resource = scene.registry.resource(first);
    let head = format!("{} {}", members.len(), plural(&resource.kind));
    let rows = TWINS_ROWS.min(members.len());
    let columns = members.len().div_ceil(rows);
    let icon = icon_class(scene, first);
    // The cells before the text: the icon and a gap, as on a node, or nothing.
    let lead = match icon {
        Some(_) => format!(
            "<td fixedsize=\"true\" width=\"{}\" height=\"{}\"></td><td width=\"{}\"></td>",
            svg::NODE_ICON,
            svg::NODE_ICON,
            svg::NODE_GAP
        ),
        None => String::new(),
    };
    let mut table = format!(
        "<table border=\"0\" cellborder=\"0\" cellspacing=\"0\" cellpadding=\"1\"><tr>{lead}<td colspan=\"{}\" align=\"left\">{}</td></tr>",
        2 * columns - 1,
        html(&head)
    );
    for row in 0..rows {
        let _ = write!(table, "<tr>{lead}");
        for column in 0..columns {
            if column > 0 {
                table.push_str("<td width=\"8\"></td>");
            }
            match members.get(column * rows + row).map(|m| m.node) {
                Some(member) => {
                    let mut name = scene.node_lines(member).into_iter().next().unwrap_or_default();
                    let places = places(scene, &scene.satellites, member);
                    if places > 1 {
                        let _ = write!(name, " ×{places}");
                    }
                    let link = scene
                        .node_link(member)
                        .map(|l| format!(" href=\"{}\" target=\"_blank\"", html(&l)))
                        .unwrap_or_default();
                    let _ = write!(table, "<td align=\"left\"{link}>{}</td>", html(&name));
                }
                None => table.push_str("<td></td>"),
            }
        }
        table.push_str("</tr>");
    }
    table.push_str("</table>");
    let mut styles = Vec::new();
    if style.shape == "rounded" {
        styles.push("rounded");
    }
    styles.extend(style.line.dot());
    if style.fill.is_some() {
        styles.push("filled");
    }
    let mut attrs = vec![format!("label=<{table}>"), "shape=box".to_owned()];
    let classes = match (icon, satellite) {
        (Some(class), true) => format!("{class} satellite twins"),
        (Some(class), false) => format!("{class} twins"),
        (None, true) => "satellite twins".to_owned(),
        (None, false) => "twins".to_owned(),
    };
    attrs.push(format!("class=\"{classes}\""));
    if !styles.is_empty() {
        attrs.push(format!("style={}", quote(&styles.join(","))));
    }
    if satellite {
        // Drawn here for the edges of this frame; the gray says they live elsewhere.
        attrs.push("color=\"#9ca3af\"".to_owned());
        attrs.push("fontcolor=\"#4b5563\"".to_owned());
    } else if let Some(color) = &style.color {
        attrs.push(format!("color={}", quote(color)));
    }
    if let Some(fill) = &style.fill {
        attrs.push(format!("fillcolor={}", quote(fill)));
        if is_dark(fill) {
            attrs.push("fontcolor=\"white\"".to_owned());
        }
    }
    let names: Vec<String> = members.iter().map(|m| scene.registry.qualified_name(m.node)).collect();
    let mut tooltip = format!("{head} {}\n{}", scene.t("with the same exchanges:"), names.join("\n"));
    if satellite {
        tooltip.push_str(&format!("\n\n{}", scene.t("(drawn here for the exchanges of this frame)")));
    }
    attrs.push(format!("tooltip={}", quote(&tooltip)));
    let _ = writeln!(out, "{indent}t{index} [{}];", attrs.join(", "));
}

/// A node with this DOT id, drawn in `places` places of the view (`×3` after its label when more
/// than one); a satellite is drawn in gray, its tooltip saying where it is.
fn dot_node_as(
    scene: &Scene,
    node: ResourceId,
    id: &str,
    satellite: bool,
    places: usize,
    indent: &str,
    out: &mut String,
) {
    let style = scene.node_style(node);
    let mut styles = Vec::new();
    let shape = match style.shape.as_str() {
        "rounded" => {
            styles.push("rounded");
            "box"
        }
        other => other,
    };
    if let Some(line) = style.line.dot() {
        styles.push(line);
    }
    if style.fill.is_some() {
        styles.push("filled");
    }
    let mut lines = scene.node_lines(node);
    if places > 1 {
        // The reader knows the node is drawn elsewhere too.
        match lines.first_mut() {
            Some(first) => first.push_str(&format!(" ×{places}")),
            None => lines.push(format!("×{places}")),
        }
    }
    let mut attrs = match icon_class(scene, node) {
        Some(class) => vec![format!("label={}", svg::node_label(&lines)), format!("class=\"{class}\"")],
        None => vec![format!("label={}", quote(&lines.join("\n")))],
    };
    attrs.push(format!("shape={shape}"));
    if !styles.is_empty() {
        attrs.push(format!("style={}", quote(&styles.join(","))));
    }
    if let Some(color) = &style.color {
        attrs.push(format!("color={}", quote(color)));
    }
    if let Some(fill) = &style.fill {
        attrs.push(format!("fillcolor={}", quote(fill)));
        if is_dark(fill) {
            attrs.push("fontcolor=\"white\"".to_owned());
        }
    }
    let mut tooltip = scene.node_tooltip(node);
    if satellite {
        // Drawn here for the edges of this frame; the gray says it lives elsewhere.
        attrs.retain(|a| !a.starts_with("color=") && !a.starts_with("class="));
        attrs.push("color=\"#9ca3af\"".to_owned());
        attrs.push("fontcolor=\"#4b5563\"".to_owned());
        if let Some(class) = icon_class(scene, node) {
            attrs.push(format!("class=\"{class} satellite\""));
        } else {
            attrs.push("class=\"satellite\"".to_owned());
        }
        let home = scene.graph.node_group.get(&node).map(|&g| scene.frame_lines(g).join(" / "));
        let place = home.map_or_else(
            || scene.t("at the root of the view").to_owned(),
            |h| scene.t("in {frame}").replace("{frame}", &h),
        );
        tooltip = format!(
            "{tooltip}\n\n{}",
            scene.t("(drawn here for the exchanges of this frame; {where})").replace("{where}", &place)
        );
    }
    attrs.push(format!("tooltip={}", quote(&tooltip)));
    if let Some(link) = scene.node_link(node) {
        attrs.push(format!("URL={}", quote(&link)));
        attrs.push("target=\"_blank\"".to_owned());
    }
    let _ = writeln!(out, "{indent}{id} [{}];", attrs.join(", "));
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

pub fn text(scene: &Scene) -> String {
    let mut out = format!("view {}\n\nnodes\n", scene.graph.view);
    text_frame(scene, None, 1, &mut out);
    out.push_str("\nedges\n");
    for edge in &scene.graph.edges {
        let arrow = match scene.arrow(edge) {
            Arrow::None if edge.symmetric => "--",
            Arrow::Back => "<-",
            Arrow::Both => "<->",
            _ => "->",
        };
        let label = scene.edge_label_line(edge);
        let _ = writeln!(
            out,
            "  {} {arrow} {}{}",
            scene.registry.qualified_name(edge.source),
            scene.registry.qualified_name(edge.target),
            if label.is_empty() { String::new() } else { format!("  [{label}]") }
        );
        for line in scene.edge_tooltip(edge).lines() {
            let _ = writeln!(out, "      {line}");
        }
    }
    if scene.graph.edges.is_empty() {
        out.push_str("  (none)\n");
    }
    let legend = scene.legend();
    if scene.graph.options.legend && !legend.is_empty() {
        out.push_str("\nlegend\n");
        for entry in &legend {
            let _ = writeln!(out, "  {}", legend_text(entry));
        }
    }
    out
}

/// A legend entry in words: `edges HTTP / HTTPS: color #1d4ed8, dashed`.
pub(crate) fn legend_text(entry: &LegendEntry) -> String {
    let mut style = Vec::new();
    if let Some(color) = &entry.color {
        style.push(format!("color {color}"));
    }
    if let Some(fill) = &entry.fill {
        style.push(format!("fill {fill}"));
    }
    match entry.line {
        Some(Line::Dashed) => style.push("dashed".to_owned()),
        Some(Line::Dotted) => style.push("dotted".to_owned()),
        _ => {}
    }
    let what = if entry.edges { "edges" } else { "nodes" };
    if style.is_empty() {
        format!("{what} {}", entry.label)
    } else {
        format!("{what} {}: {}", entry.label, style.join(", "))
    }
}

/// The sample of a legend entry, as an HTML-like table cell: a filled square for nodes and frames,
/// a thick line for edges (in dashes or dots for such lines).
fn swatch(entry: &LegendEntry) -> String {
    let color = html(entry.color.as_deref().unwrap_or("black"));
    if !entry.edges {
        let fill = html(entry.fill.as_deref().unwrap_or("white"));
        let style = match entry.line {
            Some(Line::Dashed) => " style=\"dashed\"",
            Some(Line::Dotted) => " style=\"dotted\"",
            _ => "",
        };
        return format!(
            "<table border=\"0\" cellspacing=\"0\" cellpadding=\"0\"><tr><td fixedsize=\"true\" width=\"18\" height=\"12\" bgcolor=\"{fill}\" color=\"{color}\" border=\"1\"{style}></td></tr></table>"
        );
    }
    let bar = |width: u32| format!("<td fixedsize=\"true\" width=\"{width}\" height=\"3\" bgcolor=\"{color}\"></td>");
    let gap = |width: u32| format!("<td fixedsize=\"true\" width=\"{width}\" height=\"3\"></td>");
    let cells = match entry.line {
        Some(Line::Dashed) => [bar(7), gap(3), bar(7), gap(3), bar(7)].concat(),
        Some(Line::Dotted) => [bar(3), gap(3), bar(3), gap(3), bar(3), gap(3), bar(3), gap(3), bar(3)].concat(),
        _ => bar(27),
    };
    format!("<table border=\"0\" cellspacing=\"0\" cellpadding=\"0\"><tr>{cells}</tr></table>")
}

/// Whether a background color is dark enough to need white text: hexadecimal colors by their
/// luminance, CSS names from a short list of dark ones.
fn is_dark(color: &str) -> bool {
    let hex = color.trim_start_matches('#');
    if color.starts_with('#') && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let channel =
            |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).map(|v| if len == 1 { v * 17 } else { v });
        let rgb = match hex.len() {
            3 => (channel(0, 1), channel(1, 1), channel(2, 1)),
            6 => (channel(0, 2), channel(2, 2), channel(4, 2)),
            _ => return false,
        };
        if let (Ok(r), Ok(g), Ok(b)) = rgb {
            return 0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b) < 128.0;
        }
        return false;
    }
    const DARK: &[&str] = &[
        "black",
        "navy",
        "darkblue",
        "mediumblue",
        "blue",
        "darkgreen",
        "green",
        "teal",
        "darkcyan",
        "indigo",
        "purple",
        "darkviolet",
        "darkmagenta",
        "maroon",
        "darkred",
        "brown",
        "firebrick",
        "darkslategray",
        "darkslategrey",
        "dimgray",
        "dimgrey",
        "midnightblue",
        "darkolivegreen",
        "saddlebrown",
        "crimson",
        "steelblue",
        "slateblue",
        "darkslateblue",
        "royalblue",
        "seagreen",
        "forestgreen",
        "olive",
    ];
    DARK.contains(&color.to_ascii_lowercase().as_str())
}

fn text_frame(scene: &Scene, frame: Option<usize>, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let (groups, nodes) = scene.frame_content(frame);
    if frame.is_none() && groups.is_empty() && nodes.is_empty() {
        out.push_str("  (none)\n");
    }
    for group in groups {
        let lines = scene.frame_lines(group).join(" / ");
        match scene.graph.groups[group].resource.filter(|&r| scene.node_frame(r).is_some()) {
            Some(resource) => {
                let kind = &scene.registry.resource(resource).kind;
                let _ = writeln!(out, "{indent}{lines} [{kind}]");
            }
            None if scene.graph.groups[group].zone.is_some() => {
                let _ = writeln!(out, "{indent}{lines} [zone]");
            }
            None => {
                let _ = writeln!(out, "{indent}{lines}");
            }
        }
        text_frame(scene, Some(group), depth + 1, out);
    }
    for node in nodes {
        let resource = scene.registry.resource(node);
        let _ = writeln!(out, "{indent}{} [{}]", scene.node_lines(node).join(" / "), resource.kind);
        let ctx = scene.ctx();
        for question in view::attribute_text(&ctx, node, "question") {
            let _ = writeln!(out, "{indent}    question: {question}");
        }
    }
}

// ---------------------------------------------------------------------------
// Mermaid
// ---------------------------------------------------------------------------

/// A Mermaid link: drawn from, drawn to, connector (`-->`, `-.-`…), color, labels.
type MermaidLink = (ResourceId, ResourceId, &'static str, Option<String>, Vec<String>);

pub fn mermaid(scene: &Scene) -> String {
    let direction = match scene.graph.options.direction {
        Direction::LeftRight => "LR",
        Direction::RightLeft => "RL",
        Direction::TopBottom => "TB",
        Direction::BottomTop => "BT",
    };
    // The ELK layout routes links around frames and nodes; dagre, the default one, draws links
    // between frames behind other nodes. Renderers without ELK ignore the setting.
    // The shape of the links, when not the default curves.
    let options = &scene.graph.options;
    let mut flowchart = Vec::new();
    match options.routing {
        Routing::Curved => {}
        Routing::Polyline | Routing::Straight => flowchart.push("curve: linear".to_owned()),
        Routing::Orthogonal => flowchart.push("curve: step".to_owned()),
    }
    // `spacing`: factors of the defaults of Mermaid (50 pixels).
    if options.spacing != (1.0, 1.0) {
        flowchart.push(format!("nodeSpacing: {}", (50.0 * options.spacing.0).round()));
        flowchart.push(format!("rankSpacing: {}", (50.0 * options.spacing.1).round()));
    }
    let flowchart: String = if flowchart.is_empty() {
        String::new()
    } else {
        format!("  flowchart:\n{}", flowchart.iter().map(|l| format!("    {l}\n")).collect::<String>())
    };
    let mut out = format!("---\nconfig:\n  layout: elk\n{flowchart}---\nflowchart {direction}\n");
    let mut styles = Vec::new();
    mermaid_frame(scene, None, 1, &mut out, &mut styles);

    // Parallel edges drawn alike (same ends, arrow, line and color) become one link with
    // the labels of all of them: Mermaid would draw them on top of each other.
    let mut links: Vec<MermaidLink> = Vec::new();
    for edge in &scene.graph.edges {
        let style = scene.edge_style(edge);
        let dashed = style.line != Line::Solid;
        let (from, to, arrow) = scene.drawn(edge);
        let connector = match arrow {
            Arrow::Forward | Arrow::Back => {
                if dashed {
                    "-.->"
                } else {
                    "-->"
                }
            }
            Arrow::Both => {
                if dashed {
                    "<-.->"
                } else {
                    "<-->"
                }
            }
            Arrow::None => {
                if dashed {
                    "-.-"
                } else {
                    "---"
                }
            }
        };
        let label = scene.diagram_label(edge);
        let same = |l: &&mut (ResourceId, ResourceId, &str, Option<String>, Vec<String>)| {
            l.0 == from && l.1 == to && l.2 == connector && l.3 == style.color
        };
        match links.iter_mut().find(same) {
            Some(link) => {
                if !label.is_empty() && !link.4.contains(&label) {
                    link.4.push(label);
                }
            }
            None => links.push((
                from,
                to,
                connector,
                style.color.clone(),
                [label].into_iter().filter(|l| !l.is_empty()).collect(),
            )),
        }
    }

    let mut link_styles = Vec::new();
    for (index, (from, to, connector, color, labels)) in links.into_iter().enumerate() {
        let label = labels.join(", ");
        let label = label.replace('"', "#quot;").replace('\n', "<br/>");
        let label = if label.is_empty() { String::new() } else { format!("|\"{label}\"|") };
        let _ = writeln!(out, "  {} {connector}{label} {}", mermaid_id(scene, from), mermaid_id(scene, to));
        if let Some(color) = color {
            link_styles.push(format!("  linkStyle {index} stroke:{color},color:{color}"));
        }
    }
    // `stack`: invisible links, after the drawn ones so that `linkStyle` indices stay valid.
    for (first, second) in scene.stack_pairs() {
        let id = |placed: Placed| match placed {
            Placed::Frame(g) => format!("g{g}"),
            Placed::Node(n) => mermaid_id(scene, n),
        };
        let _ = writeln!(out, "  {} ~~~ {}", id(first), id(second));
    }
    let legend = scene.legend();
    if scene.graph.options.legend && !legend.is_empty() {
        out.push_str(&format!("  subgraph legend[\"{}\"]\n", scene.t("Legend")));
        for (i, entry) in legend.iter().enumerate() {
            let sample = if entry.edges {
                match entry.line {
                    Some(Line::Dashed) => "╍╍╍",
                    Some(Line::Dotted) => "┅┅┅",
                    _ => "━━━",
                }
            } else {
                "■"
            };
            let _ = writeln!(out, "    legend{i}[\"{sample} {}\"]", entry.label.replace('"', "#quot;"));
            let color = entry.color.as_deref().unwrap_or("black");
            styles.push(if entry.edges {
                format!("  style legend{i} fill:none,stroke:none,color:{color}")
            } else {
                format!("  style legend{i} fill:{},stroke:{color}", entry.fill.as_deref().unwrap_or("white"))
            });
        }
        out.push_str("  end\n");
    }
    for line in styles.into_iter().chain(link_styles) {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// The Mermaid identifier of a node: its subgraph when it is drawn as a frame.
fn mermaid_id(scene: &Scene, id: ResourceId) -> String {
    scene.node_frame(id).map_or_else(|| node_id(id), |g| format!("g{g}"))
}

fn mermaid_frame(scene: &Scene, frame: Option<usize>, depth: usize, out: &mut String, styles: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let order = scene.ordered_content(frame, false);
    let groups = order.iter().filter_map(|p| if let Placed::Frame(g) = p { Some(*g) } else { None });
    let nodes: Vec<ResourceId> =
        order.iter().filter_map(|p| if let Placed::Node(n) = p { Some(*n) } else { None }).collect();
    for group in groups {
        let label = scene.frame_lines(group).join("<br/>").replace('"', "#quot;");
        let _ = writeln!(out, "{indent}subgraph g{group}[\"{label}\"]");
        mermaid_frame(scene, Some(group), depth + 1, out, styles);
        let _ = writeln!(out, "{indent}end");
        let style = scene.frame_style(group);
        let mut props = Vec::new();
        if let Some(color) = &style.color {
            props.push(format!("stroke:{color}"));
        }
        if let Some(fill) = &style.fill {
            props.push(format!("fill:{fill}"));
            if is_dark(fill) {
                props.push("color:white".to_owned());
            }
        }
        if !props.is_empty() {
            styles.push(format!("  style g{group} {}", props.join(",")));
        }
    }
    for node in nodes {
        let style = scene.node_style(node);
        let text = scene.node_lines(node).join("<br/>").replace('"', "#quot;");
        let shaped = match style.shape.as_str() {
            "rounded" => format!("(\"{text}\")"),
            "ellipse" => format!("([\"{text}\"])"),
            "cylinder" => format!("[(\"{text}\")]"),
            "hexagon" => format!("{{{{\"{text}\"}}}}"),
            "note" => format!(">\"{text}\"]"),
            _ => format!("[\"{text}\"]"),
        };
        let _ = writeln!(out, "{indent}{}{shaped}", node_id(node));
        if let Some(link) = scene.node_link(node) {
            styles.push(format!("  click {} href \"{}\" _blank", node_id(node), link.replace('"', "%22")));
        }
        let mut props = Vec::new();
        if let Some(color) = &style.color {
            props.push(format!("stroke:{color}"));
        }
        if let Some(fill) = &style.fill {
            props.push(format!("fill:{fill}"));
            if is_dark(fill) {
                props.push("color:white".to_owned());
            }
        }
        match style.line {
            Line::Dashed => props.push("stroke-dasharray:5 3".to_owned()),
            Line::Dotted => props.push("stroke-dasharray:2 2".to_owned()),
            Line::Solid => {}
        }
        if !props.is_empty() {
            styles.push(format!("  style {} {}", node_id(node), props.join(",")));
        }
    }
}

/// The values of a label joined on one line, or on several lines (`edge_labels_wrap`): one value
/// per line, or a line break between values once a line would exceed the width. A value is never
/// cut.
fn wrap(values: &[String], wrap: Option<Wrap>) -> String {
    match wrap {
        None => values.join(", "),
        Some(Wrap::Values) => values.join("\n"),
        Some(Wrap::Width(width)) => {
            let mut lines: Vec<String> = Vec::new();
            for value in values {
                match lines.last_mut() {
                    Some(line) if line.chars().count() + 2 + value.chars().count() <= width => {
                        line.push_str(", ");
                        line.push_str(value);
                    }
                    _ => lines.push(value.clone()),
                }
            }
            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod variants_tests {
    use super::*;

    const DOT: &str = "digraph \"v\" {\n  graph [rankdir=LR, compound=true, newrank=true];\n  node [fontsize=10];\n  subgraph cluster_0 {\n    label=\"a\";\n    n1 [label=\"x\"];\n    n4 [label=\"w\"];\n  }\n  subgraph cluster_1 {\n    label=\"b\";\n    n2 [label=\"y\"];\n  }\n  n3 [label=\"z\"];\n  n1 -> n3 [dir=forward];\n  n3 -> n2 [dir=forward];\n}\n";

    fn sorted_lines(text: &str) -> Vec<&str> {
        let mut lines: Vec<&str> = text.lines().collect();
        lines.sort_unstable();
        lines
    }

    #[test]
    fn the_items_in_several_orders_at_every_depth() {
        let tree = DotTree::parse(DOT).expect("items");
        assert!(tree.head.ends_with("node [fontsize=10];\n"), "{}", tree.head);
        assert_eq!(tree.items.len(), 3);
        assert!(tree.tail.starts_with("  n1 -> n3"));
        assert_eq!(tree.write(Order::AsWritten, Order::AsWritten), DOT);
        // n3 is linked to both frames: placed first.
        let connected = tree.write(Order::Connected, Order::AsWritten);
        assert!(connected.find("  n3 [").unwrap() < connected.find("subgraph cluster_0").unwrap(), "{connected}");
        // Reversed at every depth: the frames, and the nodes inside them.
        let reversed = tree.write(Order::Reversed, Order::Reversed);
        assert!(reversed.find("n4 [").unwrap() < reversed.find("n1 [").unwrap(), "{reversed}");
        assert!(reversed.find("cluster_1").unwrap() < reversed.find("cluster_0").unwrap(), "{reversed}");
        // Every variant keeps every line; the newrank of half of them aside.
        let variants = dot_variants(DOT);
        assert!(variants.len() > 8, "{}", variants.len());
        for variant in &variants {
            assert_eq!(
                sorted_lines(&variant.replacen(", newrank=true", "", 1)),
                sorted_lines(&DOT.replacen(", newrank=true", "", 1))
            );
        }
        // With `stack`, `first` or `last` (no newrank): the orders, the ranks frame by frame.
        let ranked = DOT.replace(", newrank=true", "");
        let variants = dot_variants(&ranked);
        let free: Vec<String> = dot_variants(DOT).into_iter().filter(|v| !v.contains("newrank")).collect();
        assert!(variants.len() > 1);
        assert_eq!(variants, free);
        // With `cross`, the order written is kept.
        let crossed = DOT.replace(", newrank=true", ", mclimit=0.01");
        assert_eq!(dot_variants(&crossed), vec![crossed.clone()]);
    }
}
