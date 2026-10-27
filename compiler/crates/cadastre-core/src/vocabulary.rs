//! The vocabulary of plans: formats, options, style properties and pseudo-attributes, with their
//! values and defaults. Validation, completion and the reference (`docs/reference.md`) read these
//! tables, so they cannot disagree.

/// The values an option or a property accepts.
#[derive(Clone, Copy, Debug)]
pub enum Values {
    /// One of these words.
    Closed(&'static [&'static str]),
    /// A list of these words or of keys written on the links or the connectors of the edge.
    EdgeLabels,
    /// A list of keys of the resources.
    Keys,
    /// A list of names of resources.
    Resources,
    /// A list of names of resources or of lists of names: levels of one or more resources.
    Levels,
    /// A name of a resource of these kinds or groups.
    Resource(&'static str),
    /// A path (`on`, `ancestor(site)`, `on?.ancestor(site)`).
    Path,
    /// A CSS color name or a quoted hexadecimal code.
    Color,
    /// A string.
    Text,
    /// A number of characters, or `values`.
    Wrap,
    /// `list`, `count`, or a number of merged links.
    MergeLabel,
    /// A list of keys, pseudo-attributes or paths, quoted when they hold parentheses.
    Columns,
    /// A list of kinds or groups.
    Kinds,
    /// A positive number.
    Number,
    /// A positive number, or a list of two: for both, or for each of two things.
    Numbers,
    /// A name of a zone of the view.
    Zone,
}

/// An option of a clause, or a property of a style rule.
#[derive(Clone, Copy, Debug)]
pub struct OptionDef {
    pub key: &'static str,
    pub values: Values,
    /// The default value, as written in a plan; empty when the option is required or has none.
    pub default: &'static str,
    pub doc: &'static str,
}

const fn option(key: &'static str, values: Values, default: &'static str, doc: &'static str) -> OptionDef {
    OptionDef { key, values, default, doc }
}

/// A word with its meaning: a format, an edge label, a pseudo-attribute.
pub type Word = (&'static str, &'static str);

pub static FORMATS: &[Word] = &[
    ("svg", "diagram laid out by Graphviz (`dot`), with icons"),
    ("dot", "Graphviz source"),
    ("text", "nodes and edges as indented text, with their provenance"),
    ("mermaid", "Mermaid flowchart (ELK layout)"),
    ("sequence", "Mermaid sequence diagram of the steps of the processes the view selects (`edges process(orders)`)"),
    (
        "traversal",
        "SVG of the devices traversed, a row per path (`edges path(a, b)`) and per step of the processes (`edges process(orders)`)",
    ),
    ("ascii", "ASCII art, laid out by the compiler"),
    ("csv", "table, comma-separated values: a row per node, or per edge when the view has edges"),
    ("tsv", "table, tab-separated values, as Excel pastes them"),
    ("markdown", "table in Markdown"),
    ("html", "table in an HTML page, to open in a browser"),
    ("xlsx", "table in an Excel workbook: the header frozen, filters, numbers and links typed"),
    ("drawio", "diagram to edit in draw.io (diagrams.net), laid out by Graphviz"),
];

pub const ROWS: &[&str] = &["nodes", "edges"];

pub const DIRECTIONS: &[&str] = &["lr", "rl", "tb", "bt"];
pub const ORIENTATIONS: &[&str] = &["connection", "data"];
pub const BOOLEANS: &[&str] = &["true", "false"];
pub const ENGINES: &[&str] = &["graphviz", "elk"];
pub const LINES: &[&str] = &["solid", "dashed", "dotted"];
pub const SHAPES: &[&str] = &["box", "rounded", "ellipse", "cylinder", "hexagon", "note"];
pub const LAYOUTS: &[&str] = &["free"];
/// The routing of an edge on its own: Graphviz routes all the edges of a graph alike, a
/// straight edge is redrawn after the layout.
pub const EDGE_ROUTINGS: &[&str] = &["straight"];
pub const LOOPS: &[&str] = &["drop", "keep"];
pub const ROUTINGS: &[&str] = &["curved", "polyline", "straight", "orthogonal"];
pub const LABEL_LAYOUTS: &[&str] = &["nodes", "free"];
pub const MERGES: &[&str] = &["endpoints", "port", "none"];
pub const SEVERITIES: &[&str] = &["error", "warning", "info"];

