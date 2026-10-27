//! View validation and evaluation (spec § 6): nodes, groups, edges, `lift`, merging.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::check::{Holder, Model};
use crate::diagnostic::Diagnostic;
use crate::plan::{
    Check, EdgeSelector, Expect, Follow, Lift, Loops, Merge, Name, Path, Plans, Predicate, Selector, Step, StepKind,
    StyleTarget, View, ZoneMembers,
};
use crate::query::{self, Ctx, EdgeView, Item, Subject, Text};
use crate::registry::{Reference, Registry, ResourceId};
use crate::resolve::{Resolution, Resolver};
use crate::schema::{self, Ty};
use crate::suggest;
use crate::vocabulary::{self, Values};

// ---------------------------------------------------------------------------
// Render options
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    LeftRight,
    RightLeft,
    TopBottom,
    BottomTop,
}

impl Direction {
    /// Whether the edges go down (or up) rather than across.
    pub fn vertical(self) -> bool {
        matches!(self, Self::TopBottom | Self::BottomTop)
    }

    /// Whether the edges go right to left, or bottom to top.
    pub fn reversed(self) -> bool {
        matches!(self, Self::RightLeft | Self::BottomTop)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Connection,
    Data,
}

/// Edge labels on several lines (`edge_labels_wrap` render option).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    /// A line break between values beyond this number of characters.
    Width(usize),
    /// One value per line.
    Values,
}

/// The shape of the edges (`routing` render option).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routing {
    Curved,
    Polyline,
    Straight,
    Orthogonal,
}

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub formats: Vec<String>,
    pub direction: Direction,
    pub orientation: Orientation,
    pub node_labels: Vec<String>,
    pub edge_labels: Vec<String>,
    /// Whether `edge_labels` is written in the view: written, the labels are drawn whatever the
    /// number of edges; by default, a dense diagram leaves them to the tooltips.
    pub edge_labels_written: bool,
    pub legend: bool,
    /// The SVG laid out by Eclipse ELK rather than Graphviz (`layout: elk`).
    pub elk: bool,
    /// The title of a sequence diagram: the label of its process.
    pub title: bool,
    /// Icons of the kinds in SVG renderings.
    pub icons: bool,
    /// A node exchanging with several frames drawn once in each of them, as a satellite, rather
    /// than once with every edge pulled to it (SVG).
    pub satellites: bool,
    /// In a frame, the nodes of one kind with exactly the same edges drawn as one box listing
    /// them, with one edge per shared exchange (SVG).
    pub twins: bool,
    /// Levels of frames or nodes to place in this order, top to bottom (`tb`, `bt`) or left to
    /// right (`lr`, `rl`), several on a level (`[[a, b], c]`): a hint for the layout, real edges
    /// going the other way prevail.
    pub stack: Vec<Vec<String>>,
    /// Frames or nodes to place in this order across the direction of the edges: top to bottom
    /// (`lr`, `rl`) or left to right (`tb`, `bt`), a hint for the layout.
    pub cross: Vec<String>,
    /// Frames or nodes placed first (top in `tb` / `bt`, left in `lr` / `rl`) or last in their
    /// frame, before or after the other items of the frame.
    pub first: Vec<String>,
    pub last: Vec<String>,
    pub routing: Routing,
    /// Edge labels placed after the layout, on the edges, rather than as nodes laid out with
    /// the others (`edge_labels_layout: free`).
    pub free_labels: bool,
    pub wrap: Option<Wrap>,
    /// From how many merged links an edge is labeled with a summary (`merge_label`).
    pub summary_from: Option<usize>,
    /// The columns of a table view, as written; the rows, `nodes` or `edges`.
    pub columns: Vec<String>,
    pub rows: Option<String>,
    /// Factors of the space between the nodes and between the ranks (`spacing`).
    pub spacing: (f64, f64),
    /// Sizes of the text, in points: nodes, edge labels, frame titles.
    pub node_label_size: f64,
    pub edge_label_size: f64,
    pub frame_label_size: f64,
}

impl RenderOptions {
    fn of(view: &View) -> Self {
        let mut options = Self {
            formats: vec!["svg".to_owned()],
            direction: Direction::LeftRight,
            orientation: Orientation::Connection,
            node_labels: vec!["label".to_owned()],
            edge_labels: vec!["port".to_owned()],
            edge_labels_written: false,
            legend: false,
            elk: false,
            title: true,
            icons: true,
            satellites: true,
            twins: true,
            stack: Vec::new(),
            cross: Vec::new(),
            first: Vec::new(),
            last: Vec::new(),
            routing: Routing::Curved,
            free_labels: false,
            wrap: None,
            summary_from: None,
            columns: Vec::new(),
            rows: None,
            spacing: (1.0, 1.0),
            node_label_size: 10.0,
            edge_label_size: 9.0,
            frame_label_size: 11.0,
        };
        let Some(render) = &view.render else { return options };
        if !render.formats.is_empty() {
            options.formats = render.formats.iter().map(|f| f.text.clone()).collect();
        }
        for option in &render.options {
            let words: Vec<String> = option.value.items().iter().map(|v| v.raw.clone()).collect();
            match (option.key.as_str(), option.value.raw.as_str()) {
                ("direction", "tb") => options.direction = Direction::TopBottom,
                ("direction", "bt") => options.direction = Direction::BottomTop,
                ("direction", "rl") => options.direction = Direction::RightLeft,
                ("direction", _) => options.direction = Direction::LeftRight,
                ("stack", _) => {
                    options.stack = option
                        .value
                        .items()
                        .iter()
                        .map(|level| level.items().iter().map(|v| v.raw.clone()).collect())
                        .collect()
                }
                ("cross", _) => options.cross = words,
                ("first", _) => options.first = words,
                ("last", _) => options.last = words,
                ("routing", "polyline") => options.routing = Routing::Polyline,
                ("routing", "straight") => options.routing = Routing::Straight,
                ("routing", "orthogonal") => options.routing = Routing::Orthogonal,
                ("routing", _) => options.routing = Routing::Curved,
                ("edge_labels_layout", v) => options.free_labels = v == "free",
                ("columns", _) => {
                    options.columns = option.value.items().iter().map(|v| crate::query::value_text(v)).collect()
                }
                ("rows", v) => options.rows = Some(v.to_owned()),
                ("merge_label", "count") => options.summary_from = Some(2),
                ("merge_label", v) => options.summary_from = v.parse().ok().filter(|&n| n >= 2),
                ("edge_labels_wrap", "values") => options.wrap = Some(Wrap::Values),
                ("edge_labels_wrap", v) => options.wrap = v.parse().ok().filter(|&w| w > 0).map(Wrap::Width),
                ("orientation", "data") => options.orientation = Orientation::Data,
                ("orientation", _) => options.orientation = Orientation::Connection,
                ("node_labels", _) => options.node_labels = words,
                ("edge_labels", _) => {
                    options.edge_labels = words.into_iter().filter(|w| w != "none").collect();
                    options.edge_labels_written = true;
                }
                ("legend", v) => options.legend = v == "true",
                ("layout", v) => options.elk = v == "elk",
                ("title", v) => options.title = v != "false",
                ("icons", v) => options.icons = v != "false",
                ("satellites", v) => options.satellites = v != "false",
                ("twins", v) => options.twins = v != "false",
                ("spacing", _) => {
                    let numbers: Vec<f64> = words.iter().filter_map(|w| positive(w)).collect();
                    options.spacing = match numbers.as_slice() {
                        [both] => (*both, *both),
                        [nodes, ranks] => (*nodes, *ranks),
                        _ => options.spacing,
                    }
                }
                ("node_label_size", v) => options.node_label_size = positive(v).unwrap_or(options.node_label_size),
                ("edge_label_size", v) => options.edge_label_size = positive(v).unwrap_or(options.edge_label_size),
                ("frame_label_size", v) => options.frame_label_size = positive(v).unwrap_or(options.frame_label_size),
                _ => {}
            }
        }
        options
    }
}

