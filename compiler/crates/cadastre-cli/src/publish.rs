//! `cadastre site` and `cadastre serve`: the site of a project for its readers (pages per view
//! and per resource, indexes, a search, the JSON model under `api/`), with the playground beside
//! it under `playground/` when one is found, for its authors.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cadastre_core::{Language, Options as CompileOptions, site};

/// The files of the playground the page needs; Graphviz for the browser, when missing, is loaded
/// from a CDN.
const PAGE: &[&str] = &[
    "index.html",
    "compose.html",
    "compose.js",
    "cadastre.js",
    "movable.js",
    "zoom.js",
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

/// `project.json` of the playground: `{ name, files: [{ path, text }], modules, site, language,
/// satellites, edit }`;
/// `site` is where the site the playground is published in starts, relative to the page (`../`);
/// `edit`, whether the composer of views may write its views into the files (`serve --edit`).
fn project_json(project: &cadastre_project::Project, title: &str, language: Language, edit: bool) -> String {
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
        "satellites": project.satellites(),
        "compiler": cadastre_core::version(),
        "edit": edit,
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
    generate_with(&project, title, playground, language, false, &mut Drawings::default())
}

/// The drawings of the DOTs laid out by Graphviz, by DOT: kept by `serve` from one generation
/// to the next, so that after an edit only the views it changes are laid out again.
#[derive(Default)]
pub struct Drawings {
    /// The SVG of a view and the DOT of the variant kept.
    svgs: HashMap<String, Option<(String, String)>>,
    /// Graphviz's JSON layout of a DOT, for the view to move by hand.
    layouts: HashMap<String, Option<String>>,
}

impl Drawings {
    /// The drawings of these DOTs, those not kept laid out now; only these are kept after.
    fn svgs(&mut self, dots: &[&str]) -> Vec<Option<(String, String)>> {
        let missing: Vec<&str> = dots.iter().copied().filter(|d| !self.svgs.contains_key(*d)).collect();
        for (dot, drawing) in missing.iter().zip(crate::drawings_of(&missing)) {
            self.svgs.insert((*dot).to_owned(), drawing);
        }
        self.svgs.retain(|dot, _| dots.contains(&dot.as_str()));
        dots.iter().map(|d| self.svgs[*d].clone()).collect()
    }

    fn layouts(&mut self, dots: &[&str]) -> Vec<Option<String>> {
        let missing: Vec<&str> = dots.iter().copied().filter(|d| !self.layouts.contains_key(*d)).collect();
        for (dot, layout) in missing.iter().zip(crate::graphviz_all(&missing, "-Tjson")) {
            self.layouts.insert((*dot).to_owned(), layout);
        }
        self.layouts.retain(|dot, _| dots.contains(&dot.as_str()));
        dots.iter().map(|d| self.layouts[*d].clone()).collect()
    }
}

/// [`generate`] for a project loaded, its drawings taken from `drawings` when their DOT is there.
fn generate_with(
    project: &cadastre_project::Project,
    title: &str,
    playground: Option<&Path>,
    language: Option<Language>,
    edit: bool,
    drawings: &mut Drawings,
) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let language = language.unwrap_or_else(|| project.language());
    let (compilation, history) =
        project.compile(CompileOptions { strict: project.strict(), language, satellites: project.satellites() });
    for warning in history {
        eprintln!("warning: {warning}");
    }
    let dots = site::dots(&compilation);
    let laid_out = drawings.svgs(&dots.iter().map(|(_, dot)| dot.as_str()).collect::<Vec<_>>());
    // The views to move by hand: the variant of each diagram laid out again, as JSON.
    let kept: Vec<(&str, &str)> = dots
        .iter()
        .zip(&laid_out)
        .filter_map(|((view, _), drawing)| drawing.as_ref().map(|(_, dot)| (view.as_str(), dot.as_str())))
        .collect();
    let layouts = drawings.layouts(&kept.iter().map(|(_, dot)| *dot).collect::<Vec<_>>());
    let elements: HashMap<String, String> = kept
        .iter()
        .zip(layouts)
        .filter_map(|((view, _), layout)| {
            Some(((*view).to_owned(), cadastre_core::cytoscape(&compilation, view, Some(&layout?))?))
        })
        .collect();
    let svgs: HashMap<String, String> =
        dots.into_iter().zip(laid_out).filter_map(|((view, _), drawing)| Some((view, drawing?.0))).collect();
    let viz = match playground {
        Some(dir) if dir.join("viz-standalone.js").is_file() => "playground/viz-standalone.js".to_owned(),
        _ => VIZ_CDN.to_owned(),
    };
    let options = site::Options {
        name: title,
        svgs: &svgs,
        elements: &elements,
        viz: Some(&viz),
        playground: playground.is_some(),
        language,
    };
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
        files.push(("playground/project.json".to_owned(), project_json(project, title, language, edit).into_bytes()));
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

/// The site generated, kept as long as the sources of the project are the same.
struct Cache {
    /// The fingerprint of the sources it was generated from ([`sources`]).
    sources: u64,
    files: HashMap<String, Vec<u8>>,
    drawings: Drawings,
}

/// A fingerprint of the sources of a project: its files and those of its modules (the `.csv`
/// read by `for_each` among them) and its `cadastre.toml`.
fn sources(project: &cadastre_project::Project) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in &project.files {
        (&file.path, &file.text).hash(&mut hasher);
    }
    std::fs::read(project.root.join("cadastre.toml")).ok().hash(&mut hasher);
    hasher.finish()
}

