//! Language server for Cadastre (`cadastre lsp`), over stdin / stdout.
//!
//! A project is a directory: the workspace folder containing the file, or else the
//! file's own directory, compiled as a whole like `cadastre check` does. Open
//! documents take precedence over the files on disk.
//!
//! Features: diagnostics for every file of the project, semantic tokens from the
//! grammar's `highlights.scm`, outline, go to definition, hover, and for plans a
//! code lens on each view with two custom requests:
//!
//! - `cadastre/views` `{ uri }` → the view names of the project;
//! - `cadastre/render` `{ uri, view }` → `{ view, outputs: [{ format, content }], messages, ok }`,
//!   rendered from the open documents (`svg` outputs are DOT, for the client to lay out);
//! - `cadastre/icons` `{ svg }` → the SVG laid out from such DOT, with the icons of the kinds.

mod hover;
mod position;
mod symbols;
mod tokens;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};

use cadastre_core::outline::{SymbolKind as OutlineKind, outline};
use cadastre_core::registry::ResourceId;
use cadastre_core::source::{FileId, InvalidUtf8, decode};
use cadastre_core::{Compilation, Diagnostic as CompilerDiagnostic, Options, Severity, SourceFile, build};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Notification as _, PublishDiagnostics,
};
use lsp_types::request::{
    CodeLensRequest, Completion, DocumentHighlightRequest, DocumentSymbolRequest, Formatting, GotoDefinition,
    HoverRequest, PrepareRenameRequest, References, RegisterCapability, Rename, Request as _,
    SemanticTokensFullRequest,
};
use lsp_types::{
    CodeLens, CodeLensOptions, Command, CompletionItem, CompletionItemKind, CompletionOptions, CompletionResponse,
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeWatchedFilesRegistrationOptions,
    DocumentHighlight, DocumentHighlightKind, DocumentSymbolResponse, FileChangeType, FileSystemWatcher, GlobPattern,
    GotoDefinitionResponse, Hover, HoverContents, HoverProviderCapability, InitializeParams, Location, MarkupContent,
    MarkupKind, NumberOrString, OneOf, PositionEncodingKind, PublishDiagnosticsParams, Registration,
    RegistrationParams, SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextEdit, Url, WorkspaceEdit,
};
use serde_json::Value;

use crate::position::{Encoding, LineIndex};

/// Runs the server on stdin / stdout until the client asks it to exit.
pub fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();

    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let encoding = if params
        .capabilities
        .general
        .as_ref()
        .and_then(|g| g.position_encodings.as_ref())
        .is_some_and(|encodings| encodings.contains(&PositionEncodingKind::UTF8))
    {
        Encoding::Utf8
    } else {
        Encoding::Utf16
    };
    let capabilities = ServerCapabilities {
        position_encoding: Some(match encoding {
            Encoding::Utf8 => PositionEncodingKind::UTF8,
            Encoding::Utf16 => PositionEncodingKind::UTF16,
        }),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: tokens::legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                ..Default::default()
            },
        )),
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(lsp_types::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        document_highlight_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(false) }),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some([":", ">", "[", ","].map(String::from).to_vec()),
            ..Default::default()
        }),
        ..Default::default()
    };
    connection.initialize_finish(
        id,
        serde_json::json!({
            "capabilities": capabilities,
            "serverInfo": ServerInfo { name: "cadastre".into(), version: Some(env!("CARGO_PKG_VERSION").into()) },
        }),
    )?;

    let watch_files = params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|w| w.did_change_watched_files.as_ref())
        .and_then(|w| w.dynamic_registration)
        .unwrap_or(false);
    #[allow(deprecated)] // `root_uri` is the fallback of clients without workspace folders.
    let roots = match params.workspace_folders {
        Some(folders) => folders.iter().filter_map(|f| f.uri.to_file_path().ok()).collect(),
        None => params.root_uri.and_then(|uri| uri.to_file_path().ok()).into_iter().collect(),
    };

    // Codicons (`$(pass)`) in the titles of the code lenses: VS Code and its forks only.
    let codicons = params
        .client_info
        .as_ref()
        .is_none_or(|info| ["Code", "Cursor", "Windsurf"].iter().any(|n| info.name.contains(n)));
    let mut server = Server {
        connection: &connection,
        encoding,
        codicons,
        config: Config::default(),
        language: cadastre_core::Language::default(),
        roots,
        disk: HashMap::new(),
        undecodable: HashMap::new(),
        scanned: HashSet::new(),
        open: HashMap::new(),
        uris: HashMap::new(),
        published: HashMap::new(),
        projects: HashMap::new(),
        next_request: 0,
    };
    if let Some(options) = &params.initialization_options {
        server.config.update(options);
    }
    if watch_files {
        server.register_file_watcher()?;
    }
    for root in server.roots.clone() {
        server.compile(&root)?;
    }
    server.main_loop()?;

    // The writer thread stops once every sender is dropped.
    drop(server);
    drop(connection);
    io_threads.join()?;
    Ok(())
}

type Result<T, E = Box<dyn Error + Send + Sync>> = std::result::Result<T, E>;

/// Settings, from the initialization options or `workspace/didChangeConfiguration`.
#[derive(Clone, Copy, Debug)]
struct Config {
    /// Severity of the incomplete elements; `None` hides them.
    incomplete: Option<DiagnosticSeverity>,
    /// Incomplete elements are errors, like `cadastre check --strict`.
    strict: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { incomplete: Some(DiagnosticSeverity::HINT), strict: false }
    }
}