/// A positive number: `1.5`, `12`.
fn positive(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Checks plans against the schema and the registry (spec § 7).
/// Validates the plans; the names they reference are recorded in `names` (see [`crate::check::Model::names`]).
pub fn validate(
    plans: &Plans,
    registry: &Registry,
    names: &mut Vec<(crate::source::Span, Vec<crate::registry::ResourceId>)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if plans.project_styles.len() > 1 {
        let mut d = Diagnostic::error("several project style blocks", plans.project_styles[1].span).with_help(
            "keep a single root-level style block: file order has no meaning, so the cascade would be undefined",
        );
        for block in &plans.project_styles[..1] {
            d = d.with_span(block.span);
        }
        diagnostics.push(d);
    }

    let keys = known_keys(registry);
    let layers: BTreeSet<&str> = registry.links.iter().flat_map(|l| l.layers.iter().map(|(n, _)| n.as_str())).collect();
    let mut validator =
        Validator { registry, resolver: Resolver::new(registry), keys: &keys, layers: &layers, names, diagnostics };

    let mut names: HashMap<&str, crate::source::Span> = HashMap::new();
    for view in &plans.views {
        if let Some(&first) = names.get(view.name.as_str()) {
            validator
                .diagnostics
                .push(Diagnostic::error(format!("view {} declared twice", view.name), view.span).with_span(first));
        } else {
            names.insert(&view.name, view.span);
        }
        // A view is read on the site, in the previews, by people who never open its file: its
        // name is for the commands, its title for them.
        if !view.attributes.iter().any(|a| a.key == "title") {
            validator.diagnostics.push(
                Diagnostic::info(format!("view {} has no title: its readers see its name", view.name), view.span)
                    .with_help("`title: \"Application flows\"`, and a `description` saying what the view shows"),
            );
        }
        validator.view(view);
    }
    for block in &plans.project_styles {
        for rule in &block.rules {
            validator.style_rule(rule);
        }
    }

    let mut names: HashMap<&str, crate::source::Span> = HashMap::new();
    for check in &plans.checks {
        if let Some(&first) = names.get(check.name.as_str()) {
            validator
                .diagnostics
                .push(Diagnostic::error(format!("check {} declared twice", check.name), check.span).with_span(first));
        } else {
            names.insert(&check.name, check.span);
        }
        validator.check(check);
    }
}

struct Validator<'a> {
    registry: &'a Registry,
    resolver: Resolver<'a>,
    keys: &'a BTreeSet<String>,
    layers: &'a BTreeSet<&'a str>,
    names: &'a mut Vec<(crate::source::Span, Vec<crate::registry::ResourceId>)>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Validator<'_> {
    fn check(&mut self, check: &Check) {
        if check.view.select.is_none() {
            self.diagnostics.push(
                Diagnostic::error(format!("check {} selects nothing", check.name), check.span)
                    .with_help("select nodes (`select app where …`), edges (`edges [flow] where …`), or both"),
            );
        } else {
            self.view(&check.view);
        }
        match &check.expect {
            None => self.diagnostics.push(
                Diagnostic::error(format!("check {} has no expect clause", check.name), check.span).with_help(format!(
                    "expectations: {}",
                    vocabulary::EXPECTATIONS
                        .iter()
                        .map(|(w, _)| format!("`expect {w}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ),
            Some(Expect::All(predicate)) => self.predicate(predicate, !check.view.edges.is_empty()),
            Some(_) => {}
        }
        for attribute in &check.attributes {
            if attribute.key == "failing" {
                self.failing(attribute);
                continue;
            }
            match vocabulary::find(vocabulary::CHECK_ATTRIBUTES, &attribute.key) {
                Some(definition) => match definition.values {
                    Values::Closed(values) if !values.contains(&attribute.value.raw.as_str()) => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!("invalid value for {}: {}", attribute.key, attribute.value.raw),
                                attribute.value.span,
                            )
                            .with_help(format!("possible values: {}", values.join(", "))),
                        );
                    }
                    Values::Text if !matches!(attribute.value.kind, crate::registry::ValueKind::String(_)) => {
                        self.diagnostics.push(
                            Diagnostic::error(format!("{} is a quoted text", attribute.key), attribute.value.span)
                                .with_help(format!("{}: \"…\"", attribute.key)),
                        );
                    }
                    _ => {}
                },
                None => self.unknown(
                    "check attribute",
                    &attribute.key,
                    attribute.key_span,
                    vocabulary::keys(vocabulary::CHECK_ATTRIBUTES),
                ),
            }
        }
    }

    /// `failing: [fw01, fw02]`: names of resources; a logical one without content fails nothing.
    fn failing(&mut self, attribute: &crate::registry::Attribute) {
        let items = attribute.value.items();
        if items.is_empty() {
            self.diagnostics.push(
                Diagnostic::error("failing lists the resources failing together", attribute.value.span)
                    .with_help("failing: [fw01, fw02]"),
            );
        }
        for item in items {
            if !matches!(item.kind, crate::registry::ValueKind::Identifier | crate::registry::ValueKind::QualifiedName)
            {
                self.diagnostics.push(
                    Diagnostic::error(format!("failing lists names of resources, not {}", item.raw), item.span)
                        .with_help("failing: [fw01, fw02]"),
                );
                continue;
            }
            let name = Name { text: item.raw.clone(), span: item.span };
            self.resource(&name, &[], "resource");
            if let Resolution::Resolved(id) = self.resolver.resolve(&reference(&name), None, &[])
                && crate::impact::failing_seeds(self.registry, &[id]).is_empty()
            {
                self.diagnostics.push(Diagnostic::warning(
                    format!("{} is a {} and contains nothing that fails", item.raw, self.registry.resource(id).kind),
                    item.span,
                ));
            }
        }
    }

    fn view(&mut self, view: &View) {
        for attribute in &view.attributes {
            if attribute.key == "failing" {
                self.failing(attribute);
                continue;
            }
            match vocabulary::find(vocabulary::VIEW_ATTRIBUTES, &attribute.key) {
                Some(_) if !matches!(attribute.value.kind, crate::registry::ValueKind::String(_)) => {
                    self.diagnostics.push(
                        Diagnostic::error(format!("{} is a quoted text", attribute.key), attribute.value.span)
                            .with_help(format!("{}: \"…\"", attribute.key)),
                    );
                }
                Some(_) => {}
                None => self.unknown(
                    "view attribute",
                    &attribute.key,
                    attribute.key_span,
                    vocabulary::keys(vocabulary::VIEW_ATTRIBUTES),
                ),
            }
        }
        let Some(select) = &view.select else {
            self.diagnostics.push(Diagnostic::error(format!("view {} has no select clause", view.name), view.span));
            return;
        };
        let mut selected = BTreeSet::new();
        for kind in &select.kinds {
            let kinds = self.registry.schema.expand(&kind.text);
            if kinds.is_empty() {
                self.unknown(
                    "kind",
                    &kind.text,
                    kind.span,
                    self.registry.schema.kind_names().into_iter().chain(self.registry.schema.group_names()),
                );
            }
            selected.extend(kinds);
            // A participant stands for its members in the steps of a process: nothing to draw. Not
            // for the every-kind selection of a check without `select` (its kinds at its span).
            if self.registry.schema.base(&kind.text) == "participant" && kind.span != select.span {
                self.diagnostics.push(
                    Diagnostic::warning("a participant stands for its members in the steps of a process", kind.span)
                        .with_help("select the members (`select server`), or the process (`select process`)"),
                );
            }
        }
        if let Some(filter) = &select.filter {
            self.predicate(filter, false);
            self.filter_keys(filter, &selected);
        }
        for level in &select.group_by {
            let kinds = self.registry.schema.expand(&level.text);
            if kinds.is_empty() && !self.is_reference_key(&level.text) && !self.keys.contains(&level.text) {
                self.unknown("group-by level", &level.text, level.span, self.registry.schema.kind_names());
            }
            if kinds.iter().any(|k| selected.contains(k)) {
                self.diagnostics.push(Diagnostic::error(
                    format!("{} is both selected and used in group-by", level.text),
                    level.span,
                ));
            }
        }
        for selector in &view.edges {
            self.edge_selector(selector);
        }
        for option in &view.edge_options {
            self.diagnostics.push(
                Diagnostic::error(
                    format!("unknown edges option: {}: {}", option.key, option.value.raw),
                    option.key_span,
                )
                .with_help(format!("edges options: {}", vocabulary::summary(vocabulary::EDGES_OPTIONS))),
            );
        }
        for rule in &view.style {
            self.style_rule(rule);
        }
        let mut zone_names: HashMap<&str, crate::source::Span> = HashMap::new();
        for zone in &view.zones {
            if let Some(&first) = zone_names.get(zone.name.text.as_str()) {
                self.diagnostics.push(
                    Diagnostic::error(format!("zone {} declared twice", zone.name.text), zone.name.span)
                        .with_span(first),
                );
            }
            zone_names.insert(&zone.name.text, zone.name.span);
            match &zone.members {
                ZoneMembers::Names(names) => {
                    for name in names {
                        self.resource(name, &[], "resource");
                    }
                }
                ZoneMembers::Filter(filter) => self.predicate(filter, false),
            }
            for property in &zone.properties {
                if property.key == "parent" {
                    self.diagnostics.push(
                        Diagnostic::error("the zone a zone is drawn in is written `in:`", property.key_span)
                            .with_help("`zone sap [site-east] { in: hostco }`"),
                    );
                    continue;
                }
                if property.key == "in" {
                    self.zone_parent(view, zone, property);
                    continue;
                }
                match vocabulary::find(vocabulary::ZONE_PROPERTIES, &property.key) {
                    Some(definition) => {
                        if let Values::Closed(values) = definition.values
                            && !values.contains(&property.value.raw.as_str())
                        {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!("invalid value for {}: {}", property.key, property.value.raw),
                                    property.value.span,
                                )
                                .with_help(format!("possible values: {}", values.join(", "))),
                            );
                        }
                    }
                    None => self.unknown(
                        "zone property",
                        &property.key,
                        property.key_span,
                        vocabulary::keys(vocabulary::ZONE_PROPERTIES),
                    ),
                }
            }
        }
        if let Some(render) = &view.render {
            for format in &render.formats {
                if !vocabulary::words(vocabulary::FORMATS).any(|f| f == format.text) {
                    self.unknown("format", &format.text, format.span, vocabulary::words(vocabulary::FORMATS));
                }
            }
            for option in &render.options {
                let items: Vec<&str> = option.value.items().iter().map(|v| v.raw.as_str()).collect();
                if let Some(nested) = option.value.nested_list().filter(|_| option.key != "stack") {
                    self.diagnostics.push(
                        Diagnostic::error(format!("{}: a list cannot contain a list", option.key), nested.span)
                            .with_help("only `stack` groups items in levels: `stack: [[a, b], c]`"),
                    );
                }
                let Some(definition) = vocabulary::find(vocabulary::RENDER_OPTIONS, &option.key) else {
                    self.unknown(
                        "render option",
                        &option.key,
                        option.key_span,
                        vocabulary::keys(vocabulary::RENDER_OPTIONS),
                    );
                    continue;
                };
                let allowed: Option<&[&str]> = match definition.values {
                    Values::Closed(values) => Some(values),
                    Values::Resources | Values::Levels => {
                        // Frames or nodes: names of resources, grouped in levels for `stack`.
                        let zone = |name: &str| view.zones.iter().any(|z| z.name.text == name);
                        for value in option.value.items().into_iter().flat_map(|v| v.items()).filter(|v| !zone(&v.raw))
                        {
                            let name = Name { text: value.raw.clone(), span: value.span };
                            self.resource(&name, &[], "resource");
                        }
                        None
                    }
                    Values::Columns => {
                        for value in option.value.items() {
                            let text = crate::query::value_text(value);
                            match crate::render::table::column(&text) {
                                Some(mut path) => {
                                    // Diagnostics on the column as written.
                                    path.span = value.span;
                                    for step in &mut path.steps {
                                        step.span = value.span;
                                    }
                                    self.path(&path)
                                }
                                None => self.diagnostics.push(
                                    Diagnostic::error(format!("invalid column: {text}"), value.span).with_help(
                                        "a key, a pseudo-attribute or a path: `ip`, `on.label`, \"ancestor(site)\"",
                                    ),
                                ),
                            }
                        }
                        None
                    }
                    Values::MergeLabel => {
                        let raw = option.value.raw.as_str();
                        if !matches!(raw, "list" | "count") && raw.parse::<usize>().ok().filter(|&n| n >= 2).is_none() {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!("invalid value for {}: {raw}", option.key),
                                    option.value.span,
                                )
                                .with_help("`list`, `count`, or a number of merged links from 2 on"),
                            );
                        }
                        None
                    }
                    Values::Number | Values::Numbers => {
                        let count = if matches!(definition.values, Values::Number) { 1..=1 } else { 1..=2 };
                        let numbers = items.iter().all(|i| positive(i).is_some());
                        if !numbers || !count.contains(&items.len()) {
                            let help = if matches!(definition.values, Values::Number) {
                                "a positive number, such as 12"
                            } else {
                                "a positive number, such as 1.5, or two: `[1, 2]`"
                            };
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!("invalid value for {}: {}", option.key, option.value.raw),
                                    option.value.span,
                                )
                                .with_help(help),
                            );
                        }
                        None
                    }
                    Values::Wrap => {
                        let raw = option.value.raw.as_str();
                        if raw != "values" && raw.parse::<usize>().ok().filter(|&w| w > 0).is_none() {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!("invalid value for {}: {raw}", option.key),
                                    option.value.span,
                                )
                                .with_help("a number of characters, such as 40, or `values` for one value per line"),
                            );
                        }
                        None
                    }
                    Values::EdgeLabels => {
                        // The standard labels, or any key written on links (`circuit_id`).
                        let standard: Vec<&str> = vocabulary::words(vocabulary::EDGE_LABELS).collect();
                        for item in items.iter().filter(|i| !standard.contains(i) && !self.keys.contains(**i)) {
                            let candidates = standard.iter().copied().chain(self.keys.iter().map(String::as_str));
                            self.unknown("edge label", item, option.value.span, candidates);
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(allowed) = allowed {
                    for item in items.iter().filter(|i| !allowed.contains(i)) {
                        self.diagnostics.push(
                            Diagnostic::error(format!("invalid value for {}: {item}", option.key), option.value.span)
                                .with_help(format!("possible values: {}", allowed.join(", "))),
                        );
                    }
                }
            }
        }
    }

    /// `in: <zone>`: a zone of the view, other than the zone itself, without a cycle.
    fn zone_parent(&mut self, view: &View, zone: &crate::plan::Zone, property: &crate::registry::Attribute) {
        let name = property.value.raw.as_str();
        if !view.zones.iter().any(|z| z.name.text == name) {
            let zones: Vec<&str> = view.zones.iter().map(|z| z.name.text.as_str()).collect();
            let mut d = Diagnostic::error(
                format!("zone {}: {name} is not a zone of the view", zone.name.text),
                property.value.span,
            );
            if let Some(suggestion) = crate::suggest::closest(name, zones.iter().copied()) {
                d = d.with_help(format!("did you mean `{suggestion}`?"));
            }
            self.diagnostics.push(d);
            return;
        }
        // Up the parents from the zone: back to it is a cycle.
        let mut seen = vec![zone.name.text.as_str()];
        let mut current = Some(name);
        while let Some(parent) = current {
            if seen.contains(&parent) {
                self.diagnostics.push(Diagnostic::error(
                    format!("zone {}: its parents make a cycle ({} → {parent})", zone.name.text, seen.join(" → ")),
                    property.value.span,
                ));
                return;
            }
            seen.push(parent);
            current = view.zones.iter().find(|z| z.name.text == parent).and_then(zone_parent);
        }
    }

    fn edge_selector(&mut self, selector: &EdgeSelector) {
        match &selector.selector {
            Selector::Layers(names) => {
                for name in names {
                    if !self.layers.contains(name.text.as_str()) {
                        let mut d = Diagnostic::warning(format!("no link carries the layer {}", name.text), name.span);
                        if let Some(s) = suggest::closest(&name.text, self.layers.iter().copied()) {
                            d = d.with_help(format!("did you mean `{s}`?"));
                        }
                        self.diagnostics.push(d);
                    }
                }
            }
            Selector::Relation(name) => {
                let connector = !self.registry.schema.expand(&name.text).is_empty()
                    && self
                        .registry
                        .schema
                        .expand(&name.text)
                        .iter()
                        .all(|k| self.registry.schema.kind_def(k).is_some_and(|d| d.connector.is_some()));
                if !connector && name.text != "parent" && !self.is_reference_key(&name.text) {
                    let candidates: Vec<String> =
                        reference_keys().into_iter().chain(["parent".to_owned(), "connector".to_owned()]).collect();
                    self.unknown("relation", &name.text, name.span, candidates.iter().map(String::as_str));
                }
            }
            Selector::Lift(lift) => self.lift(lift),
            Selector::Follow(follow) => self.follow(follow),
            Selector::Process(name) => self.resource(name, &["process"], "process"),
            Selector::Path(from, to) => {
                self.resource(from, &[], "resource");
                self.resource(to, &[], "resource");
            }
        }
        if let Some(filter) = &selector.filter {
            self.predicate(filter, true);
        }
    }

    fn lift(&mut self, lift: &Lift) {
        if matches!(lift.inner.selector, Selector::Lift(_)) {
            self.diagnostics.push(Diagnostic::error("lift cannot be nested", lift.inner.span));
        }
        self.edge_selector(&lift.inner);
        match &lift.via {
            Some(path) => self.path(path),
            None if !lift.via_kinds.is_empty() => {}
            None => self.diagnostics.push(Diagnostic::error("lift requires a `via` path", lift.span)),
        }
        for (kind, path) in &lift.via_kinds {
            if self.registry.schema.expand(&kind.text).is_empty() {
                let candidates =
                    self.registry.schema.kind_names().into_iter().chain(self.registry.schema.group_names());
                self.unknown("kind", &kind.text, kind.span, candidates);
            }
            self.path(path);
        }
        for (option, span) in &lift.invalid_options {
            self.diagnostics.push(
                Diagnostic::error(format!("invalid lift option: {option}"), *span)
                    .with_help(format!("options: {}", vocabulary::summary(vocabulary::LIFT_OPTIONS))),
            );
        }
    }

    fn follow(&mut self, follow: &Follow) {
        self.resource(&follow.data, &["data"], "data");
        // A kind or a group, otherwise a resource.
        for name in follow.relay.iter().chain(&follow.relay_to) {
            if self.registry.schema.expand(&name.text).is_empty() {
                self.resource(name, &[], "kind or resource");
            }
        }
        if let Some(from) = &follow.from {
            self.resource(from, &self.registry.schema.expand("flow_endpoint"), "flow_endpoint");
        }
        for name in follow.side.iter().filter(|n| n.text != "all") {
            self.resource(name, &["data"], "data");
        }
        for (option, span) in &follow.invalid_options {
            self.diagnostics.push(
                Diagnostic::error(format!("invalid follow option: {option}"), *span)
                    .with_help(format!("options: {}", vocabulary::summary(vocabulary::FOLLOW_OPTIONS))),
            );
        }
    }

    /// Checks that a name written in a plan designates a resource of one of the `expected` kinds.
    fn resource(&mut self, name: &Name, expected: &[&str], expected_text: &str) {
        let expected_text = expected_text.replace('|', " or ");
        let resolution = self.resolver.resolve(&reference(name), None, expected);
        crate::check::record_name(self.names, name.span, &resolution);
        match resolution {
            Resolution::Resolved(_) => {}
            Resolution::Dangling => {
                let candidates = self
                    .registry
                    .resources
                    .iter()
                    .filter(|r| expected.contains(&r.kind.as_str()))
                    .map(|r| r.name.as_str());
                let last = name.text.rsplit('.').next().unwrap_or_default();
                let mut d = Diagnostic::error(format!("dangling reference: {}", name.text), name.span);
                if let Some(s) = suggest::closest(last, candidates) {
                    d = d.with_help(format!("did you mean `{s}`?"));
                }
                self.diagnostics.push(d);
            }
            Resolution::WrongType(found) => {
                let kinds: BTreeSet<&str> = found.iter().map(|&f| self.registry.resource(f).kind.as_str()).collect();
                let kinds: Vec<&str> = kinds.into_iter().collect();
                self.diagnostics.push(Diagnostic::error(
                    format!("{} is a {}, {expected_text} expected", name.text, kinds.join(" / ")),
                    name.span,
                ));
            }
            Resolution::Ambiguous(candidates) => {
                let names: Vec<String> = candidates.iter().map(|&c| self.registry.qualified_name(c)).collect();
                let hint = self.resolver.minimal_suffix(candidates[0], &candidates);
                self.diagnostics.push(
                    Diagnostic::error(format!("ambiguous name: {} ({expected_text} expected)", name.text), name.span)
                        .with_help(format!(
                            "candidates: {}; use a more qualified name, for instance {hint}",
                            names.join(", ")
                        )),
                );
            }
        }
    }

    fn style_rule(&mut self, rule: &crate::plan::StyleRule) {
        match &rule.target {
            StyleTarget::Kind(k) if self.registry.schema.expand(k).is_empty() => {
                self.unknown("kind", k, rule.span, self.registry.schema.kind_names());
            }
            _ => {}
        }
        if let Some(filter) = &rule.filter {
            self.predicate(filter, matches!(rule.target, StyleTarget::Edges | StyleTarget::Layer(_)));
            if let StyleTarget::Kind(kind) = &rule.target {
                let kinds: BTreeSet<&str> = self.registry.schema.expand(kind).into_iter().collect();
                if !kinds.is_empty() {
                    self.filter_keys(filter, &kinds);
                }
            }
        }
        for property in &rule.properties {
            let Some((definition, _)) = vocabulary::STYLE_PROPERTIES.iter().find(|(p, _)| p.key == property.key) else {
                let properties = vocabulary::STYLE_PROPERTIES.iter().map(|(p, _)| p.key);
                self.unknown("style property", &property.key, property.key_span, properties);
                continue;
            };
            let allowed = match definition.values {
                Values::Closed(values) => Some(values),
                _ => None,
            };
            if let Some(allowed) = allowed
                && !allowed.contains(&property.value.raw.as_str())
            {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("invalid value for {}: {}", property.key, property.value.raw),
                        property.value.span,
                    )
                    .with_help(format!("possible values: {}", allowed.join(", "))),
                );
            }
        }
    }

    /// Validates a filter of nodes, or of edges (`on_edges`), whose comparisons may have a path of
    /// an end on their right.
    fn predicate(&mut self, predicate: &Predicate, on_edges: bool) {
        for path in predicate.paths() {
            self.path(path);
        }
        for (path, value) in predicate.ordered() {
            self.order_comparison(path, value);
        }
        for (function, value) in predicate.aggregates() {
            if function.text != "count" {
                self.unknown("function", &function.text, function.span, ["count"]);
            }
            if value.raw.parse::<u64>().is_err() {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("{} compares a count to {}, not a number", function.text, value.raw),
                        value.span,
                    )
                    .with_help("`count(on) >= 2`"),
                );
            }
        }
        for (_, right) in predicate.path_comparisons() {
            let end = matches!(right.steps.first().map(|s| &s.kind), Some(StepKind::Name(n)) if n == "source" || n == "target");
            if !on_edges {
                self.diagnostics.push(
                    Diagnostic::error("two paths are compared on an edge only", right.span)
                        .with_help("compare the ends of edges: `edges [flow] where source.owner = target.owner`"),
                );
            } else if !end {
                self.diagnostics.push(
                    Diagnostic::error("a path on the right of a comparison starts with source or target", right.span)
                        .with_help("`source.ancestor(system) != target.ancestor(system)`"),
                );
            }
        }
        // A name compared to what designates resources (`name`, `source`, `on`, `ancestor(site)`…)
        // is recorded when it designates one, for the editors; otherwise it stays a text to
        // compare, without a diagnostic.
        // The keys of the resources and of the links (`channel in [raw-prices]`), built in or
        // declared by the project: the names they designate, of the kinds they reference.
        let reference_keys = self.registry.schema.reference_keys();
        for (path, value) in predicate.compared() {
            let expected = match path.steps.last().map(|s| &s.kind) {
                Some(StepKind::Ancestor(kind)) => self.registry.schema.expand(kind),
                Some(StepKind::Name(n)) if ["name", "source", "target", "parent", "hosts"].contains(&n.as_str()) => {
                    Vec::new()
                }
                Some(StepKind::Name(n)) => {
                    let unions: Vec<&str> = reference_keys.iter().filter(|(k, _)| k == n).map(|(_, u)| *u).collect();
                    if unions.is_empty() {
                        continue;
                    }
                    unions.iter().flat_map(|u| self.registry.schema.expand(u)).collect()
                }
                _ => continue,
            };
            let Some(reference) = value.as_reference() else { continue };
            let resolution = self.resolver.resolve(&reference, None, &expected);
            if matches!(resolution, Resolution::Resolved(_)) {
                crate::check::record_name(self.names, value.span, &resolution);
            }
        }
    }

    fn path(&mut self, path: &Path) {
        for step in &path.steps {
            match &step.kind {
                StepKind::Ancestor(k) if self.registry.schema.expand(k).is_empty() => {
                    self.unknown("kind", k, step.span, self.registry.schema.kind_names());
                }
                StepKind::Name(n) if !vocabulary::PSEUDO.iter().any(|p| p.0 == n) && !self.keys.contains(n) => {
                    let candidates: Vec<&str> =
                        self.keys.iter().map(String::as_str).chain(vocabulary::PSEUDO.iter().map(|p| p.0)).collect();
                    self.unknown("key", n, step.span, candidates);
                }
                _ => {}
            }
        }
    }

    /// An order comparison needs a number or a version on both sides: a value of a key typed
    /// otherwise (`os`, `hosting`, a reference) has no order; a free key is compared on its text.
    fn order_comparison(&mut self, path: &Path, value: &crate::registry::Value) {
        let help = "`<`, `<=`, `>`, `>=` compare numbers and versions; compare others with `=`, `!=` or `in`";
        if !query::is_ordered(&query::value_text(value)) {
            self.diagnostics.push(
                Diagnostic::error(
                    format!("{} is neither a number nor a version: it cannot be compared in order", value.raw),
                    value.span,
                )
                .with_help(help),
            );
            return;
        }
        let reason = match path.steps.last().map(|s| &s.kind) {
            Some(StepKind::Ancestor(_)) => Some("a resource".to_owned()),
            Some(StepKind::Name(key)) if matches!(key.as_str(), "step" | "port") => None,
            Some(StepKind::Name(key)) if vocabulary::PSEUDO.iter().any(|p| p.0 == key) => {
                Some(format!("`{key}`, which is not a number"))
            }
            Some(StepKind::Name(key)) => {
                let types: Vec<&Ty> = schema::KINDS
                    .iter()
                    .flat_map(|k| k.attrs.iter())
                    .chain(schema::RESOURCE_KEYS)
                    .chain(schema::LINK_KEYS)
                    .filter(|a| a.key == key)
                    .map(|a| &a.ty)
                    .collect();
                let ordered = |ty: &Ty| matches!(ty, Ty::Number | Ty::Semver | Ty::Version | Ty::Bandwidth);
                (!types.is_empty() && !types.iter().any(|t| ordered(t))).then(|| match types[0] {
                    Ty::Ref(_) => format!("`{key}`, a reference"),
                    Ty::Symbol(_) => format!("`{key}`, a word of a closed list"),
                    _ => format!("`{key}`, which is not a number nor a version"),
                })
            }
            None => None,
        };
        if let Some(reason) = reason {
            self.diagnostics.push(
                Diagnostic::error(format!("{} cannot be compared in order: {reason}", path.text()), path.span)
                    .with_help(help),
            );
        }
    }

    /// The keys a node filter starts with must exist for at least one of the filtered kinds: in
    /// their schema, or written on a resource of these kinds.
    fn filter_keys(&mut self, filter: &Predicate, kinds: &BTreeSet<&str>) {
        let mut written: BTreeSet<&str> = BTreeSet::new();
        for resource in self.registry.resources.iter().filter(|r| kinds.contains(r.kind.as_str())) {
            written.extend(resource.attributes.iter().map(|a| a.key.as_str()));
        }
        let mut keys: BTreeSet<&str> = written.clone();
        for kind in kinds.iter().filter_map(|k| self.registry.schema.kind_def(k)) {
            keys.extend(kind.attrs.iter().map(|a| a.key));
        }
        keys.extend(schema::RESOURCE_KEYS.iter().map(|a| a.key));
        let pseudo = |key: &str| vocabulary::PSEUDO.iter().any(|p| p.0 == key && p.1 != vocabulary::Styled::Edges);
        for path in filter.paths() {
            let Some(Step { kind: StepKind::Name(key), span, .. }) = path.steps.first() else { continue };
            // A key unknown everywhere is already reported by `path`.
            if keys.contains(key.as_str()) || pseudo(key) || !self.keys.contains(key) {
                continue;
            }
            let names: Vec<&str> = kinds.iter().copied().collect();
            let mut d = Diagnostic::error(format!("{key} is not a key of {}", names.join(", ")), *span);
            if let Some(s) = suggest::closest(key, keys.iter().copied()) {
                d = d.with_help(format!("did you mean `{s}`?"));
            }
            self.diagnostics.push(d);
        }
    }

    fn is_reference_key(&self, key: &str) -> bool {
        reference_keys().iter().any(|k| k == key)
    }

    fn unknown<'c>(
        &mut self,
        what: &str,
        name: &str,
        span: crate::source::Span,
        candidates: impl IntoIterator<Item = &'c str>,
    ) {
        let mut d = Diagnostic::error(format!("unknown {what}: {name}"), span);
        if let Some(s) = suggest::closest(name, candidates) {
            d = d.with_help(format!("did you mean `{s}`?"));
        }
        self.diagnostics.push(d);
    }
}