/// What `serve` serves: the site generated last, and the fingerprint of the sources of a
/// generation under way, if any.
#[derive(Default)]
struct Served {
    cache: Option<Cache>,
    building: Option<u64>,
}

/// `cadastre serve`: the site, generated again from the files on disk when they change; the
/// views whose DOT did not change keep their drawings. A change is generated in the background,
/// the previous site served meanwhile; its pages reload themselves when the new one is there
/// (they ask `/__version`). With `edit`, the composer of views writes its views into the files of
/// the project (`POST /__edit`).
pub fn serve(
    root: &Path,
    address: &str,
    playground: Option<&Path>,
    title: &str,
    language: Option<Language>,
    edit: bool,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(address)?;
    println!("{title}: http://{address}/ (Ctrl+C to stop)");
    let served: &'static Mutex<Served> = Box::leak(Box::new(Mutex::new(Served::default())));
    let (root, playground, title) = (root.to_owned(), playground.map(Path::to_owned), title.to_owned());
    let settings: &'static Settings = Box::leak(Box::new((root, playground, title, language, edit)));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        std::thread::spawn(move || {
            if let Err(error) = respond(stream, settings, served) {
                eprintln!("request: {error}");
            }
        });
    }
    Ok(())
}

type Settings = (PathBuf, Option<PathBuf>, String, Option<Language>, bool);

/// The site to serve, generated first if there is none, generated again in the background when
/// the sources changed.
fn current(settings: &'static Settings, served: &'static Mutex<Served>) -> std::io::Result<()> {
    let (root, playground, title, language, edit) = settings;
    let mut state = served.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // The files read again at each request (a few milliseconds), the site generated again only
    // when they changed.
    let project = cadastre_project::load(root).map_err(std::io::Error::other)?;
    let fingerprint = sources(&project);
    let Served { cache, building } = &mut *state;
    match cache {
        None => {
            let mut drawings = Drawings::default();
            let files = generate_with(&project, title, playground.as_deref(), *language, *edit, &mut drawings)?;
            *cache = Some(Cache { sources: fingerprint, files: files.into_iter().collect(), drawings });
        }
        Some(last) if last.sources != fingerprint && building.is_none() => {
            *building = Some(fingerprint);
            let mut drawings = std::mem::take(&mut last.drawings);
            std::thread::spawn(move || {
                let generated = generate_with(&project, title, playground.as_deref(), *language, *edit, &mut drawings);
                let mut state = served.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.building = None;
                match generated {
                    Ok(files) => {
                        state.cache = Some(Cache { sources: fingerprint, files: files.into_iter().collect(), drawings })
                    }
                    Err(error) => eprintln!("generation: {error}"),
                }
            });
        }
        Some(_) => {}
    }
    Ok(())
}

