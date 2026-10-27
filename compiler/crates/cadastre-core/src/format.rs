//! `cadastre fmt`: the canonical form of `.ca`, `.cav` and `.cac` files.
//!
//! Two stages:
//!
//! 1. Line breaks ([`reflow`]). A block goes on one line when it only holds attributes (three
//!    at most: keys of a resource, of a link, properties of a style rule, options of `render`),
//!    no comment, and its statement fits in [`WIDTH`] columns; otherwise one item per line,
//!    without commas. The blocks of `view` and `style` always hold one item per line. The list
//!    value of an attribute goes on one line when it fits, one item per line otherwise. A
//!    `select` too long puts `where`, `with ends`, `without isolated` and `group-by` on
//!    continuation lines; an `edges` too long puts each selector on its own line, and the
//!    options of a `lift` or a `follow` after its edges; a condition still too long is broken
//!    before its `and` and `or`. Blank lines between items are kept (one at most); comments stay
//!    in place, a line comment ending its line.
//! 2. Layout ([`render`]). The indentation is two spaces per open block or bracket, one more for
//!    the continuation of a statement; tokens are one space apart (`key: value`, `-> target :
//!    https/443 [flow]`), or together when the grammar writes them so (`https/443`,
//!    `on?.ancestor(site)`). Like gofmt, columns are aligned on consecutive statements of the
//!    same shape: the target, `: protocol/port`, `[layers]` and `{` of links; the `{` of
//!    single-line declarations and style rules, and the target of a style rule; the arguments
//!    of the clauses of a view; trailing comments. Lines of a nested block, closing lines,
//!    comment lines and declarations without block do not break such a run; a blank line does, and so does a statement more
//!    than 20 columns longer or shorter than the others.
//!
//! The formatted text has the same tokens as the source, comments included, except for the
//! commas between the items of a block. A file with syntax errors, or whose formatting would
//! change something else, is left alone.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use crate::syntax;

const INDENT: &str = "  ";

/// Width of a line beyond which statements, blocks and lists are broken.
pub const WIDTH: usize = 100;

/// The canonical form of a `.ca`, `.cav` or `.cac` text; `None` when it has syntax errors.
pub fn format(source: &str) -> Option<String> {
    // A byte order mark, ignored by the parser, is kept.
    let (bom, source) = match source.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", source),
    };
    let crlf = source.contains("\r\n");
    let text = source.replace("\r\n", "\n");
    let tree = syntax::parse(&text);
    if tree.root_node().has_error() {
        return None;
    }
    let leaves = leaves(tree.root_node(), &text);

    let broken = reflow(tree.root_node(), &leaves);
    let broken_tree = syntax::parse(&broken);
    if broken_tree.root_node().has_error() {
        return None;
    }
    let mut out = render(&self::leaves(broken_tree.root_node(), &broken));

    // Safety: only whitespace and the commas between the items of a block may change.
    let formatted = syntax::parse(&out);
    if formatted.root_node().has_error()
        || significant(&leaves) != significant(&self::leaves(formatted.root_node(), &out))
    {
        return None;
    }
    if crlf {
        out = out.replace('\n', "\r\n");
    }
    Some(format!("{bom}{out}"))
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

/// A token of the syntax tree, with what the formatter needs about it.
struct Leaf<'t> {
    text: &'t str,
    kind: &'t str,
    /// Kind and identity of the parent node: tells the `:` of a link (spaced) from the `:` of a
    /// key, matches the braces of a block.
    parent: &'t str,
    parent_id: usize,
    /// Kind of the grandparent node: tells the clauses of a view.
    grandparent: &'t str,
    start: usize,
    end: usize,
    row: usize,
    end_row: usize,
    /// The statement the token belongs to (a resource, an attribute, a clause…): the first row
    /// of the statement, and its identity.
    statement: (usize, usize),
}

/// Nodes whose items, between braces, may be separated by commas.
const SEPARATED: &[&str] = &[
    "block",
    "link_block",
    "layer_style",
    "kind_style",
    "nodes_style",
    "edges_style",
    "render_clause",
    "zone_clause",
    "kind_declaration",
    "link_declaration",
    "process",
    "loop",
    "step_block",
];

