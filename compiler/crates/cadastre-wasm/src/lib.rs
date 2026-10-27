//! WebAssembly entry point.
//!
//! A JSON-in, JSON-out interface with no imports, so that the module loads with a plain
//! `WebAssembly.instantiate`. The host:
//! 1. allocates a buffer with `alloc(len)` and writes the UTF-8 request into it;
//! 2. calls `build(ptr, len)`, which returns a pointer to the UTF-8 response;
//! 3. reads `result_len()` bytes at that pointer;
//! 4. frees its request buffer with `dealloc(ptr, len)`.
//!
//! `web/cadastre.js` wraps these steps.
//!
//! Request:
//! ```json
//! { "files": [{ "path": "infra.ca", "text": "…" }], "view": "network", "strict": false,
//!   "formats": ["svg", "text"] }
//! ```
//! `view`, `strict` and `formats` are optional; `formats` replaces the formats declared by
//! each view (`svg`, `dot`, `text`, `mermaid`…). `layout`, Graphviz's `-Tjson` output for the
//! DOT of the view, lays its `drawio` output out like the SVG.
//!
//! Response:
//! ```json
//! {
//!   "summary": { "errors": 0, "warnings": 1, "incomplete": 0, "resources": 36, "links": 5 },
//!   "views": ["flows", "network"],
//!   "diagnostics": [{ "severity": "warning", "message": "…", "help": null,
//!                     "locations": [{ "path": "views.cav", "line": 12, "column": 6 }] }],
//!   "outputs": [{ "view": "network", "format": "svg", "content": "digraph …" }]
//! }
//! ```
//! For the `svg` format, `content` is DOT: the page turns it into SVG with a WebAssembly
//! build of Graphviz.
//!
//! `highlight_text(ptr, len)` follows the same steps with the text of one file instead of a
//! request; it returns `[[start, end, "keyword"], …]`, with offsets in UTF-16 code units
//! (JavaScript string indices).

use std::cell::RefCell;

use cadastre_core::{Diagnostic, Module, Options, Severity, SourceFile, Summary, build_with, compile_modules};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Request {
    files: Vec<RequestFile>,
    #[serde(default)]
    view: Option<String>,
    #[serde(default)]
    strict: bool,
    /// Formats to render, instead of those declared by each view.
    #[serde(default)]
    formats: Option<Vec<String>>,
    /// Graphviz's `-Tjson` output for the DOT of the view: the layout of its `drawio` output.
    #[serde(default)]
    layout: Option<String>,
    /// An export of the whole registry (`archimate`, `graphml`, `json`), in `export` of the response.
    #[serde(default)]
    export: Option<String>,
    /// The language of the words of the diagrams (`en`, `fr`).
    #[serde(default)]
    language: Option<String>,
    /// The modules of the project, read only: their names and the range of their files.
    #[serde(default)]
    modules: Vec<RequestModule>,
}

#[derive(Deserialize)]
struct RequestModule {
    name: String,
    files: [usize; 2],
}

#[derive(Deserialize)]
struct RequestFile {
    path: String,
    text: String,
}

#[derive(Serialize)]
struct Response {
    summary: ResponseSummary,
    views: Vec<String>,
    diagnostics: Vec<ResponseDiagnostic>,
    outputs: Vec<ResponseOutput>,
    /// The export asked for, when the project has no error.
    #[serde(skip_serializing_if = "Option::is_none")]
    export: Option<String>,
}

#[derive(Serialize)]
struct ResponseSummary {
    errors: usize,
    warnings: usize,
    infos: usize,
    incomplete: usize,
    resources: usize,
    links: usize,
}

#[derive(Serialize)]
struct ResponseDiagnostic {
    severity: &'static str,
    message: String,
    help: Option<String>,
    locations: Vec<Location>,
}

#[derive(Serialize)]
struct Location {
    path: String,
    /// 1-based.
    line: usize,
    /// 1-based.
    column: usize,
}

#[derive(Serialize)]
struct ResponseOutput {
    view: String,
    format: String,
    content: String,
}

