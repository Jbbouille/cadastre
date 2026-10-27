//! `cadastre site` and `cadastre serve`: the site of a project for its readers (pages per view
//! and per resource, indexes, a search, the JSON model under `api/`), with the playground beside
//! it under `playground/` when one is found, for its authors.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cadastre_core::{Language, Options as CompileOptions, compile_modules, site};

/// The files of the playground the page needs; Graphviz for the browser, when missing, is loaded
/// from a CDN.
const PAGE: &[&str] = &[
    "index.html",
    "cadastre.js",
    "cadastre_wasm.wasm",
    "viz-standalone.js",
    "NOTICE.txt",
    "LICENSE-MIT",
    "LICENSE-APACHE",
];

/// Graphviz for the browser, when the playground does not ship it.
const VIZ_CDN: &str = "https://cdn.jsdelivr.net/npm/@viz-js/viz@3.11.0/lib/viz-standalone.js";

/// The playground: `--playground`, the `CADASTRE_PLAYGROUND` variable, or `playground/` beside
/// the executable (the release archives hold it); `None` when there is none: the site is built
/// without it.
pub fn playground(given: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let valid = |dir: &Path| dir.join("index.html").is_file() && dir.join("cadastre_wasm.wasm").is_file();
    if let Some(dir) = given {
        return if valid(dir) {
            Ok(Some(dir.to_owned()))
        } else {
            Err(format!("not a playground (index.html, cadastre_wasm.wasm): {}", dir.display()))
        };
    }
    let candidates = std::env::var_os("CADASTRE_PLAYGROUND")
        .map(PathBuf::from)
        .into_iter()
        .chain(std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join("playground"))));
    Ok(candidates.into_iter().find(|dir| valid(dir)))
}

/// The build of the WebAssembly module of a playground (`0.1.0 (a1b2c3d)`), read from its marker;
/// `None` for a module older than the marker.
pub fn playground_build(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(dir.join("cadastre_wasm.wasm")).ok()?;
    let marker = b"cadastre-build:";
    let at = bytes.windows(marker.len()).position(|w| w == marker)? + marker.len();
    let end = bytes[at..].iter().position(|&b| b == b')')? + at + 1;
    String::from_utf8(bytes[at..end].to_vec()).ok()
}

/// The name shown for a project: `--title`, otherwise its directory, or the parent of a `src`
/// directory.
pub fn title(root: &Path, given: Option<&str>) -> String {
    if let Some(title) = given {
        return title.to_owned();
    }
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_owned());
    let name = |path: &Path| path.file_name().map(|n| n.to_string_lossy().into_owned());
    match name(&root) {
        Some(n) if n == "src" => root.parent().and_then(name).unwrap_or(n),
        Some(n) => n,
        None => "Cadastre".to_owned(),
    }
}

/// `project.json` of the playground: `{ name, files: [{ path, text }], modules, site, language }`;
/// `site` is where the site the playground is published in starts, relative to the page (`../`).
fn project_json(project: &cadastre_project::Project, title: &str, language: Language) -> String {
    let files: Vec<_> = project.files.iter().map(|f| serde_json::json!({ "path": f.path, "text": f.text })).collect();
    let modules: Vec<_> = project
        .modules
        .iter()
        .map(|m| serde_json::json!({ "name": m.name, "files": [m.files.start, m.files.end] }))
        .collect();
    serde_json::json!({
        "name": title,
        "files": files,
        "modules": modules,
        "site": "../",
        "language": language.code(),
        "compiler": cadastre_core::version(),
    })
    .to_string()
}

