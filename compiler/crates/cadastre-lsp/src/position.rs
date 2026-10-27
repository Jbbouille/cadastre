//! Byte offsets of the compiler ↔ LSP positions (UTF-16 by default, UTF-8 when negotiated).

use lsp_types::{Position, Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

/// Line starts of a text, to turn byte offsets into line / character positions.
pub struct LineIndex<'t> {
    text: &'t str,
    starts: Vec<usize>,
}

impl<'t> LineIndex<'t> {
    pub fn new(text: &'t str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self { text, starts }
    }

    /// Byte range of a line, without its line terminator.
    pub fn line_range(&self, line: usize) -> (usize, usize) {
        let start = self.starts[line];
        let end = self.starts.get(line + 1).map_or(self.text.len(), |&next| next - 1);
        let end = if end > start && self.text.as_bytes()[end - 1] == b'\r' { end - 1 } else { end };
        (start, end)
    }

    pub fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|&start| start <= offset) - 1
    }

    pub fn position(&self, offset: usize, encoding: Encoding) -> Position {
        let offset = floor_char_boundary(self.text, offset.min(self.text.len()));
        let line = self.line_of(offset);
        let prefix = &self.text[self.starts[line]..offset];
        Position::new(line as u32, width(prefix, encoding))
    }

    /// Byte offset of a position; positions past the end of a line are clamped to it.
    pub fn offset(&self, position: Position, encoding: Encoding) -> usize {
        if position.line as usize >= self.starts.len() {
            return self.text.len();
        }
        let (start, end) = self.line_range(position.line as usize);
        let mut units = 0;
        for (i, c) in self.text[start..end].char_indices() {
            if units >= position.character {
                return start + i;
            }
            units += match encoding {
                Encoding::Utf8 => c.len_utf8() as u32,
                Encoding::Utf16 => c.len_utf16() as u32,
            };
        }
        end
    }

    pub fn range(&self, start: usize, end: usize, encoding: Encoding) -> Range {
        Range::new(self.position(start, encoding), self.position(end, encoding))
    }
}

/// Length of a text in the units of the encoding.
pub fn width(text: &str, encoding: Encoding) -> u32 {
    match encoding {
        Encoding::Utf8 => text.len() as u32,
        Encoding::Utf16 => text.encode_utf16().count() as u32,
    }
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_offsets_to_positions() {
        let text = "app a\r\n// é\u{1F600}x\nend";
        let index = LineIndex::new(text);
        assert_eq!(index.position(0, Encoding::Utf16), Position::new(0, 0));
        assert_eq!(index.position(4, Encoding::Utf16), Position::new(0, 4));
        let x = text.find('x').unwrap();
        assert_eq!(index.position(x, Encoding::Utf16), Position::new(1, 6));
        assert_eq!(index.position(x, Encoding::Utf8), Position::new(1, 9));
        assert_eq!(index.position(text.len(), Encoding::Utf16), Position::new(2, 3));
        assert_eq!(index.line_range(0), (0, 5));
        assert_eq!(index.offset(Position::new(1, 6), Encoding::Utf16), x);
        assert_eq!(index.offset(Position::new(1, 9), Encoding::Utf8), x);
        assert_eq!(index.offset(Position::new(0, 99), Encoding::Utf16), 5);
        assert_eq!(index.offset(Position::new(9, 0), Encoding::Utf16), text.len());
    }
}
