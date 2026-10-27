//! Running the checks of a project (`check` blocks): rules written by the user on the model.

use crate::diagnostic::{Diagnostic, Severity};
use crate::plan::{Check, Expect, Op};
use crate::project::Compilation;
use crate::query::{Ctx, Subject};
use crate::source::Span;
use crate::view::{self, GraphEdge};

/// Offending elements named in a failure message; the others are counted.
const LISTED: usize = 5;
/// Offending elements located in a failure diagnostic.
const LOCATED: usize = 10;

/// Runs every check; a failing one gives a diagnostic, an error unless `severity: warning`.
pub fn run(compilation: &Compilation) -> Vec<Diagnostic> {
    compilation.plans.checks.iter().filter_map(|check| run_check(compilation, check)).collect()
}

/// A selected node or edge: its name, where it is declared.
#[derive(Clone, Debug)]
pub struct Item {
    pub text: String,
    pub span: Span,
}

/// The result of a check.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub passed: bool,
    /// What was selected against what was expected: `3 nodes selected, none expected`.
    pub summary: String,
    /// The nodes or edges selected.
    pub selected: Vec<Item>,
    /// Those that make it fail: all of them for `expect none`, those not meeting the condition
    /// for `expect all`.
    pub offending: Vec<Item>,
    /// The check cannot be evaluated: an error in its selection.
    pub error: Option<Diagnostic>,
}

/// Evaluates a check; `None` for a check without a selection or an expectation (reported by
/// the validation of the plans).
pub fn outcome(compilation: &Compilation, check: &Check) -> Option<Outcome> {
    let expect = check.expect.as_ref()?;
    check.view.select.as_ref()?;
    let (registry, model) = (&compilation.registry, &compilation.model);
    // Warnings of the selection (a filter keeping nothing) are what `expect none` asks for.
    let mut diagnostics = Vec::new();
    // The scenario of the check (`failing`) is that of its selection.
    let scenario;
    let view = match check.attributes.iter().find(|a| a.key == "failing") {
        Some(failing) => {
            let mut view = check.view.clone();
            view.attributes.push(failing.clone());
            scenario = view;
            &scenario
        }
        None => &check.view,
    };
    let graph = view::evaluate(view, registry, model, &mut diagnostics);
    if let Some(error) = diagnostics.into_iter().find(|d| d.severity == Severity::Error) {
        let summary = error.message.clone();
        return Some(Outcome {
            passed: false,
            summary,
            selected: Vec::new(),
            offending: Vec::new(),
            error: Some(error),
        });
    }

    let ctx = Ctx { registry, model, down: &graph.down };
    let on_edges = !check.view.edges.is_empty();
    let node = |id| Item { text: registry.qualified_name(id), span: registry.resource(id).name_span };
    let edge = |e: &GraphEdge| Item {
        text: format!("{} -> {}", registry.qualified_name(e.source), registry.qualified_name(e.target)),
        span: e
            .links
            .first()
            .map(|&l| registry.links[l].span)
            .unwrap_or_else(|| registry.resource(e.declared.first().map_or(e.source, |d| d.0)).name_span),
    };
    let selected: Vec<Item> = if on_edges {
        graph.edges.iter().map(edge).collect()
    } else {
        graph.nodes.iter().map(|&id| node(id)).collect()
    };
    let noun = |n: usize| match (on_edges, n) {
        (true, 1) => "edge",
        (true, _) => "edges",
        (false, 1) => "node",
        (false, _) => "nodes",
    };
    let count = selected.len();
    let selected_text =
        if count == 0 { format!("no {} selected", noun(1)) } else { format!("{count} {} selected", noun(count)) };
    let outcome = |passed: bool, summary: String, offending: Vec<Item>| {
        Some(Outcome { passed, summary, selected: selected.clone(), offending, error: None })
    };
    match expect {
        Expect::None if count > 0 => outcome(false, format!("{selected_text}, none expected"), selected.clone()),
        Expect::None => outcome(true, format!("{selected_text}, none expected"), Vec::new()),
        Expect::Some => outcome(count > 0, format!("{selected_text}, at least one expected"), Vec::new()),
        Expect::Count(op, n) => {
            outcome(holds(*op, count, *n), format!("{selected_text}, expected count {} {n}", op_text(*op)), Vec::new())
        }
        Expect::All(predicate) => {
            let failing: Vec<Item> = if on_edges {
                graph.edges.iter().filter(|e| !ctx.matches(&Subject::Edge(*e), predicate)).map(edge).collect()
            } else {
                graph
                    .nodes
                    .iter()
                    .filter(|&&id| !ctx.matches(&Subject::Node(id), predicate))
                    .map(|&id| node(id))
                    .collect()
            };
            if failing.is_empty() {
                outcome(true, format!("{selected_text}, all meet the expectation"), Vec::new())
            } else {
                let verb = if failing.len() == 1 { "does" } else { "do" };
                let summary = format!("{} of {count} {} {verb} not meet the expectation", failing.len(), noun(count));
                outcome(false, summary, failing)
            }
        }
    }
}

fn run_check(compilation: &Compilation, check: &Check) -> Option<Diagnostic> {
    let result = outcome(compilation, check)?;
    if let Some(error) = result.error {
        return Some(error);
    }
    if result.passed {
        return None;
    }
    let (summary, offending) = (result.summary, result.offending);
    let mut message = format!("check {} failed: {summary}", check.name);
    if !offending.is_empty() {
        let names: Vec<&str> = offending.iter().take(LISTED).map(|i| i.text.as_str()).collect();
        message.push_str(&format!(": {}", names.join(", ")));
        if offending.len() > LISTED {
            message.push_str(&format!(", … ({} more)", offending.len() - LISTED));
        }
    }
    let severity = check.severity();
    let mut diagnostic = Diagnostic::new(severity, message, check.span);
    for item in offending.iter().take(LOCATED) {
        diagnostic = diagnostic.with_span(item.span);
    }
    if let Some(text) = check.message() {
        diagnostic = diagnostic.with_help(text);
    }
    Some(diagnostic)
}

fn holds(op: Op, count: usize, n: usize) -> bool {
    match op {
        Op::Eq => count == n,
        Op::Ne => count != n,
        Op::Lt => count < n,
        Op::Le => count <= n,
        Op::Gt => count > n,
        Op::Ge => count >= n,
    }
}

fn op_text(op: Op) -> &'static str {
    match op {
        Op::Eq => "=",
        Op::Ne => "!=",
        Op::Lt => "<",
        Op::Le => "<=",
        Op::Gt => ">",
        Op::Ge => ">=",
    }
}