/// The expectations of a check.
pub static EXPECTATIONS: &[Word] = &[
    ("none", "nothing is selected"),
    ("some", "at least one node or edge is selected"),
    ("count <op> N", "the number of selected nodes or edges compared to N (`=`, `!=`, `<`, `<=`, `>`, `>=`)"),
    ("all <predicate>", "every selected node or edge satisfies the predicate, written as in `where`"),
];

/// The attributes of a check.
/// The attributes of a view, for its readers: what the site and the previews show about it.
pub static VIEW_ATTRIBUTES: &[OptionDef] = &[
    option("title", Values::Text, "", "the title of the view, for its readers; its name otherwise"),
    option("description", Values::Text, "", "what the view shows and what it is for, a sentence or two"),
    option(
        "failing",
        Values::Resources,
        "",
        "a scenario: the resources failing together (a system, as what it contains); `down` the resources unavailable then",
    ),
];

pub static CHECK_ATTRIBUTES: &[OptionDef] = &[
    option("message", Values::Text, "", "why the rule exists, shown when it fails"),
    option(
        "severity",
        Values::Closed(SEVERITIES),
        "error",
        "a failure is an error, or a warning for a rule introduced gradually",
    ),
    option(
        "failing",
        Values::Resources,
        "",
        "a scenario: the resources failing together (a system, as what it contains); `down` the resources unavailable then",
    ),
];

/// The options of `render` that only steer the layout of a diagram: hints, listed apart in the
/// reference, after the options every view may need.
pub const LAYOUT_HINTS: &[&str] = &[
    "layout",
    "stack",
    "cross",
    "first",
    "last",
    "routing",
    "edge_labels_layout",
    "edge_labels_wrap",
    "spacing",
    "node_label_size",
    "edge_label_size",
    "frame_label_size",
];

pub static RENDER_OPTIONS: &[OptionDef] = &[
    option(
        "direction",
        Values::Closed(DIRECTIONS),
        "lr",
        "direction of the edges: left to right, right to left, top to bottom, bottom to top",
    ),
    option(
        "orientation",
        Values::Closed(ORIENTATIONS),
        "connection",
        "arrow from the initiator of the connection, or along the data (`exchange`)",
    ),
    option("node_labels", Values::Keys, "[label]", "attributes shown in nodes and frames"),
    option(
        "edge_labels",
        Values::EdgeLabels,
        "[port]",
        "labels of the edges; unless written, a diagram of more than 20 edges leaves them to the tooltips",
    ),
    option("legend", Values::Closed(BOOLEANS), "false", "legend of the style rules carrying a `legend`"),
    option(
        "layout",
        Values::Closed(ENGINES),
        "graphviz",
        "engine laying out the SVG: Graphviz, or Eclipse ELK (a compiler built with the `elk` feature), drawn by the compiler",
    ),
    option("icons", Values::Closed(BOOLEANS), "true", "icon of its kind in each node and frame"),
    option(
        "satellites",
        Values::Closed(BOOLEANS),
        "true",
        "a node exchanging with two frames or more drawn once in each of them, in gray, rather than once with every edge pulled to it (SVG)",
    ),
    option(
        "twins",
        Values::Closed(BOOLEANS),
        "true",
        "in a frame, the nodes of one kind with exactly the same edges drawn as one box listing them, with one edge per shared exchange (SVG)",
    ),
    option(
        "title",
        Values::Closed(BOOLEANS),
        "true",
        "title of a sequence diagram drawing a single process: its label",
    ),
    option(
        "stack",
        Values::Levels,
        "[]",
        "frames or nodes placed in this order along the direction; a nested list puts several on a level",
    ),
    option("cross", Values::Resources, "[]", "frames or nodes placed in this order across the direction"),
    option(
        "first",
        Values::Resources,
        "[]",
        "frames or nodes placed first in their frame: at the top in `tb` and `bt`, at the left in `lr` and `rl`",
    ),
    option("last", Values::Resources, "[]", "frames or nodes placed last in their frame"),
    option(
        "routing",
        Values::Closed(ROUTINGS),
        "curved",
        "shape of the edges: curves, broken lines, straight lines, horizontal and vertical segments",
    ),
    option(
        "edge_labels_wrap",
        Values::Wrap,
        "",
        "labels on several lines: a line break between values beyond a number of characters, or one value per line",
    ),
    option(
        "columns",
        Values::Columns,
        "",
        "columns of a table (`csv`, `tsv`, `markdown`): keys, pseudo-attributes or paths, `\"ancestor(site)\"` quoted",
    ),
    option(
        "rows",
        Values::Closed(ROWS),
        "",
        "rows of a table: one per node, or one per edge; by default per edge when the view has edges",
    ),
    option(
        "merge_label",
        Values::MergeLabel,
        "list",
        "label of an edge merging links: their labels, or a summary (`sftp ×9`, the detail in the tooltip) from 2 links (`count`) or from a number of links",
    ),
    option(
        "edge_labels_layout",
        Values::Closed(LABEL_LAYOUTS),
        "nodes",
        "labels laid out as nodes on the edges, or placed on the edges after the layout, without moving anything",
    ),
    option(
        "spacing",
        Values::Numbers,
        "1",
        "space between the nodes and between the ranks, as a factor: `1.5` for both, `[1, 2]` for the nodes then the ranks",
    ),
    option("node_label_size", Values::Number, "10", "size of the text of the nodes, in points"),
    option("edge_label_size", Values::Number, "9", "size of the labels of the edges, in points"),
    option("frame_label_size", Values::Number, "11", "size of the titles of the frames, in points"),
];

