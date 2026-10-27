//! Outline: the symbols of `cadastre_core::outline`, as LSP document symbols.

use cadastre_core::outline::{Symbol, SymbolKind as Kind, outline};
use lsp_types::{DocumentSymbol, SymbolKind};

use crate::position::{Encoding, LineIndex};

pub fn document_symbols(text: &str, encoding: Encoding) -> Vec<DocumentSymbol> {
    let index = LineIndex::new(text);
    outline(text).into_iter().map(|symbol| convert(symbol, &index, encoding)).collect()
}

#[allow(deprecated)] // `deprecated` is a required field, superseded by `tags`.
fn convert(symbol: Symbol, index: &LineIndex, encoding: Encoding) -> DocumentSymbol {
    DocumentSymbol {
        name: symbol.name,
        detail: (!symbol.detail.is_empty()).then_some(symbol.detail),
        kind: kind(&symbol.kind),
        tags: None,
        deprecated: None,
        range: index.range(symbol.range.0, symbol.range.1, encoding),
        selection_range: index.range(symbol.selection.0, symbol.selection.1, encoding),
        children: Some(symbol.children.into_iter().map(|child| convert(child, index, encoding)).collect()),
    }
}

/// An icon for each family of resource kinds.
fn kind(kind: &Kind) -> SymbolKind {
    match kind {
        Kind::Resource(kind) => match kind.as_str() {
            "site" | "rack" | "vpc" | "network" | "subnet" | "fleet" => SymbolKind::NAMESPACE,
            "system" => SymbolKind::PACKAGE,
            "app" | "job" | "saas" | "component" => SymbolKind::CLASS,
            "database" | "data" => SymbolKind::STRUCT,
            "interface" => SymbolKind::INTERFACE,
            "channel" | "vpn_tunnel" | "vpc_peering" | "wan_link" => SymbolKind::EVENT,
            "actor" | "team" => SymbolKind::CONSTRUCTOR,
            _ => SymbolKind::OBJECT,
        },
        Kind::Link => SymbolKind::EVENT,
        Kind::View => SymbolKind::FUNCTION,
        Kind::Style => SymbolKind::PROPERTY,
    }
}
