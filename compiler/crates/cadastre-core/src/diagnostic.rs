//! Diagnostics: errors, warnings and incomplete elements (spec § 7).

use crate::source::{SourceFile, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    /// Worth knowing, without being wrong: a check of `severity: info` failing.
    Info,
    /// What is missing for a complete model, without being wrong.
    Incomplete,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Locations involved; the first one is the main location.
    pub spans: Vec<Span>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(severity: Severity, message: impl Into<String>, span: Span) -> Self {
        Self { severity, message: message.into(), spans: vec![span], help: None }
    }

    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Error, message, span)
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Warning, message, span)
    }

    pub fn info(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Info, message, span)
    }

    pub fn incomplete(message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Incomplete, message, span)
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.spans.push(span);
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Text rendering:
    /// ```text
    /// error: message
    ///   --> file.ca:3:5
    ///   help: …
    /// ```
    pub fn render(&self, files: &[SourceFile]) -> String {
        self.render_in(files, crate::i18n::Language::En)
    }

    /// The same, its words in a language (`Language::message`).
    pub fn render_in(&self, files: &[SourceFile], language: crate::i18n::Language) -> String {
        let label = language.text(match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
            Severity::Incomplete => "incomplete",
        });
        let mut out = format!("{label}: {}\n", language.message(&self.message));
        for span in &self.spans {
            let path = files.get(span.file.0 as usize).map_or("?", |f| f.path.as_str());
            out.push_str(&format!("  --> {path}:{}:{}\n", span.line + 1, span.column + 1));
        }
        if let Some(help) = &self.help {
            out.push_str(&format!("  {}: {}\n", language.text("help"), language.message(help)));
        }
        out
    }

    /// A copy whose message and help are in a language; English: the same.
    pub fn in_language(&self, language: crate::i18n::Language) -> Self {
        Self {
            severity: self.severity,
            message: language.message(&self.message),
            spans: self.spans.clone(),
            help: self.help.as_deref().map(|h| language.message(h)),
        }
    }
}

/// Number of diagnostics per severity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub incomplete: usize,
}

impl Summary {
    pub fn of(diagnostics: &[Diagnostic]) -> Self {
        let mut summary = Self::default();
        for d in diagnostics {
            match d.severity {
                Severity::Error => summary.errors += 1,
                Severity::Warning => summary.warnings += 1,
                Severity::Info => summary.infos += 1,
                Severity::Incomplete => summary.incomplete += 1,
            }
        }
        summary
    }
}