pub static EDGE_LABELS: &[Word] = &[
    ("port", "protocol and port of a flow"),
    ("carries", "the data carried"),
    ("through", "the devices and tunnels traversed"),
    ("description", "the descriptions of the links"),
    ("step", "the step number of a `follow` edge"),
    ("relation", "the relation name of a reference or connector edge"),
    ("interface", "the interfaces of the cables between two devices (`render traversal`)"),
    ("none", "no label"),
];

/// Properties of style rules, with what they apply to.
pub static STYLE_PROPERTIES: &[(OptionDef, Styled)] = &[
    (option("color", Values::Color, "", "outline of a node, border of a frame, line of an edge"), Styled::Both),
    (option("fill", Values::Color, "", "background of a node or a frame"), Styled::Nodes),
    (option("line", Values::Closed(LINES), "solid", "line of an edge or outline of a node"), Styled::Both),
    (option("shape", Values::Closed(SHAPES), "box", "shape of a node"), Styled::Nodes),
    (option("layout", Values::Closed(LAYOUTS), "", "an edge drawn without placing nodes"), Styled::Edges),
    (
        option(
            "routing",
            Values::Closed(EDGE_ROUTINGS),
            "",
            "an edge drawn as a straight line in SVG, from the border of a node to the other, whatever the routing of the view",
        ),
        Styled::Edges,
    ),
    (option("legend", Values::Text, "", "entry of the rule in the legend"), Styled::Both),
];

/// What a style property applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Styled {
    Nodes,
    Edges,
    Both,
}

/// Properties of the frame of a zone, written after its members.
pub static ZONE_PROPERTIES: &[OptionDef] = &[
    option("color", Values::Color, "", "border of the frame"),
    option("fill", Values::Color, "", "background of the frame"),
    option("line", Values::Closed(LINES), "dashed", "line of the border"),
    option("in", Values::Zone, "", "the zone the frame is drawn in"),
];

pub static LIFT_OPTIONS: &[OptionDef] = &[
    option("via", Values::Path, "", "the path replacing each end (required, unless every kind has its own)"),
    option(
        "via_<kind>",
        Values::Path,
        "",
        "the path replacing the ends of a kind or a group, before `via`: `via_job: on`",
    ),
    option("loops", Values::Closed(LOOPS), "drop", "edges whose two ends are replaced by the same resource"),
    option(
        "merge",
        Values::Closed(MERGES),
        "endpoints",
        "one edge per pair of ends, per pair and `port_spec`, or no merging",
    ),
];

