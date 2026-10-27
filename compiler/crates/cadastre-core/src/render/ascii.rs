//! ASCII art: the view drawn with Unicode box-drawing characters, for terminals,
//! commit messages and Markdown code blocks.
//!
//! - Layers: each node gets a rank along the edges (cycles broken), the rank axis is
//!   left → right (`direction: lr`) or top → bottom (`tb`).
//! - Bands: across the rank axis, each group frame is a band of its own, nested like the
//!   groups, so that a frame is a rectangle holding its nodes and nothing else. Within a
//!   band, nodes are ordered by the barycenter of their neighbors.
//! - Edges: orthogonal paths found by A* on the character grid, around the boxes; an edge
//!   never runs along another one, it only crosses it (`┼`).
//! - Labels: on a straight horizontal stretch of their edge, otherwise beside it, otherwise
//!   as a numbered note under the drawing.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::fmt::Write;

use super::{Arrow, Line, Scene};
use crate::registry::ResourceId;

/// Directions on the grid: east, south, west, north.
const E: u8 = 0;
const S: u8 = 1;
const W: u8 = 2;
const N: u8 = 3;

fn step(dir: u8) -> (i64, i64) {
    match dir {
        E => (1, 0),
        S => (0, 1),
        W => (-1, 0),
        _ => (0, -1),
    }
}

fn opposite(dir: u8) -> u8 {
    (dir + 2) % 4
}

fn bit(dir: u8) -> u8 {
    1 << dir
}

fn horizontal(dir: u8) -> bool {
    dir == E || dir == W
}

struct Edge {
    source: usize,
    target: usize,
    /// `layout: free`: drawn, but it places no node.
    free: bool,
    label: String,
    arrow: Arrow,
    line: Line,
    hollow: bool,
}

/// A rectangle of cells, corners included.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Rect {
    pub(super) x0: usize,
    pub(super) y0: usize,
    pub(super) x1: usize,
    pub(super) y1: usize,
}

/// The edges of a view between the indices of its nodes.
fn edges(scene: &Scene, index: &HashMap<ResourceId, usize>) -> Vec<Edge> {
    scene
        .graph
        .edges
        .iter()
        .filter_map(|e| {
            let style = scene.edge_style(e);
            let (from, to, arrow) = scene.drawn(e);
            Some(Edge {
                source: *index.get(&from)?,
                target: *index.get(&to)?,
                label: scene.diagram_label_line(e),
                arrow,
                line: style.line,
                hollow: style.hollow_arrow,
                free: style.free,
            })
        })
        .collect()
}

/// The boxes of the nodes and the frames as laid out for ASCII art, in cells: the layout of
/// the draw.io export without Graphviz.
pub(super) fn cells(scene: &Scene) -> (Vec<Rect>, Vec<Rect>) {
    let graph = scene.graph;
    let index: HashMap<ResourceId, usize> = graph.nodes.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let edges = edges(scene, &index);
    let titles: Vec<String> = (0..graph.groups.len()).map(|g| scene.frame_lines(g).join(" / ")).collect();
    let layout =
        Layout::new(scene, &edges, &titles, !graph.options.direction.vertical(), graph.options.direction.reversed());
    (layout.boxes, layout.frames)
}

