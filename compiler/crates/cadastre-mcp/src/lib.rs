//! The MCP server of Cadastre (`cadastre mcp [directory]`): AI agents explore a real project and
//! write views and checks against it. Read only: the agent writes its files with its own tools;
//! the server reads the project again at each call, compiles what the agent proposes and says
//! what it gives.
//!
//! Model Context Protocol over stdio, one JSON-RPC message per line: `initialize`, `tools/list`,
//! `tools/call`, `resources/list`, `resources/read`, `ping`.

mod tools;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The versions of the protocol this server speaks, the latest first.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The documentation of the language, as installed: the agent writes the syntax of this version.
pub const RESOURCES: &[(&str, &str, &str)] = &[
    (
        "cadastre://reference",
        "The exact syntax, kinds, keys, types and values of the language",
        include_str!(concat!(env!("OUT_DIR"), "/reference.md")),
    ),
    (
        "cadastre://views",
        "A cheat sheet for views: evaluation, recipes, pitfalls",
        include_str!(concat!(env!("OUT_DIR"), "/views.md")),
    ),
    (
        "cadastre://tutorial",
        "The three levels of the language, on a small project",
        include_str!(concat!(env!("OUT_DIR"), "/tutorial.md")),
    ),
];

const INSTRUCTIONS: &str = "Cadastre describes an information system as code: a registry (.ca files), plans (.cav files: views) and checks (.cac files). \
Explore the project with list_kinds, list_keys, search_resources and get_resource; write a view or a check, try it with check_view \
(its diagnostics) and eval_view (what it selects), then write it into a .cav file (a view) or a .cac file (a check) of the project with your own tools. \
The resources cadastre://reference, cadastre://views and cadastre://tutorial give the exact syntax of this version.";

/// Runs the server on stdin / stdout until stdin closes, on the project holding `directory`.
pub fn run(directory: PathBuf) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle(&directory, &line) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// The response to one message; `None` for a notification.
pub fn handle(directory: &Path, line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(error_response(Value::Null, -32700, &format!("parse error: {error}"))),
    };
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let version = PROTOCOL_VERSIONS.iter().find(|v| **v == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {}, "resources": {} },
                "serverInfo": { "name": "cadastre", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            Ok(match tools::call(directory, name, &arguments) {
                Ok(content) => json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&content).unwrap_or_default() }],
                    "structuredContent": content,
                }),
                Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
            })
        }
        "resources/list" => Ok(json!({
            "resources": RESOURCES
                .iter()
                .map(|(uri, description, _)| json!({ "uri": uri, "name": &uri["cadastre://".len()..], "description": description, "mimeType": "text/markdown" }))
                .collect::<Vec<_>>(),
        })),
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).unwrap_or_default();
            match RESOURCES.iter().find(|(u, _, _)| *u == uri) {
                Some((uri, _, text)) => {
                    Ok(json!({ "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": text }] }))
                }
                None => Err((-32002, format!("unknown resource: {uri}"))),
            }
        }
        _ => Err((-32601, format!("unknown method: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_response(id, code, &message),
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(tool: &str, arguments: Value) -> Value {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/tutorial");
        let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": arguments } });
        handle(&directory, &request.to_string()).unwrap()["result"].clone()
    }

    #[test]
    fn initialize_and_list() {
        let request =
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2024-11-05" } });
        let response = handle(Path::new("."), &request.to_string()).unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
        let notification = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle(Path::new("."), &notification.to_string()).is_none());
        let list =
            handle(Path::new("."), &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }).to_string()).unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn tools_on_the_tutorial() {
        let found = call("search_resources", json!({ "query": "order" }));
        let names: Vec<&str> = found["structuredContent"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        assert_eq!(names[0], "order");
        let resource = call("get_resource", json!({ "name": "orders" }))["structuredContent"].clone();
        assert_eq!(resource["id"], "sales.orders");
        assert_eq!(resource["outgoing"][0]["with"], "sales.orders-db");
        assert_eq!(resource["incoming"][0]["with"], "sales.shop");
        let checked =
            call("check_view", json!({ "text": "select app where onwer = ops" }))["structuredContent"].clone();
        assert_eq!(checked["valid"], false);
        assert_eq!(checked["diagnostics"][0]["help"], "did you mean `owner`?");
        let evaluated =
            call("eval_view", json!({ "name": "order-path", "format": "text" }))["structuredContent"].clone();
        assert_eq!(evaluated["edges"][1]["step"], 2);
        assert!(evaluated["rendering"].as_str().unwrap().starts_with("view order-path"));
        let keys = call("list_keys", json!({ "kind": "servr" }));
        assert_eq!(keys["isError"], true);
        assert!(keys["content"][0]["text"].as_str().unwrap().contains("did you mean server?"));
    }
}
