//! Renaming a resource: its declaration and every name designating it, in the registry and the
//! plans, checked by compiling the project again before anything changes.
//!
//! A written name is a suffix of the path of what it designates (`middleware.esb`, `esb`):
//! only the segment naming the renamed resource changes, so a qualified name keeps its prefix
//! and a short name stays short. Renaming a system or a site also changes that segment in the
//! names of its descendants (`middleware.esb` when `middleware` is renamed).

use crate::Severity;
use crate::project::{Compilation, compile_modules};
use crate::registry::ResourceId;
use crate::source::{SourceFile, Span};

/// A replacement in a file: the bytes of `span` become `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub text: String,
}

/// Whether a text is a name of a resource (`web01`, `srv-lil`, `eai_v2`).
pub fn is_name(text: &str) -> bool {
    let mut parts = text.split('-');
    let first = parts.next().unwrap_or_default();
    let mut chars = first.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// The edits renaming a resource to `name`, or why it cannot be renamed. `files` are those the
/// compilation was made from, in the same order.
pub fn rename(
    compilation: &Compilation,
    files: &[SourceFile],
    id: ResourceId,
    name: &str,
) -> Result<Vec<Edit>, String> {
    let registry = &compilation.registry;
    let resource = registry.resource(id);
    let old = resource.name.as_str();
    if name == old {
        return Ok(Vec::new());
    }
    if !is_name(name) {
        return Err(format!("{name} is not a valid name: letters, digits and `_`, in parts joined by `-`"));
    }
    if let Some(module) = compilation.module_of(resource.name_span.file) {
        return Err(format!("{old} is declared in the module {module}, read only: rename it there"));
    }
    let text_of = |span: &Span| files.get(span.file.0 as usize).and_then(|f| f.text.get(span.start..span.end));
    if text_of(&resource.name_span) != Some(old) {
        return Err(format!("{old} is declared by a for_each: rename it in the for_each"));
    }
    let parent = registry.ancestors(id).next();
    if registry
        .ids()
        .any(|other| other != id && registry.ancestors(other).next() == parent && registry.resource(other).name == name)
    {
        let place = parent.map_or("at the root".to_owned(), |p| format!("in {}", registry.qualified_name(p)));
        return Err(format!("{name} is already taken {place}"));
    }

    // The declaration, then the names designating the resource or one of its descendants.
    let mut edits = vec![Edit { span: resource.name_span, text: name.to_owned() }];
    let mut seen = Vec::new();
    for (span, targets) in &compilation.model.names {
        let [target] = targets.as_slice() else { continue };
        let path = registry.path(*target);
        // The depth of the renamed resource in the path of the target, if it is in it.
        let Some(depth) = std::iter::once(*target).chain(registry.ancestors(*target)).position(|r| r == id) else {
            continue;
        };
        let Some(written) = text_of(span) else { continue };
        let segments: Vec<&str> = written.split('.').collect();
        if segments.len() <= depth || seen.contains(span) {
            continue;
        }
        seen.push(*span);
        // The segment naming the renamed resource, counted from the end.
        let index = segments.len() - 1 - depth;
        if segments[index] != old || path.len() < segments.len() {
            continue;
        }
        let start = span.start + segments[..index].iter().map(|s| s.len() + 1).sum::<usize>();
        edits.push(Edit { span: Span { start, end: start + old.len(), ..*span }, text: name.to_owned() });
    }

    // The project compiled again: no new error, and the resource designated as many times.
    let renamed = apply(files, &edits);
    if let Some(module) = edits.iter().find_map(|e| compilation.module_of(e.span.file)) {
        return Err(format!("{old} is named in the module {module}, read only: rename it there first"));
    }
    let after = compile_modules(&renamed, &compilation.modules, crate::check::Options::default());
    let errors = |c: &Compilation| -> Vec<String> {
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
    };
    let mut before = errors(compilation);
    for error in errors(&after) {
        match before.iter().position(|e| *e == error) {
            Some(i) => {
                before.remove(i);
            }
            None => return Err(format!("renaming {old} to {name} would give an error: {error}")),
        }
    }
    let mut new_path: Vec<String> = registry.path(id).iter().map(|s| (*s).to_owned()).collect();
    if let Some(last) = new_path.last_mut() {
        *last = name.to_owned();
    }
    let new_id = after.registry.ids().find(|&r| after.registry.path(r) == new_path);
    let count = |c: &Compilation, r: ResourceId| c.references(r, false).len();
    match new_id {
        Some(new_id) if count(&after, new_id) == count(compilation, id) => Ok(edits),
        _ => Err(format!(
            "{old} is named in a way the renaming does not follow (a for_each, a host name): rename it by hand"
        )),
    }
}

/// The files with the edits applied.
pub fn apply(files: &[SourceFile], edits: &[Edit]) -> Vec<SourceFile> {
    let mut out = files.to_vec();
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    // From the end of each file, so that the offsets of the other edits stay valid.
    sorted.sort_by_key(|e| std::cmp::Reverse((e.span.file.0, e.span.start)));
    for edit in sorted {
        if let Some(file) = out.get_mut(edit.span.file.0 as usize) {
            file.text.replace_range(edit.span.start..edit.span.end, &edit.text);
        }
    }
    out
}