/// The tokens of a text, without the commas between the items of a block.
fn significant<'t>(leaves: &[Leaf<'t>]) -> Vec<&'t str> {
    let mut opened = HashSet::new();
    let mut out = Vec::new();
    for leaf in leaves {
        if leaf.kind == "{" {
            opened.insert(leaf.parent_id);
        }
        let separator = leaf.kind == "," && SEPARATED.contains(&leaf.parent) && opened.contains(&leaf.parent_id);
        if !separator {
            out.push(leaf.text);
        }
    }
    out
}

/// Nodes whose children are statements: the containers of blocks and clauses.
const CONTAINERS: &[&str] = &[
    "source_file",
    "block",
    "link_block",
    "view",
    "check",
    "style_block",
    "layer_style",
    "kind_style",
    "nodes_style",
    "edges_style",
    "render_clause",
    "zone_clause",
    "for_each",
    "kind_declaration",
    "link_declaration",
    "process",
    "loop",
    "step_block",
];

const STYLE_RULES: &[&str] = &["layer_style", "kind_style", "nodes_style", "edges_style"];

fn leaves<'t>(root: Node<'t>, text: &'t str) -> Vec<Leaf<'t>> {
    let mut out = Vec::new();
    collect(root, text, &mut out);
    out
}

fn collect<'t>(node: Node<'t>, text: &'t str, out: &mut Vec<Leaf<'t>>) {
    if node.child_count() == 0 {
        if node.end_byte() > node.start_byte() {
            // The statement: the highest ancestor below a container; a token right in a
            // container (its keyword, its braces) belongs to the container itself.
            let mut statement = node;
            while let Some(parent) = statement.parent() {
                if CONTAINERS.contains(&parent.kind()) {
                    if statement == node {
                        statement = parent;
                    }
                    break;
                }
                statement = parent;
            }
            let parent = node.parent();
            out.push(Leaf {
                text: &text[node.byte_range()],
                kind: node.kind(),
                parent: parent.map_or("", |p| p.kind()),
                parent_id: parent.map_or(0, |p| p.id()),
                grandparent: parent.and_then(|p| p.parent()).map_or("", |p| p.kind()),
                start: node.start_byte(),
                end: node.end_byte(),
                row: node.start_position().row,
                end_row: node.end_position().row,
                statement: (statement.start_position().row, statement.id()),
            });
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, text, out);
    }
}

const OPERATORS: &[&str] = &["=", "!=", "<", "<=", ">", ">=", "->"];

/// The spaces between two tokens of a line.
fn gap(a: &Leaf, b: &Leaf, attached: bool) -> &'static str {
    match (a.kind, b.kind) {
        (_, ",") => "",
        (",", _) => " ",
        // `-> crm : https/443`, but `os: debian12`.
        (_, ":") if matches!(b.parent, "link" | "inline_link" | "step") => " ",
        (_, ":") => "",
        (":", _) => " ",
        ("{", _) | (_, "{" | "}") => " ",
        ("[" | "(", _) | (_, "]" | ")") => "",
        // `https/443 [flow]`: the layers of a link.
        (_, "[") if b.parent == "layers" => " ",
        (_, "comment") => " ",
        (x, _) | (_, x) if OPERATORS.contains(&x) => " ",
        // Tokens written together by the grammar (`https/443`, `on?.ancestor(site)`, `lift(`)
        // stay together; others are one space apart.
        _ if attached => "",
        _ => " ",
    }
}

// ---------------------------------------------------------------------------
// Line breaks
// ---------------------------------------------------------------------------

/// What comes before a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Break {
    /// The same line: a space, or nothing between tokens written together.
    Keep,
    Line,
    /// A blank line, then the token.
    Blank,
}

/// Nodes opening a level of indentation, to estimate the width of a statement.
const NESTING: &[&str] = &[
    "block",
    "link_block",
    "view",
    "check",
    "style_block",
    "layer_style",
    "kind_style",
    "nodes_style",
    "edges_style",
    "render_clause",
    "zone_clause",
    "for_each",
    "kind_declaration",
    "link_declaration",
    "process",
    "loop",
    "step_block",
];

