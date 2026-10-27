//! Source files and positions.

/// Role of a file, from its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileRole {
    /// `.ca`: registry facts.
    Registry,
    /// `.cav`: plans (views and styles).
    Plans,
    /// `.cac`: checks, the tests of the project.
    Checks,
}

impl FileRole {
    pub fn from_path(path: &str) -> Option<Self> {
        if path.ends_with(".ca") {
            Some(Self::Registry)
        } else if path.ends_with(".cav") {
            Some(Self::Plans)
        } else if path.ends_with(".cac") {
            Some(Self::Checks)
        } else {
            None
        }
    }

    /// Whether the file holds plans: views, styles or checks, lowered by the plans module.
    pub fn is_plans(self) -> bool {
        matches!(self, Self::Plans | Self::Checks)
    }
}

/// A path relative to the file `from` (`src/sites/a.ca`), with `/` separators, `.` and `..`
/// resolved: `join("src/sites/a.ca", "../stores.csv")` is `src/stores.csv`.
pub fn join(from: &str, path: &str) -> String {
    let path = path.replace('\\', "/");
    let from = from.replace('\\', "/");
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        let mut parts: Vec<&str> = from.split('/').collect();
        parts.pop();
        parts
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let joined = parts.into_iter().filter(|p| !p.is_empty() && *p != ".").collect::<Vec<_>>().join("/");
    if path.starts_with('/') { format!("/{joined}") } else { joined }
}

/// A project file, already read by the host (CLI or browser).
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

/// Decodes the content of a file: Cadastre files are UTF-8 (a byte order mark is allowed,
/// and ignored by the parser).
pub fn decode(bytes: Vec<u8>) -> Result<String, InvalidUtf8> {
    String::from_utf8(bytes).map_err(|error| {
        let bytes = error.as_bytes();
        let offset = error.utf8_error().valid_up_to();
        let before = &bytes[..offset];
        let line = before.iter().filter(|&&b| b == b'\n').count();
        let column = offset - before.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        InvalidUtf8 { offset, line, column }
    })
}

/// Position of the first byte that is not valid UTF-8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidUtf8 {
    pub offset: usize,
    /// 0-based.
    pub line: usize,
    /// 0-based, in bytes.
    pub column: usize,
}

impl InvalidUtf8 {
    /// The diagnostic of a file that could not be decoded; the host compiles it as an empty file.
    pub fn diagnostic(&self, file: FileId) -> crate::Diagnostic {
        let span = Span { file, start: self.offset, end: self.offset + 1, line: self.line, column: self.column };
        crate::Diagnostic::error("this file is not valid UTF-8", span)
            .with_help("Cadastre files are UTF-8: convert it, for example with `iconv -f latin1 -t utf-8`")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_and_locates_invalid_bytes() {
        assert_eq!(decode(b"\xef\xbb\xbfserver a".to_vec()).unwrap(), "\u{feff}server a");
        // Latin-1 `é` on the second line, after `data `.
        let error = decode(b"team t\ndata caf\xe9".to_vec()).unwrap_err();
        assert_eq!(error, InvalidUtf8 { offset: 15, line: 1, column: 8 });
    }
}

/// Index of a file in the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// Position of an element: file, byte range, and start line / column (0-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: FileId,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn of(file: FileId, node: &tree_sitter::Node) -> Self {
        let point = node.start_position();
        Self { file, start: node.start_byte(), end: node.end_byte(), line: point.row, column: point.column }
    }

    /// The bytes `start..end` of a text, its line and column (in bytes, from 0) counted.
    pub fn at(file: FileId, text: &str, start: usize, end: usize) -> Self {
        let before = &text[..start.min(text.len())];
        let line = before.matches('\n').count();
        let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1);
        Self { file, start, end, line, column }
    }
}