fn reference(name: &Name) -> Reference {
    Reference { segments: name.text.split('.').map(str::to_owned).collect(), span: name.span }
}

/// Keys usable in paths: schema keys and keys written in the registry.
fn known_keys(registry: &Registry) -> BTreeSet<String> {
    let mut keys: BTreeSet<String> = schema::KINDS
        .iter()
        .flat_map(|k| k.attrs.iter())
        .chain(schema::RESOURCE_KEYS)
        .chain(schema::LINK_KEYS)
        .map(|a| a.key.to_owned())
        .collect();
    for resource in &registry.resources {
        keys.extend(resource.attributes.iter().map(|a| a.key.clone()));
    }
    for link in &registry.links {
        keys.extend(link.attributes.iter().map(|a| a.key.clone()));
    }
    keys
}

/// Keys whose values are references (`on`, `peer`, `owner`…).
pub(crate) fn reference_keys() -> Vec<String> {
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    for attr in schema::KINDS.iter().flat_map(|k| k.attrs.iter()).chain(schema::RESOURCE_KEYS) {
        if matches!(attr.ty, Ty::Ref(_)) {
            keys.insert(attr.key);
        }
    }
    keys.into_iter().map(str::to_owned).collect()
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Doubtful,
    Assumed,
    Confirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exchange {
    Push,
    Pull,
    Both,
}

impl Exchange {
    fn combine(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (None, x) | (x, None) => x,
            (Some(x), Some(y)) if x == y => Some(x),
            _ => Some(Self::Both),
        }
    }
}