/// The text with its canonical line breaks; its spacing is left to [`render`].
fn reflow(root: Node, leaves: &[Leaf]) -> String {
    let count = leaves.len();
    let mut reflow =
        Reflow { leaves, breaks: vec![Break::Keep; count], dropped: vec![false; count], commas: vec![false; count] };
    reflow.node(root);
    // A line comment ends its line.
    for i in 1..count {
        let previous = &leaves[i - 1];
        if previous.kind == "comment" && previous.text.starts_with("//") && reflow.breaks[i] == Break::Keep {
            reflow.breaks[i] = Break::Line;
        }
    }

    let mut out = String::new();
    for (i, leaf) in leaves.iter().enumerate() {
        if reflow.dropped[i] {
            continue;
        }
        if i > 0 {
            if reflow.commas[i] {
                out.push(',');
            }
            match reflow.breaks[i] {
                Break::Line => out.push('\n'),
                Break::Blank => out.push_str("\n\n"),
                Break::Keep if reflow.commas[i] || leaves[i - 1].end != leaf.start => out.push(' '),
                Break::Keep => {}
            }
        }
        out.push_str(leaf.text);
    }
    out.push('\n');
    out
}

struct Reflow<'a, 't> {
    leaves: &'a [Leaf<'t>],
    breaks: Vec<Break>,
    /// Commas removed: between the items of a block, one per line.
    dropped: Vec<bool>,
    /// Commas added before a token: between the items of a block on one line.
    commas: Vec<bool>,
}

impl Reflow<'_, '_> {
    fn first(&self, node: Node) -> usize {
        self.leaves.partition_point(|l| l.start < node.start_byte())
    }

    /// A line break before a node.
    fn line(&mut self, node: Node) {
        let i = self.first(node);
        self.breaks[i] = Break::Line;
    }

    fn last(&self, node: Node) -> usize {
        self.leaves.partition_point(|l| l.start < node.end_byte()).saturating_sub(1)
    }

    /// The width of a node on a single line, indentation included.
    fn width(&self, node: Node) -> usize {
        let depth =
            std::iter::successors(node.parent(), |n| n.parent()).filter(|n| NESTING.contains(&n.kind())).count();
        let (first, last) = (self.first(node), self.last(node));
        let mut width = depth * INDENT.len();
        for k in first..=last {
            if k > first {
                let (a, b) = (&self.leaves[k - 1], &self.leaves[k]);
                width += gap(a, b, a.end == b.start).len();
            }
            width += self.leaves[k].text.chars().count();
        }
        width
    }

    fn node(&mut self, node: Node) {
        match node.kind() {
            "source_file" => {
                let items = named_children(node);
                self.lines(&items, None);
            }
            "view" | "check" | "style_block" | "for_each" => {
                let items = braced(node);
                self.broken(node, &items);
            }
            kind if SEPARATED.contains(&kind) && has_brace(node) => {
                let items = braced(node);
                // The statement the block belongs to: a resource, a link, or the node itself.
                let statement =
                    if matches!(kind, "block" | "link_block") { node.parent().unwrap_or(node) } else { node };
                let attributes_only =
                    items.iter().all(|i| matches!(i.kind(), "attribute" | "key_declaration" | "identifier"));
                // On one line, the block gets a comma between its items.
                let commas = braced_commas(node);
                let width = self.width(statement) + items.len().saturating_sub(1).saturating_sub(commas);
                if !attributes_only || items.len() > 3 || width > WIDTH {
                    self.broken(node, &items);
                } else {
                    self.flat(&items);
                }
            }
            "list" if node.parent().is_some_and(|p| matches!(p.kind(), "attribute" | "zone_clause" | "for_each")) => {
                // The items of a `for_each`: its header and its list, not its body.
                let wide = match node.parent() {
                    Some(p) if p.kind() == "for_each" => {
                        self.width(p.child(0).unwrap_or(node)) + self.width(node) > WIDTH
                    }
                    parent => self.width(parent.unwrap_or(node)) > WIDTH,
                };
                if wide {
                    for item in named_children(node) {
                        self.line(item);
                    }
                    if let Some(close) = children(node).filter(|c| c.kind() == "]").last() {
                        self.line(close);
                    }
                }
            }
            "select_clause" if self.width(node) > WIDTH => {
                for child in children(node) {
                    if matches!(child.kind(), "where_clause" | "with_ends" | "without_isolated" | "group_by") {
                        self.line(child);
                    }
                    if child.kind() == "where_clause" {
                        self.conditions(child, INDENT.len());
                    }
                }
            }
            "edges_clause" if self.width(node) > WIDTH => {
                let selectors: Vec<Node> = children(node).filter(|c| c.is_named() && c.kind() != "comment").collect();
                for selector in selectors.iter().skip(1) {
                    self.line(*selector);
                }
                for selector in selectors {
                    if self.width(selector) + INDENT.len() > WIDTH {
                        self.selector(selector);
                    }
                }
            }
            _ => {}
        }
        for child in children(node) {
            self.node(child);
        }
    }

