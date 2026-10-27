//! The schema of a project (spec § 4, Schema of the project): `kind` declarations adding typed
//! keys to the built-in kinds, or declaring kinds of the project, each `like` a built-in one.
//!
//! ```text
//! kind server  { os: string, ram: number, vmware_cluster: ref(cluster), inventory_id: pattern("INV-[0-9]+") (required) }
//! kind printer { like: device, parents: site | subnet, serial: string }
//! ```

use std::collections::{BTreeSet, HashMap};

use crate::diagnostic::Diagnostic;
use crate::schema::{self, AttrDef, Card, KindDef, Pattern, RESOURCE_KEYS, Schema, Ty};
use crate::source::Span;
use crate::suggest;

/// A `kind` declaration, as written.
#[derive(Clone, Debug)]
pub struct KindDecl {
    pub name: String,
    pub name_span: Span,
    pub keys: Vec<KeyDecl>,
}

/// A `link` declaration (`layer` absent: every link) or a `layer` one, as written.
#[derive(Clone, Debug)]
pub struct LinkDecl {
    pub layer: Option<(String, Span)>,
    pub span: Span,
    pub keys: Vec<KeyDecl>,
}

/// A `group` declaration: `group storage: database | saas`.
#[derive(Clone, Debug)]
pub struct GroupDecl {
    pub name: String,
    pub name_span: Span,
    /// Kinds or groups, with their spans.
    pub members: Vec<(String, Span)>,
}

/// A key of a `kind` declaration: `os: string`, `like: device`, `parent: site | subnet`.
#[derive(Clone, Debug)]
pub struct KeyDecl {
    pub key: String,
    pub key_span: Span,
    pub ty: TypeDecl,
    pub required: bool,
}

