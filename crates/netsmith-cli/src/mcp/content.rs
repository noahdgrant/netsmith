//! The resources and the prompts (issue #1106). The resources are the project's files, as
//! `netsmith://project/<path from the project's root>`, and the language guide this netsmith
//! writes into a project, as `netsmith://guide/LANGUAGE.md` and `netsmith://guide/language/
//! NN-topic.md` (the paths it has under `docs/netsmith/`, so the index's links resolve). The
//! prompts are the skills `project init` writes, one for each, whose text is the skill's.

use std::fs;
use std::path::{Component, Path, PathBuf};

use netsmith_fabricate::project::MANIFEST;
use serde_json::{Value, json};

const PROJECT: &str = "netsmith://project/";
const GUIDE: &str = "netsmith://guide/";

/// The files of a project that are offered: its source and the text files netsmith reads or
/// writes. Others (pictures, models, archives) are left out.
const TEXT: [&str; 8] = [
    "nsm",
    "toml",
    "csv",
    "md",
    "txt",
    "json",
    "kicad_sym",
    "dxf",
];

/// Folders never offered: hidden ones (`.git`, `.netsmith`, the agent files) and build output of
/// other tools.
const SKIPPED: [&str; 2] = ["target", "node_modules"];

/// At most this many project files are listed, so a folder that is not a project's (a home
/// folder) does not list its whole tree.
const MOST: usize = 2000;

/// The root of the project the server was started in: the nearest folder up from `dir` with a
/// `netsmith.toml`, as the commands find it.
pub fn project_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|d| d.join(MANIFEST).is_file())
        .map(Path::to_path_buf)
}

/// `resources/list`.
pub fn resources(root: Option<&Path>) -> Value {
    let mut all = Vec::new();
    if let Some(root) = root {
        let mut files = Vec::new();
        walk(root, root, &mut files);
        files.sort();
        for rel in files.into_iter().take(MOST) {
            all.push(json!({
                "uri": format!("{PROJECT}{rel}"),
                "name": rel,
                "description": "A file of the project.",
                "mimeType": mime(&rel),
            }));
        }
    }
    for (path, text) in netsmith_tools::init::guide() {
        let title = text
            .lines()
            .find_map(|l| l.strip_prefix("# "))
            .unwrap_or(&path)
            .to_string();
        all.push(json!({
            "uri": format!("{GUIDE}{path}"),
            "name": format!("guide/{path}"),
            "title": title,
            "description": if path == "LANGUAGE.md" {
                "The netsmith language guide: its index, which links each chapter."
            } else {
                "A chapter of the netsmith language guide."
            },
            "mimeType": "text/markdown",
        }));
    }
    json!({ "resources": all })
}

fn walk(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() > MOST {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if !SKIPPED.contains(&name.as_str()) {
                walk(root, &path, files);
            }
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|e| TEXT.contains(&e.to_string_lossy().as_ref()))
            && let Ok(rel) = path.strip_prefix(root)
        {
            let rel: Vec<_> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect();
            files.push(rel.join("/"));
        }
    }
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("md") => "text/markdown",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("toml") => "application/toml",
        _ => "text/plain",
    }
}

/// `resources/read` of `uri`: its text, or why there is none.
pub fn read(root: Option<&Path>, uri: &str) -> Result<Value, String> {
    let text = if let Some(rel) = uri.strip_prefix(GUIDE) {
        netsmith_tools::init::guide()
            .into_iter()
            .find(|(path, _)| path == rel)
            .map(|(_, text)| text.to_string())
            .ok_or_else(|| format!("the guide has no `{rel}`"))?
    } else if let Some(rel) = uri.strip_prefix(PROJECT) {
        let root = root.ok_or("the server was not started in a project (no netsmith.toml)")?;
        let path =
            inside(root, rel).ok_or_else(|| format!("`{rel}` is not a file of the project"))?;
        fs::read_to_string(&path).map_err(|e| format!("{rel}: {e}"))?
    } else {
        return Err(format!(
            "`{uri}` is not a resource of this server: they begin with {PROJECT} or {GUIDE}"
        ));
    };
    Ok(json!({
        "contents": [{ "uri": uri, "mimeType": mime(uri), "text": text }]
    }))
}

/// The file `rel` names under `root`, when it is one and is inside it (no `..`, no absolute
/// path, no link out of the project).
fn inside(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let path = root.join(rel).canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    (path.starts_with(&root) && path.is_file()).then_some(path)
}

/// A skill's `name` and `description` from its front matter, and its text after it.
fn skill(text: &str) -> (String, String, &str) {
    let mut name = String::new();
    let mut description = String::new();
    let mut body = text;
    if let Some(rest) = text.strip_prefix("---\n")
        && let Some(end) = rest.find("\n---\n")
    {
        for line in rest[..end].lines() {
            if let Some(v) = line.strip_prefix("name:") {
                name = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("description:") {
                description = v.trim().to_string();
            }
        }
        body = rest[end + "\n---\n".len()..].trim_start();
    }
    (name, description, body)
}

/// `prompts/list`.
pub fn prompts() -> Value {
    let all: Vec<Value> = netsmith_tools::init::skills()
        .iter()
        .map(|(id, text)| {
            let (name, description, _) = skill(text);
            json!({
                "name": if name.is_empty() { id.to_string() } else { name },
                "description": description,
                "arguments": [{
                    "name": "request",
                    "description": "What to do, in the person's words; it follows the skill.",
                    "required": false,
                }],
            })
        })
        .collect();
    json!({ "prompts": all })
}

/// `prompts/get` of the skill `name`, with the request, if any, after its text.
pub fn prompt(name: &str, request: Option<&str>) -> Result<Value, String> {
    let (id, text) = netsmith_tools::init::skills()
        .iter()
        .find(|(id, text)| *id == name || skill(text).0 == name)
        .ok_or_else(|| format!("there is no prompt `{name}`"))?;
    let (_, description, body) = skill(text);
    let mut text = format!("Follow the netsmith skill `{id}`.\n\n{body}");
    if let Some(request) = request.filter(|r| !r.trim().is_empty()) {
        text.push_str(&format!("\n\nThe request: {request}"));
    }
    Ok(json!({
        "description": description,
        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_out_of_the_project_is_not_read() {
        let dir = netsmith_testkit::TempDir::new("mcp-inside");
        fs::write(dir.join(MANIFEST), "[project]\nname = \"p\"\n").unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.nsm"), "board B {}\n").unwrap();
        let root = Some(&*dir);
        assert!(read(root, "netsmith://project/src/main.nsm").is_ok());
        for uri in [
            "netsmith://project/../outside.nsm",
            "netsmith://project//etc/passwd",
            "netsmith://project/src/../../x",
            "netsmith://project/src",
            "file:///etc/passwd",
        ] {
            assert!(read(root, uri).is_err(), "{uri}");
        }
    }

    #[test]
    fn every_skill_is_a_prompt_with_its_text() {
        let listed = prompts();
        let names: Vec<_> = listed["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names.len(), netsmith_tools::init::skills().len());
        for name in &names {
            let got = prompt(name, Some("add an LED")).unwrap();
            let text = got["messages"][0]["content"]["text"].as_str().unwrap();
            assert!(
                !text.contains(&format!("name: {name}\n")),
                "{name}: the front matter"
            );
            assert!(text.ends_with("The request: add an LED"), "{name}");
        }
    }
}