    /// An edge selector too long: the options of a `lift` or a `follow` on the next line, its
    /// conditions broken when still too long.
    fn selector(&mut self, selector: Node) {
        let mut stack = vec![selector];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "where_clause" => self.conditions(node, INDENT.len()),
                "lift_option" | "follow_option" => {
                    let previous = node.prev_sibling().filter(|p| p.kind() == ",");
                    let first_option = previous
                        .and_then(|p| p.prev_sibling())
                        .is_some_and(|p| !matches!(p.kind(), "lift_option" | "follow_option"));
                    if first_option {
                        self.line(node);
                    }
                }
                _ => stack.extend(children(node)),
            }
        }
    }

    /// A condition too long, `extra` columns further than its node: broken before each of its
    /// `and` and `or`, outside parentheses.
    fn conditions(&mut self, clause: Node, extra: usize) {
        if self.width(clause) + extra <= WIDTH {
            return;
        }
        let mut stack: Vec<Node> = children(clause).collect();
        while let Some(node) = stack.pop() {
            if matches!(node.kind(), "and" | "or") && node.is_named() {
                for part in children(node) {
                    if matches!(part.kind(), "and" | "or") && !part.is_named() {
                        self.line(part);
                    } else {
                        stack.push(part);
                    }
                }
            }
        }
    }

    /// Items one per line: the statements of the root, of a block, of a view.
    fn lines(&mut self, items: &[Node], open: Option<Node>) {
        let mut previous_end = open.map(|o| o.end_position().row);
        for (k, item) in items.iter().enumerate() {
            let row = item.start_position().row;
            let index = self.first(*item);
            self.breaks[index] = match previous_end {
                // A trailing comment stays on its line.
                Some(end) if item.kind() == "comment" && row == end => Break::Keep,
                Some(end) if row > end + 1 && (k > 0 || open.is_none()) => Break::Blank,
                Some(_) => Break::Line,
                None => Break::Keep,
            };
            previous_end = Some(item.end_position().row);
        }
    }

    /// A block with one item per line, its separating commas removed.
    fn broken(&mut self, node: Node, items: &[Node]) {
        let open = children(node).find(|c| c.kind() == "{");
        self.lines(items, open);
        let mut inside = false;
        for child in children(node) {
            match child.kind() {
                "{" => inside = true,
                "}" if inside => self.line(child),
                "," if inside => {
                    let i = self.first(child);
                    self.dropped[i] = true;
                }
                _ => {}
            }
        }
    }

    /// A block on one line: its items separated by commas.
    fn flat(&mut self, items: &[Node]) {
        for item in items.iter().skip(1) {
            let index = self.first(*item);
            if index > 0 && self.leaves[index - 1].kind != "," {
                self.commas[index] = true;
            }
        }
    }
}

fn children<'t>(node: Node<'t>) -> impl Iterator<Item = Node<'t>> {
    (0..node.child_count()).filter_map(move |i| node.child(i))
}

fn named_children(node: Node) -> Vec<Node> {
    children(node).filter(|c| c.is_named()).collect()
}

fn has_brace(node: Node) -> bool {
    children(node).any(|c| c.kind() == "{")
}

/// The named children of a node between its braces: the items of a block.
fn braced(node: Node) -> Vec<Node> {
    let mut inside = false;
    let mut items = Vec::new();
    for child in children(node) {
        match child.kind() {
            "{" => inside = true,
            "}" => inside = false,
            _ if inside && child.is_named() => items.push(child),
            _ => {}
        }
    }
    items
}