/// An evaluated view, ready to be rendered.
#[derive(Clone, Debug)]
pub struct Graph {
    pub view: String,
    pub nodes: Vec<ResourceId>,
    pub groups: Vec<Group>,
    /// Node → index of its innermost group.
    pub node_group: HashMap<ResourceId, usize>,
    pub edges: Vec<GraphEdge>,
    pub options: RenderOptions,
    /// Whether the view selects edges: a table then has one row per edge.
    pub has_edge_clause: bool,
    /// The processes of its `process(…)` selectors: the steps of a sequence diagram.
    pub processes: Vec<ResourceId>,
    /// The ends of its `path(a, b)` selectors: the rows of a traversal.
    pub paths: Vec<(ResourceId, ResourceId)>,
    /// The resources failing together in its scenario (`failing: [fw01, fw02]`).
    pub failing: BTreeSet<ResourceId>,
    /// The resources unavailable in that scenario, the failing ones included (`down`).
    pub down: BTreeSet<ResourceId>,
}

#[derive(Clone, Debug)]
pub struct Group {
    /// The resource the frame stands for; `None` for a zone.
    pub resource: Option<ResourceId>,
    /// The zone the frame draws.
    pub zone: Option<ZoneFrame>,
    /// The value the frame gathers, for a level of `group-by` that is a free key or `component`.
    pub value: Option<String>,
    pub parent: Option<usize>,
}

/// A zone of the view, drawn as a top-level frame or in the frame of its `parent` zone.
#[derive(Clone, Debug)]
pub struct ZoneFrame {
    pub name: String,
    pub label: String,
    /// `color`, `fill`, `line`.
    pub properties: Vec<crate::registry::Attribute>,
}

/// The zone a zone is drawn in (`in:`), if it names one.
pub fn zone_parent(zone: &crate::plan::Zone) -> Option<&str> {
    zone.properties.iter().find(|p| p.key == "in").map(|p| p.value.raw.as_str())
}

/// The process a `process(…)` selector names.
/// The resource a name of a `path(a, b)` designates, of any kind.
/// The resources of the scenario of a view, `failing: [fw01, fw02]`, those that resolve.
fn failing_of(view: &View, registry: &Registry) -> BTreeSet<ResourceId> {
    let Some(attribute) = view.attributes.iter().find(|a| a.key == "failing") else { return BTreeSet::new() };
    let resolver = Resolver::new(registry);
    attribute
        .value
        .items()
        .into_iter()
        .filter_map(|item| {
            let name = Name { text: item.raw.clone(), span: item.span };
            match resolver.resolve(&reference(&name), None, &[]) {
                Resolution::Resolved(id) => Some(id),
                _ => None,
            }
        })
        .collect()
}

fn resource_of(ctx: &Ctx, name: &Name) -> Option<ResourceId> {
    match Resolver::new(ctx.registry).resolve(&reference(name), None, &[]) {
        Resolution::Resolved(id) => Some(id),
        _ => None,
    }
}

/// `path(a, b)`: the cables of the network path between two resources, between the devices of
/// their plugs; what runs on a machine to it (`on`), a virtual machine to its host; the
/// connectors crossed between routers.
fn path_edges(ctx: &Ctx, from: &Name, to: &Name) -> Vec<BaseEdge> {
    let (Some(a), Some(b)) = (resource_of(ctx, from), resource_of(ctx, to)) else { return Vec::new() };
    let network = crate::path::Network::new(ctx.registry, ctx.model);
    let Ok(path) = network.between(a, b) else { return Vec::new() };
    let mut edges = Vec::new();
    let device = |plug: ResourceId| {
        let resource = ctx.registry.resource(plug);
        if ctx.registry.schema.base(&resource.kind) == "interface" { resource.parent.unwrap_or(plug) } else { plug }
    };
    // The aggregated cables between two devices (a port-channel): one edge.
    let mut aggregates: HashSet<(ResourceId, ResourceId, String)> = HashSet::new();
    for &(p, q) in &path.cables {
        let aggregate = ctx.registry.resource(p).attribute("aggregate").map(|a| query::value_text(&a.value));
        if let Some(name) = aggregate
            && !aggregates.insert((device(p), device(q), name))
        {
            continue;
        }
        edges.push(BaseEdge { source: device(p), target: device(q), origin: Origin::Cable(p, q), step: None });
    }
    // Beyond the cabling: the devices of the `egress` (or `ingress`) and the end on the internet.
    for &(x, y) in &path.internet {
        edges.push(BaseEdge { source: x, target: y, origin: Origin::Reference("egress".to_owned()), step: None });
    }
    for &c in &path.connectors {
        let key =
            ctx.registry.schema.kind_def(&ctx.registry.resource(c).kind).and_then(|d| d.connector).unwrap_or("ends");
        if let [x, y] = ctx.model.reference(Holder::Resource(c), key) {
            edges.push(BaseEdge { source: *x, target: *y, origin: Origin::Connector(c), step: None });
        }
    }
    // The ends: down `on` to the machine, then to the host of a virtual machine.
    for end in [a, b] {
        let mut current = end;
        for _ in 0..8 {
            let resource = ctx.registry.resource(current);
            let machine = crate::schema::MACHINES.contains(&ctx.registry.schema.base(&resource.kind));
            if machine && !resource.is_virtual() {
                break;
            }
            let Some(&host) = ctx.model.reference(Holder::Resource(current), "on").first() else { break };
            edges.push(BaseEdge {
                source: current,
                target: host,
                origin: Origin::Reference("on".to_owned()),
                step: None,
            });
            current = host;
        }
    }
    edges
}

/// What a `path(a, b)` reports: no path, and why; the subnets of its ends routed by several
/// devices without a `gateway`.
fn path_problems(ctx: &Ctx, from: &Name, to: &Name) -> Vec<String> {
    let (Some(a), Some(b)) = (resource_of(ctx, from), resource_of(ctx, to)) else { return Vec::new() };
    let network = crate::path::Network::new(ctx.registry, ctx.model);
    let mut problems = Vec::new();
    if let Err(error) = network.between(a, b) {
        problems.push(format!("no path: {}", network.explain(&error)));
    }
    let subnets: Vec<ResourceId> =
        [a, b].into_iter().filter_map(|end| network.machines(end).first().and_then(|&m| network.subnet(m))).collect();
    for (subnet, gateways) in crate::path::ambiguous_gateways(&network, &subnets) {
        let names: Vec<String> = gateways.iter().map(|&g| ctx.registry.qualified_name(g)).collect();
        problems.push(format!(
            "subnet {} is routed by several devices: {} (declare its `gateway`)",
            ctx.registry.qualified_name(subnet),
            names.join(", ")
        ));
    }
    problems
}

fn process_of(ctx: &Ctx, name: &Name) -> Option<ResourceId> {
    match Resolver::new(ctx.registry).resolve(&reference(name), None, &["process"]) {
        Resolution::Resolved(id) => Some(id),
        _ => None,
    }
}

/// Whether the `where` of a plain edge selector (not `lift`, not `follow`) keeps none of the
/// edges the selector finds.
fn empty_filter(ctx: &Ctx, selector: &EdgeSelector) -> bool {
    if selector.filter.is_none() || matches!(selector.selector, Selector::Lift(_) | Selector::Follow(_)) {
        return false;
    }
    let unfiltered = EdgeSelector { filter: None, ..selector.clone() };
    base_edges(ctx, selector).is_empty() && !base_edges(ctx, &unfiltered).is_empty()
}

