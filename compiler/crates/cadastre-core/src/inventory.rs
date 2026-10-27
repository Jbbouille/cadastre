//! A one-off import of an inventory in CSV (a spreadsheet, an export of a CMDB) into a `.ca` file,
//! to bootstrap a registry: a resource per row, of the kind its `kind` column says, nested in the
//! resource its `parent` column names, its other columns as its keys; to be reviewed and moved
//! into the project. Unlike `for_each … in csv(…)`, which keeps the file as the source of a
//! homogeneous series, the rows may be of every kind and the file is left behind once imported.
//!
//! The columns: `kind` (`server`, `virtual server`, `app`…; or one kind for every row, given
//! apart), `name`, `parent` (optional) and any key; the headers as the keys are written, in any
//! case. The values are written as the schema types the key: a text quoted, a reference by its
//! name, a list split on its commas.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

use crate::schema::{Card, Ty};
use crate::terraform::{Imported, identifier, literal, quoted};

/// A resource of the inventory: its row, kind, name as written and as an identifier, keys.
struct Row {
    line: usize,
    virtual_: bool,
    kind: String,
    written: String,
    name: String,
    parent: Option<String>,
    attributes: Vec<(String, String)>,
    children: Vec<usize>,
    nested: bool,
}

/// How the columns and the kinds of an export are read, when it does not use the words of the
/// language: `Hostname=name,Adresse IP=ip` (a column mapped to nothing is left out), and
/// `Serveur physique=server,VM=virtual server`. Compared whatever the case.
#[derive(Clone, Debug, Default)]
pub struct Mapping {
    pub columns: Vec<(String, String)>,
    pub kinds: Vec<(String, String)>,
}

impl Mapping {
    /// The mapping of `--map` and `--kinds`: pairs `from=to` separated by commas.
    pub fn parse(columns: &str, kinds: &str) -> Result<Self, String> {
        fn pairs(text: &str, option: &str) -> Result<Vec<(String, String)>, String> {
            text.split(',')
                .filter(|p| !p.trim().is_empty())
                .map(|pair| match pair.split_once('=') {
                    Some((from, to)) => Ok((from.trim().to_lowercase(), to.trim().to_owned())),
                    None => Err(format!("{option} expects pairs `from=to` separated by commas, not `{}`", pair.trim())),
                })
                .collect()
        }
        Ok(Mapping { columns: pairs(columns, "--map")?, kinds: pairs(kinds, "--kinds")? })
    }
}

/// The `.ca` file of an inventory, named after it (`servers.csv` → `servers.ca`), formatted; an
/// error when the file is not CSV or lacks the columns it needs. `kind` is the kind of every row
/// when the file has no `kind` column.
pub fn import(text: &str, file_name: &str, kind: Option<&str>, mapping: &Mapping) -> Result<Vec<Imported>, String> {
    let records = crate::csv::parse(text).map_err(|(at, message)| format!("byte {at}: {message}"))?;
    let Some((header, rows)) = records.split_first() else {
        return Err("an empty file".to_owned());
    };
    let mut header: Vec<String> = header.iter().map(|f| f.text.trim().to_lowercase()).collect();
    for (from, to) in &mapping.columns {
        let Some(column) = header.iter().position(|h| h == from) else {
            let columns: Vec<&str> = header.iter().map(String::as_str).collect();
            return Err(format!("--map: no column `{from}` (the columns: {})", columns.join(", ")));
        };
        header[column] = to.to_lowercase();
    }
    let column = |name: &str| header.iter().position(|h| h == name);
    let kind_column = column("kind");
    if kind_column.is_none() && kind.is_none() {
        return Err("no `kind` column: add one, or give the kind of every row".to_owned());
    }
    let Some(name_column) = column("name") else {
        return Err("no `name` column".to_owned());
    };
    let parent_column = column("parent");
    let keys: Vec<(usize, &String)> = header
        .iter()
        .enumerate()
        .filter(|(i, h)| Some(*i) != kind_column && *i != name_column && Some(*i) != parent_column && !h.is_empty())
        .collect();

    let mut notes: Vec<String> = Vec::new();
    let mut unknown_kinds = BTreeSet::new();
    let mut unknown_keys = BTreeSet::new();
    let mut out: Vec<Row> = Vec::new();
    for (index, record) in rows.iter().enumerate() {
        // The line of the row in the file, for the notes: the header is line 1.
        let line = index + 2;
        let cell = |i: usize| record.get(i).map(|f| f.text.trim()).unwrap_or_default();
        let written = cell(name_column);
        let kind_text = kind_column.map(cell).filter(|k| !k.is_empty()).or(kind).unwrap_or_default();
        let kind_text = mapping
            .kinds
            .iter()
            .find(|(from, _)| *from == kind_text.to_lowercase())
            .map_or(kind_text, |(_, to)| to.as_str());
        let mut words: Vec<&str> = kind_text.split_whitespace().collect();
        let virtual_ = words.first() == Some(&"virtual") && words.len() > 1;
        if virtual_ {
            words.remove(0);
        }
        let kind = identifier(&words.join("_")).replace('-', "_");
        if written.is_empty() || words.is_empty() {
            notes.push(format!("line {line}: no {}, not imported", if written.is_empty() { "name" } else { "kind" }));
            continue;
        }
        let def = crate::schema::kind_def(&kind);
        if def.is_none() {
            unknown_kinds.insert(kind.clone());
        }
        let mut attributes = Vec::new();
        for &(i, key) in &keys {
            let value = cell(i);
            if value.is_empty() {
                continue;
            }
            let key = identifier(key).replace('-', "_");
            let attr = def.and_then(|d| crate::schema::resource_attr(d, &key));
            if attr.is_none() && def.is_some() {
                unknown_keys.insert(key.clone());
            }
            attributes.push((key, value_of(attr.map(|a| (&a.ty, a.card)), value)));
        }
        let parent = parent_column.map(cell).filter(|p| !p.is_empty()).map(str::to_owned);
        out.push(Row {
            line,
            virtual_,
            kind,
            written: written.to_owned(),
            name: identifier(written),
            parent,
            attributes,
            children: Vec::new(),
            nested: false,
        });
    }

    // The parents: by their name as written, or as an identifier.
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (index, row) in out.iter().enumerate() {
        by_name.entry(row.written.clone()).or_insert(index);
        by_name.entry(row.name.clone()).or_insert(index);
    }
    for index in 0..out.len() {
        let Some(parent) = out[index].parent.clone() else { continue };
        match by_name.get(&parent).or_else(|| by_name.get(&identifier(&parent))).copied() {
            Some(p) if p != index && !is_under(&out, p, index) => {
                out[p].children.push(index);
                out[index].nested = true;
            }
            Some(_) => {
                notes.push(format!("line {}: its parent `{parent}` is inside it, left at the root", out[index].line))
            }
            None => {}
        }
    }

    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem.strip_suffix(".csv").or_else(|| stem.strip_suffix(".CSV")).unwrap_or(stem);
    let mut text =
        format!("// Imported from {stem}.csv: review and complete (owners, flows), then move into the project.\n");
    if !unknown_kinds.is_empty() {
        let kinds: Vec<String> = unknown_kinds.into_iter().collect();
        let _ = writeln!(
            text,
            "// Kinds the language does not know, to declare in the schema of the project: {}.",
            kinds.join(", ")
        );
    }
    if !unknown_keys.is_empty() {
        let keys: Vec<String> = unknown_keys.into_iter().collect();
        let _ = writeln!(
            text,
            "// Keys the schema does not know, to declare in the project or to rename: {}.",
            keys.join(", ")
        );
    }
    for note in &notes {
        let _ = writeln!(text, "// {note}");
    }
    text.push('\n');
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    for index in (0..out.len()).filter(|&i| !out[i].nested) {
        write(&out, index, 0, &mut taken, &mut text);
        text.push('\n');
    }
    let path = format!("{}.ca", identifier(stem));
    Ok(vec![Imported { text: crate::format::format(&text).unwrap_or(text), path }])
}

