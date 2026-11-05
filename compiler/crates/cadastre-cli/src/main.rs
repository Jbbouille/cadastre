//! The `cadastre` command.
//!
//! ```text
//! cadastre check [directory] [--only <check>] [--incomplete] [--strict]
//! cadastre build [directory] [--view <name>] [--out <directory>] [--check] [--incomplete] [--strict]
//! cadastre fmt [directory] [--check]
//! cadastre export archimate | graphml | json [directory] [--out <file>]
//! cadastre import terraform <state file or directory> [--out <directory>]
//! cadastre import csv <inventory.csv> [--kind <kind>] [--map <columns>] [--kinds <kinds>] [--out <directory>]
//! cadastre serve [directory] [--port <port>] [--host <address>] [--playground <directory>] [--title <name>] [--language <code>]
//! cadastre site [directory] --out <directory> [--playground <directory>] [--title <name>] [--language <code>]
//! cadastre init [directory] [--name <name>]
//! cadastre lsp
//! cadastre mcp [directory]
//! ```

mod publish;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use cadastre_core::source::FileRole;
use cadastre_core::{
    Compilation, Diagnostic, Options as CompileOptions, Severity, SourceFile, Summary, build, compile_modules,
};
use cadastre_project::Project;

const USAGE: &str = "usage:
  cadastre init [directory] [--name <name>]
  cadastre check [directory] [--only <check>] [--incomplete] [--strict]
  cadastre build [directory] [--view <name>] [--out <directory>] [--check] [--incomplete] [--strict]
  cadastre fmt [directory] [--check]
  cadastre export archimate | graphml | json [directory] [--out <file>]
  cadastre import terraform <state file or directory> [--out <directory>]
  cadastre import csv <inventory.csv> [--kind <kind>] [--map <columns>] [--kinds <kinds>] [--out <directory>]
  cadastre anonymize [directory] [--out <directory>]
  cadastre serve [directory] [--port <port>] [--host <address>] [--playground <directory>] [--title <name>] [--language <code>]
  cadastre site [directory] --out <directory> [--playground <directory>] [--title <name>] [--language <code>]
  cadastre lsp    (language server for editors, over stdin / stdout)
  cadastre mcp [directory]    (MCP server for AI agents, over stdin / stdout)";

struct Options {
    command: String,
    /// `export <format>`, `import <source>`.
    format: Option<String>,
    root: PathBuf,
    view: Option<String>,
    out: Option<PathBuf>,
    /// The output directory without `--out`: that of the `cadastre.toml`, otherwise `out/`.
    out_default: PathBuf,
    incomplete: bool,
    strict: bool,
    /// `fmt --check`, `build --check`: report the files that are not formatted or not up to
    /// date, without writing them.
    check: bool,
    /// `check --only <name>`: the failures of this check only, among the checks of the project.
    only: Option<String>,
    /// `init --name`: the name of the project.
    name: Option<String>,
    /// `serve`, `site`: the playground, the name shown, the address listened to.
    playground: Option<PathBuf>,
    title: Option<String>,
    /// `--language`: the language of the site and of the words of the diagrams, over the one of
    /// `cadastre.toml`.
    language: Option<cadastre_core::Language>,
    /// `import csv --kind`: the kind of every row of an inventory without a `kind` column.
    kind: Option<String>,
    /// `import csv --map`, `--kinds`: the columns and the kinds of an export, in the words of
    /// the language (`Hostname=name,Adresse IP=ip`, `Serveur physique=server`).
    map: String,
    kinds: String,
    host: String,
    port: u16,
}