/// Why a filter may match nothing, from the values it compares: the label of a resource used
/// as a name, or a short name compared to `name`, which is qualified.
pub(crate) fn filter_hint(registry: &Registry, filter: &Predicate) -> Option<String> {
    for (path, value) in filter.compared() {
        let text = match &value.kind {
            crate::registry::ValueKind::String(text) => text.clone(),
            _ => value.raw.clone(),
        };
        let named = |id: ResourceId| registry.resource(id).name.eq_ignore_ascii_case(&text);
        let labeled = registry.ids().find(|&id| {
            registry
                .resource(id)
                .attribute("label")
                .is_some_and(|l| query::value_text(&l.value).eq_ignore_ascii_case(&text))
        });
        if let Some(id) = labeled.filter(|_| !registry.ids().any(named)) {
            let resource = registry.resource(id);
            return Some(format!(
                "`{text}` is the label of {} {}; filters compare names: `in [{}]`",
                resource.kind,
                registry.qualified_name(id),
                resource.name
            ));
        }
        let compares_name = matches!(path.steps.last().map(|s| &s.kind), Some(StepKind::Name(n)) if n == "name");
        if compares_name
            && !text.contains('.')
            && let Some(id) = registry.ids().find(|&id| named(id) && registry.qualified_name(id) != text)
        {
            let prefix = path.steps[..path.steps.len() - 1]
                .iter()
                .filter_map(|s| match &s.kind {
                    StepKind::Name(n) => Some(n.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(".");
            let subject = if prefix.is_empty() { "name".to_owned() } else { prefix };
            return Some(format!(
                "`name` is the qualified name ({}): write `{subject} in [{text}]`",
                registry.qualified_name(id)
            ));
        }
    }
    None
}

/// Whether a name written in a plan designates a resource: a suffix of its qualified name.
pub(crate) fn designates(registry: &Registry, name: &str, id: ResourceId) -> bool {
    let path = registry.path(id);
    let segments: Vec<&str> = name.split('.').collect();
    path.len() >= segments.len() && path[path.len() - segments.len()..] == segments[..]
}

#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub source: ResourceId,
    pub target: ResourceId,
    pub symmetric: bool,
    /// Relation or connector kind; `None` for flows.
    pub relation: Option<String>,
    pub ports: BTreeSet<String>,
    pub layers: BTreeSet<String>,
    pub carries: BTreeSet<ResourceId>,
    pub through: BTreeSet<ResourceId>,
    /// Tunnels or devices of the path shown on a segment (`through: show`).
    pub via: BTreeSet<ResourceId>,
    pub exchange: Option<Exchange>,
    pub confidence: Confidence,
    pub questions: Vec<String>,
    pub descriptions: Vec<String>,
    pub provenance: Vec<String>,
    /// Written links merged into this edge (indices in `Registry::links`).
    pub links: Vec<usize>,
    /// Connector resources drawn as this edge (`wan_link`, `vpn_tunnel`…).
    pub connectors: Vec<ResourceId>,
    /// References drawn as this edge: the resource carrying the reference and the one it
    /// designates (`peer` between two interfaces), kept through `lift`.
    pub declared: Vec<(ResourceId, ResourceId)>,
    /// Ordered `through` items of the first written link (the order of a single flow's path).
    pub through_path: Vec<ResourceId>,
    /// The steps of a `through` deduced from the cabling, each with its variants.
    pub through_steps: Vec<Vec<ResourceId>>,
    /// Where a deduced `through` comes from (`egress of site paris-dc1`).
    pub through_origins: BTreeSet<String>,
    /// Step in a `follow` walk: distance from its starting point.
    pub step: Option<usize>,
    selector: usize,
}

/// Where a base edge comes from.
#[derive(Clone, Debug)]
enum Origin {
    Link(usize),
    Reference(String),
    Connector(ResourceId),
    Parent,
    /// A cable of a network path (`path(a, b)`), drawn between the devices of its two plugs.
    Cable(ResourceId, ResourceId),
}

#[derive(Clone, Debug)]
struct BaseEdge {
    source: ResourceId,
    target: ResourceId,
    origin: Origin,
    step: Option<usize>,
}

struct BaseEdgeView<'a> {
    edge: &'a BaseEdge,
    registry: &'a Registry,
}

impl EdgeView for BaseEdgeView<'_> {
    fn source(&self) -> ResourceId {
        self.edge.source
    }

    fn target(&self) -> ResourceId {
        self.edge.target
    }

    fn attribute(&self, ctx: &Ctx, key: &str) -> Option<Vec<Item>> {
        Some(match (&self.edge.origin, key) {
            (origin, "incomplete") => {
                let connector = match origin {
                    Origin::Connector(r) => Some(*r),
                    _ => None,
                };
                let incomplete = [self.edge.source, self.edge.target]
                    .into_iter()
                    .chain(connector)
                    .any(|id| ctx.model.incomplete.contains(&id));
                vec![plain(incomplete.to_string())]
            }
            (_, "step") => self.edge.step.map(|s| plain(s.to_string())).into_iter().collect(),
            (Origin::Link(i), "proto") => link_port(self.registry, *i).map(|(p, _)| plain(p)).into_iter().collect(),
            (Origin::Link(i), "port") => link_port(self.registry, *i)
                .and_then(|(_, port)| port)
                .map(|p| Item::Value(Text { text: p, is_port: true }))
                .into_iter()
                .collect(),
            (Origin::Link(_), "kind") => vec![plain("link".to_owned())],
            (Origin::Link(i), key) => {
                let references = ctx.model.reference(Holder::Link(*i), key);
                if !references.is_empty() {
                    return Some(references.iter().copied().map(Item::Resource).collect());
                }
                match self.registry.links[*i].attribute(key) {
                    Some(a) => query::value_items(&a.value),
                    None => query::default_value(key).map(|v| plain(v.to_owned())).into_iter().collect(),
                }
            }
            (Origin::Connector(r), "kind") => vec![plain(self.registry.resource(*r).kind.clone())],
            (Origin::Connector(r), key) => ctx.step(*r, &StepKind::Name(key.to_owned())),
            (Origin::Reference(k), "kind") => vec![plain(k.clone())],
            (Origin::Parent, "kind") => vec![plain("parent".to_owned())],
            (Origin::Cable(..), "kind") => vec![plain("peer".to_owned())],
            _ => Vec::new(),
        })
    }
}

impl EdgeView for GraphEdge {
    fn source(&self) -> ResourceId {
        self.source
    }

    fn target(&self) -> ResourceId {
        self.target
    }

    fn attribute(&self, ctx: &Ctx, key: &str) -> Option<Vec<Item>> {
        let ports = || self.ports.iter().map(|p| p.split_once('/').map_or((p.as_str(), None), |(a, b)| (a, Some(b))));
        Some(match key {
            "proto" => ports().map(|(p, _)| plain(p.to_owned())).collect(),
            "port" => ports()
                .filter_map(|(_, p)| p.map(|p| Item::Value(Text { text: p.to_owned(), is_port: true })))
                .collect(),
            "kind" => vec![plain(self.relation.clone().unwrap_or_else(|| "link".to_owned()))],
            "carries" => self.carries.iter().copied().map(Item::Resource).collect(),
            "through" => self.through.iter().copied().map(Item::Resource).collect(),
            "confidence" => vec![plain(
                match self.confidence {
                    Confidence::Confirmed => "confirmed",
                    Confidence::Assumed => "assumed",
                    Confidence::Doubtful => "doubtful",
                }
                .to_owned(),
            )],
            "exchange" => self
                .exchange
                .map(|e| {
                    plain(
                        match e {
                            Exchange::Push => "push",
                            Exchange::Pull => "pull",
                            Exchange::Both => "both",
                        }
                        .to_owned(),
                    )
                })
                .into_iter()
                .collect(),
            "description" => self.descriptions.iter().cloned().map(plain).collect(),
            "question" => self.questions.iter().cloned().map(plain).collect(),
            "layer" => self.layers.iter().cloned().map(plain).collect(),
            "step" => self.step.map(|s| plain(s.to_string())).into_iter().collect(),
            // An end, or a connector it draws, is incomplete.
            "incomplete" => {
                let incomplete = [self.source, self.target]
                    .iter()
                    .chain(&self.connectors)
                    .any(|id| ctx.model.incomplete.contains(id));
                vec![plain(incomplete.to_string())]
            }
            _ => Vec::new(),
        })
    }
}

fn plain(text: String) -> Item {
    Item::Value(Text { text, is_port: false })
}

fn link_port(registry: &Registry, link: usize) -> Option<(String, Option<String>)> {
    let port = registry.links[link].port.as_ref()?;
    let normalized = port.normalized();
    Some(match normalized.split_once('/') {
        Some((p, n)) => (p.to_owned(), Some(n.to_owned())),
        None => (normalized, None),
    })
}

/// Evaluates a view; warnings (edges dropped by `lift`) and `group-by` errors go to `diagnostics`.
pub fn evaluate(view: &View, registry: &Registry, model: &Model, diagnostics: &mut Vec<Diagnostic>) -> Graph {
    let failing = failing_of(view, registry);
    let down = crate::impact::down(registry, model, &failing.iter().copied().collect::<Vec<_>>());
    let ctx = Ctx { registry, model, down: &down };
    let options = RenderOptions::of(view);
    let mut graph = Graph {
        view: view.name.clone(),
        nodes: Vec::new(),
        groups: Vec::new(),
        node_group: HashMap::new(),
        edges: Vec::new(),
        options,
        has_edge_clause: !view.edges.is_empty(),
        processes: view
            .edges
            .iter()
            .filter_map(|s| match &s.selector {
                Selector::Process(name) => process_of(&ctx, name),
                _ => None,
            })
            .collect(),
        paths: view
            .edges
            .iter()
            .filter_map(|s| match &s.selector {
                Selector::Path(from, to) => Some((resource_of(&ctx, from)?, resource_of(&ctx, to)?)),
                _ => None,
            })
            .collect(),
        failing: failing
            .iter()
            .copied()
            .chain(crate::impact::failing_seeds(registry, &failing.iter().copied().collect::<Vec<_>>()))
            .collect(),
        down: down.clone(),
    };
    let Some(select) = &view.select else { return graph };

    // Nodes.
    let kinds: Vec<&str> = select.kinds.iter().flat_map(|k| ctx.registry.schema.expand(&k.text)).collect();
    graph.nodes = registry
        .ids()
        .filter(|&id| kinds.contains(&registry.resource(id).kind.as_str()))
        .filter(|&id| select.filter.as_ref().is_none_or(|f| ctx.matches(&Subject::Node(id), f)))
        .collect();
    let selected: HashSet<ResourceId> = graph.nodes.iter().copied().collect();

    // Edges.
    let grouping: Vec<&str> = select.group_by.iter().flat_map(|l| ctx.registry.schema.expand(&l.text)).collect();
    let mut added: HashSet<ResourceId> = HashSet::new();
    for (index, selector) in view.edges.iter().enumerate() {
        let inner = match &selector.selector {
            Selector::Lift(lift) => &lift.inner,
            _ => selector,
        };
        // The filter of the selector, or of the edges a lift starts from.
        if empty_filter(&ctx, inner) {
            let filter = inner.filter.as_ref().map(|f| filter_hint(registry, f)).unwrap_or_default();
            let mut d = Diagnostic::warning(
                format!("view {}: the where of this edge selector keeps no edge", view.name),
                inner.span,
            );
            if let Some(hint) = filter {
                d = d.with_help(hint);
            }
            diagnostics.push(d);
        }
        let (pre, merge) = match &selector.selector {
            Selector::Lift(lift) => (lift_edges(&ctx, view, lift, &selected, index, diagnostics), lift.merge),
            _ => {
                let mut pre = Vec::new();
                let base = base_edges(&ctx, selector);
                if let Selector::Path(from, to) = &selector.selector {
                    for problem in path_problems(&ctx, from, to) {
                        diagnostics.push(Diagnostic::warning(
                            format!("view {}: path({}, {}): {problem}", view.name, from.text, to.text),
                            from.span,
                        ));
                    }
                }
                if let Selector::Follow(follow) = &selector.selector
                    && let Some(from) = &follow.from
                    && base.is_empty()
                {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "view {}: follow({}, from: {}) reaches no link carrying {}",
                            view.name, follow.data.text, from.text, follow.data.text
                        ),
                        follow.span,
                    ));
                }
                for edge in base {
                    let addable =
                        |end: ResourceId| select.with_ends && !grouping.contains(&registry.resource(end).kind.as_str());
                    let ends = [edge.source, edge.target];
                    if ends.iter().all(|&end| selected.contains(&end) || addable(end)) {
                        added.extend(ends.iter().copied().filter(|end| !selected.contains(end)));
                        pre.push(pre_edge(&ctx, &edge, edge.source, edge.target, index));
                    } else if select.with_ends {
                        // `with ends` would add an end whose kind frames the view: it cannot be a node.
                        let end = ends.into_iter().find(|&end| !selected.contains(&end) && !addable(end));
                        if let Some(end) = end {
                            diagnostics.push(Diagnostic::warning(
                                format!(
                                    "view {}: edge {} dropped, {} is of kind {}, which frames the view (group-by)",
                                    view.name,
                                    provenance(&ctx, &edge),
                                    registry.qualified_name(end),
                                    registry.resource(end).kind
                                ),
                                view.span,
                            ));
                        }
                    }
                }
                (pre, Merge::Port)
            }
        };
        let pre = if view.through_show { split_through(pre, &selected) } else { pre };
        let mut edges = merge_edges(pre, merge);
        // The `where` after `lift(…)` filters the lifted edges, their ends folded; the one
        // inside filters the links before lifting.
        if let (Selector::Lift(_), Some(filter)) = (&selector.selector, &selector.filter) {
            edges.retain(|edge| ctx.matches(&Subject::Edge(edge), filter));
        }
        graph.edges.extend(edges);
    }

    // Final nodes: the selection, the added ends, without isolated nodes if asked.
    let connected: HashSet<ResourceId> = graph.edges.iter().flat_map(|e| [e.source, e.target]).collect();
    graph.nodes = registry
        .ids()
        .filter(|id| selected.contains(id) || added.contains(id))
        .filter(|id| !select.without_isolated || connected.contains(id))
        .collect();

    // Groups.
    if !select.group_by.is_empty() {
        let levels: Vec<&str> = select.group_by.iter().map(|l| l.text.as_str()).collect();
        let mut framer = Framer {
            ctx: &ctx,
            levels: &levels,
            groups: &mut graph.groups,
            frames: HashMap::new(),
            values: HashMap::new(),
            errors: BTreeSet::new(),
            opening: HashSet::new(),
        };
        let mut placed = Vec::new();
        for &node in &graph.nodes {
            if let Some(index) = framer.frame_of(node, levels.len()) {
                placed.push((node, index));
            }
        }
        let errors = std::mem::take(&mut framer.errors);
        graph.node_group.extend(placed);
        for error in errors {
            diagnostics.push(Diagnostic::error(format!("view {}: {error}", view.name), view.span));
        }
    }
    zones(&ctx, view, &mut graph, diagnostics);
    graph
}

