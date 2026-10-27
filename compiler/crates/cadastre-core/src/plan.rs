//! Plans (`.cav`): views and styles, lowered from the syntax tree (spec § 6).

use tree_sitter::Node;

use crate::diagnostic::Diagnostic;
use crate::lower::{attribute_at, value_at};
use crate::registry::{Attribute, Value, ValueKind};
use crate::source::{FileId, Span};

#[derive(Clone, Debug, Default)]
pub struct Plans {
    pub views: Vec<View>,
    pub checks: Vec<Check>,
    /// Root-level `style` blocks (only one is allowed in a project).
    pub project_styles: Vec<StyleBlock>,
}

#[derive(Clone, Debug)]
pub struct View {
    pub name: String,
    pub span: Span,
    pub select: Option<Select>,
    pub edges: Vec<EdgeSelector>,
    pub through_show: bool,
    /// Unknown `edges` options, reported by the validation.
    pub edge_options: Vec<Attribute>,
    pub style: Vec<StyleRule>,
    pub render: Option<Render>,
    pub zones: Vec<Zone>,
    /// `title` and `description`, for the readers of the view; unknown keys are reported.
    pub attributes: Vec<Attribute>,
}

impl View {
    /// The title of the view for its readers: `title`, otherwise its name.
    pub fn title(&self) -> String {
        self.attribute("title").unwrap_or_else(|| self.name.clone())
    }

    pub fn description(&self) -> Option<String> {
        self.attribute("description")
    }

    fn attribute(&self, key: &str) -> Option<String> {
        self.attributes.iter().find(|a| a.key == key).map(|a| crate::query::value_text(&a.value))
    }
}

/// `check <name> { select … edges … expect … }`: a rule on the model, run by `cadastre check`.
#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    /// Span of the name.
    pub span: Span,
    /// The selection, evaluated as a view; without a `select` clause, every kind is selected.
    pub view: View,
    pub expect: Option<Expect>,
    /// `message` and `severity`, and unknown keys reported by the validation.
    pub attributes: Vec<Attribute>,
}

impl Check {
    /// `message: "…"`: why the rule exists, shown when it fails.
    pub fn message(&self) -> Option<String> {
        self.attributes.iter().find(|a| a.key == "message").map(|a| crate::query::value_text(&a.value))
    }

    /// `severity: warning` makes a failure a warning; it is an error otherwise.
    pub fn is_warning(&self) -> bool {
        self.severity() == crate::diagnostic::Severity::Warning
    }

    /// The severity of a failure: `severity: warning` or `info`, an error otherwise.
    pub fn severity(&self) -> crate::diagnostic::Severity {
        use crate::diagnostic::Severity;
        match self.attributes.iter().find(|a| a.key == "severity").map(|a| a.value.raw.as_str()) {
            Some("warning") => Severity::Warning,
            Some("info") => Severity::Info,
            _ => Severity::Error,
        }
    }
}

/// The expected result of a check, on the selected edges when it has an `edges` clause, on the
/// selected nodes otherwise.
#[derive(Clone, Debug)]
pub enum Expect {
    None,
    Some,
    Count(Op, usize),
    /// Every item satisfies the predicate.
    All(Box<Predicate>),
}