fn respond(mut stream: TcpStream, settings: &'static Settings, served: &'static Mutex<Served>) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
        line.clear();
    }
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let path = target.split(['?', '#']).next().unwrap_or_default();
    let (status, kind, body) = if method == "POST" && path == "/__edit" && settings.4 {
        let length: usize = headers.get("content-length").and_then(|l| l.parse().ok()).unwrap_or(0);
        let mut body = vec![0; length.min(1 << 20)];
        reader.read_exact(&mut body)?;
        let (status, answer) = match edit_request(&settings.0, &headers, &body) {
            Ok(answer) => ("200 OK", answer),
            Err((status, error)) => (status, serde_json::json!({ "error": error })),
        };
        (status, "application/json; charset=utf-8", answer.to_string().into_bytes())
    } else if method != "GET" {
        ("405 Method Not Allowed", "text/plain", b"GET only".to_vec())
    } else {
        let mut name = path.trim_start_matches('/').to_owned();
        if name.is_empty() || name.ends_with('/') {
            name.push_str("index.html");
        }
        let name = percent_decode(&name);
        current(settings, served)?;
        let state = served.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match (name.as_str(), &state.cache) {
            // The version of the site served: a page reloads itself when it changes.
            ("__version", Some(cache)) => ("200 OK", "text/plain", cache.sources.to_string().into_bytes()),
            (_, cache) => match cache.as_ref().and_then(|c| c.files.get(&name)) {
                Some(body) => ("200 OK", content_type(&name), body.clone()),
                None => ("404 Not Found", "text/plain", b"not found".to_vec()),
            },
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

/// `POST /__edit` (`serve --edit`): a view of the composer written into its file,
/// `{ "path": "views.cav", "replaces": "flows", "text": "view flows { … }" }`, in place of the
/// view `replaces`, or at the end of the file (a new one, or one renamed away). Answered for the
/// pages of the server only: its JSON body and its `Origin`, that of the server, keep another
/// site from writing through the browser of a reader. Answers the path and the text written.
fn edit_request(
    root: &Path,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> Result<serde_json::Value, (&'static str, String)> {
    let origin = headers.get("origin").map(String::as_str).unwrap_or_default();
    let host = headers.get("host").map(String::as_str).unwrap_or_default();
    if host.is_empty() || origin != format!("http://{host}") {
        return Err(("403 Forbidden", "only the pages of this server may write".to_owned()));
    }
    if !headers.get("content-type").is_some_and(|t| t.starts_with("application/json")) {
        return Err(("415 Unsupported Media Type", "a JSON body is expected".to_owned()));
    }
    let request: serde_json::Value = serde_json::from_slice(body).map_err(|e| ("400 Bad Request", e.to_string()))?;
    let (Some(path), Some(text)) = (request["path"].as_str(), request["text"].as_str()) else {
        return Err(("400 Bad Request", "path and text are expected".to_owned()));
    };
    let written = write_view(root, path, request["replaces"].as_str(), text).map_err(|e| ("400 Bad Request", e))?;
    Ok(serde_json::json!({ "path": path, "text": written }))
}

/// Writes the view `text` into the `.cav` file `path` of the project at `root`: in place of the
/// view `replaces`, otherwise at its end. Returns the text of the file written.
pub fn write_view(root: &Path, path: &str, replaces: Option<&str>, text: &str) -> Result<String, String> {
    let relative = Path::new(path);
    if path.is_empty()
        || relative.extension().is_none_or(|e| e != "cav")
        || !relative.components().all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!("not a .cav file of the project: {path}"));
    }
    let views = cadastre_core::compose::view_ranges(text);
    let [(name, range)] = views.as_slice() else {
        return Err("one view is expected, and nothing else".to_owned());
    };
    if text[..range.start].trim() != "" || text[range.end..].trim() != "" {
        return Err("one view is expected, and nothing else".to_owned());
    }
    let file = root.join(relative);
    let current = match std::fs::read_to_string(&file) {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && file.parent().is_some_and(Path::is_dir) => {
            String::new()
        }
        Err(error) => return Err(format!("{path}: {error}")),
    };
    let block = &text[range.clone()];
    let ranges = cadastre_core::compose::view_ranges(&current);
    let replaced = replaces.and_then(|r| ranges.iter().find(|(n, _)| n == r));
    if ranges.iter().any(|(n, _)| n == name && replaced.is_none_or(|(r, _)| r != name)) {
        return Err(format!("{path} already has a view named {name}"));
    }
    let written = match replaced {
        Some((_, at)) => format!("{}{block}{}", &current[..at.start], &current[at.end..]),
        None if current.trim().is_empty() => format!("{block}\n"),
        None => format!("{}\n\n{block}\n", current.trim_end()),
    };
    std::fs::write(&file, &written).map_err(|e| format!("{path}: {e}"))?;
    Ok(written)
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
    fn a_view_written_into_its_file() {
        let root = std::env::temp_dir().join(format!("cadastre-edit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let views = "// The views.\nview a {\n  select app\n}\n\nview b { select server }\n";
        std::fs::write(root.join("views.cav"), views).unwrap();
        // In place of the view it edits, the rest as it was.
        let written = write_view(&root, "views.cav", Some("a"), "view a {\n  select app, database\n}\n").unwrap();
        assert_eq!(written, "// The views.\nview a {\n  select app, database\n}\n\nview b { select server }\n");
        // A new one at the end.
        let written = write_view(&root, "views.cav", None, "view c { select site }").unwrap();
        assert!(written.ends_with("view b { select server }\n\nview c { select site }\n"), "{written}");
        // A new file of the project.
        write_view(&root, "more.cav", None, "view d { select app }").unwrap();
        assert_eq!(std::fs::read_to_string(root.join("more.cav")).unwrap(), "view d { select app }\n");
        // A name taken, another file than a .cav of the project, anything but one view: refused.
        assert!(write_view(&root, "views.cav", None, "view b { select app }").unwrap_err().contains("already has"));
        assert!(write_view(&root, "../views.cav", None, "view e { select app }").is_err());
        assert!(write_view(&root, "apps.ca", None, "view e { select app }").is_err());
        assert!(write_view(&root, "views.cav", None, "view e { select app }\nserver x").is_err());
        assert!(write_view(&root, "views.cav", None, "view e { select app }\nview f { select app }").is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn titles() {
        assert_eq!(title(Path::new("x"), Some("Acme")), "Acme");
        let dir = std::env::temp_dir().join(format!("cadastre-title-{}", std::process::id())).join("si").join("src");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(title(&dir, None), "si");
        assert_eq!(title(dir.parent().unwrap(), None), "si");
        std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    /// The views to move by hand of an example with twins and frames drawn as nodes: no bend of
    /// an edge inside the box of its source or its target, where Cytoscape would not draw it.
    #[test]
    fn no_bend_inside_the_ends_of_an_edge() {
        if crate::graphviz("digraph {}", "-Tsvg").is_none() {
            eprintln!("Graphviz is not installed: skipped");
            return;
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/tangled");
        let files = generate(&root, "tangled", None, None).unwrap();
        let mut views = 0;
        let mut bent = 0;
        for (path, content) in files.iter().filter(|(p, _)| p.ends_with(".elements.json")) {
            views += 1;
            let elements: serde_json::Value = serde_json::from_slice(content).unwrap();
            let boxes: HashMap<&str, (f64, f64, f64, f64)> = elements["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|n| {
                    let (d, p) = (&n["data"], &n["position"]);
                    let (w, h) = (d["width"].as_f64()?, d["height"].as_f64()?);
                    Some((d["id"].as_str()?, (p["x"].as_f64()? - w / 2.0, p["y"].as_f64()? - h / 2.0, w, h)))
                })
                .collect();
            for edge in elements["edges"].as_array().unwrap() {
                let Some(points) = edge["data"]["bendPointPositions"].as_array() else { continue };
                bent += 1;
                for end in ["source", "target"] {
                    let Some(&(x, y, w, h)) = boxes.get(edge["data"][end].as_str().unwrap()) else { continue };
                    for p in points {
                        let (px, py) = (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap());
                        assert!(
                            !(px > x && px < x + w && py > y && py < y + h),
                            "{path}: a bend of {} inside its {end}",
                            edge["data"]["id"]
                        );
                    }
                }
            }
        }
        // Bends at all: the views are placed by Graphviz, twins and satellites included, not by the
        // layout of the compiler, whose boxes the routes of Graphviz do not fit.
        assert!(views >= 8 && bent > 50, "{views} views, {bent} edges bent");
    }

    #[test]
    fn sources_change_with_the_files() {
        let dir = std::env::temp_dir().join(format!("cadastre-sources-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.ca"),
            "server web01
",
        )
        .unwrap();
        let fingerprint = || sources(&cadastre_project::load(&dir).unwrap());
        let first = fingerprint();
        assert_eq!(fingerprint(), first);
        std::fs::write(
            dir.join("a.ca"),
            "server web02
",
        )
        .unwrap();
        let edited = fingerprint();
        assert_ne!(edited, first);
        std::fs::write(
            dir.join("data.csv"),
            "name
x
",
        )
        .unwrap();
        assert_ne!(fingerprint(), edited);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("resources/a%20b.html"), "resources/a b.html");
        assert_eq!(percent_decode("plain"), "plain");
    }
}
