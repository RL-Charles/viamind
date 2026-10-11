//! A fake in-memory `CommandHost` served over an in-process transport
//! (`tokio::io::duplex`) and driven by real rmcp clients.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
};
use serde_json::{Value, json};
use viamind_mcp::{AgentId, CommandHost, HostCommand, ToolRegistry, register_host, serve};

/// A list of named items with `add_item` / `remove_item` and per-agent undo.
#[derive(Default)]
struct FakeHost {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    items: Vec<String>,
    undo: HashMap<AgentId, Vec<Op>>,
    redo: HashMap<AgentId, Vec<Op>>,
    /// Every executed command as `(agent, command)`.
    log: Vec<(String, String)>,
}

#[derive(Clone)]
enum Op {
    Add(String),
    Remove(String, usize),
}

impl State {
    fn apply(&mut self, op: &Op) {
        match op {
            Op::Add(name) => self.items.push(name.clone()),
            Op::Remove(_, index) => {
                self.items.remove(*index);
            }
        }
    }

    fn revert(&mut self, op: &Op) {
        match op {
            Op::Add(name) => self.items.retain(|item| item != name),
            Op::Remove(name, index) => self.items.insert((*index).min(self.items.len()), name.clone()),
        }
    }
}

fn describe(op: &Op) -> Value {
    match op {
        Op::Add(name) => json!({ "command": "add_item", "name": name }),
        Op::Remove(name, _) => json!({ "command": "remove_item", "name": name }),
    }
}

fn name_schema(what: &str) -> Value {
    json!({
        "type": "object",
        "properties": { "name": { "type": "string", "description": what } },
        "required": ["name"],
        "additionalProperties": false
    })
}

impl CommandHost for FakeHost {
    fn commands(&self) -> Vec<HostCommand> {
        vec![
            HostCommand {
                name: "add_item".into(),
                description: "Add an item to the end of the list.".into(),
                input_schema: name_schema("New item's name"),
            },
            HostCommand {
                name: "remove_item".into(),
                description: "Remove the item with this name.".into(),
                input_schema: name_schema("Item to remove"),
            },
        ]
    }

    fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String> {
        let item = input["name"].as_str().ok_or("`name` must be a string")?.to_owned();
        let mut state = self.state.lock().unwrap();
        let op = match name {
            "add_item" if state.items.contains(&item) => return Err(format!("an item named `{item}` already exists")),
            "add_item" => Op::Add(item),
            "remove_item" => {
                let index = state.items.iter().position(|i| *i == item).ok_or_else(|| format!("no item named `{item}`"))?;
                Op::Remove(item, index)
            }
            _ => return Err(format!("unknown command `{name}`")),
        };
        state.apply(&op);
        state.log.push((agent.0.clone(), name.to_owned()));
        state.undo.entry(agent.clone()).or_default().push(op.clone());
        state.redo.remove(agent);
        Ok(json!({ "done": describe(&op), "items": state.items }))
    }

    fn undo(&self, agent: &AgentId) -> Result<Value, String> {
        let mut state = self.state.lock().unwrap();
        let op = state.undo.get_mut(agent).and_then(Vec::pop).ok_or("nothing to undo")?;
        state.revert(&op);
        state.redo.entry(agent.clone()).or_default().push(op.clone());
        Ok(json!({ "undone": describe(&op), "items": state.items }))
    }

    fn redo(&self, agent: &AgentId) -> Result<Value, String> {
        let mut state = self.state.lock().unwrap();
        let op = state.redo.get_mut(agent).and_then(Vec::pop).ok_or("nothing to redo")?;
        state.apply(&op);
        state.undo.entry(agent.clone()).or_default().push(op.clone());
        Ok(json!({ "redone": describe(&op), "items": state.items }))
    }

    fn snapshot(&self) -> Value {
        json!({ "items": self.state.lock().unwrap().items })
    }
}

impl FakeHost {
    fn items(&self) -> Vec<String> {
        self.state.lock().unwrap().items.clone()
    }

    fn log(&self) -> Vec<(String, String)> {
        self.state.lock().unwrap().log.clone()
    }
}

type Client = RunningService<RoleClient, ClientConfig>;

fn registry_for(host: &Arc<FakeHost>) -> Arc<ToolRegistry> {
    let mut registry = ToolRegistry::with_builtins();
    register_host(&mut registry, host.clone()).unwrap();
    Arc::new(registry)
}

/// Serve `registry` in-process and connect a client that calls itself `client_name`.
async fn connect(registry: Arc<ToolRegistry>, client_name: &str) -> Client {
    let (server_end, client_end) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let server = serve(registry, server_end).await.expect("server handshake");
        server.waiting().await.expect("server task");
    });
    ClientConfig::new(ClientCapabilities::default(), Implementation::new(client_name, "1.0.0")).serve(client_end).await.expect("client handshake")
}

async fn call(client: &Client, tool: &'static str, args: Value) -> CallToolResult {
    let Value::Object(args) = args else { panic!("arguments must be an object") };
    client.call_tool(CallToolRequestParams::new(tool).with_arguments(args)).await.expect("tools/call")
}