/// Every file of the site, by its path from the root of the site: the pages and the API, the
/// playground with the project when there is one. The diagrams are laid out by Graphviz when it
/// is installed; otherwise the pages draw them in the browser.
pub fn generate(
    root: &Path,
    title: &str,
    playground: Option<&Path>,
    language: Option<Language>,
) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let project = cadastre_project::load(root).map_err(std::io::Error::other)?;
    let language = language.unwrap_or_else(|| project.language());
    let compilation =
        compile_modules(&project.files, &project.modules, CompileOptions { strict: project.strict(), language });
    let mut svgs = HashMap::new();
    for (view, dot) in site::dots(&compilation) {
        if let Some(svg) = crate::svg_of(&dot) {
            svgs.insert(view, svg);
        }
    }
    let viz = match playground {
        Some(dir) if dir.join("viz-standalone.js").is_file() => "playground/viz-standalone.js".to_owned(),
        _ => VIZ_CDN.to_owned(),
    };
    let options =
        site::Options { name: title, svgs: &svgs, viz: Some(&viz), playground: playground.is_some(), language };
    let mut files: Vec<(String, Vec<u8>)> = site::build(&compilation, &project.files, &options)
        .into_iter()
        .map(|f| (f.path, f.content.into_bytes()))
        .collect();
    if let Some(dir) = playground {
        for name in PAGE {
            if let Ok(bytes) = std::fs::read(dir.join(name)) {
                files.push((format!("playground/{name}"), bytes));
            }
        }
        files.push(("playground/project.json".to_owned(), project_json(&project, title, language).into_bytes()));
    }
    Ok(files)
}

/// `cadastre site`: the site in a folder, for any static web server.
pub fn site(
    root: &Path,
    out: &Path,
    playground: Option<&Path>,
    title: &str,
    language: Option<Language>,
) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for (path, content) in generate(root, title, playground, language)? {
        let target = out.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, content)?;
        written.push(target);
    }
    Ok(written)
}

/// The site generated once, kept for a moment: a page and its assets come from one generation.
struct Cache {
    at: Instant,
    files: HashMap<String, Vec<u8>>,
}

/// `cadastre serve`: the site, generated again from the files on disk when a request comes
/// more than a second after the previous generation.
pub fn serve(
    root: &Path,
    address: &str,
    playground: Option<&Path>,
    title: &str,
    language: Option<Language>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(address)?;
    println!("{title}: http://{address}/ (Ctrl+C to stop)");
    let cache: &'static Mutex<Option<Cache>> = Box::leak(Box::new(Mutex::new(None)));
    let (root, playground, title) = (root.to_owned(), playground.map(Path::to_owned), title.to_owned());
    let settings: &'static (PathBuf, Option<PathBuf>, String, Option<Language>) =
        Box::leak(Box::new((root, playground, title, language)));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        std::thread::spawn(move || {
            if let Err(error) = respond(stream, settings, cache) {
                eprintln!("request: {error}");
            }
        });
    }
    Ok(())
}

fn respond(
    mut stream: TcpStream,
    settings: &(PathBuf, Option<PathBuf>, String, Option<Language>),
    cache: &Mutex<Option<Cache>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // The headers, unused.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let path = target.split(['?', '#']).next().unwrap_or_default();
    let (status, kind, body) = if method != "GET" {
        ("405 Method Not Allowed", "text/plain", b"GET only".to_vec())
    } else {
        let mut name = path.trim_start_matches('/').to_owned();
        if name.is_empty() || name.ends_with('/') {
            name.push_str("index.html");
        }
        let name = percent_decode(&name);
        let (root, playground, title, language) = settings;
        let mut guard = cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.as_ref().is_none_or(|c| c.at.elapsed() > Duration::from_secs(1)) {
            let files = generate(root, title, playground.as_deref(), *language)?;
            *guard = Some(Cache { at: Instant::now(), files: files.into_iter().collect() });
        }
        match guard.as_ref().and_then(|c| c.files.get(&name)) {
            Some(body) => ("200 OK", content_type(&name), body.clone()),
            None => ("404 Not Found", "text/plain", b"not found".to_vec()),
        }
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()
}

/// `%20` and the like in a path, decoded.
fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        _ => "text/plain; charset=utf-8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(title(Path::new("x"), Some("Acme")), "Acme");
        let dir = std::env::temp_dir().join(format!("cadastre-title-{}", std::process::id())).join("si").join("src");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(title(&dir, None), "si");
        assert_eq!(title(dir.parent().unwrap(), None), "si");
        std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("resources/a%20b.html"), "resources/a b.html");
        assert_eq!(percent_decode("plain"), "plain");
    }
}