/// Zones: top-level frames gathering their members, nodes or frames of the view, taken out of
/// their `group-by` frames; a frame comes with its content.
fn zones(ctx: &Ctx, view: &View, graph: &mut Graph, diagnostics: &mut Vec<Diagnostic>) {
    let registry = ctx.registry;
    // The nodes and frame resources of the view, as they are before the zones.
    let frames: Vec<ResourceId> = graph.groups.iter().filter_map(|g| g.resource).collect();
    let candidates: Vec<ResourceId> = graph.nodes.iter().copied().chain(frames.iter().copied()).collect();
    let mut taken: HashMap<ResourceId, &str> = HashMap::new();
    let mut zone_groups: HashMap<&str, usize> = HashMap::new();
    for zone in &view.zones {
        let members: Vec<ResourceId> = match &zone.members {
            ZoneMembers::Names(names) => names
                .iter()
                .filter_map(|name| {
                    // A frame first, as `stack` does.
                    let found =
                        frames.iter().chain(&graph.nodes).copied().find(|&id| designates(registry, &name.text, id));
                    if found.is_none() {
                        diagnostics.push(Diagnostic::warning(
                            format!("view {}: zone {}: {} is not in the view", view.name, zone.name.text, name.text),
                            name.span,
                        ));
                    }
                    found
                })
                .collect(),
            ZoneMembers::Filter(filter) => {
                let members: Vec<ResourceId> =
                    candidates.iter().copied().filter(|&id| ctx.matches(&Subject::Node(id), filter)).collect();
                if members.is_empty() {
                    let mut d = Diagnostic::warning(
                        format!("view {}: zone {}: its filter matches no node nor frame", view.name, zone.name.text),
                        zone.span,
                    );
                    if let Some(hint) = filter_hint(registry, filter) {
                        d = d.with_help(hint);
                    }
                    diagnostics.push(d);
                }
                members
            }
        };
        let index = graph.groups.len();
        graph.groups.push(Group {
            resource: None,
            value: None,
            zone: Some(ZoneFrame {
                name: zone.name.text.clone(),
                label: zone.label.clone().unwrap_or_else(|| zone.name.text.clone()),
                properties: zone.properties.clone(),
            }),
            parent: None,
        });
        zone_groups.insert(zone.name.text.as_str(), index);
        let frame_of = |groups: &[Group], id: ResourceId| groups.iter().position(|g| g.resource == Some(id));
        for &id in &members {
            if let Some(other) = taken.get(&id) {
                if *other != zone.name.text {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "view {}: {} is in the zones {other} and {}",
                            view.name,
                            registry.qualified_name(id),
                            zone.name.text
                        ),
                        zone.span,
                    ));
                }
                continue;
            }
            taken.insert(id, &zone.name.text);
        }
        // A member inside another member stays where it is: it comes with it.
        let inside_member = |groups: &[Group], start: Option<usize>| {
            std::iter::successors(start, |&g| groups[g].parent)
                .any(|g| groups[g].resource.is_some_and(|r| members.contains(&r)))
        };
        for &id in &members {
            if taken.get(&id) != Some(&zone.name.text.as_str()) {
                continue;
            }
            match frame_of(&graph.groups, id) {
                Some(group) => {
                    if !inside_member(&graph.groups, graph.groups[group].parent) {
                        graph.groups[group].parent = Some(index);
                    }
                }
                None => {
                    if !inside_member(&graph.groups, graph.node_group.get(&id).copied()) {
                        graph.node_group.insert(id, index);
                    }
                }
            }
        }
    }
    // Nested zones: each frame in the frame of its parent, unless the parents make a cycle
    // (reported by the validation).
    for zone in &view.zones {
        let mut chain = vec![zone.name.text.as_str()];
        let mut current = zone_parent(zone);
        while let Some(parent) = current.filter(|p| !chain.contains(p)) {
            chain.push(parent);
            current = view.zones.iter().find(|z| z.name.text == parent).and_then(zone_parent);
        }
        if current.is_none()
            && let (Some(parent), Some(&group)) = (zone_parent(zone), zone_groups.get(zone.name.text.as_str()))
        {
            graph.groups[group].parent = zone_groups.get(parent).copied();
        }
    }
}

/// Group of `id` at a level: ancestor of that kind, otherwise a single-valued reference. A
/// reference with several values (an app `on` two servers) gives no group at this level: the
/// resource goes into an outer level.
fn group_of(ctx: &Ctx, id: ResourceId, level: &str, errors: &mut BTreeSet<String>) -> Option<ResourceId> {
    let kinds = ctx.registry.schema.expand(level);
    let registry = ctx.registry;
    let name = || registry.qualified_name(id);
    if kinds.is_empty() {
        // A reference key: `owner`, `operator`…
        let targets: Vec<ResourceId> = match level {
            "owner" | "operator" => ctx.model.inherited(registry, id, level).into_iter().collect(),
            key => ctx.model.reference(Holder::Resource(id), key).to_vec(),
        };
        return match targets.as_slice() {
            [target] => Some(*target),
            _ => None,
        };
    }
    if let Some(ancestor) = registry.ancestors(id).find(|&a| kinds.contains(&registry.resource(a).kind.as_str())) {
        return Some(ancestor);
    }
    let mut found: Vec<(&str, &[ResourceId])> = Vec::new();
    for attribute in &registry.resource(id).attributes {
        let targets = ctx.model.reference(Holder::Resource(id), &attribute.key);
        if targets.iter().any(|&t| kinds.contains(&registry.resource(t).kind.as_str())) {
            found.push((attribute.key.as_str(), targets));
        }
    }
    match found.as_slice() {
        [] => hosted_group(ctx, id, level, errors).or_else(|| hosting_group(ctx, id, &kinds)),
        [(_, [target])] => Some(*target),
        [_] => None,
        _ => {
            errors.insert(format!("cannot group {} by {level}: several references lead to a {level}", name()));
            None
        }
    }
}

/// Group of what a resource runs on (`on`), when all its hosts share it: an app on a server
/// goes into the server's frame. Hosts in different groups leave the resource outside any frame.
/// A database may run on another one, which runs on a machine: the checks reject a cycle.
fn hosted_group(ctx: &Ctx, id: ResourceId, level: &str, errors: &mut BTreeSet<String>) -> Option<ResourceId> {
    let hosts = ctx.model.reference(Holder::Resource(id), "on");
    let mut groups = hosts.iter().map(|&host| group_of(ctx, host, level, errors));
    let first = groups.next()??;
    groups.all(|group| group == Some(first)).then_some(first)
}

/// Group of what a resource hosts (`hosts`, the inverse of `on`), when all of it shares one
/// group of these kinds by containment or reference: a server without `system:` goes into the
/// frame of the system of its apps. A server hosting several systems stays outside.
fn hosting_group(ctx: &Ctx, id: ResourceId, kinds: &[&str]) -> Option<ResourceId> {
    let registry = ctx.registry;
    let direct = |hosted: ResourceId| {
        let of_kind = |r: ResourceId| kinds.contains(&registry.resource(r).kind.as_str());
        registry.ancestors(hosted).find(|&a| of_kind(a)).or_else(|| {
            let targets: Vec<ResourceId> = registry
                .resource(hosted)
                .attributes
                .iter()
                .flat_map(|a| ctx.model.reference(Holder::Resource(hosted), &a.key).iter().copied())
                .filter(|&t| of_kind(t))
                .collect();
            match targets.as_slice() {
                [target] => Some(*target),
                _ => None,
            }
        })
    };
    let mut groups = ctx.model.hosts.get(&id)?.iter().map(|&hosted| direct(hosted));
    let first = groups.next()??;
    groups.all(|group| group == Some(first)).then_some(first)
}

/// Whether a level of `group-by` groups by value: `component`, or a key that is neither a kind
/// nor a reference key (a free key: `middleware`).
fn value_level(schema: &schema::Schema, level: &str) -> bool {
    level == "component" || (schema.expand(level).is_empty() && !reference_keys().iter().any(|k| k == level))
}

/// The value of a resource at a value level: the name and version of its component (the label
/// when it has one), or the value of the key; none when there are several.
fn value_of(ctx: &Ctx, id: ResourceId, level: &str) -> Option<String> {
    let registry = ctx.registry;
    let values: BTreeSet<String> = if level == "component" {
        registry
            .resource(id)
            .children
            .iter()
            .map(|&c| registry.resource(c))
            .filter(|c| c.kind == "component")
            .map(|c| match (c.attribute("label"), c.attribute("version")) {
                (Some(label), _) => query::value_text(&label.value),
                (None, Some(version)) => format!("{} {}", c.name, query::value_text(&version.value)),
                (None, None) => c.name.clone(),
            })
            .collect()
    } else {
        registry
            .resource(id)
            .attribute(level)
            .map(|a| a.value.items().into_iter().map(query::value_text).collect())
            .unwrap_or_default()
    };
    if values.len() == 1 { values.into_iter().next() } else { None }
}

/// The frames of `group-by`: one per resource of a level, or per value of a value level within
/// its parent frame; each created with its parent frames along the outer levels.
struct Framer<'a, 'c> {
    ctx: &'a Ctx<'c>,
    levels: &'a [&'a str],
    groups: &'a mut Vec<Group>,
    frames: HashMap<ResourceId, usize>,
    values: HashMap<(Option<usize>, usize, String), usize>,
    errors: BTreeSet<String>,
    /// The frames of `group-by on` being opened, against a cycle of `on`.
    opening: HashSet<ResourceId>,
}

