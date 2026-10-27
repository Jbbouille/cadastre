//! SPDX license expressions (`license: "Apache-2.0 OR MIT"`): their syntax, and their
//! identifiers against the SPDX License List, embedded.
//!
//! ```text
//! expression := and ("OR" and)*
//! and        := with ("AND" with)*
//! with       := simple ("WITH" <exception>)?
//! simple     := <license>["+"] | LicenseRef-<id> | DocumentRef-<id>:LicenseRef-<id> | "(" expression ")"
//! ```

/// The license identifiers of the SPDX License List, deprecated ones included.
const LICENSES: &str = include_str!("spdx/licenses.txt");
/// The exception identifiers, after `WITH`.
const EXCEPTIONS: &str = include_str!("spdx/exceptions.txt");

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The expression does not follow the syntax: what was expected.
    Syntax(String),
    /// A license identifier that is not in the list, with the closest one.
    UnknownLicense(String, Option<&'static str>),
    UnknownException(String, Option<&'static str>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Syntax(what) => write!(f, "{what}"),
            Error::UnknownLicense(id, _) => write!(f, "{id} is not an SPDX license identifier"),
            Error::UnknownException(id, _) => write!(f, "{id} is not an SPDX license exception"),
        }
    }
}

impl Error {
    /// The closest known identifier, for a help message.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            Error::UnknownLicense(_, s) | Error::UnknownException(_, s) => *s,
            Error::Syntax(_) => None,
        }
    }
}

/// Checks an SPDX license expression.
pub fn check(expression: &str) -> Result<(), Error> {
    let tokens = tokens(expression);
    if tokens.is_empty() {
        return Err(Error::Syntax("an empty license expression".to_owned()));
    }
    let mut parser = Parser { tokens, at: 0 };
    parser.expression()?;
    match parser.tokens.get(parser.at) {
        None => Ok(()),
        Some(&word @ ("and" | "or" | "with")) => {
            Err(Error::Syntax(format!("`{word}`: the operators are written AND, OR, WITH")))
        }
        Some(token) => Err(Error::Syntax(format!("`{token}` after a complete expression"))),
    }
}

fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() || c == '(' || c == ')' {
            if let Some(s) = start.take() {
                out.push(&text[s..i]);
            }
            if !c.is_whitespace() {
                out.push(&text[i..i + 1]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
}

struct Parser<'t> {
    tokens: Vec<&'t str>,
    at: usize,
}

impl<'t> Parser<'t> {
    fn next(&mut self) -> Option<&'t str> {
        let token = self.tokens.get(self.at).copied();
        self.at += 1;
        token
    }

    fn eat(&mut self, word: &str) -> bool {
        let found = self.tokens.get(self.at) == Some(&word);
        if found {
            self.at += 1;
        }
        found
    }

    fn expression(&mut self) -> Result<(), Error> {
        self.and()?;
        while self.eat("OR") {
            self.and()?;
        }
        Ok(())
    }

    fn and(&mut self) -> Result<(), Error> {
        self.with()?;
        while self.eat("AND") {
            self.with()?;
        }
        Ok(())
    }

    fn with(&mut self) -> Result<(), Error> {
        self.simple()?;
        if self.eat("WITH") {
            let Some(exception) = self.next() else {
                return Err(Error::Syntax("an exception expected after WITH".to_owned()));
            };
            if !known(EXCEPTIONS, exception) {
                return Err(Error::UnknownException(exception.to_owned(), closest(EXCEPTIONS, exception)));
            }
        }
        Ok(())
    }

    fn simple(&mut self) -> Result<(), Error> {
        match self.next() {
            None => Err(Error::Syntax("a license expected at the end".to_owned())),
            Some("(") => {
                self.expression()?;
                match self.next() {
                    Some(")") => Ok(()),
                    _ => Err(Error::Syntax("`)` expected".to_owned())),
                }
            }
            Some(op @ (")" | "AND" | "OR" | "WITH")) => Err(Error::Syntax(format!("a license expected before `{op}`"))),
            Some(word) => {
                // A lowercase operator is a common slip: the operators are in capitals.
                if matches!(word, "and" | "or" | "with") {
                    return Err(Error::Syntax(format!("`{word}`: the operators are written AND, OR, WITH")));
                }
                let id = word.strip_suffix('+').unwrap_or(word);
                let reference = id
                    .split_once(':')
                    .map_or(id, |(document, license)| if document.starts_with("DocumentRef-") { license } else { id });
                if let Some(rest) = reference.strip_prefix("LicenseRef-") {
                    return if !rest.is_empty()
                        && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
                    {
                        Ok(())
                    } else {
                        Err(Error::Syntax(format!("{word}: LicenseRef- followed by letters, digits, `-` or `.`")))
                    };
                }
                if known(LICENSES, id) {
                    Ok(())
                } else {
                    Err(Error::UnknownLicense(id.to_owned(), closest(LICENSES, id)))
                }
            }
        }
    }
}

/// Identifiers match without regard to case.
fn known(list: &str, id: &str) -> bool {
    list.lines().any(|l| l.eq_ignore_ascii_case(id))
}

fn closest(list: &'static str, id: &str) -> Option<&'static str> {
    crate::suggest::closest(id, list.lines())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions() {
        for ok in [
            "MIT",
            "Apache-2.0",
            "apache-2.0",
            "GPL-2.0-or-later",
            "GPL-2.0+",
            "MIT OR Apache-2.0",
            "(MIT OR Apache-2.0) AND BSD-3-Clause",
            "GPL-2.0-only WITH Classpath-exception-2.0",
            "LicenseRef-Acme-Proprietary",
            "DocumentRef-spdx-tool-1.2:LicenseRef-MIT-Style-2",
        ] {
            assert_eq!(check(ok), Ok(()), "{ok}");
        }
        assert_eq!(check("Apache 2.0").unwrap_err().to_string(), "Apache is not an SPDX license identifier");
        assert_eq!(check("Apache-2").unwrap_err().suggestion(), Some("Apache-2.0"));
        assert_eq!(
            check("MIT or Apache-2.0").unwrap_err().to_string(),
            "`or`: the operators are written AND, OR, WITH"
        );
        assert_eq!(check("MIT AND").unwrap_err().to_string(), "a license expected at the end");
        assert_eq!(check("(MIT").unwrap_err().to_string(), "`)` expected");
        assert_eq!(check("").unwrap_err().to_string(), "an empty license expression");
        assert!(matches!(check("GPL-2.0 WITH Nope"), Err(Error::UnknownException(..))));
    }
}