fn main() -> ExitCode {
    // The MCP server, for AI agents: `cadastre mcp [directory]`.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        let directory = std::env::args().nth(2).map_or_else(|| PathBuf::from("."), PathBuf::from);
        return match cadastre_mcp::run(directory) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("MCP server: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if std::env::args().nth(1).as_deref() == Some("lsp") {
        return match cadastre_lsp::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("language server: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let mut options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    if options.command == "init" {
        return init(&options);
    }
    if options.command == "fmt" {
        return fmt(&mut options);
    }
    if options.command == "import" {
        return import(&options);
    }
    if options.command == "anonymize" {
        return anonymize(&mut options);
    }
    if matches!(options.command.as_str(), "serve" | "site") {
        return publish(&options);
    }
    if !matches!(options.command.as_str(), "check" | "build" | "export") {
        eprintln!("unknown command: {}", options.command);
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let Some(project) = read_project(&mut options) else { return ExitCode::from(2) };
    let (files, invalid) = (&project.files, &project.invalid);
    let language = options.language.unwrap_or_else(|| project.language());
    let compilation = compile_modules(
        files,
        &project.modules,
        CompileOptions { strict: options.strict, language, satellites: project.satellites() },
    );
    let mut diagnostics = compilation.diagnostics.clone();
    diagnostics.extend(invalid.iter().map(|(file, error)| error.diagnostic(*file)));
    if let Some(name) = &options.only {
        if !compilation.plans.checks.iter().any(|c| &c.name == name) {
            eprintln!("unknown check: {name}");
            return ExitCode::from(2);
        }
        // A failure is located first at the name of its check.
        let others: Vec<_> = compilation.plans.checks.iter().filter(|c| &c.name != name).map(|c| c.span).collect();
        diagnostics.retain(|d| !(d.message.starts_with("check ") && others.contains(&d.spans[0])));
    }

    if options.command == "export" {
        return export(files, &compilation, diagnostics, &options);
    }

    let mut written = Vec::new();
    if options.command == "build" {
        if let Some(name) = &options.view
            && !compilation.plans.views.iter().any(|v| &v.name == name)
        {
            eprintln!("unknown view: {name}");
            return ExitCode::from(2);
        }
        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
        if !has_errors {
            match render(&compilation, &options) {
                Ok((paths, view_diagnostics)) => {
                    written = paths;
                    diagnostics.extend(view_diagnostics);
                }
                Err(error) => {
                    eprintln!("cannot write renderings: {error}");
                    return ExitCode::from(2);
                }
            }
        }
    }

    let failed = diagnostics.iter().any(|d| d.severity == Severity::Error)
        || (options.strict && diagnostics.iter().any(|d| d.severity == Severity::Incomplete));
    report(files, &compilation, diagnostics, &options);
    if options.check {
        for path in &written {
            println!("not up to date: {}", path.display());
        }
    } else {
        for path in &written {
            println!("wrote {}", path.display());
        }
    }
    if failed || (options.check && !written.is_empty()) { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        command: String::new(),
        format: None,
        root: PathBuf::from("."),
        view: None,
        out: None,
        out_default: PathBuf::from("out"),
        name: None,
        incomplete: false,
        strict: false,
        check: false,
        only: None,
        playground: None,
        title: None,
        language: None,
        kind: None,
        map: String::new(),
        kinds: String::new(),
        host: "127.0.0.1".to_owned(),
        port: 8080,
    };
    let mut command = None;
    let mut root = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--incomplete" => options.incomplete = true,
            "--strict" => options.strict = true,
            "--check" => options.check = true,
            "--only" => options.only = Some(args.next().ok_or("--only expects a check name")?),
            "--view" => options.view = Some(args.next().ok_or("--view expects a view name")?),
            "--out" => options.out = Some(PathBuf::from(args.next().ok_or("--out expects a directory")?)),
            "--name" => options.name = Some(args.next().ok_or("--name expects a name")?),
            "--playground" => {
                options.playground = Some(PathBuf::from(args.next().ok_or("--playground expects a directory")?))
            }
            "--title" => options.title = Some(args.next().ok_or("--title expects a name")?),
            "--kind" => options.kind = Some(args.next().ok_or("--kind expects a kind")?),
            "--map" => options.map = args.next().ok_or("--map expects columns: Hostname=name,Adresse IP=ip")?,
            "--kinds" => options.kinds = args.next().ok_or("--kinds expects kinds: Serveur physique=server")?,
            "--language" => {
                let code = args.next().ok_or("--language expects a code: en, fr")?;
                options.language = Some(cadastre_core::Language::parse(&code).ok_or("--language expects en or fr")?);
            }
            "--host" => options.host = args.next().ok_or("--host expects an address")?,
            "--port" => {
                options.port = args.next().and_then(|p| p.parse().ok()).ok_or("--port expects a port number")?
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option: {flag}")),
            _ if command.is_none() => command = Some(arg),
            _ if matches!(command.as_deref(), Some("export" | "import")) && options.format.is_none() => {
                options.format = Some(arg)
            }
            _ if root.is_none() => root = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument: {arg}")),
        }
    }
    options.command = command.ok_or("missing command")?;
    if let Some(root) = root {
        options.root = root;
    }
    Ok(options)
}

/// `cadastre serve`: the playground showing the project, read again at each loading of the page;
/// `cadastre site`: the same, written to a folder for a static web server.
fn publish(options: &Options) -> ExitCode {
    let playground = match publish::playground(options.playground.as_deref()) {
        Ok(dir) => dir,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    if !options.root.is_dir() {
        eprintln!("not a directory: {}", options.root.display());
        return ExitCode::from(2);
    }
    // A playground of another build than the compiler: its diagnostics are not those of
    // `cadastre check` (a `cargo install` replaces the executable, not the playground beside it).
    if let Some(dir) = &playground {
        let own = cadastre_core::version();
        let found = publish::playground_build(dir);
        if found.as_deref() != Some(own) {
            eprintln!(
                "warning: the playground {} is {}, not the compiler {own}: its readers may see other diagnostics; \
                 rebuild it (web/README.md), or give another one with --playground",
                dir.display(),
                found.as_deref().unwrap_or("of an older build")
            );
        }
    }
    let title = publish::title(&options.root, options.title.as_deref());
    let language = options.language;
    if options.command == "serve" {
        let address = format!("{}:{}", options.host, options.port);
        if let Err(error) = publish::serve(&options.root, &address, playground.as_deref(), &title, language) {
            eprintln!("cannot serve on {address}: {error}");
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    }
    let Some(out) = &options.out else {
        eprintln!("cadastre site expects --out <directory>");
        return ExitCode::from(2);
    };
    match publish::site(&options.root, out, playground.as_deref(), &title, language) {
        Ok(paths) => {
            println!("wrote {} file(s) to {}", paths.len(), out.display());
            if playground.is_none() {
                println!("without the playground: give --playground <directory>, or set CADASTRE_PLAYGROUND");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write {}: {error}", out.display());
            ExitCode::from(2)
        }
    }
}

/// A one-off import into `.ca` files to review, in `imported/` by default, existing files never
/// overwritten: of a Terraform state (the JSON of `terraform show -json`, or a `.tfstate` file; in
/// a directory, its `terraform.tfstate`), or of an inventory in CSV.
fn import(options: &Options) -> ExitCode {
    let csv = match options.format.as_deref() {
        Some("terraform") => false,
        Some("csv") => true,
        _ => {
            eprintln!("import expects a source: terraform, csv");
            return ExitCode::from(2);
        }
    };
    let input =
        if options.root.is_dir() && !csv { options.root.join("terraform.tfstate") } else { options.root.clone() };
    let text = match std::fs::read_to_string(&input) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {}: {error}", input.display());
            return ExitCode::from(2);
        }
    };
    let imported = if csv {
        let name = input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        cadastre_core::inventory::Mapping::parse(&options.map, &options.kinds)
            .and_then(|mapping| cadastre_core::inventory::import(&text, &name, options.kind.as_deref(), &mapping))
    } else {
        cadastre_core::terraform::import(&text)
    };
    let files = match imported {
        Ok(files) => files,
        Err(error) => {
            eprintln!("{}: {error}", input.display());
            return ExitCode::from(2);
        }
    };
    let out = options.out.clone().unwrap_or_else(|| PathBuf::from("imported"));
    let existing: Vec<String> =
        files.iter().map(|f| out.join(&f.path)).filter(|p| p.exists()).map(|p| p.display().to_string()).collect();
    if !existing.is_empty() {
        eprintln!("not overwritten, move or remove them first: {}", existing.join(", "));
        return ExitCode::from(2);
    }
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("cannot create {}: {error}", out.display());
        return ExitCode::from(2);
    }
    for file in &files {
        let path = out.join(&file.path);
        if let Err(error) = std::fs::write(&path, &file.text) {
            eprintln!("cannot write {}: {error}", path.display());
            return ExitCode::from(2);
        }
        println!("wrote {}", path.display());
    }
    println!("review the imported files, complete them (owners, flows), then move them into the project");
    ExitCode::SUCCESS
}

/// Exports the whole registry (`out/model.<extension>` by default); nothing when the project has errors.
fn export(
    files: &[SourceFile],
    compilation: &Compilation,
    diagnostics: Vec<Diagnostic>,
    options: &Options,
) -> ExitCode {
    let formats: Vec<&str> = cadastre_core::export::FORMATS.iter().map(|(f, _)| *f).collect();
    let Some(format) = options.format.as_deref().filter(|f| formats.contains(f)) else {
        eprintln!("export expects a format: {}", formats.join(", "));
        return ExitCode::from(2);
    };
    let failed = diagnostics.iter().any(|d| d.severity == Severity::Error);
    report(files, compilation, diagnostics, options);
    if failed {
        return ExitCode::FAILURE;
    }
    let name = publish::title(&options.root, options.title.as_deref());
    let content = cadastre_core::export::export_with(compilation, files, &name, format).expect("a known format");
    let extension = match format {
        "archimate" => "archimate.xml",
        "graphml" => "graphml",
        _ => "json",
    };
    let path = options.out.clone().unwrap_or_else(|| options.out_default.join(format!("model.{extension}")));
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, content));
    match written {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write {}: {error}", path.display());
            ExitCode::from(2)
        }
    }
}

