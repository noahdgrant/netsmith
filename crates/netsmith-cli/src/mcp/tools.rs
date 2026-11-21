//! The tools: one for each command of clap's model of the command line, named by its words
//! (`board check` is `board_check`), with an argument for each of its flags and positional
//! arguments (issue #1106). The names, the help, the types, the choices and the defaults are
//! read from the model as the process runs, so a command or a flag added to the command line is
//! a tool or an argument with no change here, and nothing can describe the commands a second
//! time. A call is turned back into the command's words ([`Tool::argv`]) and run as the command
//! line runs it.
//!
//! The list is short (issue #1133), because a host sends all of it to the model: a tool has the
//! first line of its command's help and an argument the first line of its help, with its choices
//! and default. The whole `--help` of a command is a resource ([`Tool::help`], `netsmith://help/
//! board-fab`), which an agent reads when it needs it.

use std::any::TypeId;
use std::path::PathBuf;

use clap::ArgAction;
use clap::Subcommand;
use serde_json::{Map, Value, json};

/// The commands that are not tools: the server itself, and the editor's server, which never ends
/// (a running editor is reached through its own address: `mcp --editor`, issue #1108).
const NOT_TOOLS: [&[&str]; 2] = [&["mcp"], &["view", "editor"]];

/// The resources that hold the help of the commands.
pub const HELP: &str = "netsmith://help/";

/// What `netsmith <words> --help` prints, as clap prints it (the short layout when no argument
/// has a long help, the long one otherwise), without colour.
fn help(words: &[String], command: &clap::Command) -> String {
    let name = format!("netsmith {}", words.join(" "));
    match command
        .clone()
        .bin_name(&name)
        .try_get_matches_from([name.as_str(), "--help"])
    {
        Err(e) => e.to_string(),
        Ok(_) => String::new(),
    }
}