pub fn ascii(scene: &Scene) -> String {
    let graph = scene.graph;
    if graph.nodes.is_empty() {
        return format!("(view {} has no nodes)\n", graph.view);
    }
    // `rl` and `bt` reverse the ranks of `lr` and `tb`.
    let lr = !graph.options.direction.vertical();
    let reversed = graph.options.direction.reversed();
    let index: HashMap<ResourceId, usize> = graph.nodes.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let edges = edges(scene, &index);

    let titles: Vec<String> = (0..graph.groups.len()).map(|g| scene.frame_lines(g).join(" / ")).collect();
    let layout = Layout::new(scene, &edges, &titles, lr, reversed);
    let mut canvas = Canvas::new(layout.width, layout.height);
    for &group in &layout.frames_outside_in {
        canvas.frame(layout.frames[group], &titles[group]);
    }
    for (node, &id) in graph.nodes.iter().enumerate() {
        let style = scene.node_style(id);
        canvas.node(layout.boxes[node], &scene.node_lines(id), style.line, &style.shape);
    }

    // Short edges first: they get the straight paths.
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.sort_by_key(|&e| layout.rank[edges[e].source].abs_diff(layout.rank[edges[e].target]));
    let mut paths: Vec<Option<Vec<(usize, u8)>>> = vec![None; edges.len()];
    for e in order {
        let edge = &edges[e];
        let path = canvas.route(layout.boxes[edge.source], layout.boxes[edge.target]);
        if let Some(path) = &path {
            canvas.draw(path, edge);
        }
        paths[e] = path;
    }

    let mut notes = Vec::new();
    for (e, edge) in edges.iter().enumerate() {
        if edge.label.is_empty() {
            continue;
        }
        let placed = paths[e].as_ref().is_some_and(|path| canvas.label(path, &edge.label));
        if placed {
            continue;
        }
        let number = notes.len() + 1;
        let marker = format!("({number})");
        let marked = paths[e].as_ref().is_some_and(|path| canvas.label(path, &marker));
        let ends = format!(
            "{} → {}",
            scene.node_lines(graph.nodes[edge.source])[0],
            scene.node_lines(graph.nodes[edge.target])[0]
        );
        if marked {
            notes.push(format!("{marker} {ends}: {}", edge.label));
        } else {
            notes.push(format!("{ends}: {}", edge.label));
        }
    }
    for (e, edge) in edges.iter().enumerate() {
        if paths[e].is_none() {
            let ends = format!(
                "{} → {}",
                scene.node_lines(graph.nodes[edge.source])[0],
                scene.node_lines(graph.nodes[edge.target])[0]
            );
            notes.push(format!("not drawn: {ends}"));
        }
    }

    let mut out = canvas.to_text();
    let legend = scene.legend();
    if graph.options.legend && !legend.is_empty() {
        out.push('\n');
        for entry in &legend {
            let sample = match (entry.edges, entry.line) {
                (false, _) => "▢  ",
                (true, Some(Line::Dashed)) => "╌╌╌",
                (true, Some(Line::Dotted)) => "┈┈┈",
                (true, _) => "───",
            };
            let _ = writeln!(out, "{sample} {}", super::legend_text(entry));
        }
    }
    if !notes.is_empty() {
        out.push('\n');
        for note in notes {
            let _ = writeln!(out, "{note}");
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

struct Layout {
    rank: Vec<usize>,
    boxes: Vec<Rect>,
    frames: Vec<Rect>,
    /// Frames from the outermost to the innermost, the drawing order.
    frames_outside_in: Vec<usize>,
    width: usize,
    height: usize,
}

/// Cells between two nodes of a column, and between two bands.
const NODE_GAP: usize = 1;
const BAND_GAP: usize = 2;
/// Free cells around the drawing, for edges going around.
const MARGIN: usize = 3;

impl Layout {
    fn new(scene: &Scene, edges: &[Edge], titles: &[String], lr: bool, reversed: bool) -> Self {
        let graph = scene.graph;
        let n = graph.nodes.len();
        let mut rank = ranks(n, edges);
        let max_rank = rank.iter().copied().max().unwrap_or(0);
        if reversed {
            rank.iter_mut().for_each(|r| *r = max_rank - *r);
        }
        // A frame is wide enough for its title in the top border: `╭┄ title ┄╮`.
        let title_extent: Vec<usize> = titles.iter().map(|t| t.chars().count() + 6).collect();

        // Node sizes: text plus border, and room on the sides for the edges.
        let (mut incoming, mut outgoing) = (vec![0; n], vec![0; n]);
        for e in edges.iter().filter(|e| e.source != e.target) {
            outgoing[e.source] += 1;
            incoming[e.target] += 1;
        }
        let sizes: Vec<(usize, usize)> = (0..n)
            .map(|i| {
                let lines = scene.node_lines(graph.nodes[i]);
                let text = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
                let ports = incoming[i].max(outgoing[i]);
                if lr { (text + 4, lines.len().max(ports) + 2) } else { (text.max(2 * ports) + 4, lines.len() + 2) }
            })
            .collect();
        // Extents along the rank axis and across it.
        let rank_extent = |i: usize| if lr { sizes[i].0 } else { sizes[i].1 };
        let order_extent = |i: usize| if lr { sizes[i].1 } else { sizes[i].0 };

        // Frames: depth, children, ranks spanned.
        let groups = &graph.groups;
        let depth: Vec<usize> =
            (0..groups.len()).map(|g| std::iter::successors(Some(g), |&g| groups[g].parent).count()).collect();
        let max_depth = depth.iter().copied().max().unwrap_or(0);
        let frame_of: Vec<Option<usize>> = graph.nodes.iter().map(|id| graph.node_group.get(id).copied()).collect();

        // Initial order: as listed by the view, then by barycenter of the neighbors.
        let mut columns: HashMap<(Option<usize>, usize), Vec<usize>> = HashMap::new();
        for i in 0..n {
            columns.entry((frame_of[i], rank[i])).or_default().push(i);
        }
        let mut neighbors = vec![Vec::new(); n];
        for e in edges.iter().filter(|e| e.source != e.target) {
            neighbors[e.source].push(e.target);
            neighbors[e.target].push(e.source);
        }
        let spacing = if lr { NODE_GAP } else { 2 * NODE_GAP };
        // Top to bottom, the title runs across the bands: a band is at least as wide as it.
        let min_band: Vec<usize> = if lr { vec![0; groups.len()] } else { title_extent.clone() };
        let place = |columns: &HashMap<(Option<usize>, usize), Vec<usize>>| {
            let mut bands = Bands {
                columns,
                groups,
                max_rank,
                order_extent: &order_extent,
                spacing,
                min_extent: &min_band,
                node: vec![0; n],
                frame: vec![(0, 0); groups.len()],
            };
            let end = bands.place(None, MARGIN);
            (bands.node, bands.frame, end)
        };
        for _ in 0..4 {
            let (position, _, _) = place(&columns);
            let center: Vec<f64> = (0..n).map(|i| position[i] as f64 + order_extent(i) as f64 / 2.0).collect();
            for nodes in columns.values_mut() {
                let key = |i: usize| {
                    if neighbors[i].is_empty() {
                        center[i]
                    } else {
                        neighbors[i].iter().map(|&j| center[j]).sum::<f64>() / neighbors[i].len() as f64
                    }
                };
                nodes.sort_by(|&a, &b| key(a).total_cmp(&key(b)));
            }
        }
        let (order_position, frame_order, order_end) = place(&columns);

        // Rank axis: columns separated by gaps holding frame borders, routing and labels.
        let mut column_extent = vec![0; max_rank + 1];
        for i in 0..n {
            column_extent[rank[i]] = column_extent[rank[i]].max(rank_extent(i));
        }
        let mut label_room = vec![0; max_rank + 1];
        for e in edges {
            let r = rank[e.source].min(rank[e.target]);
            label_room[r] = label_room[r].max(e.label.chars().count().min(40));
        }
        // A frame spans the ranks of its nodes, nested frames inside, borders in the gaps.
        let mut span: Vec<Option<(usize, usize)>> = vec![None; groups.len()];
        for i in 0..n {
            let mut group = frame_of[i];
            while let Some(g) = group {
                let (lo, hi) = span[g].unwrap_or((rank[i], rank[i]));
                span[g] = Some((lo.min(rank[i]), hi.max(rank[i])));
                group = groups[g].parent;
            }
        }
        // Outer frames stand further from their nodes than inner ones.
        let inset = |g: usize| 2 * (max_depth - depth[g] + 1);

        // Between two ranks: the borders of the frames closing and opening there, plus room
        // for routing and, left to right, for the labels.
        let border_room = 2 * max_depth;
        let mut gap = vec![0; max_rank + 1];
        for r in 1..=max_rank {
            let closing =
                (0..groups.len()).filter(|&g| span[g].is_some_and(|(_, hi)| hi == r - 1)).map(inset).max().unwrap_or(0);
            let opening =
                (0..groups.len()).filter(|&g| span[g].is_some_and(|(lo, _)| lo == r)).map(inset).max().unwrap_or(0);
            gap[r] = closing + opening + if lr { label_room[r - 1] + 6 } else { 4 };
        }
        let starts = |column_extent: &[usize]| {
            let mut column_start = vec![MARGIN + border_room; max_rank + 1];
            for r in 1..=max_rank {
                column_start[r] = column_start[r - 1] + column_extent[r - 1] + gap[r];
            }
            column_start
        };
        let mut column_start = starts(&column_extent);
        // Left to right, the title runs along the ranks: widen the last column of a frame
        // too narrow for it, until every title fits.
        if lr {
            for _ in 0..=groups.len() {
                let mut widened = false;
                for g in 0..groups.len() {
                    let Some((lo, hi)) = span[g] else { continue };
                    let extent = column_start[hi] + column_extent[hi] + 2 * inset(g) - column_start[lo];
                    if extent < title_extent[g] {
                        column_extent[hi] += title_extent[g] - extent;
                        widened = true;
                    }
                }
                if !widened {
                    break;
                }
                column_start = starts(&column_extent);
            }
        }
        let rank_end = column_start[max_rank] + column_extent[max_rank] + border_room + MARGIN;

        let boxes: Vec<Rect> = (0..n)
            .map(|i| {
                let r = rank[i];
                let along = column_start[r] + (column_extent[r] - rank_extent(i)) / 2;
                rect(lr, along, rank_extent(i), order_position[i], order_extent(i))
            })
            .collect();

        let frames: Vec<Rect> = (0..groups.len())
            .map(|g| {
                let (lo, hi) = span[g].unwrap_or((0, 0));
                let start = column_start[lo] - inset(g);
                let end = column_start[hi] + column_extent[hi] + inset(g);
                let (o0, o1) = frame_order[g];
                rect(lr, start, end - start, o0, o1 - o0)
            })
            .collect();
        let mut frames_outside_in: Vec<usize> = (0..groups.len()).filter(|&g| span[g].is_some()).collect();
        frames_outside_in.sort_by_key(|&g| depth[g]);

        let order_total = order_end + MARGIN;
        let (width, height) = if lr { (rank_end, order_total) } else { (order_total, rank_end) };
        Self { rank, boxes, frames, frames_outside_in, width, height }
    }
}

/// A rectangle from its extents along the rank axis and across it.
fn rect(lr: bool, along: usize, along_extent: usize, across: usize, across_extent: usize) -> Rect {
    if lr {
        Rect { x0: along, x1: along + along_extent - 1, y0: across, y1: across + across_extent - 1 }
    } else {
        Rect { x0: across, x1: across + across_extent - 1, y0: along, y1: along + along_extent - 1 }
    }
}

/// Positions across the rank axis: frames as nested bands.
struct Bands<'a, F: Fn(usize) -> usize> {
    columns: &'a HashMap<(Option<usize>, usize), Vec<usize>>,
    groups: &'a [crate::view::Group],
    max_rank: usize,
    order_extent: &'a F,
    spacing: usize,
    /// Smallest extent of each frame across the ranks (its title, top to bottom).
    min_extent: &'a [usize],
    node: Vec<usize>,
    frame: Vec<(usize, usize)>,
}

impl<F: Fn(usize) -> usize> Bands<'_, F> {
    /// Places the content of a frame (`None`: the root) from `start`; returns its end.
    fn place(&mut self, frame: Option<usize>, start: usize) -> usize {
        // A frame has a border and a blank line on each side.
        let mut position = start + if frame.is_some() { 2 } else { 0 };
        let mut parts = 0;

        // The nodes of the frame itself, one stack per rank, centered on the band.
        let stacks: Vec<(usize, &Vec<usize>)> = (0..=self.max_rank)
            .filter_map(|r| self.columns.get(&(frame, r)))
            .map(|nodes| {
                let extent: usize =
                    nodes.iter().map(|&i| (self.order_extent)(i)).sum::<usize>() + self.spacing * (nodes.len() - 1);
                (extent, nodes)
            })
            .collect();
        let band = stacks.iter().map(|(extent, _)| *extent).max().unwrap_or(0);
        for (extent, nodes) in &stacks {
            let mut at = position + (band - extent) / 2;
            for &i in nodes.iter() {
                self.node[i] = at;
                at += (self.order_extent)(i) + self.spacing;
            }
        }
        if band > 0 {
            position += band;
            parts += 1;
        }

        let children: Vec<usize> = (0..self.groups.len()).filter(|&g| self.groups[g].parent == frame).collect();
        for child in children {
            if parts > 0 {
                position += BAND_GAP;
            }
            position = self.place(Some(child), position);
            parts += 1;
        }

        let mut end = position + if frame.is_some() { 2 } else { 0 };
        if let Some(f) = frame {
            end = end.max(start + self.min_extent[f]);
            self.frame[f] = (start, end);
        }
        end
    }
}

/// Ranks along the edges: longest path once the back edges of a depth-first search are
/// ignored; a node with outgoing edges only is moved next to its nearest successor.
fn ranks(n: usize, edges: &[Edge]) -> Vec<usize> {
    let mut out = vec![Vec::new(); n];
    for e in edges.iter().filter(|e| e.source != e.target && !e.free) {
        out[e.source].push(e.target);
    }
    let mut state = vec![0u8; n];
    let mut forward = Vec::new();
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack = vec![(root, 0)];
        while let Some(top) = stack.last_mut() {
            let (v, i) = *top;
            if i < out[v].len() {
                top.1 += 1;
                let w = out[v][i];
                match state[w] {
                    0 => {
                        forward.push((v, w));
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    2 => forward.push((v, w)),
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }

    let mut successors = vec![Vec::new(); n];
    let mut indegree = vec![0; n];
    for &(a, b) in &forward {
        successors[a].push(b);
        indegree[b] += 1;
    }
    let sources: Vec<usize> = (0..n).filter(|&v| indegree[v] == 0).collect();
    let mut rank = vec![0usize; n];
    let mut queue: VecDeque<usize> = sources.iter().copied().collect();
    let mut remaining = indegree.clone();
    let mut topological = Vec::with_capacity(n);
    while let Some(v) = queue.pop_front() {
        topological.push(v);
        for &w in &successors[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            remaining[w] -= 1;
            if remaining[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    for &v in &sources {
        if let Some(nearest) = successors[v].iter().map(|&w| rank[w]).min() {
            rank[v] = nearest.saturating_sub(1).max(rank[v]);
        }
    }

    // Nodes without edges have no natural rank: spread them over a grid rather than
    // stacking them all in the first column.
    let mut linked = vec![false; n];
    for e in edges.iter().filter(|e| e.source != e.target) {
        linked[e.source] = true;
        linked[e.target] = true;
    }
    let isolated: Vec<usize> = (0..n).filter(|&v| !linked[v]).collect();
    if !isolated.is_empty() {
        let used = (0..n).filter(|&v| linked[v]).map(|v| rank[v] + 1).max().unwrap_or(0);
        let side = (isolated.len() as f64).sqrt().ceil() as usize;
        let columns = used.max(side).max(1);
        for (k, &v) in isolated.iter().enumerate() {
            rank[v] = k % columns;
        }
    }
    rank
}

// ---------------------------------------------------------------------------
// Canvas and routing
// ---------------------------------------------------------------------------

struct Canvas {
    width: usize,
    height: usize,
    /// Boxes, frames and text.
    chars: Vec<char>,
    /// Node boxes and frame titles: no edge goes through.
    blocked: Vec<bool>,
    /// Frame borders: crossed at a cost.
    border: Vec<bool>,
    /// Edge cells: horizontal or vertical stretch, or a corner / an end (full).
    used_horizontal: Vec<bool>,
    used_vertical: Vec<bool>,
    used_full: Vec<bool>,
    /// Edge lines: directions of the cell (bits), line of the edge, number of edges.
    lines: Vec<u8>,
    line_style: Vec<Line>,
    edges_here: Vec<u8>,
    arrows: Vec<Option<char>>,
    /// Labels and notes, over everything.
    text: Vec<Option<char>>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        let cells = width * height;
        Self {
            width,
            height,
            chars: vec![' '; cells],
            blocked: vec![false; cells],
            border: vec![false; cells],
            used_horizontal: vec![false; cells],
            used_vertical: vec![false; cells],
            used_full: vec![false; cells],
            lines: vec![0; cells],
            line_style: vec![Line::Solid; cells],
            edges_here: vec![0; cells],
            arrows: vec![None; cells],
            text: vec![None; cells],
        }
    }

    fn at(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    fn moved(&self, cell: usize, dir: u8) -> Option<usize> {
        let (dx, dy) = step(dir);
        let x = (cell % self.width) as i64 + dx;
        let y = (cell / self.width) as i64 + dy;
        (x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| y as usize * self.width + x as usize)
    }

    fn frame(&mut self, r: Rect, title: &str) {
        for x in r.x0..=r.x1 {
            for y in [r.y0, r.y1] {
                let c = self.at(x, y);
                self.chars[c] = '┄';
                self.border[c] = true;
            }
        }
        for y in r.y0..=r.y1 {
            for x in [r.x0, r.x1] {
                let c = self.at(x, y);
                self.chars[c] = '┆';
                self.border[c] = true;
            }
        }
        for (x, y, ch) in [(r.x0, r.y0, '╭'), (r.x1, r.y0, '╮'), (r.x0, r.y1, '╰'), (r.x1, r.y1, '╯')] {
            let c = self.at(x, y);
            self.chars[c] = ch;
        }
        // The title in the top border, shortened to the frame width.
        let room = (r.x1 - r.x0).saturating_sub(5);
        if room == 0 || title.is_empty() {
            return;
        }
        let title = if title.chars().count() > room {
            format!(" {}… ", title.chars().take(room - 1).collect::<String>())
        } else {
            format!(" {title} ")
        };
        for (i, ch) in title.chars().enumerate() {
            let c = self.at(r.x0 + 2 + i, r.y0);
            self.chars[c] = ch;
            self.blocked[c] = true;
        }
    }

    fn node(&mut self, r: Rect, lines: &[String], line: Line, shape: &str) {
        let (h, v) = match line {
            Line::Solid => ('─', '│'),
            Line::Dashed => ('╌', '╎'),
            Line::Dotted => ('┈', '┊'),
        };
        let rounded = matches!(shape, "rounded" | "ellipse" | "cylinder");
        let corners = if rounded { ['╭', '╮', '╰', '╯'] } else { ['┌', '┐', '└', '┘'] };
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                let c = self.at(x, y);
                self.blocked[c] = true;
                self.chars[c] = match (x == r.x0 || x == r.x1, y == r.y0 || y == r.y1) {
                    (true, true) => {
                        let i = usize::from(x == r.x1) + 2 * usize::from(y == r.y1);
                        corners[i]
                    }
                    (false, true) => h,
                    (true, false) => v,
                    (false, false) => ' ',
                };
            }
        }
        let inner = r.x1 - r.x0 - 1;
        let top = r.y0 + 1 + (r.y1 - r.y0 - 1 - lines.len()) / 2;
        for (row, text) in lines.iter().enumerate() {
            let length = text.chars().count();
            let left = r.x0 + 1 + (inner - length) / 2;
            for (i, ch) in text.chars().enumerate() {
                let c = self.at(left + i, top + row);
                self.chars[c] = ch;
            }
        }
    }

    /// Shortest orthogonal path from a side of `source` to a side of `target`: the cells,
    /// each with the direction it is entered from.
    fn route(&self, source: Rect, target: Rect) -> Option<Vec<(usize, u8)>> {
        const TURN: u32 = 3;
        const CROSSING: u32 = 6;
        const BORDER: u32 = 2;

        let free =
            |c: usize| !self.blocked[c] && !self.used_full[c] && !self.used_horizontal[c] && !self.used_vertical[c];
        let (tx, ty) = ((target.x0 + target.x1) as i64 / 2, (target.y0 + target.y1) as i64 / 2);
        let (sx, sy) = ((source.x0 + source.x1) as i64 / 2, (source.y0 + source.y1) as i64 / 2);
        let heuristic = |c: usize| -> u32 {
            let (x, y) = ((c % self.width) as i64, (c / self.width) as i64);
            let dx = (target.x0 as i64 - 1 - x).max(x - target.x1 as i64 - 1).max(0);
            let dy = (target.y0 as i64 - 1 - y).max(y - target.y1 as i64 - 1).max(0);
            (dx + dy) as u32
        };
        // The side facing the target is preferred.
        let side_cost = |dir: u8| -> u32 {
            let (dx, dy) = step(dir);
            let along = dx * (tx - sx) + dy * (ty - sy);
            let main = if (tx - sx).abs() >= (ty - sy).abs() { horizontal(dir) } else { !horizontal(dir) };
            match (along > 0, main) {
                (true, true) => 0,
                (true, false) => 2,
                _ => 6,
            }
        };

        let states = self.width * self.height * 4;
        let mut cost = vec![u32::MAX; states];
        let mut previous = vec![u32::MAX; states];
        let mut heap = BinaryHeap::new();
        for (dir, cells) in sides(source) {
            for (x, y) in cells {
                let Some(c) = self.checked(x, y) else { continue };
                if !free(c) {
                    continue;
                }
                let state = c * 4 + dir as usize;
                let g = 1 + side_cost(dir);
                if g < cost[state] {
                    cost[state] = g;
                    heap.push(Reverse((g + heuristic(c), g, state)));
                }
            }
        }

        let goal = |c: usize, dir: u8| -> bool {
            let (x, y) = (c % self.width, c / self.width);
            let inside_x = x > target.x0 && x < target.x1;
            let inside_y = y > target.y0 && y < target.y1;
            free(c)
                && match dir {
                    E => x + 1 == target.x0 && inside_y,
                    W => x == target.x1 + 1 && inside_y,
                    S => y + 1 == target.y0 && inside_x,
                    _ => y == target.y1 + 1 && inside_x,
                }
        };

        while let Some(Reverse((_, g, state))) = heap.pop() {
            if g > cost[state] {
                continue;
            }
            let (c, din) = (state / 4, (state % 4) as u8);
            if goal(c, din) {
                let mut path = vec![(c, din)];
                let mut s = state;
                while previous[s] != u32::MAX {
                    s = previous[s] as usize;
                    path.push((s / 4, (s % 4) as u8));
                }
                path.reverse();
                return Some(path);
            }
            for dout in 0..4u8 {
                if dout == opposite(din) {
                    continue;
                }
                let straight = dout == din;
                let mut extra = 0;
                if straight {
                    let (same, other) = if horizontal(din) {
                        (self.used_horizontal[c], self.used_vertical[c])
                    } else {
                        (self.used_vertical[c], self.used_horizontal[c])
                    };
                    if same || self.used_full[c] {
                        continue;
                    }
                    if other {
                        extra += CROSSING;
                    }
                } else {
                    if !free(c) {
                        continue;
                    }
                    extra += TURN;
                }
                let Some(next) = self.moved(c, dout) else { continue };
                if self.blocked[next] || self.used_full[next] {
                    continue;
                }
                if self.border[next] {
                    extra += BORDER;
                }
                let state_next = next * 4 + dout as usize;
                let g_next = g + 1 + extra;
                if g_next < cost[state_next] {
                    cost[state_next] = g_next;
                    previous[state_next] = state as u32;
                    heap.push(Reverse((g_next + heuristic(next), g_next, state_next)));
                }
            }
        }
        None
    }

    fn checked(&self, x: i64, y: i64) -> Option<usize> {
        (x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| self.at(x as usize, y as usize))
    }

    /// Records and draws a routed path: lines, arrowheads, junctions on the boxes.
    fn draw(&mut self, path: &[(usize, u8)], edge: &Edge) {
        let last = path.len() - 1;
        for (i, &(c, din)) in path.iter().enumerate() {
            let dout = if i < last { path[i + 1].1 } else { din };
            if i == 0 || i == last || din != dout {
                self.used_full[c] = true;
            } else if horizontal(din) {
                self.used_horizontal[c] = true;
            } else {
                self.used_vertical[c] = true;
            }
            self.lines[c] |= bit(opposite(din)) | bit(dout);
            self.line_style[c] = edge.line;
            self.edges_here[c] += 1;
        }

        let head = |dir: u8, hollow: bool| match (dir, hollow) {
            (E, false) => '▶',
            (W, false) => '◀',
            (S, false) => '▼',
            (N, false) => '▲',
            (E, true) => '▷',
            (W, true) => '◁',
            (S, true) => '▽',
            _ => '△',
        };
        let junction = |dir: u8| match dir {
            E => '├',
            W => '┤',
            S => '┬',
            _ => '┴',
        };
        let (first, first_dir) = path[0];
        let (end, end_dir) = path[last];
        // The start: an arrowhead into the source box, or a junction on its border.
        if matches!(edge.arrow, Arrow::Back | Arrow::Both) {
            self.arrows[first] = Some(head(opposite(first_dir), edge.hollow));
        } else if let Some(b) = self.moved(first, opposite(first_dir)) {
            self.chars[b] = junction(first_dir);
        }
        if matches!(edge.arrow, Arrow::Forward | Arrow::Both) {
            self.arrows[end] = Some(head(end_dir, edge.hollow));
        } else if let Some(b) = self.moved(end, end_dir) {
            self.chars[b] = junction(opposite(end_dir));
        }
    }

    /// Writes a label along its edge: on a horizontal stretch, otherwise beside the edge.
    fn label(&mut self, path: &[(usize, u8)], label: &str) -> bool {
        let length = label.chars().count();
        let last = path.len() - 1;
        // Straight horizontal cells crossed by this edge only.
        let plain = |canvas: &Self, i: usize| {
            let (c, din) = path[i];
            i > 0
                && i < last
                && horizontal(din)
                && path[i + 1].1 == din
                && canvas.lines[c] == (bit(E) | bit(W))
                && canvas.edges_here[c] == 1
                && canvas.text[c].is_none()
        };
        let mut runs = Vec::new();
        let mut i = 0;
        while i <= last {
            if plain(self, i) {
                let start = i;
                while i <= last && plain(self, i) {
                    i += 1;
                }
                runs.push((start, i - start));
            } else {
                i += 1;
            }
        }
        // On the line, with a cell of line on each side.
        if let Some(&(start, run)) = runs.iter().find(|&&(_, run)| run >= length + 2) {
            let offset = start + (run - length) / 2;
            let cells: Vec<usize> = (0..length).map(|k| path[offset + k].0).collect();
            let mut cells = cells;
            // A path going west is written from its last cell.
            if path[start].1 == W {
                cells.reverse();
            }
            self.write(&cells, label);
            return true;
        }
        // Above or below a horizontal stretch, or right of a vertical one.
        for (k, &(c, din)) in path.iter().enumerate() {
            if k == 0 || k == last {
                continue;
            }
            let (x, y) = (c % self.width, c / self.width);
            let candidates: Vec<(usize, usize)> =
                if horizontal(din) { vec![(x, y.wrapping_sub(1)), (x, y + 1)] } else { vec![(x + 2, y)] };
            for (lx, ly) in candidates {
                if ly >= self.height || lx + length + 1 >= self.width || lx == 0 {
                    continue;
                }
                let cells: Vec<usize> = (0..length).map(|k| self.at(lx + k, ly)).collect();
                let around = [self.at(lx - 1, ly), self.at(lx + length, ly)];
                if cells.iter().chain(&around).all(|&c| self.blank(c)) {
                    self.write(&cells, label);
                    return true;
                }
            }
        }
        false
    }

    fn blank(&self, c: usize) -> bool {
        self.chars[c] == ' ' && !self.blocked[c] && !self.border[c] && self.lines[c] == 0 && self.text[c].is_none()
    }

    fn write(&mut self, cells: &[usize], text: &str) {
        for (&c, ch) in cells.iter().zip(text.chars()) {
            self.text[c] = Some(ch);
        }
    }

    fn to_text(&self) -> String {
        let mut rows: Vec<String> = Vec::with_capacity(self.height);
        for y in 0..self.height {
            let row: String = (0..self.width)
                .map(|x| {
                    let c = self.at(x, y);
                    if let Some(ch) = self.text[c] {
                        ch
                    } else if let Some(ch) = self.arrows[c] {
                        ch
                    } else if self.lines[c] != 0 {
                        line_char(self.lines[c], self.line_style[c])
                    } else {
                        self.chars[c]
                    }
                })
                .collect();
            rows.push(row.trim_end().to_owned());
        }
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        let first = rows.iter().position(|r| !r.is_empty()).unwrap_or(0);
        let rows = &rows[first..];
        let indent = rows
            .iter()
            .filter(|r| !r.is_empty())
            .map(|r| r.chars().take_while(|&c| c == ' ').count())
            .min()
            .unwrap_or(0);
        let mut out = String::new();
        for row in rows {
            out.extend(row.chars().skip(indent));
            out.push('\n');
        }
        out
    }
}

/// The cells next to each side of a box, corners excluded, with the direction leaving it.
fn sides(r: Rect) -> [(u8, Vec<(i64, i64)>); 4] {
    let (x0, y0, x1, y1) = (r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    [
        (E, (y0 + 1..y1).map(|y| (x1 + 1, y)).collect()),
        (W, (y0 + 1..y1).map(|y| (x0 - 1, y)).collect()),
        (S, (x0 + 1..x1).map(|x| (x, y1 + 1)).collect()),
        (N, (x0 + 1..x1).map(|x| (x, y0 - 1)).collect()),
    ]
}

fn line_char(bits: u8, line: Line) -> char {
    let (e, s, w, n) = (bits & bit(E) != 0, bits & bit(S) != 0, bits & bit(W) != 0, bits & bit(N) != 0);
    match (e, s, w, n) {
        (true, false, true, false) | (true, false, false, false) | (false, false, true, false) => match line {
            Line::Solid => '─',
            Line::Dashed => '╌',
            Line::Dotted => '┈',
        },
        (false, true, false, true) | (false, true, false, false) | (false, false, false, true) => match line {
            Line::Solid => '│',
            Line::Dashed => '╎',
            Line::Dotted => '┊',
        },
        (true, true, false, false) => '┌',
        (false, true, true, false) => '┐',
        (true, false, false, true) => '└',
        (false, false, true, true) => '┘',
        (true, true, false, true) => '├',
        (false, true, true, true) => '┤',
        (true, true, true, false) => '┬',
        (true, false, true, true) => '┴',
        (true, true, true, true) => '┼',
        (false, false, false, false) => ' ',
    }
}
