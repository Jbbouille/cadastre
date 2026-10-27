//! Semantic tokens: the captures of `highlights.scm`, as LSP token types.

use cadastre_core::highlight::highlight;
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend};

use crate::position::{Encoding, LineIndex, width};

const TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::TYPE,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::NUMBER,
    SemanticTokenType::DECORATOR,
    SemanticTokenType::STRING,
    SemanticTokenType::REGEXP,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::OPERATOR,
];

const MODIFIERS: &[SemanticTokenModifier] = &[SemanticTokenModifier::DEFAULT_LIBRARY];

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend { token_types: TYPES.to_vec(), token_modifiers: MODIFIERS.to_vec() }
}

/// Token type (index in [`TYPES`]) and modifier bitset of a capture name.
/// Punctuation is left to the editor's TextMate grammar.
fn classify(capture: &str) -> Option<(u32, u32)> {
    let ty = match capture {
        "comment" => SemanticTokenType::COMMENT,
        "type" => SemanticTokenType::TYPE,
        "variable" => SemanticTokenType::VARIABLE,
        "property" => SemanticTokenType::PROPERTY,
        "constant" | "constant.builtin" => SemanticTokenType::ENUM_MEMBER,
        "number" => SemanticTokenType::NUMBER,
        "label" => SemanticTokenType::DECORATOR,
        "string" => SemanticTokenType::STRING,
        "string.special" => SemanticTokenType::REGEXP,
        "function" => SemanticTokenType::FUNCTION,
        "keyword" | "keyword.operator" => SemanticTokenType::KEYWORD,
        "operator" => SemanticTokenType::OPERATOR,
        _ => return None,
    };
    let index = TYPES.iter().position(|t| *t == ty)? as u32;
    let modifiers = if capture == "constant.builtin" { 1 } else { 0 };
    Some((index, modifiers))
}

/// Semantic tokens of a whole text, delta-encoded, one token per line.
pub fn semantic_tokens(text: &str, encoding: Encoding) -> Vec<SemanticToken> {
    let index = LineIndex::new(text);
    let mut tokens = Vec::new();
    let (mut previous_line, mut previous_start) = (0, 0);

    for h in highlight(text) {
        let Some((token_type, modifiers)) = classify(h.name) else {
            continue;
        };
        // Multiline ranges (block comments) are split per line, which every client accepts.
        for line in index.line_of(h.start)..=index.line_of(h.end) {
            let (line_start, line_end) = index.line_range(line);
            let (start, end) = (h.start.max(line_start), h.end.min(line_end));
            if start >= end {
                continue;
            }
            let line = line as u32;
            let column = width(&text[line_start..start], encoding);
            tokens.push(SemanticToken {
                delta_line: line - previous_line,
                delta_start: if line == previous_line { column - previous_start } else { column },
                length: width(&text[start..end], encoding),
                token_type,
                token_modifiers_bitset: modifiers,
            });
            (previous_line, previous_start) = (line, column);
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, &'static str)> {
        let (mut line, mut start) = (0, 0);
        tokens
            .iter()
            .map(|t| {
                if t.delta_line > 0 {
                    start = 0;
                }
                line += t.delta_line;
                start += t.delta_start;
                (line, start, t.length, TYPES[t.token_type as usize].as_str())
            })
            .collect()
    }

    #[test]
    fn encodes_relative_positions() {
        let tokens = semantic_tokens("app crm {\n  on: web01\n}\n", Encoding::Utf16);
        assert_eq!(decode(&tokens), [(0, 0, 3, "type"), (0, 4, 3, "variable"), (1, 2, 2, "property")]);
    }

    #[test]
    fn splits_block_comments_per_line() {
        let tokens = semantic_tokens("/* été\r\n   fin */ app a\n", Encoding::Utf16);
        assert_eq!(
            decode(&tokens),
            [(0, 0, 6, "comment"), (1, 0, 9, "comment"), (1, 10, 3, "type"), (1, 14, 1, "variable")]
        );
    }

    #[test]
    fn marks_builtin_protocols() {
        let tokens = semantic_tokens("a -> b : https/443\n", Encoding::Utf16);
        let https = tokens.iter().find(|t| TYPES[t.token_type as usize] == SemanticTokenType::ENUM_MEMBER).unwrap();
        assert_eq!(https.token_modifiers_bitset, 1);
    }
}