#[derive(Clone, Debug)]
pub struct TypeDecl {
    pub form: TypeForm,
    pub card: Option<Card>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TypeForm {
    /// `string`, `number`…; also the kind of `like`.
    Named(String),
    /// `ref(server | cluster)`.
    Ref(Vec<String>),
    /// `pattern("APP-[0-9]{3}")`, its text as written between the quotes.
    Pattern(String),
    /// `vmware | hyperv`: a closed list of values, or the kinds of `parents`.
    Choices(Vec<String>),
}

/// The named types, as written in a declaration.
pub const TYPES: &[(&str, Ty)] = &[
    ("string", Ty::String),
    ("number", Ty::Number),
    ("semver", Ty::Semver),
    ("version", Ty::Version),
    ("ip", Ty::Ip),
    ("cidr", Ty::Cidr),
    ("host", Ty::Host),
    ("url", Ty::Url),
    ("bandwidth", Ty::Bandwidth),
    ("spdx", Ty::Spdx),
];

/// Builds the schema of a project from its declarations, reporting what cannot be.
pub fn schema(
    declarations: &[KindDecl],
    links: &[LinkDecl],
    groups: &[GroupDecl],
    diagnostics: &mut Vec<Diagnostic>,
) -> Schema {
    // The blocks of a kind, merged: the schema may be split by theme.
    let mut merged: Vec<KindDecl> = Vec::new();
    for decl in declarations {
        match merged.iter_mut().find(|m| m.name == decl.name) {
            Some(first) => first.keys.extend(decl.keys.iter().cloned()),
            None => merged.push(decl.clone()),
        }
    }
    let declarations = merged.as_slice();
    // `like` and `parents` once per kind, in one of its blocks.
    for decl in declarations {
        for key in ["like", "parents"] {
            let mut written = decl.keys.iter().filter(|k| k.key == key);
            let first = written.next();
            for again in written {
                diagnostics.push(
                    Diagnostic::error(format!("{key} of {} written twice", decl.name), again.key_span)
                        .with_span(first.map_or(again.key_span, |f| f.key_span)),
                );
            }
        }
    }

    // The kinds of the project first: a reference may name one declared further.
    let mut own: Vec<(&KindDecl, &'static KindDef)> = Vec::new();
    for decl in declarations {
        if schema::kind_def(&decl.name).is_some() {
            continue;
        }
        if schema::GROUPS.iter().any(|(g, _)| *g == decl.name) || groups.iter().any(|g| g.name == decl.name) {
            diagnostics
                .push(Diagnostic::error(format!("{} is a group of kinds, not a kind", decl.name), decl.name_span));
            continue;
        }
        let Some(like) = decl.keys.iter().find(|k| k.key == "like") else {
            let help = if decl.name == "link" {
                "the keys of the links: `link { … }`, or of a layer: `layer flow { … }`"
            } else {
                "name the built-in kind it behaves as: `like: device`"
            };
            diagnostics.push(
                Diagnostic::error(
                    format!("kind {}: a kind of the project is like a built-in one", decl.name),
                    decl.name_span,
                )
                .with_help(help),
            );
            continue;
        };
        let model = match &like.ty.form {
            TypeForm::Named(name) => schema::kind_def(name),
            _ => None,
        };
        let Some(model) = model else {
            let written = match &like.ty.form {
                TypeForm::Named(name) => name.clone(),
                _ => String::new(),
            };
            let mut d = Diagnostic::error(format!("like: {written} is not a built-in kind"), like.ty.span);
            if let Some(s) = suggest::closest(&written, schema::kind_names()) {
                d = d.with_help(format!("did you mean `{s}`?"));
            }
            diagnostics.push(d);
            continue;
        };
        let name = schema::intern_str(&decl.name);
        let parents = decl.keys.iter().find(|k| k.key == "parents");
        let parents: &'static [&'static str] = match parents.map(|p| &p.ty.form) {
            Some(TypeForm::Named(one)) => intern_slice(&[one.as_str()]),
            Some(TypeForm::Choices(kinds)) => intern_slice(&kinds.iter().map(String::as_str).collect::<Vec<_>>()),
            Some(_) => {
                diagnostics.push(Diagnostic::error(
                    "parents: a kind, or kinds separated by `|`",
                    parents.map_or(decl.name_span, |p| p.ty.span),
                ));
                model.parents
            }
            None => model.parents,
        };
        let def = KindDef {
            name,
            parents,
            attrs: model.attrs,
            connector: model.connector,
            owner_required: model.owner_required,
            within: model.within,
        };
        own.push((decl, intern_kind(def)));
    }
    let likes: HashMap<&'static str, &'static str> = own
        .iter()
        .filter_map(|(decl, def)| {
            let like = decl.keys.iter().find(|k| k.key == "like")?;
            match &like.ty.form {
                TypeForm::Named(name) => Some((def.name, schema::kind_def(name)?.name)),
                _ => None,
            }
        })
        .collect();
    let draft = Schema::new(own.iter().map(|(_, d)| *d).collect(), likes.clone());
    let groups = project_groups(groups, &draft, diagnostics);
    let draft = draft.with_groups(groups.clone());

    // The parents of the kinds of the project: kinds, or the root.
    for (decl, def) in &own {
        for parent in def.parents.iter().filter(|p| **p != "none" && draft.kind_def(p).is_none()) {
            let span = decl.keys.iter().find(|k| k.key == "parents").map_or(decl.name_span, |k| k.ty.span);
            diagnostics.push(unknown_kind(parent, span, &draft));
        }
    }

    // The keys: added to a built-in kind, or to a kind of the project after those of its model.
    let mut declared = Vec::new();
    for decl in declarations {
        let Some(base) = draft.kind_def(&decl.name) else { continue };
        let builtin = schema::kind_def(&decl.name).is_some();
        let mut attrs: Vec<AttrDef> = base.attrs.to_vec();
        let mut keys: BTreeSet<&str> = BTreeSet::new();
        for key in &decl.keys {
            if key.key == "parent" {
                diagnostics.push(
                    Diagnostic::error("the parents of a kind are written `parents:`", key.key_span).with_help(
                        "`parent:` places a resource; `kind printer { like: device, parents: site | subnet }`",
                    ),
                );
                continue;
            }
            if matches!(key.key.as_str(), "like" | "parents") {
                if builtin {
                    diagnostics.push(
                        Diagnostic::error(
                            format!("{}: the {} of a built-in kind does not change", key.key, key.key),
                            key.key_span,
                        )
                        .with_help("declare a kind of the project: `kind printer { like: device, parents: … }`"),
                    );
                }
                continue;
            }
            if !keys.insert(&key.key) {
                let first = decl.keys.iter().find(|k| k.key == key.key).map_or(key.key_span, |k| k.key_span);
                diagnostics.push(
                    Diagnostic::error(format!("key {} of {} declared twice", key.key, decl.name), key.key_span)
                        .with_span(first),
                );
                continue;
            }
            if let Some(existing) = base.attrs.iter().chain(RESOURCE_KEYS).find(|a| a.key == key.key) {
                // Declared by the project before the built-in schema had it, with its type: the
                // declaration is only redundant.
                let same = ty(key, &draft, &mut Vec::new())
                    .is_some_and(|t| schema::type_text(&AttrDef { ty: t, ..*existing }) == schema::type_text(existing))
                    && key.ty.card.unwrap_or(Card::One) == existing.card;
                diagnostics.push(if same {
                    Diagnostic::warning(format!("{} is now a built-in key of {}", key.key, decl.name), key.key_span)
                        .with_help("remove its declaration: the built-in one has the same type")
                } else {
                    Diagnostic::error(format!("{} is already a key of {}", key.key, decl.name), key.key_span)
                        .with_help(format!("its type: {}", schema::type_text(existing)))
                });
                continue;
            }
            if let Some(ty) = ty(key, &draft, diagnostics) {
                let card = key.ty.card.unwrap_or(Card::One);
                attrs.push(AttrDef { key: schema::intern_str(&key.key), ty, card, required: key.required });
            }
        }
        let attrs = intern(format!("{attrs:?}"), attrs).as_slice();
        declared.push(intern_kind(KindDef { attrs, ..clone(base) }));
    }

    // The keys of the links: of every link, then of a layer.
    let mut every: Vec<AttrDef> = Vec::new();
    let mut layers: Vec<(&'static str, Vec<AttrDef>)> = Vec::new();
    // The blocks of the links, and of a layer, merged.
    let mut merged: Vec<LinkDecl> = Vec::new();
    for decl in links {
        let layer = |d: &LinkDecl| d.layer.as_ref().map(|(name, _)| name.clone());
        match merged.iter_mut().find(|m| layer(m) == layer(decl)) {
            Some(first) => first.keys.extend(decl.keys.iter().cloned()),
            None => merged.push(decl.clone()),
        }
    }
    for decl in &merged {
        let layer = decl.layer.as_ref().map(|(name, _)| name.as_str());
        let mut attrs: Vec<AttrDef> = Vec::new();
        for key in &decl.keys {
            let taken = schema::LINK_KEYS.iter().chain(&every).chain(&attrs).find(|a| a.key == key.key);
            if let Some(existing) = taken {
                diagnostics.push(
                    Diagnostic::error(format!("{} is already a key of the links", key.key), key.key_span)
                        .with_help(format!("its type: {}", schema::type_text(existing))),
                );
                continue;
            }
            if matches!(key.key.as_str(), "like" | "parents") {
                diagnostics.push(Diagnostic::error(
                    format!("{}: a key of the kinds, not of the links", key.key),
                    key.key_span,
                ));
                continue;
            }
            if let Some(ty) = ty(key, &draft_with(&declared, &likes, &groups), diagnostics) {
                let card = key.ty.card.unwrap_or(Card::One);
                attrs.push(AttrDef { key: schema::intern_str(&key.key), ty, card, required: key.required });
            }
        }
        match layer {
            None => every.extend(attrs),
            Some(layer) => layers.push((schema::intern_str(layer), attrs)),
        }
    }
    let every = intern(format!("{every:?}"), every).as_slice();
    let layers =
        layers.into_iter().map(|(layer, attrs)| (layer, intern(format!("{attrs:?}"), attrs).as_slice())).collect();
    Schema::new(declared, likes).with_links(every, layers).with_groups(groups)
}

/// The schema of the kinds, to resolve the references of the keys of the links.
fn draft_with(
    declared: &[&'static KindDef],
    likes: &HashMap<&'static str, &'static str>,
    groups: &[(&'static str, &'static [&'static str])],
) -> Schema {
    Schema::new(declared.to_vec(), likes.clone()).with_groups(groups.to_vec())
}

/// The groups of the project, each with its kinds: kinds, built-in groups or other groups of the
/// project, expanded.
fn project_groups(
    decls: &[GroupDecl],
    draft: &Schema,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(&'static str, &'static [&'static str])> {
    let mut valid: Vec<&GroupDecl> = Vec::new();
    for decl in decls {
        if let Some(first) = valid.iter().find(|g| g.name == decl.name) {
            diagnostics.push(
                Diagnostic::error(format!("group {} declared twice", decl.name), decl.name_span)
                    .with_span(first.name_span),
            );
        } else if draft.kind_def(&decl.name).is_some() || schema::GROUPS.iter().any(|(g, _)| *g == decl.name) {
            let what = if draft.kind_def(&decl.name).is_some() { "a kind" } else { "a built-in group" };
            diagnostics.push(Diagnostic::error(format!("{} is already {what}", decl.name), decl.name_span));
        } else {
            valid.push(decl);
        }
    }
    let mut out: Vec<(&'static str, &'static [&'static str])> = Vec::new();
    for decl in &valid {
        let mut kinds: Vec<&'static str> = Vec::new();
        let mut ok = true;
        // Depth first through the groups of the project, the path kept to report a cycle.
        let mut stack: Vec<(&GroupDecl, usize)> = vec![(decl, 0)];
        while let Some((group, index)) = stack.pop() {
            let Some((member, span)) = group.members.get(index) else { continue };
            stack.push((group, index + 1));
            if let Some(inner) = valid.iter().find(|g| g.name == *member) {
                if stack.iter().any(|(g, _)| g.name == inner.name) {
                    let path: Vec<&str> = stack.iter().map(|(g, _)| g.name.as_str()).collect();
                    diagnostics.push(Diagnostic::error(
                        format!("group {}: its members make a cycle ({} → {member})", decl.name, path.join(" → ")),
                        *span,
                    ));
                    ok = false;
                    break;
                }
                stack.push((inner, 0));
                continue;
            }
            let expanded = draft.expand(member);
            if expanded.is_empty() {
                // Reported once, on the group that writes it.
                if std::ptr::eq(group, *decl) {
                    diagnostics.push(unknown_kind(member, *span, draft));
                }
                ok = false;
                continue;
            }
            kinds.extend(expanded.into_iter().filter(|k| !kinds.contains(k)).collect::<Vec<_>>());
        }
        if ok {
            out.push((schema::intern_str(&decl.name), intern_slice(&kinds)));
        }
    }
    out
}

/// The type of a key, checked: a named type, kinds that exist, a regular expression that compiles.
fn ty(key: &KeyDecl, schema: &Schema, diagnostics: &mut Vec<Diagnostic>) -> Option<Ty> {
    match &key.ty.form {
        TypeForm::Named(name) => match TYPES.iter().find(|(n, _)| n == name) {
            Some((_, ty)) => Some(*ty),
            // A list of one value: `hypervisor: vmware`; a name close to a type is a typo.
            None if suggest::closest(name, TYPES.iter().map(|(n, _)| *n)).is_none() => {
                Some(Ty::Symbol(intern_slice(&[name.as_str()])))
            }
            None => {
                let mut d = Diagnostic::error(format!("unknown type: {name}"), key.ty.span);
                d = match suggest::closest(name, TYPES.iter().map(|(n, _)| *n)) {
                    Some(s) => {
                        d.with_help(format!("did you mean `{s}`? or `ref({name})`, or a list of values: `a | b`"))
                    }
                    None => d.with_help(format!(
                        "{}, ref(<kind>), pattern(\"…\"), or a list of values: `a | b`",
                        TYPES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                    )),
                };
                diagnostics.push(d);
                None
            }
        },
        TypeForm::Ref(kinds) => {
            let mut ok = true;
            for kind in kinds.iter().filter(|k| schema.expand(k).is_empty()) {
                diagnostics.push(unknown_kind(kind, key.ty.span, schema));
                ok = false;
            }
            ok.then(|| Ty::Ref(schema::intern_str(&kinds.join("|"))))
        }
        TypeForm::Pattern(source) => match regex_lite::Regex::new(&format!("^(?:{source})$")) {
            Ok(regex) => {
                let source = schema::intern_str(source);
                Some(Ty::Pattern(intern(source.to_owned(), Pattern { source, regex })))
            }
            Err(error) => {
                diagnostics.push(
                    Diagnostic::error(format!("invalid pattern: {source}"), key.ty.span).with_help(error.to_string()),
                );
                None
            }
        },
        TypeForm::Choices(values) => {
            Some(Ty::Symbol(intern_slice(&values.iter().map(String::as_str).collect::<Vec<_>>())))
        }
    }
}

fn unknown_kind(kind: &str, span: Span, schema: &Schema) -> Diagnostic {
    let mut d = Diagnostic::error(format!("unknown kind: {kind}"), span);
    let names = schema.kind_names();
    if let Some(s) = suggest::closest(kind, names.into_iter().chain(schema.group_names())) {
        d = d.with_help(format!("did you mean `{s}`?"));
    }
    d
}

fn clone(def: &KindDef) -> KindDef {
    KindDef {
        name: def.name,
        parents: def.parents,
        attrs: def.attrs,
        connector: def.connector,
        owner_required: def.owner_required,
        within: def.within,
    }
}

fn intern<T: std::fmt::Debug + Send + Sync + 'static>(key: String, value: T) -> &'static T {
    schema::intern(key, || value)
}

fn intern_kind(def: KindDef) -> &'static KindDef {
    intern(format!("{def:?}"), def)
}

fn intern_slice(items: &[&str]) -> &'static [&'static str] {
    let items: Vec<&'static str> = items.iter().map(|i| schema::intern_str(i)).collect();
    intern(format!("{items:?}"), items).as_slice()
}