impl Config {
    /// Reads `{ diagnostics: { incomplete, strict } }`, possibly under a `cadastre` section.
    fn update(&mut self, settings: &Value) {
        let settings = settings.get("cadastre").unwrap_or(settings);
        if let Some(incomplete) = settings.pointer("/diagnostics/incomplete").and_then(Value::as_str) {
            self.incomplete = match incomplete {
                "off" => None,
                "information" => Some(DiagnosticSeverity::INFORMATION),
                "warning" => Some(DiagnosticSeverity::WARNING),
                _ => Some(DiagnosticSeverity::HINT),
            };
        }
        if let Some(strict) = settings.pointer("/diagnostics/strict").and_then(Value::as_bool) {
            self.strict = strict;
        }
    }
}

struct Server<'c> {
    connection: &'c Connection,
    encoding: Encoding,
    /// The client draws codicons in the titles of code lenses (`$(pass)`); otherwise these are
    /// replaced by characters.
    codicons: bool,
    config: Config,
    /// The language of the diagnostics shown: that of the `cadastre.toml` of the last project compiled.
    language: cadastre_core::Language,
    /// Workspace folders.
    roots: Vec<PathBuf>,
    /// Text on disk of the `.ca` / `.cav` files of the scanned projects; empty for a file that is
    /// not valid UTF-8.
    disk: HashMap<PathBuf, String>,
    /// Files on disk that are not valid UTF-8: where, and their valid text up to there.
    undecodable: HashMap<PathBuf, (InvalidUtf8, String)>,
    scanned: HashSet<PathBuf>,
    /// Text of the open documents.
    open: HashMap<PathBuf, String>,
    /// URIs as sent by the client, reused when publishing.
    uris: HashMap<PathBuf, Url>,
    /// Files with published diagnostics, per project root.
    published: HashMap<PathBuf, BTreeSet<PathBuf>>,
    /// Last compilation of each project, per root.
    projects: HashMap<PathBuf, Project>,
    next_request: i32,
}

/// A compiled project: its files (same order as the file ids) and the compilation.
struct Project {
    paths: Vec<PathBuf>,
    files: Vec<SourceFile>,
    compilation: Compilation,
}