/// Renders the views and writes them to the output directory (default: `<project>/out`); with
/// `--check`, writes nothing and returns the files that are missing or differ.
fn render(compilation: &Compilation, options: &Options) -> std::io::Result<(Vec<PathBuf>, Vec<Diagnostic>)> {
    let (outputs, diagnostics) = build(compilation, options.view.as_deref());
    let out_dir = options.out.clone().unwrap_or_else(|| options.out_default.clone());
    if !options.check {
        std::fs::create_dir_all(&out_dir)?;
    }

    let mut written = Vec::new();
    let mut graphviz_missing = false;
    // The SVGs laid out by Graphviz, all of them at once.
    let to_lay_out: Vec<&str> = outputs
        .iter()
        .filter(|o| o.format == "svg" && !cadastre_core::render::is_drawn(&o.content))
        .map(|o| o.content.as_str())
        .collect();
    let mut laid_out = svgs_of(&to_lay_out).into_iter();
    for output in outputs {
        let (extension, content) = match output.format.as_str() {
            "svg" if cadastre_core::render::is_drawn(&output.content) => ("svg", output.content),
            "svg" => match laid_out.next().flatten() {
                Some(svg) => ("svg", svg),
                None => {
                    graphviz_missing = true;
                    ("dot", output.content)
                }
            },
            "text" => ("txt", output.content),
            "mermaid" => ("mmd", output.content),
            "sequence" => ("sequence.mmd", output.content),
            "traversal" => ("traversal.svg", output.content),
            "racks" => ("racks.svg", output.content),
            "ascii" => ("ascii.txt", output.content),
            "csv" => ("csv", output.content),
            "tsv" => ("tsv", output.content),
            "markdown" => ("md", output.content),
            "html" => ("html", output.content),
            "xlsx" => {
                let bytes = cadastre_core::xlsx(compilation, &output.view).unwrap_or_default();
                let path = out_dir.join(format!("{}.xlsx", output.view));
                if !options.check {
                    std::fs::write(&path, &bytes)?;
                    written.push(path);
                } else if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                    written.push(path);
                }
                continue;
            }
            // Laid out by Graphviz like the SVG, from the DOT of the view.
            "drawio" => {
                let (dots, _) = cadastre_core::build_with(compilation, Some(&output.view), Some(&["dot".to_owned()]));
                match dots.first().and_then(|dot| graphviz(&dot.content, "-Tjson")) {
                    Some(json) => (
                        "drawio",
                        cadastre_core::drawio(compilation, &output.view, Some(&json)).unwrap_or(output.content),
                    ),
                    None => {
                        graphviz_missing = true;
                        ("drawio", output.content)
                    }
                }
            }
            _ => ("dot", output.content),
        };
        let path = out_dir.join(format!("{}.{extension}", output.view));
        if options.check {
            if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
                written.push(path);
            }
        } else {
            std::fs::write(&path, content)?;
            written.push(path);
        }
    }
    if graphviz_missing {
        eprintln!(
            "warning: Graphviz (`dot`) not found: SVG renderings were written as .dot files, \
             draw.io files laid out by the compiler"
        );
    }
    Ok((written, diagnostics))
}