/// The structured result of a successful call.
fn ok(result: CallToolResult) -> Value {
    assert_eq!(result.is_error, Some(false), "{result:?}");
    result.structured_content.expect("structured content")
}

/// The message of a failed call.
fn err(result: CallToolResult) -> String {
    assert_eq!(result.is_error, Some(true), "{result:?}");
    assert!(result.structured_content.is_none());
    result.content[0].as_text().expect("text content").text.clone()
}

#[tokio::test]
async fn tools_list_shows_host_commands_and_builtins() {
    let host = Arc::new(FakeHost::default());
    let client = connect(registry_for(&host), "lister").await;

    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, ["add_item", "echo", "get_design", "redo", "remove_item", "undo", "viamind_version"]);

    for command in host.commands() {
        let tool = tools.iter().find(|t| t.name == command.name).unwrap();
        assert_eq!(tool.description.as_deref(), Some(command.description.as_str()));
        assert_eq!(Value::Object((*tool.input_schema).clone()), command.input_schema, "schema of {}", command.name);
    }
    for builtin in ["undo", "redo", "get_design"] {
        let tool = tools.iter().find(|t| t.name == builtin).unwrap();
        assert!(tool.description.as_deref().is_some_and(|d| d.len() > 40), "{builtin} needs a real description");
        assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn call_execute_undo_redo_and_get_design() {
    let host = Arc::new(FakeHost::default());
    let client = connect(registry_for(&host), "agent-a").await;

    let out = ok(call(&client, "add_item", json!({ "name": "R1" })).await);
    assert_eq!(out["items"], json!(["R1"]));
    ok(call(&client, "add_item", json!({ "name": "C1" })).await);
    assert_eq!(host.items(), ["R1", "C1"]);
    ok(call(&client, "remove_item", json!({ "name": "R1" })).await);
    assert_eq!(host.items(), ["C1"]);

    assert_eq!(ok(call(&client, "get_design", json!({})).await), json!({ "items": ["C1"] }));

    let undone = ok(call(&client, "undo", json!({})).await);
    assert_eq!(undone["undone"], json!({ "command": "remove_item", "name": "R1" }));
    assert_eq!(host.items(), ["R1", "C1"]);
    ok(call(&client, "undo", json!({})).await);
    assert_eq!(host.items(), ["R1"]);

    let redone = ok(call(&client, "redo", json!({})).await);
    assert_eq!(redone["redone"], json!({ "command": "add_item", "name": "C1" }));
    assert_eq!(ok(call(&client, "get_design", json!({})).await), json!({ "items": ["R1", "C1"] }));

    // Every call ran as this connection's agent.
    let log = host.log();
    assert_eq!(log.len(), 3);
    assert!(log.iter().all(|(agent, _)| agent.starts_with("agent-a#")), "{log:?}");

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn host_errors_are_tool_errors() {
    let host = Arc::new(FakeHost::default());
    let client = connect(registry_for(&host), "agent-e").await;

    assert_eq!(err(call(&client, "remove_item", json!({ "name": "U9" })).await), "no item named `U9`");
    assert_eq!(err(call(&client, "add_item", json!({})).await), "`name` must be a string");
    assert_eq!(err(call(&client, "undo", json!({})).await), "nothing to undo");
    assert_eq!(err(call(&client, "redo", json!({})).await), "nothing to redo");
    ok(call(&client, "add_item", json!({ "name": "R1" })).await);
    assert_eq!(err(call(&client, "add_item", json!({ "name": "R1" })).await), "an item named `R1` already exists");
    assert_eq!(host.items(), ["R1"]);

    // Unknown tools stay protocol errors, not tool errors.
    assert!(client.call_tool(CallToolRequestParams::new("no_such_tool")).await.is_err());

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn each_connection_is_its_own_agent() {
    let host = Arc::new(FakeHost::default());
    let registry = registry_for(&host);
    let alice = connect(registry.clone(), "alice").await;
    let bob = connect(registry.clone(), "bob").await;
    let alice_again = connect(registry, "alice").await;

    ok(call(&alice, "add_item", json!({ "name": "R1" })).await);
    ok(call(&bob, "add_item", json!({ "name": "R2" })).await);
    ok(call(&alice_again, "add_item", json!({ "name": "R3" })).await);

    let agents: Vec<_> = host.log().into_iter().map(|(agent, _)| agent).collect();
    assert!(agents[0].starts_with("alice#") && agents[1].starts_with("bob#") && agents[2].starts_with("alice#"), "{agents:?}");
    assert_ne!(agents[0], agents[2], "two connections from the same client are different agents");

    // Undo is per agent: bob's undo leaves both of alice's sessions' items alone.
    let undone = ok(call(&bob, "undo", json!({})).await);
    assert_eq!(undone["undone"]["name"], "R2");
    assert_eq!(host.items(), ["R1", "R3"]);
    assert_eq!(err(call(&bob, "undo", json!({})).await), "nothing to undo");

    for client in [alice, bob, alice_again] {
        client.cancel().await.unwrap();
    }
}