pub static FOLLOW_OPTIONS: &[OptionDef] = &[
    option(
        "from",
        Values::Resource("flow_endpoint"),
        "",
        "where the data starts; without it, every link carrying the data",
    ),
    option(
        "relay",
        Values::Kinds,
        "",
        "kinds or resources passing the data on along links that do not declare it, in the direction of `exchange` or outwards: `relay: [database]`, `relay: [stock.db]`",
    ),
    option(
        "relay_to",
        Values::Kinds,
        "",
        "the kinds or resources the relayed links may lead to: `relay_to: [app]`; any by default",
    ),
    option(
        "side",
        Values::Resources,
        "",
        "data whose links leaving the resources on the way are drawn too, without a step: the calls serving the processing, `side: [sites]`; `side: all`, whatever they carry",
    ),
];

pub static EDGES_OPTIONS: &[OptionDef] =
    &[option("through", Values::Closed(&["show"]), "", "the devices traversed drawn as nodes")];

/// Pseudo-attributes of filters, with what they apply to.
pub static PSEUDO: &[(&str, Styled, &str)] = &[
    ("kind", Styled::Both, "kind of the resource, relation or connector kind of the edge (`link` for a link)"),
    ("name", Styled::Both, "qualified name"),
    ("incomplete", Styled::Both, "true if the element is incomplete"),
    ("source", Styled::Edges, "source end, usable at the head of a path"),
    ("target", Styled::Edges, "target end, usable at the head of a path"),
    ("proto", Styled::Edges, "protocol of the `port_spec`"),
    ("port", Styled::Edges, "port of the `port_spec`; true if the value falls within a range"),
    ("parent", Styled::Nodes, "the parent resource"),
    ("step", Styled::Edges, "step of a `follow` edge"),
    ("hosts", Styled::Nodes, "the resources whose `on` designates the node, usable at the head of a path"),
    ("shared", Styled::Nodes, "true if what the node hosts belongs to at least two systems"),
    ("virtual", Styled::Nodes, "true if the node is a virtual machine (`virtual server web01 { on: esx01 }`)"),
    (
        "down",
        Styled::Nodes,
        "true if the node is unavailable in the scenario of the view or the check (`failing: [fw01, fw02]`), failing or through them",
    ),
    (
        "failures",
        Styled::Nodes,
        "the resources whose failure alone makes the node unavailable (impact analysis), usable at the head of a path",
    ),
    (
        "failures(hosting)",
        Styled::Nodes,
        "the same through hosting only (containment, racks, `on`, cluster and pool members, connector ends), not the links: a single point of failure of the hosting",
    ),
    (
        "failures(pairs)",
        Styled::Nodes,
        "the resources of the pairs whose failure together, and not of either alone, makes the node unavailable: what its redundancy rests on",
    ),
];

/// The keys of a table.
pub fn keys(options: &'static [OptionDef]) -> impl Iterator<Item = &'static str> {
    options.iter().map(|o| o.key)
}

/// The words of a table.
pub fn words(words: &'static [Word]) -> impl Iterator<Item = &'static str> {
    words.iter().map(|w| w.0)
}

/// The option of a table with this key.
pub fn find(options: &'static [OptionDef], key: &str) -> Option<&'static OptionDef> {
    options.iter().find(|o| o.key == key)
}

/// `via: <path>, loops: drop | keep`: the options of a table, for a help message.
pub fn summary(options: &[OptionDef]) -> String {
    let parts: Vec<String> = options.iter().map(|o| format!("{}: {}", o.key, values_text(o.values))).collect();
    parts.join(", ")
}

/// The values of an option, as written in a message or the reference.
pub fn values_text(values: Values) -> String {
    match values {
        Values::Closed(words) => words.join(" | "),
        Values::EdgeLabels => "[<label>, …]".to_owned(),
        Values::Keys => "[<key>, …]".to_owned(),
        Values::Resources => "[<resource>, …]".to_owned(),
        Values::Levels => "[<resource> | [<resource>, …], …]".to_owned(),
        Values::Resource(_) => "<resource>".to_owned(),
        Values::Path => "<path>".to_owned(),
        Values::Color => "<color>".to_owned(),
        Values::Text => "<string>".to_owned(),
        Values::Wrap => "<number> | values".to_owned(),
        Values::MergeLabel => "list | count | <number>".to_owned(),
        Values::Columns => "[<key or path>, …]".to_owned(),
        Values::Kinds => "[<kind or resource>, …]".to_owned(),
        Values::Number => "<number>".to_owned(),
        Values::Numbers => "<number> | [<number>, <number>]".to_owned(),
        Values::Zone => "<zone>".to_owned(),
    }
}