/// `zone <name> ["<label>"] [members] | where … [{ properties }]`: a named frame of the view,
/// gathering its members whatever their `group-by` frames.
#[derive(Clone, Debug)]
pub struct Zone {
    pub name: Name,
    pub label: Option<String>,
    pub members: ZoneMembers,
    /// `color`, `fill`, `line` of the frame.
    pub properties: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ZoneMembers {
    /// Names of nodes or frames of the view.
    Names(Vec<Name>),
    /// The nodes and frames of the view matching a filter.
    Filter(Predicate),
}

#[derive(Clone, Debug)]
pub struct Select {
    pub kinds: Vec<Name>,
    pub filter: Option<Predicate>,
    /// `with ends`: the ends of the edges of simple selectors are added to the nodes.
    pub with_ends: bool,
    /// `without isolated`: nodes without any edge are removed.
    pub without_isolated: bool,
    pub group_by: Vec<Name>,
    pub span: Span,
}

/// An identifier written in a plan, with its position.
#[derive(Clone, Debug)]
pub struct Name {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EdgeSelector {
    pub selector: Selector,
    pub filter: Option<Predicate>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Selector {
    /// `[flow, auth]`
    Layers(Vec<Name>),
    /// A relation (`on`, `peer`, `parent`…) or a connector kind or group.
    Relation(Name),
    Lift(Box<Lift>),
    /// `follow(item-prices, from: pricing.ui)`
    Follow(Follow),
    /// `process(orders)`: the links of the steps of a process, and those serving it.
    Process(Name),
    /// `path(proxy, esx-02)`: the network path between two resources, deduced from the cabling.
    Path(Name, Name),
}

/// The links carrying a data object, walked in the direction of the data from `from`.
#[derive(Clone, Debug)]
pub struct Follow {
    pub data: Name,
    pub from: Option<Name>,
    /// `relay: [database]`, `relay: [stock.db]`: kinds, or resources, passing the data on along
    /// links that do not declare it.
    pub relay: Vec<Name>,
    /// `relay_to: [app]`: the kinds or resources the relayed links may lead to; any when empty.
    pub relay_to: Vec<Name>,
    /// `side: [sites]`: data whose links leaving the resources the data goes through are drawn
    /// too, without a step: the calls serving the processing.
    pub side: Vec<Name>,
    /// Options other than `from`, reported by the validation.
    pub invalid_options: Vec<(String, Span)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Lift {
    pub inner: EdgeSelector,
    pub via: Option<Path>,
    /// `via_job: on`: the path for the ends of a kind or a group, before `via`.
    pub via_kinds: Vec<(Name, Path)>,
    pub loops: Loops,
    pub merge: Merge,
    /// Options other than `via`, `loops`, `merge`, or with an invalid value.
    pub invalid_options: Vec<(String, Span)>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Loops {
    #[default]
    Drop,
    Keep,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Merge {
    #[default]
    Endpoints,
    Port,
    None,
}

#[derive(Clone, Debug)]
pub struct Path {
    pub steps: Vec<Step>,
    pub span: Span,
}

impl Path {
    /// `on?.ancestor(site)` as written.
    pub fn text(&self) -> String {
        self.steps
            .iter()
            .map(|s| {
                let base = match &s.kind {
                    StepKind::Name(n) => n.clone(),
                    StepKind::Ancestor(k) => format!("ancestor({k})"),
                };
                if s.optional { format!("{base}?") } else { base }
            })
            .collect::<Vec<_>>()
            .join(".")
    }
}

#[derive(Clone, Debug)]
pub struct Step {
    pub kind: StepKind,
    pub optional: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepKind {
    /// A reference key, an attribute, `parent`, or a pseudo-attribute.
    Name(String),
    /// `ancestor(K)`
    Ancestor(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
pub enum Predicate {
    Flag(Path),
    Compare(Path, Op, Value),
    /// Two paths of the ends of an edge: `source.ancestor(system) != target.ancestor(system)`.
    ComparePaths(Path, Op, Path),
    /// An aggregate of the items of a path compared to a value: `count(on.ancestor(site)) >= 2`.
    Aggregate(Box<Aggregate>),
    Has(Path, Value),
    In(Path, Vec<Value>),
    Exists(Path),
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
}

impl Predicate {
    /// Every path of the predicate, for validation.
    pub fn paths(&self) -> Vec<&Path> {
        match self {
            Self::Flag(p) | Self::Compare(p, _, _) | Self::Has(p, _) | Self::In(p, _) | Self::Exists(p) => vec![p],
            Self::ComparePaths(a, _, b) => vec![a, b],
            Self::Aggregate(a) => vec![&a.path],
            Self::And(a, b) | Self::Or(a, b) => a.paths().into_iter().chain(b.paths()).collect(),
            Self::Not(a) => a.paths(),
        }
    }

    /// The aggregates of the predicate, with the value they are compared to.
    pub fn aggregates(&self) -> Vec<(&Name, &Value)> {
        match self {
            Self::Aggregate(a) => vec![(&a.function, &a.value)],
            Self::And(a, b) | Self::Or(a, b) => a.aggregates().into_iter().chain(b.aggregates()).collect(),
            Self::Not(a) => a.aggregates(),
            _ => Vec::new(),
        }
    }

    /// The order comparisons (`<`, `<=`, `>`, `>=`) of a path with a value.
    pub fn ordered(&self) -> Vec<(&Path, &Value)> {
        match self {
            Self::Compare(p, op, v) if !matches!(op, Op::Eq | Op::Ne) => vec![(p, v)],
            Self::And(a, b) | Self::Or(a, b) => a.ordered().into_iter().chain(b.ordered()).collect(),
            Self::Not(a) => a.ordered(),
            _ => Vec::new(),
        }
    }

    /// The comparisons between two paths, as (left, right).
    pub fn path_comparisons(&self) -> Vec<(&Path, &Path)> {
        match self {
            Self::ComparePaths(a, _, b) => vec![(a, b)],
            Self::And(a, b) | Self::Or(a, b) => a.path_comparisons().into_iter().chain(b.path_comparisons()).collect(),
            Self::Not(a) => a.path_comparisons(),
            _ => Vec::new(),
        }
    }

    /// Every value compared to a path (`=`, `!=`, `has`, `in`), with that path.
    pub fn compared(&self) -> Vec<(&Path, &Value)> {
        match self {
            Self::Compare(p, _, v) | Self::Has(p, v) => vec![(p, v)],
            Self::In(p, values) => values.iter().map(|v| (p, v)).collect(),
            Self::Flag(_) | Self::Exists(_) | Self::ComparePaths(..) | Self::Aggregate(..) => Vec::new(),
            Self::And(a, b) | Self::Or(a, b) => a.compared().into_iter().chain(b.compared()).collect(),
            Self::Not(a) => a.compared(),
        }
    }
}

/// `count(path) >= 2`.
#[derive(Clone, Debug)]
pub struct Aggregate {
    pub function: Name,
    pub path: Path,
    pub op: Op,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct StyleBlock {
    pub rules: Vec<StyleRule>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct StyleRule {
    pub target: StyleTarget,
    pub filter: Option<Predicate>,
    pub properties: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleTarget {
    Kind(String),
    Layer(String),
    Nodes,
    Edges,
}

#[derive(Clone, Debug)]
pub struct Render {
    pub formats: Vec<Name>,
    pub options: Vec<Attribute>,
    pub span: Span,
}

/// A `select` clause written in the registry: the members of a pool.
pub fn lower_select(file: FileId, source: &str, node: Node) -> Select {
    Ctx { file, source }.select(node)
}

/// Lowers the root-level `view` and `style` blocks of a `.cav` file, the `check` blocks of a
/// `.cac` file; a block in the wrong file is reported by the lowering of the registry, and left.
pub fn lower_plans(
    file: FileId,
    role: crate::source::FileRole,
    source: &str,
    root: Node,
    plans: &mut Plans,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ctx = Ctx { file, source };
    let checks = role == crate::source::FileRole::Checks;
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        match node.kind() {
            "view" if !checks => plans.views.push(ctx.view(node, diagnostics)),
            "check" if checks => plans.checks.push(ctx.check(node, diagnostics)),
            "style_block" if !checks => {
                plans.project_styles.push(StyleBlock { rules: ctx.style_rules(node), span: ctx.span(&node) })
            }
            _ => {}
        }
    }
}

struct Ctx<'s> {
    file: FileId,
    source: &'s str,
}

impl Ctx<'_> {
    fn text(&self, node: &Node) -> String {
        self.source[node.byte_range()].to_owned()
    }

    fn span(&self, node: &Node) -> Span {
        Span::of(self.file, node)
    }

    fn name(&self, node: &Node) -> Name {
        Name { text: self.text(node), span: self.span(node) }
    }

    fn view(&self, node: Node, diagnostics: &mut Vec<Diagnostic>) -> View {
        let mut view = self.empty_view(node);
        let mut cursor = node.walk();
        for clause in node.named_children(&mut cursor) {
            self.view_clause(&mut view, "view", clause, diagnostics);
        }
        view
    }

    fn check(&self, node: Node, diagnostics: &mut Vec<Diagnostic>) -> Check {
        let name = node.child_by_field_name("name");
        let mut check = Check {
            name: name.map(|n| self.text(&n)).unwrap_or_default(),
            span: name.map_or_else(|| self.span(&node), |n| self.span(&n)),
            view: self.empty_view(node),
            expect: None,
            attributes: Vec::new(),
        };
        let mut cursor = node.walk();
        for clause in node.named_children(&mut cursor) {
            match clause.kind() {
                "expect_clause" => {
                    if check.expect.is_some() {
                        diagnostics.push(Diagnostic::error(
                            format!("check {} has several expect clauses", check.name),
                            self.span(&clause),
                        ));
                    }
                    check.expect = self.expect(clause);
                }
                "attribute" => check.attributes.extend(attribute_at(self.file, self.source, clause)),
                _ => self.view_clause(&mut check.view, "check", clause, diagnostics),
            }
        }
        // Without `select`, every kind is selected: the edges are then only filtered by their `where`.
        if check.view.select.is_none() && !check.view.edges.is_empty() {
            let kinds = crate::schema::kind_names().map(|k| Name { text: k.to_owned(), span: check.span }).collect();
            check.view.select = Some(Select {
                kinds,
                filter: None,
                with_ends: false,
                without_isolated: false,
                group_by: Vec::new(),
                span: check.span,
            });
        }
        check
    }

    fn expect(&self, node: Node) -> Option<Expect> {
        let expectation = node.child_by_field_name("expectation")?;
        Some(match expectation.kind() {
            "none" => Expect::None,
            "some" => Expect::Some,
            "count" => {
                let op = match expectation.child_by_field_name("operator").map(|o| o.kind()) {
                    Some("=") => Op::Eq,
                    Some("!=") => Op::Ne,
                    Some("<") => Op::Lt,
                    Some("<=") => Op::Le,
                    Some(">") => Op::Gt,
                    Some(">=") => Op::Ge,
                    _ => return None,
                };
                Expect::Count(op, self.text(&expectation.child_by_field_name("value")?).parse().ok()?)
            }
            "all" => {
                let mut c = expectation.walk();
                let predicate = expectation.named_children(&mut c).find(|n| n.kind() != "comment")?;
                Expect::All(Box::new(self.predicate(predicate)?))
            }
            _ => return None,
        })
    }

    fn empty_view(&self, node: Node) -> View {
        View {
            name: node.child_by_field_name("name").map(|n| self.text(&n)).unwrap_or_default(),
            span: node.child_by_field_name("name").map_or_else(|| self.span(&node), |n| self.span(&n)),
            select: None,
            edges: Vec::new(),
            through_show: false,
            edge_options: Vec::new(),
            style: Vec::new(),
            render: None,
            zones: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// A clause of a view, or of a check (`what`), which only has `select` and `edges`.
    fn view_clause(&self, view: &mut View, what: &str, clause: Node, diagnostics: &mut Vec<Diagnostic>) {
        match clause.kind() {
            "select_clause" => {
                if view.select.is_some() {
                    diagnostics.push(Diagnostic::error(
                        format!("{what} {} has several select clauses", view.name),
                        self.span(&clause),
                    ));
                }
                view.select = Some(self.select(clause));
            }
            "edges_clause" => {
                let mut c = clause.walk();
                for item in clause.named_children(&mut c) {
                    match item.kind() {
                        "edge_selector" => view.edges.push(self.edge_selector(item)),
                        "edges_option" => {
                            if let Some(a) = attribute_like(self, item) {
                                if a.key == "through" && a.value.raw == "show" {
                                    view.through_show = true;
                                } else {
                                    view.edge_options.push(a);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "style_block" => view.style.extend(self.style_rules(clause)),
            "zone_clause" => view.zones.extend(self.zone(clause)),
            "attribute" => view.attributes.extend(attribute_at(self.file, self.source, clause)),
            "render_clause" => {
                if view.render.is_some() {
                    diagnostics.push(Diagnostic::error(
                        format!("view {} has several render clauses", view.name),
                        self.span(&clause),
                    ));
                }
                view.render = Some(self.render(clause));
            }
            _ => {}
        }
    }

    fn select(&self, node: Node) -> Select {
        let mut cursor = node.walk();
        let kinds = node.children_by_field_name("kind", &mut cursor).map(|n| self.name(&n)).collect();
        let filter = child_of_kind(node, "where_clause").and_then(|w| self.where_clause(w));
        let group_by = child_of_kind(node, "group_by")
            .map(|g| {
                let mut c = g.walk();
                g.children_by_field_name("level", &mut c).map(|n| self.name(&n)).collect()
            })
            .unwrap_or_default();
        Select {
            kinds,
            filter,
            with_ends: child_of_kind(node, "with_ends").is_some(),
            without_isolated: child_of_kind(node, "without_isolated").is_some(),
            group_by,
            span: self.span(&node),
        }
    }

    fn edge_selector(&self, node: Node) -> EdgeSelector {
        let selector = match node.child_by_field_name("selector") {
            Some(s) if s.kind() == "layers" => {
                let mut c = s.walk();
                Selector::Layers(
                    s.named_children(&mut c).filter(|n| n.kind() == "identifier").map(|n| self.name(&n)).collect(),
                )
            }
            Some(s) if s.kind() == "lift" => Selector::Lift(Box::new(self.lift(s))),
            Some(s) if s.kind() == "follow" => Selector::Follow(self.follow(s)),
            Some(s) if s.kind() == "process_selector" => match s.child_by_field_name("name") {
                Some(name) => Selector::Process(self.name(&name)),
                None => Selector::Layers(Vec::new()),
            },
            Some(s) if s.kind() == "path_selector" => {
                match (s.child_by_field_name("from"), s.child_by_field_name("to")) {
                    (Some(from), Some(to)) => Selector::Path(self.name(&from), self.name(&to)),
                    _ => Selector::Layers(Vec::new()),
                }
            }
            Some(s) => Selector::Relation(self.name(&s)),
            None => Selector::Layers(Vec::new()),
        };
        let filter = child_of_kind(node, "where_clause").and_then(|w| self.where_clause(w));
        EdgeSelector { selector, filter, span: self.span(&node) }
    }

    fn lift(&self, node: Node) -> Lift {
        let inner = node.child_by_field_name("edges").map(|e| self.edge_selector(e)).unwrap_or(EdgeSelector {
            selector: Selector::Layers(Vec::new()),
            filter: None,
            span: self.span(&node),
        });
        let mut lift = Lift {
            inner,
            via: None,
            via_kinds: Vec::new(),
            loops: Loops::default(),
            merge: Merge::default(),
            invalid_options: Vec::new(),
            span: self.span(&node),
        };
        let mut cursor = node.walk();
        for option in node.named_children(&mut cursor).filter(|n| n.kind() == "lift_option") {
            let (Some(key), Some(value)) = (option.child_by_field_name("key"), option.child_by_field_name("value"))
            else {
                continue;
            };
            let key_text = self.text(&key);
            let path = self.path(value);
            let simple = (path.steps.len() == 1 && !path.steps[0].optional)
                .then(|| match &path.steps[0].kind {
                    StepKind::Name(n) => Some(n.clone()),
                    StepKind::Ancestor(_) => None,
                })
                .flatten();
            match (key_text.as_str(), simple.as_deref()) {
                ("via", _) => lift.via = Some(path),
                (text, _) if text.starts_with("via_") => {
                    lift.via_kinds.push((Name { text: text["via_".len()..].to_owned(), span: self.span(&key) }, path))
                }
                ("loops", Some("drop")) => lift.loops = Loops::Drop,
                ("loops", Some("keep")) => lift.loops = Loops::Keep,
                ("merge", Some("endpoints")) => lift.merge = Merge::Endpoints,
                ("merge", Some("port")) => lift.merge = Merge::Port,
                ("merge", Some("none")) => lift.merge = Merge::None,
                _ => lift.invalid_options.push((format!("{key_text}: {}", path.text()), self.span(&option))),
            }
        }
        lift
    }

    fn follow(&self, node: Node) -> Follow {
        let mut follow = Follow {
            data: node
                .child_by_field_name("data")
                .map_or_else(|| Name { text: String::new(), span: self.span(&node) }, |d| self.name(&d)),
            from: None,
            relay: Vec::new(),
            relay_to: Vec::new(),
            side: Vec::new(),
            invalid_options: Vec::new(),
            span: self.span(&node),
        };
        let mut cursor = node.walk();
        for option in node.named_children(&mut cursor).filter(|n| n.kind() == "follow_option") {
            let (Some(key), Some(value)) = (option.child_by_field_name("key"), option.child_by_field_name("value"))
            else {
                continue;
            };
            match self.text(&key).as_str() {
                "from" if follow.from.is_none() && value.kind() != "list" => follow.from = Some(self.name(&value)),
                "relay" => follow.relay.extend(self.names(value)),
                "relay_to" => follow.relay_to.extend(self.names(value)),
                "side" => follow.side.extend(self.names(value)),
                key => follow.invalid_options.push((format!("{key}: {}", self.text(&value)), self.span(&option))),
            }
        }
        follow
    }

    /// The names of a value: a name, or the names of a list.
    fn names(&self, value: Node) -> Vec<Name> {
        if value.kind() != "list" {
            return vec![self.name(&value)];
        }
        let mut c = value.walk();
        value
            .named_children(&mut c)
            .filter(|n| matches!(n.kind(), "identifier" | "qualified_name"))
            .map(|n| self.name(&n))
            .collect()
    }

    fn path(&self, node: Node) -> Path {
        let mut cursor = node.walk();
        let steps = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "path_step")
            .filter_map(|step| {
                let kind = self.step_kind(step.child_by_field_name("step")?);
                Some(Step { kind, optional: step.child_by_field_name("optional").is_some(), span: self.span(&step) })
            })
            .collect();
        Path { steps, span: self.span(&node) }
    }

    /// A step of a path: `ancestor(K)`, a key, or a key with an argument (`failures(hosting)`,
    /// named as written).
    fn step_kind(&self, target: Node) -> StepKind {
        match target.kind() {
            "ancestor" => {
                StepKind::Ancestor(target.child_by_field_name("kind").map(|k| self.text(&k)).unwrap_or_default())
            }
            "call" => {
                let part = |field: &str| target.child_by_field_name(field).map(|n| self.text(&n)).unwrap_or_default();
                StepKind::Name(format!("{}({})", part("function"), part("argument")))
            }
            _ => StepKind::Name(self.text(&target)),
        }
    }

    fn where_clause(&self, node: Node) -> Option<Predicate> {
        let mut cursor = node.walk();
        let predicate = node.named_children(&mut cursor).find(|n| n.kind() != "comment")?;
        self.predicate(predicate)
    }

    fn predicate(&self, node: Node) -> Option<Predicate> {
        let path = |field: &str| node.child_by_field_name(field).map(|p| self.path(p));
        let value = |field: &str| node.child_by_field_name(field).and_then(|v| value_at(self.file, self.source, v));
        let operands = || {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .filter(|n| n.kind() != "comment")
                .filter_map(|n| self.predicate(n))
                .collect::<Vec<_>>()
        };
        Some(match node.kind() {
            "flag" => Predicate::Flag(path("attribute")?),
            "exists" => Predicate::Exists(path("attribute")?),
            "has" => Predicate::Has(path("left")?, value("right")?),
            "in" => {
                let list = value("right")?;
                Predicate::In(path("left")?, list.items().into_iter().cloned().collect())
            }
            "comparison" => {
                let op = match node.child_by_field_name("operator").map(|o| o.kind()) {
                    Some("=") => Op::Eq,
                    Some("!=") => Op::Ne,
                    Some("<") => Op::Lt,
                    Some("<=") => Op::Le,
                    Some(">") => Op::Gt,
                    Some(">=") => Op::Ge,
                    _ => return None,
                };
                let left_node = node.child_by_field_name("left")?;
                // `failures(hosting) = x` reads like an aggregate: a key with an argument unless
                // the function is one.
                if left_node.kind() == "aggregate"
                    && let Some(key) = self.keyed(left_node)
                {
                    let span = self.span(&left_node);
                    let left = Path { steps: vec![Step { kind: StepKind::Name(key), optional: false, span }], span };
                    let right = node.child_by_field_name("right")?;
                    return Some(match self.right_path(right) {
                        Some(right) => Predicate::ComparePaths(left, op, right),
                        None => Predicate::Compare(left, op, value("right")?),
                    });
                }
                if left_node.kind() == "aggregate" {
                    let function = self.name(&left_node.child_by_field_name("function")?);
                    let inner = self.path(left_node.child_by_field_name("path")?);
                    let value = value("right")?;
                    return Some(Predicate::Aggregate(Box::new(Aggregate { function, path: inner, op, value })));
                }
                let left = path("left")?;
                let right = node.child_by_field_name("right")?;
                match self.right_path(right) {
                    Some(right) => Predicate::ComparePaths(left, op, right),
                    None => Predicate::Compare(left, op, value("right")?),
                }
            }
            "and" | "or" => {
                let mut ops = operands().into_iter();
                let (a, b) = (Box::new(ops.next()?), Box::new(ops.next()?));
                if node.kind() == "and" { Predicate::And(a, b) } else { Predicate::Or(a, b) }
            }
            "not" => Predicate::Not(Box::new(operands().into_iter().next()?)),
            "parenthesized" => operands().into_iter().next()?,
            _ => return None,
        })
    }

    /// An aggregate that is a key with an argument: `failures(hosting)`, not `count(on)`.
    fn keyed(&self, node: Node) -> Option<String> {
        let function = self.text(&node.child_by_field_name("function")?);
        let path = self.path(node.child_by_field_name("path")?);
        match path.steps.as_slice() {
            [Step { kind: StepKind::Name(argument), optional: false, .. }] if function == "failures" => {
                Some(format!("{function}({argument})"))
            }
            _ => None,
        }
    }

    /// The right side of a comparison as a path: a `right_path`, or a name starting with `source` or
    /// `target` (`target`, `target.owner`); `None` for a value.
    fn right_path(&self, node: Node) -> Option<Path> {
        let span = self.span(&node);
        let head = match node.kind() {
            "right_path" => node.child_by_field_name("head")?,
            "identifier" | "qualified_name" => node,
            _ => return None,
        };
        let text = self.text(&head);
        if node.kind() != "right_path" && !matches!(text.split('.').next(), Some("source" | "target")) {
            return None;
        }
        let mut steps: Vec<Step> =
            text.split('.').map(|s| Step { kind: StepKind::Name(s.to_owned()), optional: false, span }).collect();
        if node.kind() == "right_path" {
            // `target.ancestor(system)`: the last name of the head takes the kind.
            if let (Some(kind), Some(last)) = (node.child_by_field_name("kind"), steps.last_mut()) {
                let kind = self.text(&kind);
                last.kind = match &last.kind {
                    StepKind::Name(n) if n == "ancestor" => StepKind::Ancestor(kind),
                    StepKind::Name(n) => StepKind::Name(format!("{n}({kind})")),
                    other => other.clone(),
                };
            }
            if let Some(last) = steps.last_mut() {
                last.optional = node.child_by_field_name("optional").is_some();
            }
            let mut cursor = node.walk();
            for step in node.named_children(&mut cursor).filter(|n| n.kind() == "path_step") {
                let Some(target) = step.child_by_field_name("step") else { continue };
                let kind = self.step_kind(target);
                steps.push(Step {
                    kind,
                    optional: step.child_by_field_name("optional").is_some(),
                    span: self.span(&step),
                });
            }
        }
        Some(Path { steps, span })
    }

    fn style_rules(&self, node: Node) -> Vec<StyleRule> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .filter_map(|rule| {
                let target = match rule.kind() {
                    "layer_style" => StyleTarget::Layer(self.text(&rule.child_by_field_name("layer")?)),
                    "kind_style" => StyleTarget::Kind(self.text(&rule.child_by_field_name("kind")?)),
                    "nodes_style" => StyleTarget::Nodes,
                    "edges_style" => StyleTarget::Edges,
                    _ => return None,
                };
                let filter = child_of_kind(rule, "where_clause").and_then(|w| self.where_clause(w));
                let mut c = rule.walk();
                let properties = rule
                    .named_children(&mut c)
                    .filter(|n| n.kind() == "attribute")
                    .filter_map(|n| attribute_at(self.file, self.source, n))
                    .collect();
                Some(StyleRule { target, filter, properties, span: self.span(&rule) })
            })
            .collect()
    }

    fn zone(&self, node: Node) -> Option<Zone> {
        let name = self.name(&node.child_by_field_name("name")?);
        let label = node.child_by_field_name("label").and_then(|l| value_at(self.file, self.source, l)).and_then(|v| {
            match v.kind {
                ValueKind::String(text) => Some(text),
                _ => None,
            }
        });
        let members = match node.child_by_field_name("members") {
            Some(list) => {
                let mut c = list.walk();
                ZoneMembers::Names(
                    list.named_children(&mut c)
                        .filter(|n| matches!(n.kind(), "identifier" | "qualified_name"))
                        .map(|n| self.name(&n))
                        .collect(),
                )
            }
            None => ZoneMembers::Filter(child_of_kind(node, "where_clause").and_then(|w| self.where_clause(w))?),
        };
        let mut c = node.walk();
        let properties = node
            .named_children(&mut c)
            .filter(|n| n.kind() == "attribute")
            .filter_map(|n| attribute_at(self.file, self.source, n))
            .collect();
        Some(Zone { name, label, members, properties, span: self.span(&node) })
    }

    fn render(&self, node: Node) -> Render {
        let mut cursor = node.walk();
        let formats = node.children_by_field_name("format", &mut cursor).map(|n| self.name(&n)).collect();
        let mut c = node.walk();
        let options = node
            .named_children(&mut c)
            .filter(|n| n.kind() == "attribute")
            .filter_map(|n| attribute_at(self.file, self.source, n))
            .collect();
        Render { formats, options, span: self.span(&node) }
    }
}

/// `edges_option` nodes have the same shape as attributes (`key: identifier`).
fn attribute_like(ctx: &Ctx, node: Node) -> Option<Attribute> {
    let key = node.child_by_field_name("key")?;
    let value = node.child_by_field_name("value")?;
    Some(Attribute { key: ctx.text(&key), key_span: ctx.span(&key), value: value_at(ctx.file, ctx.source, value)? })
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|n| n.kind() == kind)
}