impl Framer<'_, '_> {
    /// The innermost frame of a resource among the first `depth` levels.
    fn frame_of(&mut self, id: ResourceId, depth: usize) -> Option<usize> {
        for level in (0..depth).rev() {
            let name = self.levels[level];
            if value_level(&self.ctx.registry.schema, name) {
                if let Some(value) = value_of(self.ctx, id, name) {
                    let parent = self.frame_of(id, level);
                    return Some(self.value_frame(parent, level, value));
                }
            } else if let Some(group) = group_of(self.ctx, id, name, &mut self.errors) {
                return Some(self.resource_frame(group, level));
            }
        }
        None
    }

    fn resource_frame(&mut self, group: ResourceId, level: usize) -> usize {
        if let Some(&index) = self.frames.get(&group) {
            return index;
        }
        // `group-by on`: a host that runs on another one (a database in a shared database) goes
        // into the frame of its own host.
        let host = (self.levels[level] == "on" && self.opening.insert(group))
            .then(|| group_of(self.ctx, group, "on", &mut self.errors))
            .flatten()
            .filter(|&host| host != group);
        let parent = match host {
            Some(host) => Some(self.resource_frame(host, level)),
            None => self.frame_of(group, level),
        };
        if let Some(&index) = self.frames.get(&group) {
            return index;
        }
        self.groups.push(Group { resource: Some(group), zone: None, value: None, parent });
        self.frames.insert(group, self.groups.len() - 1);
        self.groups.len() - 1
    }

    fn value_frame(&mut self, parent: Option<usize>, level: usize, value: String) -> usize {
        let key = (parent, level, value.clone());
        if let Some(&index) = self.values.get(&key) {
            return index;
        }
        self.groups.push(Group { resource: None, zone: None, value: Some(value), parent });
        self.values.insert(key, self.groups.len() - 1);
        self.groups.len() - 1
    }
}

/// Edges of a non-lift selector, filtered by its `where`, before checking the selection.
fn base_edges(ctx: &Ctx, selector: &EdgeSelector) -> Vec<BaseEdge> {
    let registry = ctx.registry;
    let mut edges = Vec::new();
    match &selector.selector {
        Selector::Layers(layers) => {
            for (index, link) in registry.links.iter().enumerate() {
                let Some((source, target)) = ctx.model.link_ends.get(index).copied().flatten() else { continue };
                if link.layers.iter().any(|(l, _)| layers.iter().any(|n| &n.text == l)) {
                    edges.push(BaseEdge { source, target, origin: Origin::Link(index), step: None });
                }
            }
        }
        Selector::Relation(name) if name.text == "parent" => {
            for id in registry.ids() {
                if let Some(parent) = registry.resource(id).parent {
                    edges.push(BaseEdge { source: parent, target: id, origin: Origin::Parent, step: None });
                }
            }
        }
        Selector::Relation(name) => {
            let connectors: Vec<&str> = ctx
                .registry
                .schema
                .expand(&name.text)
                .into_iter()
                .filter(|k| ctx.registry.schema.kind_def(k).is_some_and(|d| d.connector.is_some()))
                .collect();
            if !connectors.is_empty() {
                for id in registry.ids() {
                    let resource = registry.resource(id);
                    if !connectors.contains(&resource.kind.as_str()) {
                        continue;
                    }
                    let attr = ctx.registry.schema.kind_def(&resource.kind).and_then(|d| d.connector).unwrap_or("ends");
                    if let [a, b] = ctx.model.reference(Holder::Resource(id), attr) {
                        edges.push(BaseEdge { source: *a, target: *b, origin: Origin::Connector(id), step: None });
                    }
                }
            } else {
                for id in registry.ids() {
                    for &target in ctx.model.reference(Holder::Resource(id), &name.text) {
                        edges.push(BaseEdge {
                            source: id,
                            target,
                            origin: Origin::Reference(name.text.clone()),
                            step: None,
                        });
                    }
                }
            }
        }
        Selector::Lift(_) => {}
        Selector::Path(from, to) => edges.extend(path_edges(ctx, from, to)),
        Selector::Follow(follow) => return follow_edges(ctx, follow, selector.filter.as_ref()),
        Selector::Process(name) => {
            // The links of its steps, numbered in their order (a link once, at its first step; the
            // links of a step to a participant standing for several resources, one per member,
            // with the same number), then those serving it (`processes: [orders]`), without a step.
            let Some(process) = process_of(ctx, name) else { return Vec::new() };
            let mut seen = HashSet::new();
            let steps = ctx.model.processes.get(&process).map(Vec::as_slice).unwrap_or_default();
            for (number, step) in steps.iter().filter_map(crate::process::Event::step).enumerate() {
                for &index in &step.links {
                    let Some((source, target)) = ctx.model.link_ends.get(index).copied().flatten() else { continue };
                    if seen.insert(index) {
                        edges.push(BaseEdge { source, target, origin: Origin::Link(index), step: Some(number + 1) });
                    }
                }
            }
            for index in 0..registry.links.len() {
                let Some((source, target)) = ctx.model.link_ends.get(index).copied().flatten() else { continue };
                if ctx.model.reference(Holder::Link(index), "processes").contains(&process) && seen.insert(index) {
                    edges.push(BaseEdge { source, target, origin: Origin::Link(index), step: None });
                }
            }
        }
    }
    match &selector.filter {
        Some(filter) => edges
            .into_iter()
            .filter(|e| ctx.matches(&Subject::Edge(&BaseEdgeView { edge: e, registry }), filter))
            .collect(),
        None => edges,
    }
}

/// `follow`: the links carrying a data object, directly (`carries`) or through one of their channels.
/// With `from`, only the links reached from that resource walking in the direction of the data
/// (`exchange`, the connection direction without it), each with its step: the distance from `from`.
/// The `where` filter applies to the links before the walk.
fn follow_edges(ctx: &Ctx, follow: &Follow, filter: Option<&Predicate>) -> Vec<BaseEdge> {
    let registry = ctx.registry;
    let resolver = Resolver::new(registry);
    let resolve = |name: &Name, expected: &[&str]| match resolver.resolve(&reference(name), None, expected) {
        Resolution::Resolved(id) => Some(id),
        _ => None,
    };
    let Some(data) = resolve(&follow.data, &["data"]) else { return Vec::new() };
    let carries = |index: usize| {
        let link = Holder::Link(index);
        ctx.model.reference(link, "carries").contains(&data)
            || ctx
                .model
                .reference(link, "channel")
                .iter()
                .any(|&channel| ctx.model.reference(Holder::Resource(channel), "carries").contains(&data))
    };

    // Hops in the direction of the data: (link edge, data source, data target), each with the
    // channels of its link carrying the data.
    let mut hops: Vec<(BaseEdge, ResourceId, ResourceId)> = Vec::new();
    let channels_of = |index: usize| -> Vec<ResourceId> {
        ctx.model
            .reference(Holder::Link(index), "channel")
            .iter()
            .copied()
            .filter(|&c| ctx.model.reference(Holder::Resource(c), "carries").contains(&data))
            .collect()
    };
    for (index, link) in registry.links.iter().enumerate() {
        let Some((source, target)) = ctx.model.link_ends.get(index).copied().flatten() else { continue };
        if !carries(index) {
            continue;
        }
        let edge = BaseEdge { source, target, origin: Origin::Link(index), step: None };
        if filter.is_some_and(|f| !ctx.matches(&Subject::Edge(&BaseEdgeView { edge: &edge, registry }), f)) {
            continue;
        }
        match link.attribute("exchange").map(|a| a.value.raw.as_str()) {
            Some("pull") => hops.push((edge, target, source)),
            Some("both") => {
                hops.push((edge.clone(), source, target));
                hops.push((edge, target, source));
            }
            _ => hops.push((edge, source, target)),
        }
    }

    // A resource of a `relay` kind passes the data on along its links that do not declare it,
    // in the direction of `exchange`, or from the relay towards the other end without one.
    // A name of `relay` or `relay_to`: a kind or a group, otherwise a resource.
    let matcher = |names: &[Name]| {
        let kinds: Vec<&str> = names.iter().flat_map(|n| ctx.registry.schema.expand(&n.text)).collect();
        let ids: Vec<ResourceId> = names
            .iter()
            .filter(|n| ctx.registry.schema.expand(&n.text).is_empty())
            .filter_map(|n| resolve(n, &[]))
            .collect();
        move |id: ResourceId| kinds.contains(&registry.resource(id).kind.as_str()) || ids.contains(&id)
    };
    if !follow.relay.is_empty() {
        let relay = matcher(&follow.relay);
        let leads_to = matcher(&follow.relay_to);
        let any_target = follow.relay_to.is_empty();
        for (index, link) in registry.links.iter().enumerate() {
            let Some((source, target)) = ctx.model.link_ends.get(index).copied().flatten() else { continue };
            if carries(index) || !(relay(source) || relay(target)) {
                continue;
            }
            let edge = BaseEdge { source, target, origin: Origin::Link(index), step: None };
            if filter.is_some_and(|f| !ctx.matches(&Subject::Edge(&BaseEdgeView { edge: &edge, registry }), f)) {
                continue;
            }
            let directions = match link.attribute("exchange").map(|a| a.value.raw.as_str()) {
                Some("pull") => vec![(target, source)],
                Some("push") => vec![(source, target)],
                _ => vec![(source, target), (target, source)],
            };
            for (from, to) in directions.into_iter().filter(|&(from, to)| relay(from) && (any_target || leads_to(to))) {
                hops.push((edge.clone(), from, to));
            }
        }
    }

    let Some(start) = follow.from.as_ref().and_then(|from| resolve(from, &ctx.registry.schema.expand("flow_endpoint")))
    else {
        if follow.from.is_some() {
            return Vec::new();
        }
        let mut seen = HashSet::new();
        let out: Vec<BaseEdge> = hops
            .iter()
            .map(|(edge, _, _)| edge.clone())
            .filter(|edge| matches!(edge.origin, Origin::Link(i) if seen.insert(i)))
            .collect();
        let on_the_way: HashSet<ResourceId> = hops.iter().flat_map(|(_, from, to)| [*from, *to]).collect();
        return with_side(ctx, follow, filter, out, &on_the_way);
    };

    // Breadth-first walk: a link gets the step following the depth of the end the data leaves from.
    // The data reaching, through a channel, the resource holding it (a topic of a broker) leaves it
    // through that channel only: a broker is crossed once per channel, in the order of the data.
    let holder = |channel: ResourceId| registry.resource(channel).parent;
    let mut depth: HashMap<(ResourceId, Option<ResourceId>), usize> = HashMap::from([((start, None), 0)]);
    let mut queue = VecDeque::from([(start, None)]);
    let mut reached = HashSet::new();
    let mut out = Vec::new();
    while let Some(state) = queue.pop_front() {
        let (node, channel) = state;
        let d = depth[&state];
        for (edge, from, to) in &hops {
            let Origin::Link(index) = &edge.origin else { continue };
            if *from != node {
                continue;
            }
            let channels = channels_of(*index);
            if channel.is_some_and(|c| !channels.contains(&c)) {
                continue;
            }
            if reached.insert(*index) {
                out.push(BaseEdge { step: Some(d + 1), ..edge.clone() });
            }
            let arrivals: Vec<(ResourceId, Option<ResourceId>)> =
                match channels.iter().filter(|&&c| holder(c) == Some(*to)).copied().collect::<Vec<_>>() {
                    held if !held.is_empty() => held.into_iter().map(|c| (*to, Some(c))).collect(),
                    _ => vec![(*to, None)],
                };
            for arrival in arrivals {
                if let std::collections::hash_map::Entry::Vacant(entry) = depth.entry(arrival) {
                    entry.insert(d + 1);
                    queue.push_back(arrival);
                }
            }
        }
    }
    let on_the_way: HashSet<ResourceId> = depth.keys().map(|(node, _)| *node).collect();
    with_side(ctx, follow, filter, out, &on_the_way)
}