/// The SVGs of views, with their icons: Graphviz draws each variant of a DOT (the ranks over the
/// whole view, or frame by frame) and the drawing that reads best is kept; `None` when Graphviz
/// is not installed or fails. The variants of all the views are laid out by a few `dot`
/// processes rather than one process per variant: Graphviz takes about a second to start on some
/// machines (its fonts), for a few milliseconds of layout.
pub(crate) fn svgs_of<S: AsRef<str>>(dots: &[S]) -> Vec<Option<String>> {
    let variants: Vec<Vec<String>> = dots.iter().map(|d| cadastre_core::render::dot_variants(d.as_ref())).collect();
    let all: Vec<&str> = variants.iter().flatten().map(String::as_str).collect();
    let mut drawings = graphviz_all(&all, "-Tsvg").into_iter();
    variants
        .iter()
        .map(|v| {
            let drawn: Vec<String> = drawings.by_ref().take(v.len()).flatten().collect();
            let best = drawn.get(cadastre_core::render::best_drawing(&drawn))?;
            Some(cadastre_core::render::with_icons(best))
        })
        .collect()
}

/// Runs `dot` on several graphs, in a few processes working at the same time, each given a share
/// of the graphs one after the other in its input: one drawing per graph, `None` for a graph
/// Graphviz fails on, or for all of them when it is not installed.
fn graphviz_all(dots: &[&str], format: &str) -> Vec<Option<String>> {
    let workers = std::thread::available_parallelism().map_or(1, usize::from).min(dots.len()).max(1);
    let share = dots.len().div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> =
            dots.chunks(share).map(|chunk| (chunk.len(), scope.spawn(move || graphviz_batch(chunk, format)))).collect();
        // A share whose thread panics gives no drawing, but as many `None` as graphs: the
        // drawings of the next shares stay with their graphs.
        handles.into_iter().flat_map(|(len, h)| h.join().unwrap_or_else(|_| vec![None; len])).collect()
    })
}

