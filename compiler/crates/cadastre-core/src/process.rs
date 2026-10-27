//! Business processes (spec § 3, Processes): a `process` lists its steps in their order, each a
//! link already declared, the reply to one, or a processing within a resource; loops of steps.
//!
//! ```text
//! process orders {
//!   step scheduler -> broker [trigger]
//!   step broker -> extractor : hessian { async }
//!   step extractor -> broker { reply, label: "ack" }
//!   step broker "transforms the orders"
//!   loop "for each store" { step broker -> store : hessian }
//! }
//! ```

use crate::registry::{Link, ResourceId};
use crate::source::Span;

/// A process as written: the resource it is (`kind: process`), its steps and loops.
#[derive(Clone, Debug)]
pub struct Process {
    pub id: ResourceId,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Step(Box<Step>),
    Loop { label: String, span: Span, items: Vec<Item> },
}

/// A step as written: a link without a target for a processing within a resource (`text`).
#[derive(Clone, Debug)]
pub struct Step {
    pub link: Link,
    pub text: Option<String>,
    /// `async`, `reply`.
    pub flags: Vec<(String, Span)>,
}

/// The flags of a step.
pub const FLAGS: &[(&str, &str)] = &[
    ("async", "the caller does not wait for an answer"),
    ("reply", "the answer to a call: the link declared the other way"),
];

/// The keys of a step.
pub const STEP_KEYS: &[(&str, &str)] = &[
    ("label", "the text of the arrow, in place of the protocol and the data carried"),
    ("note", "a note beside the step, in the sequence diagram"),
];

/// A step of a process, resolved: in the order of the process, loops opening and closing it.
#[derive(Clone, Debug)]
pub enum Event {
    Step(ResolvedStep),
    LoopStart(String),
    LoopEnd,
}

#[derive(Clone, Debug)]
pub struct ResolvedStep {
    pub source: ResourceId,
    /// `None` for a processing within the source.
    pub target: Option<ResourceId>,
    /// The links the step is (declared the other way for a reply): one, or one per member of a
    /// participant standing for several resources; none for a processing.
    pub links: Vec<usize>,
    pub reply: bool,
    pub asynchronous: bool,
    pub label: Option<String>,
    pub note: Option<String>,
    /// The text of a processing within the source.
    pub text: Option<String>,
    pub span: Span,
}

impl Event {
    pub fn step(&self) -> Option<&ResolvedStep> {
        match self {
            Event::Step(step) => Some(step),
            _ => None,
        }
    }
}