impl Server<'_> {
    fn main_loop(&mut self) -> Result<()> {
        for message in &self.connection.receiver {
            match message {
                Message::Request(request) => {
                    if self.connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => self.handle_notification(notification)?,
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) -> Result<()> {
        let response = match request.method.as_str() {
            SemanticTokensFullRequest::METHOD => {
                let params: lsp_types::SemanticTokensParams = serde_json::from_value(request.params)?;
                let text = path_of(&params.text_document.uri).and_then(|path| self.text(&path));
                let data = text.map(|text| tokens::semantic_tokens(&text, self.encoding)).unwrap_or_default();
                let result = SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data });
                Response::new_ok(request.id, result)
            }
            DocumentSymbolRequest::METHOD => {
                let params: lsp_types::DocumentSymbolParams = serde_json::from_value(request.params)?;
                let text = path_of(&params.text_document.uri).and_then(|path| self.text(&path));
                let symbols = text.map(|text| symbols::document_symbols(&text, self.encoding)).unwrap_or_default();
                Response::new_ok(request.id, DocumentSymbolResponse::Nested(symbols))
            }
            GotoDefinition::METHOD => {
                let params: lsp_types::GotoDefinitionParams = serde_json::from_value(request.params)?;
                let position = params.text_document_position_params;
                let locations = path_of(&position.text_document.uri)
                    .map(|path| self.definitions(&path, position.position))
                    .unwrap_or_default();
                Response::new_ok(request.id, GotoDefinitionResponse::Array(locations))
            }
            References::METHOD => {
                let params: lsp_types::ReferenceParams = serde_json::from_value(request.params)?;
                let position = params.text_document_position;
                let declaration = params.context.include_declaration;
                let locations: Vec<Location> = path_of(&position.text_document.uri)
                    .map(|path| {
                        self.references(&path, position.position, declaration)
                            .into_iter()
                            .map(|(location, _)| location)
                            .collect()
                    })
                    .unwrap_or_default();
                Response::new_ok(request.id, locations)
            }
            Formatting::METHOD => {
                let params: lsp_types::DocumentFormattingParams = serde_json::from_value(request.params)?;
                // The whole document replaced by its canonical form; nothing for a document
                // already formatted, or with syntax errors (`cadastre_core::format`).
                let text = path_of(&params.text_document.uri).and_then(|path| self.text(&path));
                let edits: Vec<TextEdit> = text
                    .and_then(|text| {
                        let formatted = cadastre_core::format::format(&text).filter(|f| *f != text)?;
                        let range = LineIndex::new(&text).range(0, text.len(), self.encoding);
                        Some(vec![TextEdit::new(range, formatted)])
                    })
                    .unwrap_or_default();
                Response::new_ok(request.id, edits)
            }
            DocumentHighlightRequest::METHOD => {
                let params: lsp_types::DocumentHighlightParams = serde_json::from_value(request.params)?;
                let position = params.text_document_position_params;
                let uri = position.text_document.uri;
                // The occurrences of the name in this file: its declaration, then its uses.
                let highlights: Vec<DocumentHighlight> = path_of(&uri)
                    .map(|path| self.references(&path, position.position, true))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(location, _)| path_of(&location.uri) == path_of(&uri))
                    .map(|(location, declaration)| DocumentHighlight {
                        range: location.range,
                        kind: Some(if declaration {
                            DocumentHighlightKind::WRITE
                        } else {
                            DocumentHighlightKind::READ
                        }),
                    })
                    .collect();
                Response::new_ok(request.id, highlights)
            }
            // The name under the cursor, if it designates one resource: what F2 offers to rename.
            PrepareRenameRequest::METHOD => {
                let params: lsp_types::TextDocumentPositionParams = serde_json::from_value(request.params)?;
                let prepared = path_of(&params.text_document.uri)
                    .ok_or_else(|| "not a file".to_owned())
                    .and_then(|path| self.prepare_rename(&path, params.position));
                match prepared {
                    Ok(range) => Response::new_ok(request.id, range),
                    Err(message) => Response::new_err(request.id, ErrorCode::RequestFailed as i32, message),
                }
            }
            Rename::METHOD => {
                let params: lsp_types::RenameParams = serde_json::from_value(request.params)?;
                let position = params.text_document_position;
                let edit = path_of(&position.text_document.uri)
                    .ok_or_else(|| "not a file".to_owned())
                    .and_then(|path| self.rename(&path, position.position, &params.new_name));
                match edit {
                    Ok(edit) => Response::new_ok(request.id, edit),
                    Err(message) => Response::new_err(request.id, ErrorCode::RequestFailed as i32, message),
                }
            }
            HoverRequest::METHOD => {
                let params: lsp_types::HoverParams = serde_json::from_value(request.params)?;
                let position = params.text_document_position_params;
                let hover = path_of(&position.text_document.uri).and_then(|path| self.hover(&path, position.position));
                Response::new_ok(request.id, hover)
            }
            Completion::METHOD => {
                let params: lsp_types::CompletionParams = serde_json::from_value(request.params)?;
                let position = params.text_document_position;
                let items = match path_of(&position.text_document.uri) {
                    Some(path) => self.completions(&path, position.position)?,
                    None => Vec::new(),
                };
                Response::new_ok(request.id, CompletionResponse::Array(items))
            }
            CodeLensRequest::METHOD => {
                let params: lsp_types::CodeLensParams = serde_json::from_value(request.params)?;
                let uri = params.text_document.uri;
                let path = path_of(&uri).filter(|path| path.extension().is_some_and(|e| e == "cav" || e == "cac"));
                let lenses = match path.and_then(|path| self.text(&path).map(|text| (path, text))) {
                    Some((path, text)) => self.view_lenses(&uri, &path, &text)?,
                    None => Vec::new(),
                };
                Response::new_ok(request.id, lenses)
            }
            "cadastre/views" => {
                let params: ViewParams = serde_json::from_value(request.params)?;
                let views: Vec<String> = match path_of(&params.uri) {
                    Some(path) => self.project(&path)?.compilation.plans.views.iter().map(|v| v.name.clone()).collect(),
                    None => Vec::new(),
                };
                Response::new_ok(request.id, views)
            }
            // The result of a check, as text: its expectation, what it selects, what fails it.
            "cadastre/check" => {
                let params: ViewParams = serde_json::from_value(request.params)?;
                let name = params.view.unwrap_or_default();
                match path_of(&params.uri) {
                    Some(path) => {
                        let project = self.project(&path)?;
                        Response::new_ok(request.id, check_report(project, &name))
                    }
                    None => Response::new_err(request.id, ErrorCode::InvalidParams as i32, "not a file".into()),
                }
            }
            "cadastre/render" => {
                let params: ViewParams = serde_json::from_value(request.params)?;
                let view = params.view.unwrap_or_default();
                match path_of(&params.uri) {
                    Some(path) => {
                        let project = self.project(&path)?;
                        Response::new_ok(request.id, render(project, &view))
                    }
                    None => Response::new_err(request.id, ErrorCode::InvalidParams as i32, "not a file".into()),
                }
            }
            // An export of the whole registry; `null` when the project has errors.
            "cadastre/export" => {
                let params: ExportParams = serde_json::from_value(request.params)?;
                match path_of(&params.uri) {
                    Some(path) => {
                        let compilation = &self.project(&path)?.compilation;
                        let failed = compilation.diagnostics.iter().any(|d| d.severity == Severity::Error);
                        let content =
                            if failed { None } else { cadastre_core::export::export(compilation, &params.format) };
                        Response::new_ok(request.id, content)
                    }
                    None => Response::new_err(request.id, ErrorCode::InvalidParams as i32, "not a file".into()),
                }
            }
            // A draw.io file laid out by Graphviz: `layout`, the `-Tjson` output of the DOT of the view.
            "cadastre/drawio" => {
                let params: DrawioParams = serde_json::from_value(request.params)?;
                match path_of(&params.uri) {
                    Some(path) => {
                        let project = self.project(&path)?;
                        let file = cadastre_core::drawio(&project.compilation, &params.view, Some(&params.layout));
                        Response::new_ok(request.id, file)
                    }
                    None => Response::new_err(request.id, ErrorCode::InvalidParams as i32, "not a file".into()),
                }
            }
            "cadastre/icons" => {
                let params: IconsParams = serde_json::from_value(request.params)?;
                Response::new_ok(request.id, cadastre_core::render::with_icons(&params.svg))
            }
            _ => Response::new_err(
                request.id,
                ErrorCode::MethodNotFound as i32,
                format!("unsupported request: {}", request.method),
            ),
        };
        self.connection.sender.send(response.into())?;
        Ok(())
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<()> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
                if let Some(path) = path_of(&params.text_document.uri) {
                    self.uris.insert(path.clone(), params.text_document.uri);
                    self.open.insert(path.clone(), params.text_document.text);
                    self.compile_all(&path)?;
                }
            }
            DidChangeTextDocument::METHOD => {
                let params: lsp_types::DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
                // Full synchronization: the last change holds the whole text.
                if let (Some(path), Some(change)) = (path_of(&params.text_document.uri), params.content_changes.last())
                {
                    self.open.insert(path.clone(), change.text.clone());
                    self.compile_all(&path)?;
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: lsp_types::DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
                if let Some(path) = path_of(&params.text_document.uri) {
                    self.open.remove(&path);
                    self.compile_all(&path)?;
                }
            }
            DidSaveTextDocument::METHOD => {
                let params: lsp_types::DidSaveTextDocumentParams = serde_json::from_value(notification.params)?;
                if let Some(path) = path_of(&params.text_document.uri) {
                    self.read(&path);
                }
            }
            DidChangeWatchedFiles::METHOD => {
                let params: lsp_types::DidChangeWatchedFilesParams = serde_json::from_value(notification.params)?;
                let mut roots = BTreeSet::new();
                for change in params.changes {
                    let Some(path) = path_of(&change.uri) else { continue };
                    if change.typ == FileChangeType::DELETED {
                        self.disk.remove(&path);
                        self.undecodable.remove(&path);
                    } else {
                        self.read(&path);
                    }
                    roots.extend(self.roots_of(&path));
                }
                for root in roots {
                    self.compile(&root)?;
                }
            }
            DidChangeConfiguration::METHOD => {
                let params: lsp_types::DidChangeConfigurationParams = serde_json::from_value(notification.params)?;
                self.config.update(&params.settings);
                for root in self.published.keys().cloned().collect::<Vec<_>>() {
                    self.compile(&root)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Asks the client to report the changes of `.ca` / `.cav` files made outside the editor.
    fn register_file_watcher(&mut self) -> Result<()> {
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/{*.ca,*.cav,*.cac,*.csv,cadastre.toml}".into()),
                kind: None,
            }],
        };
        let params = RegistrationParams {
            registrations: vec![Registration {
                id: "cadastre-files".into(),
                method: DidChangeWatchedFiles::METHOD.into(),
                register_options: Some(serde_json::to_value(options)?),
            }],
        };
        self.next_request += 1;
        let request = Request::new(RequestId::from(self.next_request), RegisterCapability::METHOD.into(), params);
        self.connection.sender.send(request.into())?;
        Ok(())
    }

    /// The project of a file: the deepest workspace folder containing it, or its directory.
    /// An error the editor shows (a `cadastre.toml` that cannot be read).
    fn show_error(&self, message: &str) -> Result<()> {
        let params = lsp_types::ShowMessageParams { typ: lsp_types::MessageType::ERROR, message: message.to_owned() };
        let notification = Notification::new(lsp_types::notification::ShowMessage::METHOD.to_owned(), params);
        self.connection.sender.send(notification.into())?;
        Ok(())
    }

    /// The projects a file concerns: its own, and those that read it as a file of a module.
    fn roots_of(&self, path: &Path) -> BTreeSet<PathBuf> {
        let mut roots: BTreeSet<PathBuf> =
            self.projects.iter().filter(|(_, p)| p.paths.iter().any(|p| p == path)).map(|(r, _)| r.clone()).collect();
        roots.insert(self.root_of(path));
        roots
    }

    fn compile_all(&mut self, path: &Path) -> Result<()> {
        for root in self.roots_of(path) {
            self.compile(&root)?;
        }
        Ok(())
    }

    /// The project of a file: the directory of the `cadastre.toml` above it, otherwise the
    /// workspace folder holding it, otherwise its directory.
    fn root_of(&self, path: &Path) -> PathBuf {
        if let Some(root) = path.parent().and_then(cadastre_project::find_root) {
            return root;
        }
        self.roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.components().count())
            .cloned()
            .unwrap_or_else(|| path.parent().unwrap_or(path).to_path_buf())
    }

    /// Where the name at a position is declared, from the last compilation of its project.
    fn definitions(&self, path: &Path, position: lsp_types::Position) -> Vec<Location> {
        let Some((project, targets)) = self.targets(path, position) else {
            return Vec::new();
        };
        let registry = &project.compilation.registry;
        targets.into_iter().map(|id| self.location(project, registry.resource(id).name_span)).collect()
    }

    /// Where the resource named at a position is named, each place with whether it is its
    /// declaration. A name designating several resources (an ambiguity) gives the places of each.
    fn references(&self, path: &Path, position: lsp_types::Position, declaration: bool) -> Vec<(Location, bool)> {
        let Some((project, targets)) = self.targets(path, position) else {
            return Vec::new();
        };
        let registry = &project.compilation.registry;
        targets
            .into_iter()
            .flat_map(|id| {
                let declared = registry.resource(id).name_span;
                project.compilation.references(id, declaration).into_iter().map(move |span| (span, span == declared))
            })
            .map(|(span, is_declaration)| (self.location(project, span), is_declaration))
            .collect()
    }

    /// The resource named at a position, alone: a name designating several (an ambiguity) or
    /// none cannot be renamed.
    fn rename_target(&self, path: &Path, position: lsp_types::Position) -> Result<(&Project, ResourceId), String> {
        let (project, targets) = self.targets(path, position).ok_or_else(|| "not a file of a project".to_owned())?;
        match targets.as_slice() {
            [id] => Ok((project, *id)),
            [] => Err("no resource is named here".to_owned()),
            _ => Err("this name designates several resources: make it unambiguous first".to_owned()),
        }
    }

    /// The range of the name of the resource at a position, and its name.
    fn prepare_rename(
        &self,
        path: &Path,
        position: lsp_types::Position,
    ) -> Result<lsp_types::PrepareRenameResponse, String> {
        let (project, id) = self.rename_target(path, position)?;
        let name = project.compilation.registry.resource(id).name.clone();
        // The segment of the written name that names the resource: the last one.
        let file = project.paths.iter().position(|p| p == path).ok_or("not a file of a project")?;
        let text = &project.files[file].text;
        let offset = LineIndex::new(text).offset(position, self.encoding);
        let span = project
            .compilation
            .references(id, true)
            .into_iter()
            .find(|s| s.file.0 as usize == file && s.start <= offset && offset <= s.end)
            .ok_or("no resource is named here")?;
        let start = span.end - name.len().min(span.end - span.start);
        let range = LineIndex::new(text).range(start, span.end, self.encoding);
        Ok(lsp_types::PrepareRenameResponse::RangeWithPlaceholder { range, placeholder: name })
    }

    /// The edits renaming the resource at a position, in every file of its project.
    fn rename(&self, path: &Path, position: lsp_types::Position, name: &str) -> Result<WorkspaceEdit, String> {
        let (project, id) = self.rename_target(path, position)?;
        let edits = cadastre_core::rename::rename(&project.compilation, &project.files, id, name)?;
        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> = std::collections::HashMap::new();
        for edit in edits {
            let location = self.location(project, edit.span);
            changes.entry(location.uri).or_default().push(TextEdit::new(location.range, edit.text));
        }
        Ok(WorkspaceEdit { changes: Some(changes), ..Default::default() })
    }

    fn location(&self, project: &Project, span: cadastre_core::source::Span) -> Location {
        let text = &project.files[span.file.0 as usize].text;
        let range = LineIndex::new(text).range(span.start, span.end, self.encoding);
        Location::new(self.uri(&project.paths[span.file.0 as usize]), range)
    }

    /// The project of a file and the resources designated at a position, from its last compilation.
    fn targets(&self, path: &Path, position: lsp_types::Position) -> Option<(&Project, Vec<ResourceId>)> {
        let project = self.projects.get(&self.root_of(path))?;
        let file = project.paths.iter().position(|p| p == path)?;
        let offset = LineIndex::new(&project.files[file].text).offset(position, self.encoding);
        Some((project, project.compilation.definitions(FileId(file as u32), offset)))
    }

    fn hover(&self, path: &Path, position: lsp_types::Position) -> Option<Hover> {
        let (project, targets) = self.targets(path, position)?;
        let markdown = hover::describe(&project.compilation, &project.files, &targets)?;
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: markdown }),
            range: None,
        })
    }

    /// What may be written at a position: kinds, keys, values, resource names (`cadastre_core::complete`).
    fn completions(&mut self, path: &Path, position: lsp_types::Position) -> Result<Vec<CompletionItem>> {
        let Some(role) = path.to_str().and_then(cadastre_core::source::FileRole::from_path) else {
            return Ok(Vec::new());
        };
        let Some(text) = self.text(path) else { return Ok(Vec::new()) };
        let offset = LineIndex::new(&text).offset(position, self.encoding);
        let compilation = &self.project(path)?.compilation;
        let items = cadastre_core::complete::complete(compilation, role, &text, offset);
        Ok(items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                use cadastre_core::complete::ItemKind;
                CompletionItem {
                    kind: Some(match item.kind {
                        ItemKind::Kind => CompletionItemKind::CLASS,
                        ItemKind::Key => CompletionItemKind::PROPERTY,
                        ItemKind::Value => CompletionItemKind::ENUM_MEMBER,
                        ItemKind::Resource => CompletionItemKind::REFERENCE,
                        ItemKind::Keyword => CompletionItemKind::KEYWORD,
                    }),
                    detail: Some(item.detail),
                    insert_text: item.insert,
                    // The order of the compiler: precise kinds first, the declared order of keys.
                    sort_text: Some(format!("{i:04}")),
                    label: item.label,
                    ..Default::default()
                }
            })
            .collect())
    }

    /// "Preview" and "Build" above each view of a plans file; above each check, its result, which
    /// shows the detail. The client implements the commands.
    fn view_lenses(&mut self, uri: &Url, path: &Path, text: &str) -> Result<Vec<CodeLens>> {
        let index = LineIndex::new(text);
        let encoding = self.encoding;
        let codicons = self.codicons;
        let project = self.project(path)?;
        let lens = |range, title: String, command: &str, arguments: &Option<Vec<Value>>| CodeLens {
            range,
            command: Some(Command {
                title: if codicons { title } else { without_codicons(&title) },
                command: command.into(),
                arguments: arguments.clone(),
            }),
            data: None,
        };
        Ok(outline(text)
            .into_iter()
            .filter(|symbol| symbol.kind == OutlineKind::View)
            .flat_map(|block| {
                let range = index.range(block.selection.0, block.selection.1, encoding);
                let arguments = Some(vec![Value::from(uri.as_str()), Value::from(block.name.as_str())]);
                if block.detail == "check" {
                    vec![lens(range, check_title(project, &block.name), "cadastre.showCheck", &arguments)]
                } else {
                    vec![
                        lens(range, "$(open-preview) Preview".into(), "cadastre.previewView", &arguments),
                        lens(range, "$(run) Build".into(), "cadastre.buildView", &arguments),
                    ]
                }
            })
            .collect())
    }

    /// The compiled project of a file, compiling it first if needed.
    fn project(&mut self, path: &Path) -> Result<&Project> {
        let root = self.root_of(path);
        if !self.projects.contains_key(&root) {
            self.compile(&root)?;
        }
        Ok(&self.projects[&root])
    }

    fn text(&mut self, path: &Path) -> Option<String> {
        if let Some(text) = self.open.get(path).or_else(|| self.disk.get(path)) {
            return Some(text.clone());
        }
        std::fs::read(path).ok().and_then(|bytes| decode(bytes).ok())
    }

    /// Reads a file of a project from disk: a file that is not valid UTF-8 is compiled empty and
    /// reported, as by the command line.
    fn read(&mut self, path: &Path) {
        let Ok(bytes) = std::fs::read(path) else { return };
        match decode(bytes.clone()) {
            Ok(text) => {
                self.undecodable.remove(path);
                self.disk.insert(path.to_path_buf(), text);
            }
            Err(error) => {
                let valid = String::from_utf8_lossy(&bytes[..error.offset]).into_owned();
                self.undecodable.insert(path.to_path_buf(), (error, valid));
                self.disk.insert(path.to_path_buf(), String::new());
            }
        }
    }

    fn scan(&mut self, root: &Path) {
        if !self.scanned.insert(root.to_path_buf()) {
            return;
        }
        let mut paths = Vec::new();
        collect(root, &mut paths);
        for path in paths {
            if !self.disk.contains_key(&path) {
                self.read(&path);
            }
        }
    }

    /// Compiles a project and publishes the diagnostics of each of its files.
    fn compile(&mut self, root: &Path) -> Result<()> {
        // With a `cadastre.toml`: its sources and its modules, as `cadastre check` reads them.
        let (paths, names, modules, strict, language) = match cadastre_project::find_root(root).filter(|r| r == root) {
            Some(_) => match cadastre_project::load(root) {
                Ok(project) => {
                    for path in &project.paths {
                        if !self.disk.contains_key(path) {
                            self.read(path);
                        }
                    }
                    let names: Vec<String> = project.files.iter().map(|f| f.path.clone()).collect();
                    let strict = project.strict();
                    let language = project.language();
                    (project.paths, names, project.modules, strict, language)
                }
                Err(error) => {
                    self.show_error(&format!("Cadastre: {error}"))?;
                    return Ok(());
                }
            },
            None => {
                self.scan(root);
                let paths: BTreeSet<PathBuf> = self
                    .disk
                    .keys()
                    .chain(self.open.keys())
                    .filter(|path| path.starts_with(root) && is_cadastre(path))
                    .cloned()
                    .collect();
                let paths: Vec<PathBuf> = paths.into_iter().collect();
                let names: Vec<String> = paths
                    .iter()
                    .map(|path| path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/"))
                    .collect();
                (paths, names, Vec::new(), false, cadastre_core::Language::default())
            }
        };
        let files: Vec<SourceFile> = paths
            .iter()
            .zip(names)
            .map(|(path, name)| SourceFile {
                path: name,
                text: self.open.get(path).or_else(|| self.disk.get(path)).cloned().unwrap_or_default(),
            })
            .collect();

        self.language = language;
        let options = Options { strict: self.config.strict || strict, language };
        let mut compilation = cadastre_core::compile_modules(&files, &modules, options);
        let mut per_file: Vec<Vec<Diagnostic>> = vec![Vec::new(); paths.len()];
        for diagnostic in &compilation.diagnostics {
            let Some(span) = diagnostic.spans.first() else { continue };
            if let Some(converted) = self.convert(diagnostic, &paths, &files) {
                per_file[span.file.0 as usize].push(converted);
            }
        }
        // An open document is text sent by the editor: only files read from disk are undecodable.
        for (index, path) in paths.iter().enumerate() {
            let Some((error, valid)) = self.undecodable.get(path).filter(|_| !self.open.contains_key(path)) else {
                continue;
            };
            let diagnostic = error.diagnostic(FileId(index as u32));
            let mut message = diagnostic.message.clone();
            if let Some(help) = &diagnostic.help {
                message.push_str("\nhelp: ");
                message.push_str(help);
            }
            per_file[index].push(Diagnostic {
                range: LineIndex::new(valid).range(error.offset, error.offset, self.encoding),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("cadastre".into()),
                message,
                ..Default::default()
            });
            // The project has an error: views are not rendered, as with `cadastre build`.
            compilation.diagnostics.push(diagnostic);
        }

        let previous = self.published.insert(root.to_path_buf(), paths.iter().cloned().collect()).unwrap_or_default();
        for (path, diagnostics) in paths.iter().zip(per_file) {
            self.publish(path, diagnostics)?;
        }
        for path in previous.iter().filter(|path| !paths.contains(path)) {
            self.publish(path, Vec::new())?;
        }
        self.projects.insert(root.to_path_buf(), Project { paths, files, compilation });
        Ok(())
    }

    fn convert(&self, diagnostic: &CompilerDiagnostic, paths: &[PathBuf], files: &[SourceFile]) -> Option<Diagnostic> {
        let diagnostic = &diagnostic.in_language(self.language);
        let severity = match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
            Severity::Incomplete if self.config.strict => DiagnosticSeverity::ERROR,
            Severity::Incomplete => self.config.incomplete?,
        };
        let range = |span: &cadastre_core::source::Span| {
            let text = &files[span.file.0 as usize].text;
            LineIndex::new(text).range(span.start, span.end, self.encoding)
        };
        let mut message = diagnostic.message.clone();
        if let Some(help) = &diagnostic.help {
            message.push_str("\nhelp: ");
            message.push_str(help);
        }
        let related: Vec<DiagnosticRelatedInformation> = diagnostic.spans[1..]
            .iter()
            .map(|span| DiagnosticRelatedInformation {
                location: Location::new(self.uri(&paths[span.file.0 as usize]), range(span)),
                message: "related location".into(),
            })
            .collect();
        Some(Diagnostic {
            range: range(&diagnostic.spans[0]),
            severity: Some(severity),
            code: (diagnostic.severity == Severity::Incomplete).then(|| NumberOrString::String("incomplete".into())),
            source: Some("cadastre".into()),
            message,
            related_information: (!related.is_empty()).then_some(related),
            ..Default::default()
        })
    }

    fn uri(&self, path: &Path) -> Url {
        self.uris.get(path).cloned().or_else(|| Url::from_file_path(path).ok()).expect("project paths are absolute")
    }

    fn publish(&self, path: &Path, diagnostics: Vec<Diagnostic>) -> Result<()> {
        let params = PublishDiagnosticsParams::new(self.uri(path), diagnostics, None);
        let notification = Notification::new(PublishDiagnostics::METHOD.into(), params);
        self.connection.sender.send(notification.into())?;
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct IconsParams {
    svg: String,
}

#[derive(serde::Deserialize)]
struct ExportParams {
    uri: Url,
    format: String,
}

#[derive(serde::Deserialize)]
struct DrawioParams {
    uri: Url,
    view: String,
    layout: String,
}

#[derive(serde::Deserialize)]
struct ViewParams {
    uri: Url,
    view: Option<String>,
}

/// Whether the checks of a project run: not while it has errors (other than failing checks).
fn checks_run(project: &Project) -> bool {
    !project.compilation.diagnostics.iter().any(|d| d.severity == Severity::Error && !d.message.starts_with("check "))
}

/// The title of the lens above a check: its result, kept up to date as the project changes.
/// A title without codicons, for the clients other than VS Code: `$(pass) Passed` → `✓ Passed`.
fn without_codicons(title: &str) -> String {
    let Some(rest) = title.strip_prefix("$(") else { return title.to_owned() };
    let Some((icon, text)) = rest.split_once(") ") else { return title.to_owned() };
    let symbol = match icon {
        "pass" => "✓ ",
        "error" => "✗ ",
        "warning" => "⚠ ",
        "info" => "ℹ ",
        "circle-slash" => "⊘ ",
        "question" => "? ",
        "run" => "▶ ",
        _ => "",
    };
    format!("{symbol}{text}")
}

fn check_title(project: &Project, name: &str) -> String {
    if !checks_run(project) {
        return "$(circle-slash) Not run: the project has errors".into();
    }
    let Some(check) = project.compilation.plans.checks.iter().find(|c| c.name == name) else {
        return "$(question) Unknown check".into();
    };
    match cadastre_core::rules::outcome(&project.compilation, check) {
        None => "$(question) No selection or expectation".into(),
        Some(outcome) if outcome.passed => format!("$(pass) Passed · {}", outcome.summary),
        Some(outcome) if outcome.error.is_some() => format!("$(error) {}", outcome.summary),
        Some(outcome) if check.is_warning() => format!("$(warning) Failed · {}", outcome.summary),
        Some(outcome) if check.severity() == Severity::Info => format!("$(info) Failed · {}", outcome.summary),
        Some(outcome) => format!("$(error) Failed · {}", outcome.summary),
    }
}

/// The result of a check as text, for the output panel: each element with the file and line
/// it is declared at.
fn check_report(project: &Project, name: &str) -> Value {
    let compilation = &project.compilation;
    let Some(check) = compilation.plans.checks.iter().find(|c| c.name == name) else {
        return Value::from(format!("unknown check: {name}"));
    };
    let place = |span: cadastre_core::source::Span| {
        project
            .paths
            .get(span.file.0 as usize)
            .map(|path| format!("{}:{}:{}", path.display(), span.line + 1, span.column + 1))
            .unwrap_or_default()
    };
    let mut out = format!("check {}  ({})\n", check.name, place(check.span));
    if !checks_run(project) {
        out.push_str("  not run: the project has errors (see the Problems panel)\n");
        return Value::from(out);
    }
    let Some(outcome) = cadastre_core::rules::outcome(compilation, check) else {
        out.push_str("  nothing to run: a check selects nodes or edges and expects something of them\n");
        return Value::from(out);
    };
    let verdict = match (outcome.passed, outcome.error.is_some(), check.severity()) {
        (true, _, _) => "passed",
        (false, true, _) => "cannot be evaluated",
        (false, false, Severity::Warning) => "failed (warning)",
        (false, false, Severity::Info) => "failed (info)",
        (false, false, _) => "failed (error)",
    };
    out.push_str(&format!("  {verdict}: {}\n", outcome.summary));
    if let Some(message) = check.message() {
        out.push_str(&format!("  message: {message}\n"));
    }
    let width = outcome.selected.iter().map(|i| i.text.chars().count()).max().unwrap_or(0);
    let list = |out: &mut String, title: &str, items: &[cadastre_core::rules::Item]| {
        out.push_str(&format!("\n{title} ({}):\n", items.len()));
        for item in items {
            out.push_str(&format!("  {:width$}  {}\n", item.text, place(item.span)));
        }
    };
    if !outcome.offending.is_empty() && outcome.offending.len() != outcome.selected.len() {
        list(&mut out, "Not meeting the expectation", &outcome.offending);
    }
    if !outcome.selected.is_empty() {
        list(&mut out, "Selected", &outcome.selected);
    }
    Value::from(out)
}

/// Renders a view like `cadastre build --view`, without writing files: nothing when the project has errors.
fn render(project: &Project, view: &str) -> Value {
    let compilation = &project.compilation;
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.render(&project.files).trim_end().to_owned())
        .collect();
    if !errors.is_empty() {
        return serde_json::json!({ "view": view, "outputs": [], "messages": errors, "ok": false });
    }
    if !compilation.plans.views.iter().any(|v| v.name == view) {
        let message = format!("unknown view: {view}");
        return serde_json::json!({ "view": view, "outputs": [], "messages": [message], "ok": false });
    }
    let (outputs, diagnostics) = build(compilation, Some(view));
    // A draw.io output comes with the DOT of the view, for the client to lay it out (`cadastre/drawio`);
    // an xlsx one (in base64) with the table as HTML, to show it.
    let other = |format: &str| {
        let (outputs, _) = cadastre_core::build_with(compilation, Some(view), Some(&[format.to_owned()]));
        outputs.into_iter().next().map(|o| o.content)
    };
    let dot = || other("dot");
    let outputs: Vec<Value> = outputs
        .into_iter()
        .map(|o| match o.format.as_str() {
            "drawio" => serde_json::json!({ "format": o.format, "content": o.content, "dot": dot() }),
            "xlsx" => serde_json::json!({ "format": o.format, "content": o.content, "html": other("html") }),
            _ => serde_json::json!({ "format": o.format, "content": o.content }),
        })
        .collect();
    let messages: Vec<String> = diagnostics.iter().map(|d| d.render(&project.files).trim_end().to_owned()).collect();
    serde_json::json!({ "view": view, "outputs": outputs, "messages": messages, "ok": true })
}