/// Whether `node` is `ancestor` or under it.
fn is_under(rows: &[Row], node: usize, ancestor: usize) -> bool {
    node == ancestor || rows[ancestor].children.iter().any(|&c| is_under(rows, node, c))
}

/// A row and the rows nested in it; a name taken already among its siblings gets a number.
fn write(rows: &[Row], index: usize, depth: usize, taken: &mut BTreeMap<String, usize>, out: &mut String) {
    let row = &rows[index];
    let indent = "  ".repeat(depth);
    let count = taken.entry(row.name.clone()).or_insert(0);
    *count += 1;
    let name = if *count == 1 { row.name.clone() } else { format!("{}-{count}", row.name) };
    let virtual_ = if row.virtual_ { "virtual " } else { "" };
    let labelled = row.name != row.written && !row.attributes.iter().any(|(k, _)| k == "label");
    if !labelled && row.attributes.is_empty() && row.children.is_empty() {
        let _ = writeln!(out, "{indent}{virtual_}{} {name}", row.kind);
        return;
    }
    let _ = writeln!(out, "{indent}{virtual_}{} {name} {{", row.kind);
    if labelled {
        let _ = writeln!(out, "{indent}  label: {}", quoted(&row.written));
    }
    for (key, value) in &row.attributes {
        let _ = writeln!(out, "{indent}  {key}: {value}");
    }
    let mut inner = BTreeMap::new();
    for &child in &row.children {
        write(rows, child, depth + 1, &mut inner, out);
    }
    let _ = writeln!(out, "{indent}}}");
}

/// A value as written for a key of this type: a text quoted, a reference by its name as an
/// identifier, a list split on its commas, a number bare; an unknown key as it looks.
fn value_of(ty: Option<(&Ty, Card)>, value: &str) -> String {
    let many = matches!(ty, Some((_, Card::Many | Card::OneOrMore)));
    let items: Vec<&str> =
        if many { value.split(',').map(str::trim).filter(|v| !v.is_empty()).collect() } else { vec![value] };
    let one = |item: &str| match ty.map(|t| t.0) {
        Some(Ty::String | Ty::Strings | Ty::Url | Ty::Spdx | Ty::Pattern(_)) => quoted(item),
        Some(Ty::Ref(_)) => item.split('.').map(identifier).collect::<Vec<_>>().join("."),
        Some(Ty::Number) | Option::None if item.parse::<f64>().is_ok() => item.to_owned(),
        _ => literal(item),
    };
    match items.as_slice() {
        [item] => one(item),
        _ => format!("[{}]", items.iter().map(|i| one(i)).collect::<Vec<_>>().join(", ")),
    }
}
