//! Reading a Cadastre project from the file system, for the command line and the language
//! server (the core reads no file).
//!
//! A project is the directory of its `cadastre.toml`, found from the directory given and up, or
//! that directory itself without one. Its files are the `.ca`, `.cav`, `.cac` and `.csv` of its sources;
//! its modules, the projects of its `[dependencies]`, read only:
//!
//! ```toml
//! [project]
//! name = "acme-is"
//! sources = ["src"]        # directories read; the directory of the file by default
//! out = "out"              # where `cadastre build` writes; out/ by default
//! strict = false           # as --strict
//! language = "fr"          # the language of the site and of the words of the diagrams (en, fr)
//!
//! [render]
//! satellites = true        # the default of the views that do not write it; false otherwise
//!
//! [history]
//! git = true               # the last change of each resource from `git blame`: `changed`, `changed_by`
//!
//! [dependencies]
//! shared = { path = "../acme-shared" }
//! ```

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use cadastre_core::Module;
use cadastre_core::source::{FileId, InvalidUtf8, SourceFile, decode};

/// The name of the project file.
pub const MANIFEST: &str = "cadastre.toml";

/// The content of a `cadastre.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub name: Option<String>,
    /// The directories read, relative to the project; empty: the project directory.
    pub sources: Vec<String>,
    pub out: Option<String>,
    pub strict: bool,
    /// The language of what is written for the readers (`en`, `fr`); English by default.
    pub language: Option<String>,
    /// `[render] satellites`: the default of the views that do not write it.
    pub satellites: bool,
    /// `[history] git`: the last change of each resource, from `git blame`.
    pub history: bool,
    /// Name → path of the module, relative to the project.
    pub dependencies: BTreeMap<String, String>,
}

/// A project read: its files, those of its modules after its own.
#[derive(Debug, Default)]
pub struct Project {
    /// The directory of the project: that of its `cadastre.toml`, or the directory given.
    pub root: PathBuf,
    pub manifest: Option<Manifest>,
    /// Paths relative to `root` (`src/apps.ca`, `../shared/teams.ca`), with `/`.
    pub files: Vec<SourceFile>,
    /// The same files, as paths of the file system.
    pub paths: Vec<PathBuf>,
    /// Files that are not valid UTF-8, kept empty in `files`.
    pub invalid: Vec<(FileId, InvalidUtf8)>,
    pub modules: Vec<Module>,
}

impl Project {
    /// Whether a file is one of the project, not of a module.
    pub fn is_own(&self, index: usize) -> bool {
        !self.modules.iter().any(|m| m.files.contains(&index))
    }

    /// Where `cadastre build` writes by default.
    pub fn out_dir(&self) -> PathBuf {
        let out = self.manifest.as_ref().and_then(|m| m.out.clone()).unwrap_or_else(|| "out".to_owned());
        self.root.join(out)
    }

    pub fn strict(&self) -> bool {
        self.manifest.as_ref().is_some_and(|m| m.strict)
    }

    /// The language of the readers, from `cadastre.toml`; English otherwise.
    pub fn language(&self) -> cadastre_core::Language {
        self.manifest
            .as_ref()
            .and_then(|m| m.language.as_deref())
            .and_then(cadastre_core::Language::parse)
            .unwrap_or_default()
    }

    /// The `satellites` of the views that do not write it, from `cadastre.toml`.
    pub fn satellites(&self) -> bool {
        self.manifest.as_ref().is_some_and(|m| m.satellites)
    }

    /// Whether the history of the resources is read from git (`[history] git = true`).
    pub fn history_enabled(&self) -> bool {
        self.manifest.as_ref().is_some_and(|m| m.history)
    }

    /// Compiles the project, with the history of its resources when `cadastre.toml` asks for
    /// it; what keeps it from being read (no git, a file outside a repository) as warnings.
    pub fn compile(&self, options: cadastre_core::Options) -> (cadastre_core::Compilation, Vec<String>) {
        let (history, warnings) = if self.history_enabled() { history(self) } else { (Vec::new(), Vec::new()) };
        (cadastre_core::compile_project(&self.files, &self.modules, options, &history), warnings)
    }
}