fn path_of(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

fn is_cadastre(path: &Path) -> bool {
    cadastre_project::is_project_file(path)
}

/// The `.ca`, `.cav` and `.csv` files of a directory, recursively, like `cadastre check`.
fn collect(dir: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, paths);
        } else if is_cadastre(&path) {
            paths.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(registry: &str, plans: &str) -> Project {
        let files = vec![
            SourceFile { path: "a.ca".into(), text: registry.into() },
            SourceFile { path: "checks.cac".into(), text: plans.into() },
        ];
        let compilation = cadastre_core::compile(&files);
        Project { paths: vec![PathBuf::from("/p/a.ca"), PathBuf::from("/p/checks.cac")], files, compilation }
    }

    #[test]
    fn the_result_of_a_check() {
        let plans = "check owned {
  select app where not exists(owner)
  expect none
  severity: warning
  message: \"Every app has an owner\"
}
";
        let failing = project(
            "team t
app billing
app crm { owner: t }
",
            plans,
        );
        assert_eq!(check_title(&failing, "owned"), "$(warning) Failed · 1 node selected, none expected");
        assert_eq!(without_codicons(&check_title(&failing, "owned")), "⚠ Failed · 1 node selected, none expected");
        assert_eq!(without_codicons("$(open-preview) Preview"), "Preview");
        let report = check_report(&failing, "owned");
        let report = report.as_str().unwrap();
        assert!(report.contains("failed (warning): 1 node selected, none expected"), "{report}");
        assert!(report.contains("message: Every app has an owner"), "{report}");
        assert!(
            report.contains(
                "Selected (1):
  billing  /p/a.ca:2:5"
            ),
            "{report}"
        );

        let passing = project(
            "team t
app crm { owner: t }
",
            plans,
        );
        assert_eq!(check_title(&passing, "owned"), "$(pass) Passed · no node selected, none expected");
        let broken = project(
            "app crm { on: nowhere }
",
            plans,
        );
        assert_eq!(check_title(&broken, "owned"), "$(circle-slash) Not run: the project has errors");
    }

    #[test]
    fn reports_files_that_are_not_utf8() {
        let root = std::env::temp_dir().join(format!("cadastre-lsp-utf8-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.ca"), "team t\n").unwrap();
        // Latin-1 `é` on the second line, at character 8.
        std::fs::write(root.join("b.ca"), b"team u\ndata caf\xe9\n").unwrap();

        let (server_side, client_side) = Connection::memory();
        let mut server = Server {
            connection: &server_side,
            encoding: Encoding::Utf16,
            codicons: true,
            config: Config::default(),
            language: cadastre_core::Language::default(),
            roots: vec![root.clone()],
            disk: HashMap::new(),
            undecodable: HashMap::new(),
            scanned: HashSet::new(),
            open: HashMap::new(),
            uris: HashMap::new(),
            published: HashMap::new(),
            projects: HashMap::new(),
            next_request: 0,
        };
        server.compile(&root).unwrap();

        let mut published = HashMap::new();
        while let Ok(Message::Notification(n)) = client_side.receiver.try_recv() {
            let params: PublishDiagnosticsParams = serde_json::from_value(n.params).unwrap();
            published.insert(params.uri.path().rsplit('/').next().unwrap().to_owned(), params.diagnostics);
        }
        std::fs::remove_dir_all(&root).unwrap();

        assert!(published["a.ca"].is_empty(), "{:?}", published["a.ca"]);
        let b = &published["b.ca"];
        assert_eq!(b.len(), 1, "{b:?}");
        assert!(b[0].message.starts_with("this file is not valid UTF-8"), "{}", b[0].message);
        assert_eq!((b[0].range.start.line, b[0].range.start.character), (1, 8));
        // The project has an error: its views are not rendered.
        assert!(server.projects[&root].compilation.diagnostics.iter().any(|d| d.severity == Severity::Error));
    }
}