/// A one-off import of a Terraform state (`{ "terraform": "…" }`) or of an inventory in CSV
/// (`{ "csv": "…", "name": "servers.csv", "kind": "server", "map": "Hostname=name", "kinds": "VM=virtual
/// server" }`, `kind` for a file without a `kind` column, `map` and `kinds` for an export in its own
/// words): `{ "files": [{ "path", "text" }] }`, or `{ "error": "…" }`.
pub fn import_files(request: &str) -> String {
    #[derive(Deserialize)]
    struct ImportRequest {
        terraform: Option<String>,
        csv: Option<String>,
        #[serde(default)]
        name: String,
        kind: Option<String>,
        #[serde(default)]
        map: String,
        #[serde(default)]
        kinds: String,
    }
    let result = serde_json::from_str::<ImportRequest>(request).map_err(|e| format!("invalid request: {e}")).and_then(
        |r| match (r.terraform, r.csv) {
            (Some(terraform), _) => cadastre_core::terraform::import(&terraform),
            (None, Some(csv)) => cadastre_core::inventory::Mapping::parse(&r.map, &r.kinds)
                .and_then(|mapping| cadastre_core::inventory::import(&csv, &r.name, r.kind.as_deref(), &mapping)),
            (None, None) => Err("invalid request: terraform or csv expected".to_owned()),
        },
    );
    match result {
        Ok(files) => {
            let files: Vec<serde_json::Value> =
                files.into_iter().map(|f| serde_json::json!({ "path": f.path, "text": f.text })).collect();
            serde_json::json!({ "files": files }).to_string()
        }
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    }
}

/// Compiles a project and renders its views; the JSON request and response are described
/// in the module documentation.
pub fn run(request: &str) -> String {
    let request: Request = match serde_json::from_str(request) {
        Ok(request) => request,
        Err(error) => return serde_json::json!({ "error": format!("invalid request: {error}") }).to_string(),
    };
    let files: Vec<SourceFile> = request.files.into_iter().map(|f| SourceFile { path: f.path, text: f.text }).collect();
    let modules: Vec<Module> =
        request.modules.into_iter().map(|m| Module { name: m.name, files: m.files[0]..m.files[1] }).collect();
    let language = request.language.as_deref().and_then(cadastre_core::Language::parse).unwrap_or_default();
    let compilation = compile_modules(&files, &modules, Options { strict: request.strict, language });

    let mut diagnostics: Vec<Diagnostic> = compilation.diagnostics.iter().map(|d| d.in_language(language)).collect();
    let mut outputs = Vec::new();
    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
    let export = request
        .export
        .as_deref()
        .filter(|_| !has_errors)
        .and_then(|format| cadastre_core::export::export(&compilation, format));
    if !has_errors {
        let (rendered, view_diagnostics) =
            build_with(&compilation, request.view.as_deref(), request.formats.as_deref());
        diagnostics.extend(view_diagnostics.iter().map(|d| d.in_language(language)));
        outputs = rendered
            .into_iter()
            .map(|o| {
                let content = match (&request.layout, o.format.as_str()) {
                    (Some(layout), "drawio") => {
                        cadastre_core::drawio(&compilation, &o.view, Some(layout)).unwrap_or(o.content)
                    }
                    _ => o.content,
                };
                ResponseOutput { view: o.view, format: o.format, content }
            })
            .collect();
    }
    if request.strict {
        for d in &mut diagnostics {
            if d.severity == Severity::Incomplete {
                d.severity = Severity::Error;
            }
        }
    }
    diagnostics.sort_by_key(|d| (d.severity, d.spans[0].file, d.spans[0].start));

    let summary = Summary::of(&diagnostics);
    let response = Response {
        summary: ResponseSummary {
            errors: summary.errors,
            warnings: summary.warnings,
            infos: summary.infos,
            incomplete: summary.incomplete,
            resources: compilation.registry.resources.len(),
            links: compilation.registry.links.len(),
        },
        views: compilation.plans.views.iter().map(|v| v.name.clone()).collect(),
        diagnostics: diagnostics
            .into_iter()
            .map(|d| ResponseDiagnostic {
                severity: match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                    Severity::Incomplete => "incomplete",
                },
                message: d.message,
                help: d.help,
                locations: d
                    .spans
                    .iter()
                    .map(|s| Location {
                        path: files.get(s.file.0 as usize).map_or_else(String::new, |f| f.path.clone()),
                        line: s.line + 1,
                        column: s.column + 1,
                    })
                    .collect(),
            })
            .collect(),
        outputs,
        export,
    };
    serde_json::to_string(&response).expect("the response is serializable")
}