/// `side: [sites]`: the links leaving the resources the data goes through and carrying these
/// data (directly or through a channel), added without a step: the calls serving the processing.
fn with_side(
    ctx: &Ctx,
    follow: &Follow,
    filter: Option<&Predicate>,
    mut out: Vec<BaseEdge>,
    on_the_way: &HashSet<ResourceId>,
) -> Vec<BaseEdge> {
    if follow.side.is_empty() {
        return out;
    }
    let registry = ctx.registry;
    let resolver = Resolver::new(registry);
    let side: Vec<ResourceId> = follow
        .side
        .iter()
        .filter_map(|name| match resolver.resolve(&reference(name), None, &["data"]) {
            Resolution::Resolved(id) => Some(id),
            _ => None,
        })
        .collect();
    // `side: all`: whatever the links carry.
    let all = follow.side.iter().any(|n| n.text == "all");
    let carries_side = |index: usize| {
        if all {
            return true;
        }
        let link = Holder::Link(index);
        let channels = ctx.model.reference(link, "channel");
        ctx.model
            .reference(link, "carries")
            .iter()
            .chain(channels.iter().flat_map(|&c| ctx.model.reference(Holder::Resource(c), "carries")))
            .any(|d| side.contains(d))
    };
    let drawn: HashSet<usize> =
        out.iter().filter_map(|e| if let Origin::Link(i) = e.origin { Some(i) } else { None }).collect();
    for index in 0..registry.links.len() {
        let Some((source, target)) = ctx.model.link_ends.get(index).copied().flatten() else { continue };
        if drawn.contains(&index) || !on_the_way.contains(&source) || !carries_side(index) {
            continue;
        }
        let edge = BaseEdge { source, target, origin: Origin::Link(index), step: None };
        if filter.is_some_and(|f| !ctx.matches(&Subject::Edge(&BaseEdgeView { edge: &edge, registry }), f)) {
            continue;
        }
        out.push(edge);
    }
    out
}

fn lift_edges(
    ctx: &Ctx,
    view: &View,
    lift: &Lift,
    selected: &HashSet<ResourceId>,
    index: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<GraphEdge> {
    let mut out = Vec::new();
    for edge in base_edges(ctx, &lift.inner) {
        let ends = [edge.source, edge.target].map(|end| {
            if selected.contains(&end) {
                return vec![end];
            }
            // The path for the kind of the end (`via_job`), otherwise `via`; a resource reached
            // that is not selected is lifted in turn: a database on a shared database, lifted
            // `via: on`, reaches the server of the latter.
            let mut found = Vec::new();
            let mut seen = HashSet::from([end]);
            let mut todo = vec![end];
            while let Some(current) = todo.pop() {
                let kind = ctx.registry.resource(current).kind.as_str();
                let specific = lift.via_kinds.iter().find(|(k, _)| ctx.registry.schema.expand(&k.text).contains(&kind));
                let Some(via) = specific.map(|(_, path)| path).or(lift.via.as_ref()) else { continue };
                for item in ctx.path(&Subject::Node(current), via) {
                    let Item::Resource(r) = item else { continue };
                    if selected.contains(&r) {
                        if !found.contains(&r) {
                            found.push(r);
                        }
                    } else if seen.len() < 16 && seen.insert(r) {
                        todo.push(r);
                    }
                }
            }
            found
        });
        if let Some(missing) = [0, 1].into_iter().find(|&i| ends[i].is_empty()) {
            let end = if missing == 0 { edge.source } else { edge.target };
            diagnostics.push(Diagnostic::warning(
                format!(
                    "view {}: edge {} dropped, {} is neither selected nor liftable via {}",
                    view.name,
                    provenance(ctx, &edge),
                    ctx.registry.qualified_name(end),
                    lift.via.as_ref().map_or_else(String::new, Path::text)
                ),
                view.span,
            ));
            continue;
        }
        for &source in &ends[0] {
            for &target in &ends[1] {
                if source == target && lift.loops == Loops::Drop {
                    continue;
                }
                out.push(pre_edge(ctx, &edge, source, target, index));
            }
        }
    }
    out
}

fn provenance(ctx: &Ctx, edge: &BaseEdge) -> String {
    let name = |id| ctx.registry.qualified_name(id);
    match &edge.origin {
        Origin::Link(i) => {
            let port =
                ctx.registry.links[*i].port.as_ref().map(|p| format!(" : {}", p.normalized())).unwrap_or_default();
            format!("{} -> {}{port}", name(edge.source), name(edge.target))
        }
        Origin::Reference(key) => format!("{} {key} {}", name(edge.source), name(edge.target)),
        Origin::Connector(r) => name(*r),
        Origin::Parent => format!("{} contains {}", name(edge.source), name(edge.target)),
        Origin::Cable(a, b) => format!("{} peer {}", name(*a), name(*b)),
    }
}

fn pre_edge(ctx: &Ctx, edge: &BaseEdge, source: ResourceId, target: ResourceId, selector: usize) -> GraphEdge {
    let registry = ctx.registry;
    let mut out = GraphEdge {
        source,
        target,
        symmetric: false,
        relation: None,
        ports: BTreeSet::new(),
        layers: BTreeSet::new(),
        carries: BTreeSet::new(),
        through: BTreeSet::new(),
        via: BTreeSet::new(),
        exchange: None,
        confidence: Confidence::Confirmed,
        questions: Vec::new(),
        descriptions: Vec::new(),
        provenance: vec![provenance(ctx, edge)],
        links: Vec::new(),
        connectors: Vec::new(),
        declared: Vec::new(),
        through_path: Vec::new(),
        through_steps: Vec::new(),
        through_origins: BTreeSet::new(),
        step: edge.step,
        selector,
    };
    let text_of = |attrs: &[crate::registry::Attribute], key: &str| -> Vec<String> {
        attrs
            .iter()
            .filter(|a| a.key == key)
            .flat_map(|a| a.value.items().into_iter().map(query::value_text).collect::<Vec<_>>())
            .collect()
    };
    let confidence_of =
        |attrs: &[crate::registry::Attribute]| match text_of(attrs, "confidence").first().map(String::as_str) {
            Some("assumed") => Confidence::Assumed,
            Some("doubtful") => Confidence::Doubtful,
            _ => Confidence::Confirmed,
        };
    match &edge.origin {
        Origin::Link(i) => {
            out.links.push(*i);
            let link = &registry.links[*i];
            if let Some(port) = &link.port {
                out.ports.insert(port.normalized());
            }
            out.layers = link.layers.iter().map(|(l, _)| l.clone()).collect();
            out.carries = ctx.model.reference(Holder::Link(*i), "carries").iter().copied().collect();
            out.through_path = ctx.model.reference(Holder::Link(*i), "through").to_vec();
            out.through = out.through_path.iter().copied().collect();
            out.through_origins = ctx.model.deduced_through.get(i).into_iter().flatten().cloned().collect();
            out.through_steps = ctx.model.through_steps.get(i).cloned().unwrap_or_default();
            out.exchange = match text_of(&link.attributes, "exchange").first().map(String::as_str) {
                Some("push") => Some(Exchange::Push),
                Some("pull") => Some(Exchange::Pull),
                Some("both") => Some(Exchange::Both),
                _ => None,
            };
            out.confidence = confidence_of(&link.attributes);
            out.questions = text_of(&link.attributes, "question");
            out.descriptions = text_of(&link.attributes, "description");
        }
        Origin::Reference(key) => {
            out.declared.push((edge.source, edge.target));
            out.relation = Some(key.clone());
            out.symmetric = key == "peer";
        }
        Origin::Connector(r) => {
            out.connectors.push(*r);
            let resource = registry.resource(*r);
            out.relation = Some(resource.kind.clone());
            out.symmetric = true;
            out.confidence = confidence_of(&resource.attributes);
            out.questions = text_of(&resource.attributes, "question");
            out.descriptions = text_of(&resource.attributes, "description");
        }
        Origin::Parent => out.relation = Some("parent".to_owned()),
        Origin::Cable(a, b) => {
            out.declared.push((*a, *b));
            out.relation = Some("peer".to_owned());
            out.symmetric = true;
        }
    }
    out
}

/// `through: show`: splits flows on the selected devices of their path.
fn split_through(edges: Vec<GraphEdge>, selected: &HashSet<ResourceId>) -> Vec<GraphEdge> {
    let mut out = Vec::new();
    for edge in edges {
        let devices: Vec<ResourceId> = edge.through_path.iter().copied().filter(|t| selected.contains(t)).collect();
        if devices.is_empty() {
            out.push(edge);
            continue;
        }
        let mut chain = vec![edge.source];
        chain.extend(&devices);
        chain.push(edge.target);
        // Items of the path that are not nodes (tunnels…) annotate the segment they fall in.
        let mut via: Vec<BTreeSet<ResourceId>> = vec![BTreeSet::new(); chain.len() - 1];
        let last = via.len() - 1;
        let mut segment = 0;
        for item in &edge.through_path {
            if selected.contains(item) {
                segment += 1;
            } else {
                via[segment.min(last)].insert(*item);
            }
        }
        for (i, pair) in chain.windows(2).enumerate() {
            if pair[0] == pair[1] {
                continue;
            }
            let mut piece = edge.clone();
            piece.source = pair[0];
            piece.target = pair[1];
            piece.via = via[i].clone();
            out.push(piece);
        }
    }
    out
}

fn merge_edges(edges: Vec<GraphEdge>, merge: Merge) -> Vec<GraphEdge> {
    let mut out: Vec<GraphEdge> = Vec::new();
    let mut index: HashMap<(ResourceId, ResourceId, Option<String>, String), usize> = HashMap::new();
    for (n, mut edge) in edges.into_iter().enumerate() {
        if edge.symmetric && edge.target < edge.source {
            std::mem::swap(&mut edge.source, &mut edge.target);
        }
        let (a, b) = (edge.source, edge.target);
        let port = match merge {
            Merge::Endpoints => String::new(),
            Merge::Port => edge.ports.iter().cloned().collect::<Vec<_>>().join(","),
            Merge::None => format!("#{n}"),
        };
        // The steps of a `follow` stay apart: two links between the same ends at two steps (a
        // pull on a topic, a push on another one) are two edges.
        let step = edge.step.map(|s| format!("|{s}")).unwrap_or_default();
        let key = (a, b, edge.relation.clone(), format!("{}|{port}{step}", edge.selector));
        match index.get(&key) {
            Some(&i) => {
                let target = &mut out[i];
                target.ports.extend(edge.ports);
                target.layers.extend(edge.layers);
                target.carries.extend(edge.carries);
                target.through.extend(edge.through);
                target.through_origins.extend(edge.through_origins);
                target.via.extend(edge.via);
                target.exchange = Exchange::combine(target.exchange, edge.exchange);
                target.confidence = target.confidence.max(edge.confidence);
                target.step = match (target.step, edge.step) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                for q in edge.questions {
                    if !target.questions.contains(&q) {
                        target.questions.push(q);
                    }
                }
                for d in edge.descriptions {
                    if !target.descriptions.contains(&d) {
                        target.descriptions.push(d);
                    }
                }
                for p in edge.provenance {
                    if !target.provenance.contains(&p) {
                        target.provenance.push(p);
                    }
                }
                for l in edge.links {
                    if !target.links.contains(&l) {
                        target.links.push(l);
                    }
                }
                for c in edge.connectors {
                    if !target.connectors.contains(&c) {
                        target.connectors.push(c);
                    }
                }
                for d in edge.declared {
                    if !target.declared.contains(&d) {
                        target.declared.push(d);
                    }
                }
            }
            None => {
                index.insert(key, out.len());
                out.push(edge);
            }
        }
    }
    out
}

/// Text of a value kind, used by renderers for node labels.
pub fn attribute_text(ctx: &Ctx, id: ResourceId, key: &str) -> Vec<String> {
    // A cron schedule in plain words.
    if key == "schedule"
        && let Some(plain) = crate::schedule::plain(ctx.registry, ctx.model, id)
    {
        return vec![plain];
    }
    ctx.step(id, &StepKind::Name(key.to_owned()))
        .into_iter()
        .map(|i| match i {
            Item::Resource(r) => ctx.registry.resource(r).name.clone(),
            Item::Value(t) => t.text,
        })
        .collect()
}