/// The last change of each line of the files of a project, from `git blame`: the files git does
/// not know, and the lines not committed yet, left out; why a file could not be read, as warnings.
pub fn history(project: &Project) -> (Vec<(FileId, cadastre_core::history::Lines)>, Vec<String>) {
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for (index, path) in project.paths.iter().enumerate() {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { continue };
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["blame", "--line-porcelain", "--"])
            .arg(name)
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let text = &project.files[index].text;
                files.push((FileId(index as u32), blame_lines(text, &String::from_utf8_lossy(&output.stdout))));
            }
            Ok(output) => {
                let error = String::from_utf8_lossy(&output.stderr);
                // A file git does not know yet: no history, nothing to say.
                if !error.contains("no such path") {
                    warnings.push(format!("history of {}: {}", project.files[index].path, error.trim()));
                }
            }
            Err(error) => {
                warnings.push(format!("history: git cannot be run: {error}"));
                break;
            }
        }
    }
    (files, warnings)
}

/// The lines of `git blame --line-porcelain`, each with its bytes in `text`.
fn blame_lines(text: &str, porcelain: &str) -> cadastre_core::history::Lines {
    use cadastre_core::history::Change;
    // The bytes of each line of the text, from 1.
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    // The bytes of a line, its indentation and its end left out: those of a resource nested in
    // another one are within its span.
    let line = |n: usize| -> Option<std::ops::Range<usize>> {
        let start = *starts.get(n.checked_sub(1)?)?;
        let end = starts.get(n).map_or(text.len(), |e| e - 1);
        let content = text.get(start..end)?;
        let leading = content.len() - content.trim_start().len();
        Some(start + leading..start + content.trim_end().len())
    };
    let mut lines = Vec::new();
    let (mut commit, mut number, mut author, mut time) = (String::new(), 0, String::new(), 0i64);
    for row in porcelain.lines() {
        if row.starts_with('\t') {
            // The text of the line, closing its record. A line not committed yet has a commit
            // of zeros.
            if !commit.chars().all(|c| c == '0')
                && let Some(bytes) = line(number)
            {
                lines.push((bytes, Change { time, author: author.clone(), commit: commit.clone() }));
            }
            continue;
        }
        let mut words = row.split(' ');
        let first = words.next().unwrap_or_default();
        if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            commit = first.to_owned();
            number = words.nth(1).and_then(|n| n.parse().ok()).unwrap_or(0);
        } else if let Some(name) = row.strip_prefix("author ") {
            author = name.to_owned();
        } else if let Some(seconds) = row.strip_prefix("author-time ") {
            time = seconds.parse().unwrap_or(0);
        }
    }
    lines
}

/// The directory of the `cadastre.toml` holding `start`, if any.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = std::path::absolute(start).ok()?;
    start.ancestors().find(|dir| dir.join(MANIFEST).is_file()).map(Path::to_path_buf)
}

/// Reads the project holding `start`: its `cadastre.toml` above it, otherwise `start` alone.
pub fn load(start: &Path) -> Result<Project, String> {
    let root = match find_root(start) {
        Some(root) => root,
        None => std::path::absolute(start).map_err(|e| format!("{}: {e}", start.display()))?,
    };
    let mut project = Project { root: root.clone(), ..Project::default() };
    let mut visiting = Vec::new();
    let mut loaded = Vec::new();
    read(&root, None, &mut project, &mut visiting, &mut loaded)?;
    Ok(project)
}