/// Highlights one file; the ranges are in UTF-16 code units, as JavaScript strings index them.
pub fn highlight(text: &str) -> String {
    let mut ranges = Vec::new();
    let (mut byte, mut unit) = (0, 0);
    let mut to_unit = |target: usize| {
        unit += text[byte..target].encode_utf16().count();
        byte = target;
        unit
    };
    for h in cadastre_core::highlight::highlight(text) {
        let start = to_unit(h.start);
        let end = to_unit(h.end);
        ranges.push((start, end, h.name));
    }
    serde_json::to_string(&ranges).expect("the ranges are serializable")
}

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Allocates `len` bytes in the module memory, for the host to write a request into.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// Frees a buffer obtained from `alloc`.
///
/// # Safety
///
/// `ptr` must come from `alloc(len)`, with the same `len`, and must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    // SAFETY: `ptr` comes from `alloc(len)`, which leaked a `Vec` of capacity `len.max(1)`.
    unsafe { drop(Vec::from_raw_parts(ptr, 0, len.max(1))) }
}

/// The version and commit of the module (`0.1.0 (a1b2c3d)`), compared by a published playground
/// with those of the compiler that published it; the request is ignored.
///
/// # Safety
///
/// Nothing is read: `ptr` and `len` are those of any request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn version(_ptr: *const u8, _len: usize) -> *const u8 {
    store(cadastre_core::version().to_owned())
}

/// Runs a build on the UTF-8 request at `ptr..ptr+len`; returns a pointer to the response,
/// valid until the next call, whose length is given by `result_len`.
///
/// # Safety
///
/// `ptr..ptr+len` must be readable: a buffer obtained from `alloc(len)` and filled by the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn build(ptr: *const u8, len: usize) -> *const u8 {
    // SAFETY: the host wrote `len` bytes at `ptr`, obtained from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let response = match std::str::from_utf8(bytes) {
        Ok(request) => run(request),
        Err(_) => serde_json::json!({ "error": "the request is not valid UTF-8" }).to_string(),
    };
    store(response)
}

/// Highlights the UTF-8 text at `ptr..ptr+len`; returns a pointer to the JSON ranges, valid
/// until the next call, whose length is given by `result_len`.
///
/// # Safety
///
/// `ptr..ptr+len` must be readable: a buffer obtained from `alloc(len)` and filled by the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn highlight_text(ptr: *const u8, len: usize) -> *const u8 {
    // SAFETY: the host wrote `len` bytes at `ptr`, obtained from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    store(highlight(&String::from_utf8_lossy(bytes)))
}

/// Draws the icons of the kinds in the SVG at `ptr..ptr+len`, laid out by Graphviz from the
/// DOT of a `svg` output; returns a pointer to the new SVG, valid until the next call, whose
/// length is given by `result_len`.
///
/// # Safety
///
/// `ptr..ptr+len` must be readable: a buffer obtained from `alloc(len)` and filled by the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn with_icons(ptr: *const u8, len: usize) -> *const u8 {
    // SAFETY: the host wrote `len` bytes at `ptr`, obtained from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    store(cadastre_core::render::with_icons(&String::from_utf8_lossy(bytes)))
}