/// One `dot` process for several graphs, its output cut back into one drawing per graph; when it
/// does not give as many drawings as graphs (a graph Graphviz rejects), each graph is run alone.
fn graphviz_batch(dots: &[&str], format: &str) -> Vec<Option<String>> {
    if dots.len() > 1 && format == "-Tsvg" {
        let input: String = dots.iter().map(|d| format!("{d}\n")).collect();
        if let Some(output) = graphviz(&input, format) {
            let drawings: Vec<String> = output
                .split("<?xml ")
                .skip(1)
                .map(|svg| unpaged(&anchors_from_zero(&format!("<?xml {svg}"))))
                .collect();
            if drawings.len() == dots.len() {
                return drawings.into_iter().map(Some).collect();
            }
        }
    }
    dots.iter().map(|d| graphviz(d, format)).collect()
}

/// Some versions of Graphviz (14) take the graphs after the first of a batch for pages, and
/// prefix the ids of their drawing with it (`id="page0,1_graph0"`): the prefix is removed, as a
/// process of its own does not write it.
fn unpaged(svg: &str) -> String {
    const ID: &str = "id=\"page";
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find(ID) {
        let start = at + ID.len();
        // `page<n>,<n>_`: digits, a comma, digits, an underscore.
        let prefix = rest[start..].find('_').filter(|&end| {
            let page = &rest[start..start + end];
            page.split_once(',')
                .is_some_and(|(a, b)| [a, b].iter().all(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit())))
        });
        match prefix {
            Some(end) => {
                out.push_str(&rest[..at]);
                out.push_str("id=\"");
                rest = &rest[start + end + 1..];
            }
            None => {
                out.push_str(&rest[..start]);
                rest = &rest[start..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The ids Graphviz gives the links of a drawing (`id="a_node6_0"`) end with a counter of the
/// process: in a batch, the drawings after the first go on from the previous ones. They are
/// numbered again from zero, as a process of its own would have, for a drawing that does not
/// depend on the graphs drawn with it.
fn anchors_from_zero(svg: &str) -> String {
    const ID: &str = "id=\"a_";
    let counter = |rest: &str| -> Option<(usize, usize, u64)> {
        let id = &rest[..rest.find('"')?];
        let underscore = id.rfind('_')?;
        let digits = &id[underscore + 1..];
        Some((underscore + 1, id.len(), digits.parse().ok()?))
    };
    let first = svg.match_indices(ID).filter_map(|(at, _)| counter(&svg[at + ID.len()..])).map(|(_, _, n)| n).min();
    let Some(first) = first.filter(|&n| n > 0) else {
        return svg.to_owned();
    };
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find(ID) {
        let start = at + ID.len();
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        if let Some((digits, end, n)) = counter(rest) {
            out.push_str(&rest[..digits]);
            out.push_str(&(n - first).to_string());
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    out
}

/// Runs `dot` with an output format (`-Tsvg`, `-Tjson`); `None` when Graphviz is not installed
/// or fails.
pub(crate) fn graphviz(dot: &str, format: &str) -> Option<String> {
    let mut child = Command::new("dot")
        .arg(format)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Written from another thread: a large input would otherwise fill the pipe of the output
    // while `dot` waits for it to be read.
    let mut stdin = child.stdin.take()?;
    let input = dot.to_owned();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()).is_ok());
    let output = child.wait_with_output().ok()?;
    let written = writer.join().unwrap_or(false);
    (written && output.status.success()).then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn report(files: &[SourceFile], compilation: &Compilation, mut diagnostics: Vec<Diagnostic>, options: &Options) {
    if options.strict {
        for d in &mut diagnostics {
            if d.severity == Severity::Incomplete {
                d.severity = Severity::Error;
            }
        }
    }
    diagnostics.sort_by_key(|d| (d.severity, d.spans[0].file, d.spans[0].start));
    for d in &diagnostics {
        if d.severity != Severity::Incomplete || options.incomplete {
            print!("{}", d.render_in(files, compilation.language));
        }
    }
    let summary = Summary::of(&diagnostics);
    let language = compilation.language;
    println!(
        "{}",
        language.message(&format!(
            "{} file(s), {} resource(s), {} link(s), {} view(s)",
            files.len(),
            compilation.registry.resources.len(),
            compilation.registry.links.len(),
            compilation.plans.views.len()
        ))
    );
    let infos = if summary.infos > 0 { format!(", {} info(s)", summary.infos) } else { String::new() };
    println!("{}", language.message(&format!("{} error(s), {} warning(s){infos}", summary.errors, summary.warnings)));
    if summary.incomplete > 0 && !options.incomplete {
        println!(
            "{}",
            language.message(&format!("{} incomplete element(s) (--incomplete for details)", summary.incomplete))
        );
    }
}

/// `cadastre init`: a new project in a directory (the current one by default): its `cadastre.toml`,
/// the recommended layout of `src/` with a first site, system and view, and `out/` ignored by Git.
/// Nothing is overwritten: a directory holding a `cadastre.toml` or a `src/` is left alone.
fn init(options: &Options) -> ExitCode {
    let root = &options.root;
    let name = options.name.clone().unwrap_or_else(|| {
        std::path::absolute(root)
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "my-system".to_owned())
    });
    for existing in ["cadastre.toml", "src"] {
        if root.join(existing).exists() {
            eprintln!("{} already holds {existing}: nothing written", root.display());
            return ExitCode::from(2);
        }
    }
    let files: &[(&str, String)] = &[
        (
            "cadastre.toml",
            format!("[project]\nname = \"{name}\"\nsources = [\"src\"]\nout = \"out\"\n\n# Another project read with this one, read only: its teams, sites, schema and checks.\n# [dependencies]\n# shared = {{ path = \"../shared\" }}\n"),
        ),
        (".gitignore", "out/\n".to_owned()),
        (
            "src/schema.ca",
            "// The schema of the project: the keys it types, the kinds of its own.\n// kind server { inventory_id: pattern(\"INV-[0-9]+\") }\n// kind printer { like: device }\n".to_owned(),
        ),
        ("src/teams.ca", "team ops\n".to_owned()),
        ("src/data.ca", "// Business objects, carried by the flows (carries) and stored by the databases (stores).\ndata order\n".to_owned()),
        (
            "src/sites/main.ca",
            "site dc1 {\n  hosting: on-prem\n  owner: ops\n  network lan {\n    subnet apps {\n      cidr: 10.0.1.0/24\n      server web01 { ip: 10.0.1.10 }\n      server db01  { ip: 10.0.1.20 }\n    }\n  }\n}\n".to_owned(),
        ),
        (
            "src/apps/sales.ca",
            "system sales {\n  owner: ops\n  app shop {\n    on: web01\n    -> orders : https/443 [flow] { carries: [order] }\n  }\n  app orders {\n    on: web01\n    -> orders-db : tcp/5432 [flow] { carries: [order] }\n  }\n  database orders-db { on: db01 }\n}\n".to_owned(),
        ),
        (
            "src/views/sales.cav",
            "view apps {\n  select app, database group-by system\n  edges  [flow]\n  render svg, text\n}\n\nview servers {\n  select server group-by site > subnet\n  edges  lift([flow], via: on)\n  render svg\n}\n".to_owned(),
        ),
        (
            "src/checks/main.cac",
            "// The rules of the project, run by cadastre check.\ncheck owned {\n  select system where not exists(owner)\n  expect none\n  message: \"every system has an owner\"\n}\n".to_owned(),
        ),
    ];
    for (path, text) in files {
        let path = root.join(path);
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, text));
        if let Err(error) = written {
            eprintln!("cannot write {}: {error}", path.display());
            return ExitCode::from(2);
        }
        println!("wrote {}", path.display());
    }
    println!(
        "\nnext: cadastre check {0}  ·  cadastre build {0}  ·  docs: https://github.com/Jbbouille/cadastre/blob/master/docs/tutorial.md",
        root.display()
    );
    ExitCode::SUCCESS
}

/// `cadastre anonymize`: a copy of the project with every name, text and address replaced and
/// its shape kept (`anonymized/` beside the project by default), to share without exposing it.
fn anonymize(options: &mut Options) -> ExitCode {
    let Some(project) = read_project(options) else { return ExitCode::from(2) };
    let out = options.out.clone().unwrap_or_else(|| options.root.join("anonymized"));
    let anonymized = cadastre_core::anonymize::anonymize(&project.files);
    let mut written = 0;
    for (index, file) in anonymized.files.iter().enumerate() {
        // The files of the modules, outside the project, are not copied.
        if !project.is_own(index) {
            continue;
        }
        let target = out.join(&file.path);
        let result =
            target.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&target, &file.text));
        if let Err(error) = result {
            eprintln!("cannot write {}: {error}", target.display());
            return ExitCode::from(2);
        }
        written += 1;
    }
    for warning in &anonymized.warnings {
        eprintln!("warning: {warning}");
    }
    if !project.modules.is_empty() {
        eprintln!("warning: the modules of the project are not copied: the copy compiles only with them");
    }
    println!("wrote {written} file(s) to {}", out.display());
    println!("read the copy before sharing it: keys and values the tool does not know may still identify something");
    ExitCode::SUCCESS
}