/// The commas of a node between its braces.
fn braced_commas(node: Node) -> usize {
    let mut inside = false;
    let mut count = 0;
    for child in children(node) {
        match child.kind() {
            "{" => inside = true,
            "}" => inside = false,
            "," if inside => count += 1,
            _ => {}
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

/// Statements whose columns are aligned on consecutive lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// A link: target, `: port`, `[layers]`, `{ … }`.
    Link,
    /// A declaration or a style rule on one line: before `{`, then `{ … }`.
    Declaration,
    /// A clause of a view: its keyword, then its arguments.
    Clause,
}

/// A formatted line, before alignment.
struct Line {
    /// Indentation, in spaces.
    indent: usize,
    /// The code, cut into the columns of its shape (a single column otherwise).
    columns: Vec<String>,
    shape: Option<Shape>,
    /// A trailing `//` comment after code.
    comment: Option<String>,
    /// Only closing braces or brackets, or only a comment: it does not break a run of aligned
    /// lines.
    closing: bool,
    /// A declaration without block (`data articles`): it does not break a run of declarations.
    bare: bool,
    /// A blank line before it.
    blank: bool,
}

impl Line {
    fn code(&self) -> String {
        self.columns.iter().filter(|c| !c.is_empty()).cloned().collect::<Vec<_>>().join(" ")
    }
}

fn render(leaves: &[Leaf]) -> String {
    let mut lines: Vec<Line> = Vec::new();
    // Nesting: braces, and brackets / parentheses, before each token.
    let (mut braces, mut brackets) = (0usize, 0usize);
    // Nesting when each statement starts.
    let mut statement_depth: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut previous_end_row: Option<usize> = None;

    let mut i = 0;
    while i < leaves.len() {
        let row = leaves[i].row;
        let mut j = i;
        while j < leaves.len() && leaves[j].row == row {
            j += 1;
        }
        let line = &leaves[i..j];

        // Indentation, from the nesting before the first token.
        let first = &line[0];
        for leaf in line {
            statement_depth.entry(leaf.statement.1).or_insert((braces, brackets));
        }
        let closing =
            line.iter().all(|l| matches!(l.kind, "}" | "]" | ")")) || (line.len() == 1 && first.kind == "comment");
        let closes = matches!(first.kind, "}" | "]" | ")");
        let (statement_row, statement) = first.statement;
        let continuation = statement_row < row
            && !closes
            && first.kind != "comment"
            && statement_depth.get(&statement) == Some(&(braces, brackets));
        let mut level = braces + brackets + usize::from(continuation);
        if closes {
            level = level.saturating_sub(1);
        }
        let indent = level * INDENT.len();

        // A trailing comment apart; the code cut into the columns of its shape.
        let (code, comment) = match line.split_last() {
            Some((last, rest)) if last.kind == "comment" && !rest.is_empty() && last.text.starts_with("//") => {
                (rest, Some(last.text.to_owned()))
            }
            _ => (line, None),
        };
        let shape = if continuation { None } else { shape(code) };
        let mut columns = vec![String::new()];
        for (k, leaf) in code.iter().enumerate() {
            if k > 0 {
                if shape.is_some_and(|s| starts_column(s, &code[..k], leaf)) {
                    columns.push(String::new());
                } else {
                    let previous = &code[k - 1];
                    columns.last_mut().unwrap().push_str(gap(previous, leaf, previous.end == leaf.start));
                }
            }
            columns.last_mut().unwrap().push_str(leaf.text);
        }
        if shape == Some(Shape::Link) {
            columns = link_columns(code, columns);
        }
        // A style rule without target nor filter (`nodes { … }`): an empty middle column.
        if shape == Some(Shape::Declaration) && STYLE_RULES.contains(&first.parent) && columns.len() == 2 {
            columns.insert(1, String::new());
        }

        for leaf in line {
            match leaf.kind {
                "{" => braces += 1,
                "}" => braces = braces.saturating_sub(1),
                "[" | "(" => brackets += 1,
                "]" | ")" => brackets = brackets.saturating_sub(1),
                _ => {}
            }
        }

        let blank = previous_end_row.is_some_and(|end| row > end + 1);
        let bare = !continuation && code.iter().all(|l| l.parent == "resource");
        lines.push(Line { indent, columns, shape, comment, closing, bare, blank });
        previous_end_row = Some(line.last().map_or(row, |l| l.end_row));
        i = j;
    }

    align(&mut lines);

    let mut out = String::new();
    for line in &lines {
        if line.blank {
            out.push('\n');
        }
        out.push_str(&" ".repeat(line.indent));
        out.push_str(&line.code());
        if let Some(comment) = &line.comment {
            out.push(' ');
            out.push_str(comment);
        }
        let end = out.trim_end_matches([' ', '\t']).len();
        out.truncate(end);
        out.push('\n');
    }
    out
}

/// The shape of the statement starting a line, if its columns are aligned.
fn shape(code: &[Leaf]) -> Option<Shape> {
    let first = code.first()?;
    // `-> target …` in a block, `source -> target …` at the root.
    if first.parent == "inline_link" || first.parent == "link" {
        return Some(Shape::Link);
    }
    // `select …`, `edges …`, `render …` in a view or a check, and `style { … }` on a single line.
    let single_line_style = || code.iter().any(|l| l.kind == "}" && l.parent == "style_block");
    if matches!(first.kind, "select" | "zone" | "edges" | "render" | "style" | "expect")
        && matches!(first.grandparent, "view" | "check")
        && code.len() > 1
        && (first.kind != "style" || single_line_style())
    {
        return Some(Shape::Clause);
    }
    // A resource or a style rule whose block opens and closes on the line.
    let declares = first.parent == "resource" || STYLE_RULES.contains(&first.parent);
    let open = code.iter().find(|l| l.kind == "{" && (l.parent == "block" || STYLE_RULES.contains(&l.parent)))?;
    let closed = code.iter().any(|l| l.kind == "}" && l.parent_id == open.parent_id);
    (declares && closed).then_some(Shape::Declaration)
}

/// Whether a token starts a new column of its line.
fn starts_column(shape: Shape, before: &[Leaf], leaf: &Leaf) -> bool {
    match shape {
        Shape::Link => {
            (leaf.kind == ":" && matches!(leaf.parent, "link" | "inline_link"))
                || (leaf.kind == "[" && leaf.parent == "layers")
                || (leaf.kind == "{" && leaf.parent == "link_block")
        }
        Shape::Declaration => {
            // A style rule also aligns what follows its keyword: `kind  connector {`.
            (before.len() == 1 && STYLE_RULES.contains(&before[0].parent))
                || (leaf.kind == "{"
                    && (leaf.parent == "block" || STYLE_RULES.contains(&leaf.parent))
                    && !before.iter().any(|l| l.kind == "{"))
        }
        Shape::Clause => before.len() == 1,
    }
}

/// The four columns of a link: target, `: port`, `[layers]`, `{ … }` (empty when absent).
fn link_columns(code: &[Leaf], columns: Vec<String>) -> Vec<String> {
    let mut out = vec![String::new(); 4];
    let mut parts = columns.into_iter();
    out[0] = parts.next().unwrap_or_default();
    let mut slot = 0;
    for leaf in code {
        let next = match (leaf.kind, leaf.parent) {
            (":", "link" | "inline_link") => 1,
            ("[", "layers") => 2,
            ("{", "link_block") => 3,
            _ => continue,
        };
        if next > slot {
            slot = next;
            out[slot] = parts.next().unwrap_or_default();
        }
    }
    out
}

/// Aligns the columns of runs of statements of the same shape at the same indentation, then
/// trailing comments. Lines of nested blocks, and closing lines, do not break a run.
fn align(lines: &mut [Line]) {
    let mut start = 0;
    while start < lines.len() {
        let Some(shape) = lines[start].shape else {
            start += 1;
            continue;
        };
        let indent = lines[start].indent;
        let mut members = vec![start];
        let mut k = start + 1;
        while k < lines.len() && !lines[k].blank {
            let line = &lines[k];
            if line.indent == indent && line.shape == Some(shape) {
                members.push(k);
            } else if !(line.indent > indent
                || (line.closing && line.indent >= indent)
                || (line.bare && line.indent == indent && shape == Shape::Declaration))
            {
                break;
            }
            k += 1;
        }
        for section in sections(lines, &members) {
            align_columns(lines, &section);
        }
        start += 1;
    }

    // Trailing comments of consecutive lines at the same indentation.
    let mut start = 0;
    while start < lines.len() {
        if lines[start].comment.is_none() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < lines.len()
            && lines[end].comment.is_some()
            && !lines[end].blank
            && lines[end].indent == lines[start].indent
        {
            end += 1;
        }
        let width = lines[start..end].iter().map(|l| l.code().chars().count()).max().unwrap_or(0);
        for line in &mut lines[start..end] {
            let code = line.code();
            let padding = width - code.chars().count();
            line.columns = vec![format!("{code}{}", " ".repeat(padding))];
        }
        start = end;
    }
}

/// Splits a run of aligned lines where a statement is much longer, or shorter, than the others
/// (more than 20 columns before its last column): its padding would be excessive.
fn sections(lines: &[Line], members: &[usize]) -> Vec<Vec<usize>> {
    const GAP: usize = 20;
    let key = |m: usize| {
        let columns = &lines[m].columns;
        columns[..columns.len().saturating_sub(1)].iter().map(|c| c.chars().count() + 1).sum::<usize>()
    };
    let mut sections: Vec<Vec<usize>> = Vec::new();
    let (mut low, mut high) = (usize::MAX, 0);
    for &m in members {
        let width = key(m);
        match sections.last_mut() {
            Some(section) if width <= high + GAP && width + GAP >= low => {
                section.push(m);
                (low, high) = (low.min(width), high.max(width));
            }
            _ => {
                sections.push(vec![m]);
                (low, high) = (width, width);
            }
        }
    }
    sections
}

/// Pads the columns of the lines of a section to the widest of each column.
fn align_columns(lines: &mut [Line], members: &[usize]) {
    if members.len() < 2 {
        return;
    }
    let count = members.iter().map(|&m| lines[m].columns.len()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..count)
        .map(|c| members.iter().filter_map(|&m| lines[m].columns.get(c)).map(|s| s.chars().count()).max().unwrap_or(0))
        .collect();
    for &m in members {
        let line = &mut lines[m];
        // Columns up to the last one present, each padded to the width of its column.
        let last = line.columns.iter().rposition(|c| !c.is_empty()).unwrap_or(0);
        let mut code = String::new();
        for (c, column) in line.columns.iter().enumerate().take(last + 1) {
            if c > 0 {
                code.push(' ');
            }
            code.push_str(column);
            if c < last {
                code.push_str(&" ".repeat(widths[c] - column.chars().count()));
            }
        }
        line.columns = vec![code];
        // Aligned: no longer the start of another run.
        line.shape = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_blocks_with_resources_and_joins_short_ones() {
        let source = "site   s{hosting:on-prem\n      vpc v {\n subnet n {cidr: 10.0.1.0/24,server web01{os:debian12}}\n}\n}\n\ndata a {\n  description: \"x\"\n}\n\napp b { on: x  label: \"y\" }\nserver c { a: 1, b: 2, c: 3, d: 4 }\n";
        assert_eq!(
            format(source).unwrap(),
            "site s {\n  hosting: on-prem\n  vpc v {\n    subnet n {\n      cidr: 10.0.1.0/24\n      server web01 { os: debian12 }\n    }\n  }\n}\n\ndata a { description: \"x\" }\n\napp b { on: x, label: \"y\" }\nserver c {\n  a: 1\n  b: 2\n  c: 3\n  d: 4\n}\n"
        );
    }

    #[test]
    fn links_lists_and_paths() {
        let source = "app a {\n->b:https/443[flow,auth]{carries:[x,\ny]}\n}\nview v { edges lift( [flow],via:on?.ancestor(site) ) where not (proto=https) }\n";
        assert_eq!(
            format(source).unwrap(),
            "app a {\n  -> b : https/443 [flow, auth] { carries: [x, y] }\n}\nview v {\n  edges lift([flow], via: on?.ancestor(site)) where not (proto = https)\n}\n"
        );
    }

    #[test]
    fn long_clauses_and_lists() {
        let source = "view v {\n  select app where criticality >= 2 and owner = data-team and exists(description) and not (scope = external) group-by system\n  edges lift(peer where source.ancestor(site) in [carrier-pa2, carrier-pa7, cluster-a-site-north] and target.ancestor(site) in [carrier-pa2, carrier-pa7], via: parent, merge: none), wan_link\n}\napp x { carries: [customer-accounts, invoices, payments, price-lists, item-prices, product-files, label-content] }\n";
        assert_eq!(
            format(source).unwrap(),
            "view v {\n  select app\n    where criticality >= 2 and owner = data-team and exists(description) and not (scope = external)\n    group-by system\n  edges  lift(peer where source.ancestor(site) in [carrier-pa2, carrier-pa7, cluster-a-site-north]\n    and target.ancestor(site) in [carrier-pa2, carrier-pa7],\n    via: parent, merge: none),\n    wan_link\n}\napp x {\n  carries: [\n    customer-accounts,\n    invoices,\n    payments,\n    price-lists,\n    item-prices,\n    product-files,\n    label-content\n  ]\n}\n"
        );
    }

    #[test]
    fn checks_like_views() {
        let source = "check owners { select app where not exists(owner) edges [flow]   expect none message:\"x\" }\n";
        assert_eq!(
            format(source).unwrap(),
            "check owners {\n  select app where not exists(owner)\n  edges  [flow]\n  expect none\n  message: \"x\"\n}\n"
        );
    }

    #[test]
    fn comments_and_blank_lines() {
        let source =
            "// head\n\n\n\nview v {\n// a comment\nselect app\n\n\nrender svg // r\n}\napp a { // trailing\non: x }\n";
        assert_eq!(
            format(source).unwrap(),
            "// head\n\nview v {\n  // a comment\n  select app\n\n  render svg // r\n}\napp a { // trailing\n  on: x\n}\n"
        );
    }

    #[test]
    fn aligns_links() {
        let source = "app ui {\n  -> db : jdbc [flow] { exchange: both }\n  -> filehub : sftp/22 [flow]\n  -> logistics : http [flow] {\n    confidence: assumed\n  }\n  -> crm [flow]\n  -> keycloak : https/443 [auth]\n}\n";
        assert_eq!(
            format(source).unwrap(),
            "app ui {\n  -> db        : jdbc      [flow] { exchange: both }\n  -> filehub   : sftp/22   [flow]\n  -> logistics : http      [flow] { confidence: assumed }\n  -> crm                   [flow]\n  -> keycloak  : https/443 [auth]\n}\n"
        );
    }

    #[test]
    fn aligns_declarations_clauses_and_comments() {
        let source = "data item-prices { description: \"Item prices\" }\ndata labels\ndata articles { description: \"Items\" }\nview v {\n  select app group-by system\n  edges [flow]\n  render svg\n}\nstyle {\n  edges where proto in [http, https] { color: red }\n  kind connector { line: dotted }\n  nodes { shape: box }\n\n  layer trigger { line: dashed }\n}\napp a { on: x } // first\napp bb { on: y }  // second\n";
        assert_eq!(
            format(source).unwrap(),
            "data item-prices { description: \"Item prices\" }\ndata labels\ndata articles    { description: \"Items\" }\nview v {\n  select app group-by system\n  edges  [flow]\n  render svg\n}\nstyle {\n  edges where proto in [http, https] { color: red }\n  kind  connector                    { line: dotted }\n  nodes                              { shape: box }\n\n  layer trigger { line: dashed }\n}\napp a  { on: x } // first\napp bb { on: y } // second\n"
        );
    }

    #[test]
    fn keeps_block_comments_line_endings_and_byte_order_mark() {
        let source = "\u{feff}/* a\r\n   comment */\r\napp a {\r\n  on: x\r\n}\r\n";
        assert_eq!(format(source).unwrap(), "\u{feff}/* a\r\n   comment */\r\napp a { on: x }\r\n");
    }

    #[test]
    fn leaves_invalid_files_alone() {
        assert_eq!(format("app a { on: }"), None);
    }

    #[test]
    fn is_idempotent_on_the_examples() {
        // Every source file of the examples, in their subdirectories too.
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, out);
                } else if crate::source::FileRole::from_path(&path.to_string_lossy()).is_some() {
                    out.push(path);
                }
            }
        }
        let examples = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../examples");
        let mut files = Vec::new();
        sources(std::path::Path::new(examples), &mut files);
        assert!(files.len() > 10);
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let once = format(&text).unwrap_or_else(|| panic!("{} not formatted", path.display()));
            assert_eq!(format(&once).unwrap(), once, "{}", path.display());
        }
    }
}