/// The variants of the DOT of a view to lay out (`dot_variants`), as a JSON array of strings.
///
/// # Safety
///
/// `ptr..ptr+len` must be readable: a buffer obtained from `alloc(len)` and filled by the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dot_variants(ptr: *const u8, len: usize) -> *const u8 {
    // SAFETY: the host wrote `len` bytes at `ptr`, obtained from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let variants = cadastre_core::render::dot_variants(&String::from_utf8_lossy(bytes));
    store(serde_json::to_string(&variants).unwrap_or_else(|_| "[]".to_owned()))
}

/// Several SVG drawings of a view in one text, as laid out from the variants of its DOT
/// (`dot_variants`): the one that reads best.
///
/// # Safety
///
/// `ptr..ptr+len` must be readable: a buffer obtained from `alloc(len)` and filled by the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn best_drawing(ptr: *const u8, len: usize) -> *const u8 {
    // SAFETY: the host wrote `len` bytes at `ptr`, obtained from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    store(cadastre_core::render::best_of(&String::from_utf8_lossy(bytes)))
}

fn store(response: String) -> *const u8 {
    RESULT.with(|result| {
        let mut result = result.borrow_mut();
        *result = response.into_bytes();
        result.as_ptr()
    })
}

/// Imports a Terraform state: the UTF-8 request at `ptr..ptr+len`, as for `build`.
///
/// # Safety
///
/// `ptr..ptr+len` must be readable: a buffer obtained from `alloc(len)` and filled by the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn import(ptr: *const u8, len: usize) -> *const u8 {
    // SAFETY: the host wrote `len` bytes at `ptr`, obtained from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    store(import_files(&String::from_utf8_lossy(bytes)))
}

/// Length of the last response returned by `build`, `highlight_text`, `with_icons` or `best_drawing`.
#[unsafe(no_mangle)]
pub extern "C" fn result_len() -> usize {
    RESULT.with(|result| result.borrow().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_project() {
        let request = serde_json::json!({
            "files": [
                { "path": "a.ca", "text": "team t\napp a { owner: t, on: x, -> b : https/443 [flow] }\napp b { owner: t, on: x }\nfleet x" },
                { "path": "v.cav", "text": "view flows { select app edges [flow] render svg, text }" }
            ]
        });
        let response: serde_json::Value = serde_json::from_str(&run(&request.to_string())).unwrap();
        assert_eq!(response["summary"]["errors"], 0, "{response}");
        assert_eq!(response["views"], serde_json::json!(["flows"]));
        let outputs = response["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 2);
        assert!(outputs[0]["content"].as_str().unwrap().starts_with("digraph"));
        assert!(outputs[1]["content"].as_str().unwrap().contains("a -> b"));
    }

    #[test]
    fn reports_diagnostics_with_locations() {
        let request = serde_json::json!({ "files": [{ "path": "a.ca", "text": "serveur web01" }] });
        let response: serde_json::Value = serde_json::from_str(&run(&request.to_string())).unwrap();
        let diagnostic = &response["diagnostics"][0];
        assert_eq!(diagnostic["severity"], "error");
        assert_eq!(diagnostic["help"], "did you mean `server`?");
        assert_eq!(diagnostic["locations"][0], serde_json::json!({ "path": "a.ca", "line": 1, "column": 1 }));
        assert!(response["outputs"].as_array().unwrap().is_empty());
    }

    #[test]
    fn highlights_with_utf16_offsets() {
        // `é` is two bytes but one UTF-16 unit, `🔥` four bytes but two units.
        let ranges: Vec<(usize, usize, String)> = serde_json::from_str(&highlight(
            "// é🔥
server web01",
        ))
        .unwrap();
        assert_eq!(ranges[0], (0, 6, "comment".to_owned()));
        assert_eq!(ranges[1], (7, 13, "type".to_owned()));
        assert_eq!(ranges[2], (14, 19, "variable".to_owned()));
    }

    #[test]
    fn rejects_invalid_requests() {
        let response: serde_json::Value = serde_json::from_str(&run("{")).unwrap();
        assert!(response["error"].as_str().unwrap().starts_with("invalid request"));
    }
}