/// `cadastre fmt`: formats the `.ca`, `.cav` and `.cac` files of the project in place, or with
/// `--check` lists those that are not formatted. Files with syntax errors are left alone.
fn fmt(options: &mut Options) -> ExitCode {
    let Some(project) = read_project(options) else { return ExitCode::from(2) };
    let (files, invalid) = (&project.files, &project.invalid);
    let mut failed = !invalid.is_empty();
    for (file, error) in invalid {
        eprintln!(
            "not formatted, not UTF-8: {} ({}:{})",
            files[file.0 as usize].path,
            error.line + 1,
            error.column + 1
        );
    }
    let mut changed = 0;
    for (index, file) in files.iter().enumerate() {
        // The files of the project: not its modules, read only, nor its CSV files.
        if invalid.iter().any(|(id, _)| id.0 as usize == index)
            || !project.is_own(index)
            || FileRole::from_path(&file.path).is_none()
        {
            continue;
        }
        match cadastre_core::format::format(&file.text) {
            None => {
                eprintln!("not formatted, syntax errors: {} (cadastre check for details)", file.path);
                failed = true;
            }
            Some(formatted) if formatted != file.text => {
                changed += 1;
                if options.check {
                    println!("not formatted: {}", file.path);
                } else if let Err(error) = std::fs::write(&project.paths[index], formatted) {
                    eprintln!("cannot write {}: {error}", file.path);
                    failed = true;
                } else {
                    println!("formatted {}", file.path);
                }
            }
            Some(_) => {}
        }
    }
    if options.check && changed > 0 {
        println!("{changed} file(s) to format (cadastre fmt)");
    }
    if failed || (options.check && changed > 0) { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// Reads the project holding the directory given (`cadastre.toml` above it, or the directory
/// alone), with its modules; its directory, its output directory and `strict` become those of
/// the commands. `None` when it cannot be read (reported).
fn read_project(options: &mut Options) -> Option<Project> {
    match cadastre_project::load(&options.root) {
        Ok(project) => {
            options.root = project.root.clone();
            options.strict |= project.strict();
            options.out_default = project.out_dir();
            Some(project)
        }
        Err(error) => {
            eprintln!("cannot read {}: {error}", options.root.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{anchors_from_zero, graphviz, graphviz_batch, unpaged};

    #[test]
    fn pages_of_a_batch_removed() {
        let svg = r#"<g id="page0,1_graph0" class="graph"><g id="node1"/><g id="pager"/><g id="page2"/>"#;
        assert_eq!(unpaged(svg), r#"<g id="graph0" class="graph"><g id="node1"/><g id="pager"/><g id="page2"/>"#);
        assert_eq!(unpaged(r#"id="page12,34_node5""#), r#"id="node5""#);
    }

    /// The graphs of a batch drawn as each would be by a process of its own, what Graphviz
    /// numbers per process included (links, gradients), and those of the views of an example;
    /// skipped without Graphviz.
    #[test]
    fn a_batch_draws_as_one_process_per_graph() {
        if graphviz("digraph {}", "-Tsvg").is_none() {
            eprintln!("Graphviz is not installed: skipped");
            return;
        }
        let linked = r#"digraph { subgraph cluster_a { URL="https://a"; label="a"; x [URL="https://x", tooltip="x"] }
            y [URL="https://y"]; x -> y [URL="https://e", label="e"]; y -> x [URL="https://f"] }"#;
        let gradients = r#"digraph { a [style=filled, fillcolor="red:blue"]; b [style=radial, fillcolor="white:green"];
            subgraph cluster_c { style=filled; fillcolor="yellow:orange"; c } a -> b -> c }"#;
        let html = r#"digraph { n [shape=plaintext, label=<<table><tr><td href="https://t">t</td></tr></table>>]; n -> m [URL="https://g"] }"#;
        // The DOT of every view of an example, each of its variants.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/reference");
        let files: Vec<cadastre_core::SourceFile> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| ["ca", "cav", "cac"].contains(&x.to_str().unwrap())))
            .map(|p| cadastre_core::SourceFile {
                path: p.file_name().unwrap().to_string_lossy().into_owned(),
                text: std::fs::read_to_string(&p).unwrap(),
            })
            .collect();
        let compilation = cadastre_core::compile(&files);
        let (outputs, _) = cadastre_core::build_with(&compilation, None, Some(&["svg".to_owned()]));
        let mut dots: Vec<String> = [linked, gradients, html, linked].iter().map(|d| (*d).to_owned()).collect();
        for output in outputs.iter().filter(|o| !cadastre_core::render::is_drawn(&o.content)) {
            dots.extend(cadastre_core::render::dot_variants(&output.content));
        }
        // A dozen graphs: each graph run alone costs Graphviz its start (about a second here).
        dots.truncate(12);
        assert_eq!(dots.len(), 12);
        let dots: Vec<&str> = dots.iter().map(String::as_str).collect();
        let batch = graphviz_batch(&dots, "-Tsvg");
        for (i, dot) in dots.iter().enumerate() {
            assert_eq!(batch[i], graphviz(dot, "-Tsvg"), "graph {i}:\n{dot}");
        }
    }

    #[test]
    fn anchors_numbered_from_zero() {
        let svg = r#"<g id="a_node6_4"><a xlink:href="x"/></g><g id="a_node7_5"/><g id="node8"/>"#;
        assert_eq!(
            anchors_from_zero(svg),
            r#"<g id="a_node6_0"><a xlink:href="x"/></g><g id="a_node7_1"/><g id="node8"/>"#
        );
        let first = r#"<g id="a_node1_0"/><g id="a_node2_1"/>"#;
        assert_eq!(anchors_from_zero(first), first);
    }
}