/// The first line of a text.
fn first_line(text: &str) -> String {
    text.trim()
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// One command as a tool.
#[derive(Debug, Clone)]
pub struct Tool {
    /// `board_check`.
    pub name: String,
    /// `["board", "check"]`.
    pub words: Vec<String>,
    pub description: String,
    /// What `netsmith <words> --help` prints.
    pub help: String,
    /// The JSON Schema of the arguments.
    pub schema: Value,
    params: Vec<Param>,
}

/// One argument of a command.
#[derive(Debug, Clone)]
struct Param {
    /// The property of the call's arguments: clap's id of the argument (`place_seeds`).
    key: String,
    /// `--place-seeds` (or `-x` for a flag with a short name only); `None` for a positional.
    flag: Option<String>,
    how: How,
}

#[derive(Debug, Clone, PartialEq)]
enum How {
    /// A flag with no value: `true` gives it.
    Switch,
    /// A flag given as many times as the number.
    Count,
    /// A flag with a value, or with a value that may be left out (`true` gives the flag alone).
    Value {
        optional: bool,
    },
    /// A flag whose values are a list: each occurrence takes `per` of them (`None`: all).
    Values {
        per: Option<usize>,
    },
    Positional {
        many: bool,
    },
}

/// Every command of the command line that is a tool, in the order of `netsmith --help`.
pub fn tools() -> Vec<Tool> {
    let mut model = crate::commands::Command::augment_subcommands(clap::Command::new("netsmith"));
    model.build();
    let mut all = Vec::new();
    walk(&[], &model, &mut all);
    all
}

fn walk(words: &[String], command: &clap::Command, all: &mut Vec<Tool>) {
    let subs: Vec<_> = command
        .get_subcommands()
        .filter(|s| s.get_name() != "help")
        .collect();
    if subs.is_empty() && !words.is_empty() {
        if !NOT_TOOLS.iter().any(|n| n.iter().eq(words.iter())) {
            all.push(Tool::of(words, command));
        }
        return;
    }
    for sub in subs {
        let mut words = words.to_vec();
        words.push(sub.get_name().to_string());
        walk(&words, sub, all);
    }
}

impl Tool {
    fn of(words: &[String], command: &clap::Command) -> Tool {
        let mut params = Vec::new();
        let mut properties = Map::new();
        let mut required = Vec::new();
        let mut options: Vec<&clap::Arg> = Vec::new();
        let mut positionals: Vec<&clap::Arg> = Vec::new();
        for arg in command.get_arguments() {
            if arg.is_hide_set()
                || matches!(
                    arg.get_action(),
                    ArgAction::Help
                        | ArgAction::HelpShort
                        | ArgAction::HelpLong
                        | ArgAction::Version
                )
            {
                continue;
            }
            if arg.is_positional() {
                positionals.push(arg);
            } else {
                options.push(arg);
            }
        }
        positionals.sort_by_key(|a| a.get_index());
        for arg in options.into_iter().chain(positionals) {
            let (param, schema) = param(arg);
            if arg.is_required_set() {
                required.push(Value::String(param.key.clone()));
            }
            properties.insert(param.key.clone(), schema);
            params.push(param);
        }
        // `--format json` and `--progress json` are what a call asks for unless it chooses (the
        // server's instructions say so once).
        for key in ["format", "progress"] {
            if let Some(schema) = properties.get_mut(key)
                && schema["enum"]
                    .as_array()
                    .is_some_and(|e| e.iter().any(|v| v == "json"))
            {
                schema["default"] = Value::String("json".into());
            }
        }
        let about = command
            .get_about()
            .or(command.get_long_about())
            .map(|s| first_line(&s.to_string()))
            .unwrap_or_default();
        Tool {
            name: words.join("_").replace('-', "_"),
            words: words.to_vec(),
            description: format!("`netsmith {}`: {about}", words.join(" ")),
            help: help(words, command),
            schema: json!({
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            }),
            params,
        }
    }

    /// The resource that has the command's whole help: `netsmith://help/board-fab`.
    pub fn help_uri(&self) -> String {
        format!("{HELP}{}", self.words.join("-"))
    }

    /// The tool as `tools/list` gives it.
    pub fn listed(&self) -> Value {
        json!({
            "name": self.name,
            "title": format!("netsmith {}", self.words.join(" ")),
            "description": self.description,
            "inputSchema": self.schema,
        })
    }

    /// Whether the command has `--format json`: the tool asks for it unless told otherwise.
    pub fn has_json_format(&self) -> bool {
        self.has_choice("format", "json")
    }

    fn has_choice(&self, key: &str, value: &str) -> bool {
        self.schema["properties"][key]["enum"]
            .as_array()
            .is_some_and(|e| e.iter().any(|v| v == value))
    }

    /// Whether a call with `arguments` asks for the command's JSON.
    pub fn wants_json(&self, arguments: &Map<String, Value>) -> bool {
        match arguments.get("format") {
            Some(Value::String(f)) => f == "json",
            Some(Value::Null) | None => self.has_json_format(),
            _ => false,
        }
    }

    /// [`Tool::argv`], and what the command reads on stdin: an argument of one value given a
    /// JSON object (a document, such as the shapes of `routes_add`, issue #1472) is given to the
    /// command as `-`, its file being stdin, and the object is written there as JSON. A document
    /// so has no limit of the length of a command line's word. At most one argument may be one.
    pub fn argv_with_input(
        &self,
        arguments: &Map<String, Value>,
    ) -> Result<(Vec<String>, Option<String>), String> {
        let mut arguments = arguments.clone();
        let mut input = None;
        for param in &self.params {
            let single = matches!(
                param.how,
                How::Value { .. } | How::Positional { many: false }
            );
            if let Some(Value::Object(doc)) = arguments.get(&param.key)
                && single
            {
                if input.is_some() {
                    return Err(format!(
                        "`{}`: only one argument may be a document (a JSON object)",
                        param.key
                    ));
                }
                input = Some(Value::Object(doc.clone()).to_string());
                arguments.insert(param.key.clone(), Value::String("-".into()));
            }
        }
        Ok((self.argv(&arguments)?, input))
    }

    /// The words of the command line for a call with `arguments`, after `netsmith`: the
    /// command's words, its flags, then `--` and its positional arguments. `--format json` and
    /// `--progress json` are added where the command has them and the call does not choose,
    /// so the result is the JSON document and the progress lines can be told from the rest.
    pub fn argv(&self, arguments: &Map<String, Value>) -> Result<Vec<String>, String> {
        for key in arguments.keys() {
            if !self.params.iter().any(|p| &p.key == key) {
                let known: Vec<_> = self.params.iter().map(|p| p.key.as_str()).collect();
                return Err(format!(
                    "`{}` has no argument `{key}`; it has: {}",
                    self.name,
                    if known.is_empty() {
                        "none".to_string()
                    } else {
                        known.join(", ")
                    }
                ));
            }
        }
        let mut argv = self.words.clone();
        let mut positional = Vec::new();
        for param in &self.params {
            let given = arguments.get(&param.key).filter(|v| !v.is_null());
            let value = match given {
                Some(v) => v.clone(),
                None if (param.key == "format" && self.has_json_format())
                    || (param.key == "progress" && self.has_choice("progress", "json")) =>
                {
                    Value::String("json".into())
                }
                None => continue,
            };
            let wrong = |what: &str| format!("`{}`: {what}, not {value}", param.key);
            let flag = param.flag.clone().unwrap_or_default();
            match &param.how {
                How::Switch => match value {
                    Value::Bool(true) => argv.push(flag),
                    Value::Bool(false) => {}
                    _ => return Err(wrong("true or false")),
                },
                How::Count => {
                    let n = value.as_u64().ok_or_else(|| wrong("a count"))?;
                    for _ in 0..n {
                        argv.push(flag.clone());
                    }
                }
                How::Value { optional } => match &value {
                    Value::Bool(true) if *optional => argv.push(flag),
                    Value::Bool(false) if *optional => {}
                    v => argv.push(format!(
                        "{flag}={}",
                        scalar(v).ok_or_else(|| wrong("a value"))?
                    )),
                },
                How::Values { per } => {
                    let items = list(&value).ok_or_else(|| wrong("a value or a list of values"))?;
                    match per {
                        Some(1) => {
                            for item in items {
                                argv.push(format!("{flag}={item}"));
                            }
                        }
                        Some(n) => {
                            for chunk in items.chunks(*n) {
                                argv.push(flag.clone());
                                argv.extend(chunk.iter().cloned());
                            }
                        }
                        None => {
                            argv.push(flag);
                            argv.extend(items);
                        }
                    }
                }
                How::Positional { many } => {
                    if *many {
                        positional.extend(list(&value).ok_or_else(|| wrong("a list of values"))?);
                    } else {
                        positional.push(scalar(&value).ok_or_else(|| wrong("a value"))?);
                    }
                }
            }
        }
        if !positional.is_empty() {
            argv.push("--".into());
            argv.extend(positional);
        }
        Ok(argv)
    }
}

/// An argument of the model as a [`Param`] and the JSON Schema of its value.
fn param(arg: &clap::Arg) -> (Param, Value) {
    let key = arg.get_id().to_string();
    let flag = arg
        .get_long()
        .map(|l| format!("--{l}"))
        .or_else(|| arg.get_short().map(|s| format!("-{s}")));
    let range = arg.get_num_args().unwrap_or_default();
    let many_per = range.max_values() > 1;
    let appends = matches!(arg.get_action(), ArgAction::Append);
    let how = match arg.get_action() {
        ArgAction::SetTrue | ArgAction::SetFalse => How::Switch,
        ArgAction::Count => How::Count,
        _ if arg.is_positional() => How::Positional {
            many: many_per || appends,
        },
        _ if many_per => How::Values {
            per: (range.max_values() != usize::MAX).then_some(range.max_values()),
        },
        _ if appends => How::Values { per: Some(1) },
        _ => How::Value {
            optional: range.min_values() == 0,
        },
    };
    let mut item = value_schema(arg);
    let mut schema = match &how {
        How::Switch => json!({ "type": "boolean" }),
        How::Count => json!({ "type": "integer", "minimum": 0 }),
        How::Values { .. } | How::Positional { many: true } => {
            json!({ "type": "array", "items": item })
        }
        How::Value { optional: true } => {
            if let Some(t) = item.get("type").cloned() {
                item["type"] = json!([t, "boolean"]);
            }
            item
        }
        _ => item,
    };
    let said = arg
        .get_help()
        .or(arg.get_long_help())
        .map(|s| first_line(&s.to_string()))
        .unwrap_or_default();
    // The property is the flag's name unless clap's id differs, so the flag is said only then.
    let named = |f: &String| f.trim_start_matches('-').replace('-', "_") == key;
    let name = match (&flag, &how) {
        (Some(f), How::Value { optional: true }) => {
            format!("`true` gives `{f}` with no value.")
        }
        (Some(f), _) if !named(f) => format!("The flag `{f}`."),
        (Some(_), _) => String::new(),
        (None, _) => {
            let name = arg
                .get_value_names()
                .map(|n| {
                    n.iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| key.to_uppercase());
            format!("The argument `{name}`.")
        }
    };
    let description = format!("{said} {name}");
    schema["description"] = Value::String(description.trim().to_string());
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    if let [default] = defaults.as_slice()
        && !matches!(how, How::Switch | How::Count)
    {
        schema["default"] = typed(default, &schema);
    }
    (Param { key, flag, how }, schema)
}

/// The schema of one value of `arg`: its choices, or the type its parser makes.
fn value_schema(arg: &clap::Arg) -> Value {
    let choices: Vec<Value> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| Value::String(v.get_name().to_string()))
        .collect();
    if !choices.is_empty() {
        return json!({ "type": "string", "enum": choices });
    }
    let id = arg.get_value_parser().type_id();
    let integer = [
        TypeId::of::<u8>(),
        TypeId::of::<u16>(),
        TypeId::of::<u32>(),
        TypeId::of::<u64>(),
        TypeId::of::<usize>(),
        TypeId::of::<i32>(),
        TypeId::of::<i64>(),
    ];
    if integer.iter().any(|t| id == *t) {
        json!({ "type": "integer" })
    } else if id == TypeId::of::<f64>() || id == TypeId::of::<f32>() {
        json!({ "type": "number" })
    } else if id == TypeId::of::<PathBuf>() {
        json!({ "type": "string", "format": "path" })
    } else {
        json!({ "type": "string" })
    }
}

/// A default as the schema's type says (`8787` an integer, `text` a string).
fn typed(default: &str, schema: &Value) -> Value {
    let kind = schema["type"].as_str().or(schema["type"][0].as_str());
    match kind {
        Some("integer") => default.parse::<i64>().map(Value::from).ok(),
        Some("number") => default
            .parse::<f64>()
            .ok()
            .and_then(|f| serde_json::Number::from_f64(f).map(Value::Number)),
        _ => None,
    }
    .unwrap_or_else(|| Value::String(default.to_string()))
}

/// One value as the command line's word.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A list of values (or one value) as the command line's words.
fn list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Array(items) => items.iter().map(scalar).collect(),
        v => scalar(v).map(|s| vec![s]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> Tool {
        tools().into_iter().find(|t| t.name == name).unwrap()
    }

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn the_server_and_the_editor_are_not_tools_and_every_name_is_its_own() {
        let all = tools();
        assert!(all.len() > 30, "{}", all.len());
        for name in ["mcp", "view_editor"] {
            assert!(all.iter().all(|t| t.name != name), "{name}");
        }
        assert!(all.iter().any(|t| t.name == "view_editor_3d"));
        let mut names: Vec<_> = all.iter().map(|t| &t.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all.len());
    }

    #[test]
    fn a_call_is_the_commands_words_with_json_asked_for() {
        let check = tool("board_check");
        assert_eq!(
            check
                .argv(&args(json!({ "file": "led.nsm", "strict": true })))
                .unwrap(),
            [
                "board",
                "check",
                "--strict",
                "--format=json",
                "--",
                "led.nsm"
            ]
        );
        assert_eq!(
            check
                .argv(&args(json!({ "format": "text", "notes": "none" })))
                .unwrap(),
            ["board", "check", "--format=text", "--notes=none"]
        );
        assert!(check.has_json_format());
        assert_eq!(check.schema["properties"]["format"]["default"], "json");
        assert!(!check.wants_json(&args(json!({ "format": "text" }))));
    }

    #[test]
    fn lists_counts_and_optional_values_become_their_flags() {
        let fab = tool("board_fab");
        let argv = fab
            .argv(&args(
                json!({ "to_editor": true, "place_seeds": 2, "format": "text" }),
            ))
            .unwrap();
        assert!(argv.contains(&"--to-editor".to_string()), "{argv:?}");
        assert!(argv.contains(&"--place-seeds=2".to_string()), "{argv:?}");
        assert!(argv.contains(&"--progress=json".to_string()), "{argv:?}");
        assert_eq!(fab.schema["properties"]["place_seeds"]["type"], "integer");
        let lock = tool("project_lock");
        assert_eq!(
            lock.argv(&args(json!({ "part": ["a.r1", "a.r2"] })))
                .unwrap(),
            [
                "project",
                "lock",
                "--part=a.r1",
                "--part=a.r2",
                "--format=json"
            ]
        );
        let summary = tool("layout_summary");
        let argv = summary
            .argv(&args(
                json!({ "between": ["U1.1", "U2.3", "J1.1", "J1.2"] }),
            ))
            .unwrap();
        assert!(
            argv.windows(3).any(|w| w == ["--between", "U1.1", "U2.3"])
                && argv.windows(3).any(|w| w == ["--between", "J1.1", "J1.2"]),
            "{argv:?}"
        );
    }

    #[test]
    fn a_document_given_to_the_call_is_the_commands_stdin() {
        // `routes_add` (issue #1472): the shapes as an object are read from stdin, `-`.
        let add = tool("routes_add");
        let doc = json!({ "track": [{ "net": "GND", "from": [1, 1], "to": [2, 1] }] });
        let (argv, input) = add
            .argv_with_input(&args(json!({ "shapes": doc, "stop_on_refusal": true })))
            .unwrap();
        assert_eq!(
            argv,
            [
                "routes",
                "add",
                "--stop-on-refusal",
                "--format=json",
                "--",
                "-"
            ]
        );
        let input: Value = serde_json::from_str(&input.unwrap()).unwrap();
        assert_eq!(input, doc);
        // A path is a path, and nothing is read on stdin.
        let (argv, input) = add
            .argv_with_input(&args(json!({ "shapes": "shapes.toml" })))
            .unwrap();
        assert_eq!(argv.last().unwrap(), "shapes.toml");
        assert!(input.is_none());
    }

    #[test]
    fn an_argument_the_command_lacks_or_of_the_wrong_kind_is_refused() {
        let check = tool("board_check");
        let e = check.argv(&args(json!({ "frobnicate": 1 }))).unwrap_err();
        assert!(
            e.contains("no argument `frobnicate`") && e.contains("strict"),
            "{e}"
        );
        let e = check.argv(&args(json!({ "strict": "yes" }))).unwrap_err();
        assert!(e.contains("true or false"), "{e}");
    }
}