/// Reads the project at `dir` into `project`: itself when `module` is `None`, otherwise a module
/// of that name, then its own modules.
fn read(
    dir: &Path,
    module: Option<&str>,
    project: &mut Project,
    visiting: &mut Vec<PathBuf>,
    loaded: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let canonical = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    if visiting.contains(&canonical) {
        let cycle: Vec<String> = visiting.iter().chain([&canonical]).map(|p| p.display().to_string()).collect();
        return Err(format!("the dependencies make a cycle: {}", cycle.join(" → ")));
    }
    // A module reached by two ways is read once.
    if loaded.contains(&canonical) {
        return Ok(());
    }
    let manifest_path = dir.join(MANIFEST);
    let manifest = if manifest_path.is_file() {
        let text = std::fs::read_to_string(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        Some(parse(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?)
    } else {
        None
    };
    let sources: Vec<PathBuf> = match manifest.as_ref().map(|m| m.sources.as_slice()) {
        Some(sources) if !sources.is_empty() => sources.iter().map(|s| dir.join(s)).collect(),
        _ => vec![dir.to_path_buf()],
    };
    let mut paths = Vec::new();
    for source in &sources {
        if !source.is_dir() {
            return Err(format!(
                "{}: the source directory {} does not exist",
                manifest_path.display(),
                source.display()
            ));
        }
        collect(source, &mut paths).map_err(|e| format!("{}: {e}", source.display()))?;
    }
    paths.sort();
    paths.dedup();
    let first = project.files.len();
    for path in paths {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let index = project.files.len();
        let text = decode(bytes).unwrap_or_else(|error| {
            project.invalid.push((FileId(index as u32), error));
            String::new()
        });
        project.files.push(SourceFile { path: relative(&project.root, &path), text });
        project.paths.push(path);
    }
    if let Some(name) = module {
        project.modules.push(Module { name: name.to_owned(), files: first..project.files.len() });
    }
    loaded.push(canonical.clone());
    visiting.push(canonical);
    for (name, path) in manifest.iter().flat_map(|m| m.dependencies.iter()) {
        let dependency = dir.join(path);
        if !dependency.is_dir() {
            visiting.pop();
            return Err(format!(
                "{}: the module {name} is not a directory: {}",
                manifest_path.display(),
                dependency.display()
            ));
        }
        read(&dependency, Some(name), project, visiting, loaded)?;
    }
    visiting.pop();
    if module.is_none() {
        project.manifest = manifest;
    }
    Ok(())
}

/// Parses a `cadastre.toml`.
pub fn parse(text: &str) -> Result<Manifest, String> {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.message().to_owned())?;
    let mut manifest = Manifest::default();
    for (key, value) in &table {
        match key.as_str() {
            "project" => {
                let project = value.as_table().ok_or("[project] is a table")?;
                for (key, value) in project {
                    match key.as_str() {
                        "name" => manifest.name = Some(string(value, "project.name")?),
                        "out" => manifest.out = Some(string(value, "project.out")?),
                        "strict" => manifest.strict = value.as_bool().ok_or("project.strict is true or false")?,
                        "language" => {
                            let code = string(value, "project.language")?;
                            if cadastre_core::Language::parse(&code).is_none() {
                                return Err(format!("unknown project.language: {code} (en, fr)"));
                            }
                            manifest.language = Some(code);
                        }
                        "sources" => {
                            let list = value.as_array().ok_or("project.sources is a list of directories")?;
                            manifest.sources =
                                list.iter().map(|v| string(v, "project.sources")).collect::<Result<_, _>>()?;
                        }
                        other => {
                            return Err(format!("unknown key project.{other}: name, sources, out, strict, language"));
                        }
                    }
                }
            }
            "render" => {
                let render = value.as_table().ok_or("[render] is a table")?;
                for (key, value) in render {
                    match key.as_str() {
                        "satellites" => {
                            manifest.satellites = value.as_bool().ok_or("render.satellites is true or false")?
                        }
                        other => return Err(format!("unknown key render.{other}: satellites")),
                    }
                }
            }
            "history" => {
                let history = value.as_table().ok_or("[history] is a table")?;
                for (key, value) in history {
                    match key.as_str() {
                        "git" => manifest.history = value.as_bool().ok_or("history.git is true or false")?,
                        other => return Err(format!("unknown key history.{other}: git")),
                    }
                }
            }
            "dependencies" => {
                let dependencies = value.as_table().ok_or("[dependencies] is a table")?;
                for (name, dependency) in dependencies {
                    let path = dependency
                        .as_table()
                        .and_then(|t| t.get("path"))
                        .and_then(|p| p.as_str())
                        .ok_or_else(|| format!("dependencies.{name}: {{ path = \"<directory>\" }}"))?;
                    if let Some(other) = dependency.as_table().and_then(|t| t.keys().find(|k| *k != "path")) {
                        return Err(format!("dependencies.{name}: unknown key {other}: only path, for now"));
                    }
                    manifest.dependencies.insert(name.clone(), path.to_owned());
                }
            }
            other => return Err(format!("unknown table [{other}]: [project], [render], [history], [dependencies]")),
        }
    }
    Ok(manifest)
}

fn string(value: &toml::Value, key: &str) -> Result<String, String> {
    value.as_str().map(str::to_owned).ok_or_else(|| format!("{key} is a string"))
}

/// The `.ca`, `.cav`, `.cac` and `.csv` files of a directory, recursively; hidden directories left out.
pub fn collect(dir: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if !name.starts_with('.') {
                collect(&path, paths)?;
            }
        } else if is_project_file(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

/// Whether a path is that of a file a project reads.
pub fn is_project_file(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("ca" | "cav" | "cac" | "csv"))
}

/// `path` relative to `base`, with `/` (`../shared/teams.ca` for a module beside the project).
fn relative(base: &Path, path: &Path) -> String {
    let base: Vec<Component> = base.components().collect();
    let path_components: Vec<Component> = path.components().collect();
    let common = base.iter().zip(&path_components).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return path.to_string_lossy().replace('\\', "/");
    }
    let mut parts: Vec<String> = base[common..].iter().map(|_| "..".to_owned()).collect();
    parts.extend(path_components[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests() {
        let manifest = parse(
            "[project]\nname = \"acme\"\nsources = [\"src\"]\nout = \"site\"\nstrict = true\n\n[dependencies]\nshared = { path = \"../shared\" }\n",
        )
        .unwrap();
        assert_eq!(manifest.name.as_deref(), Some("acme"));
        assert_eq!(manifest.sources, ["src"]);
        assert_eq!(manifest.dependencies["shared"], "../shared");
        assert!(manifest.strict);
        assert_eq!(
            parse("[projet]\n").unwrap_err(),
            "unknown table [projet]: [project], [render], [history], [dependencies]"
        );
        assert!(parse("[history]\ngit = true\n").unwrap().history);
        assert_eq!(parse("[history]\ngit = 1\n").unwrap_err(), "history.git is true or false");
        assert!(
            parse(
                "[render]
satellites = true
"
            )
            .unwrap()
            .satellites
        );
        assert_eq!(
            parse(
                "[render]
satelites = true
"
            )
            .unwrap_err(),
            "unknown key render.satelites: satellites"
        );
        assert!(parse("[dependencies]\nx = { git = \"…\" }\n").is_err());
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative(Path::new("/a/project"), Path::new("/a/project/src/x.ca")), "src/x.ca");
        assert_eq!(relative(Path::new("/a/project"), Path::new("/a/shared/t.ca")), "../shared/t.ca");
    }

    #[test]
    fn a_project_and_its_modules() {
        let dir = std::env::temp_dir().join(format!("cadastre-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |path: &str, text: &str| {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "app/cadastre.toml",
            "[project]\nsources = [\"src\"]\n[dependencies]\nshared = { path = \"../shared\" }\nnet = { path = \"../net\" }\n",
        );
        write("app/src/apps.ca", "system billing { owner: ops }\n");
        write("app/out/stale.ca", "not read\n");
        write("shared/cadastre.toml", "[dependencies]\nnet = { path = \"../net\" }\n");
        write("shared/teams.ca", "team ops\n");
        write("net/sites.ca", "site dc { hosting: on-prem }\n");
        let project = load(&dir.join("app/src")).unwrap();
        assert_eq!(project.root, std::path::absolute(dir.join("app")).unwrap());
        let paths: Vec<&str> = project.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/apps.ca", "../net/sites.ca", "../shared/teams.ca"]);
        let names: Vec<&str> = project.modules.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["net", "shared"]);
        assert!(project.is_own(0) && !project.is_own(1));

        write("net/cadastre.toml", "[dependencies]\napp = { path = \"../app\" }\n");
        assert!(load(&dir.join("app")).unwrap_err().starts_with("the dependencies make a cycle"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_history_from_git() {
        let dir = std::env::temp_dir().join(format!("cadastre-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str], date: &str| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=Ana", "-c", "user.email=ana@example.com", "-c", "commit.gpgsign=false"])
                .args(args)
                .env("GIT_AUTHOR_DATE", date)
                .env("GIT_COMMITTER_DATE", date)
                .output()
                .expect("git");
            assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
        };
        std::fs::write(dir.join(MANIFEST), "[history]\ngit = true\n").unwrap();
        std::fs::write(dir.join("a.ca"), "site dc {\n  hosting: on-prem\n  server old\n}\n").unwrap();
        git(&["init", "-q"], "2025-01-01T10:00:00Z");
        git(&["add", "."], "2025-01-01T10:00:00Z");
        git(&["commit", "-q", "-m", "first"], "2025-01-01T10:00:00Z");
        // A server added later, then one not committed yet.
        std::fs::write(dir.join("a.ca"), "site dc {\n  hosting: on-prem\n  server old\n  server new\n}\n").unwrap();
        git(&["commit", "-q", "-am", "second"], "2025-03-04T10:00:00Z");
        std::fs::write(
            dir.join("a.ca"),
            "site dc {\n  hosting: on-prem\n  server old\n  server new\n  server next\n}\n",
        )
        .unwrap();

        let project = load(&dir).unwrap();
        assert!(project.history_enabled());
        let (compilation, warnings) = project.compile(cadastre_core::Options::default());
        assert!(warnings.is_empty(), "{warnings:?}");
        let changed = |name: &str| {
            let id = compilation.registry.ids().find(|&id| compilation.registry.resource(id).name == name).unwrap();
            compilation.model.changes.get(&id).map(|c| (c.date(), c.author.clone()))
        };
        assert_eq!(changed("dc"), Some(("2025-01-01".to_owned(), "Ana".to_owned())));
        assert_eq!(changed("new"), Some(("2025-03-04".to_owned(), "Ana".to_owned())));
        assert_eq!(changed("next"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
