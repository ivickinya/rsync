use std::{
    env, fs,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::State;
use tempfile::NamedTempFile;
use uuid::Uuid;

const MAX_EVENTS: usize = 500;
const MAX_SUMMARY_CHARS: usize = 1200;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivityEvent {
    id: String,
    event_type: String,
    summary: String,
    created_at_millis: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    source: String,
}

#[derive(Clone)]
pub struct AgentActivityStore {
    data_directory: PathBuf,
    events_directory: PathBuf,
}

impl AgentActivityStore {
    pub fn new(data_directory: PathBuf) -> Self {
        Self {
            events_directory: data_directory.join("agent-activity"),
            data_directory,
        }
    }

    fn record(
        &self,
        event_type: &str,
        summary: &str,
        tool_name: Option<&str>,
        session_id: Option<&str>,
        source: &str,
    ) -> Result<AgentActivityEvent, String> {
        let summary = summary.trim();
        if summary.is_empty() || summary.chars().count() > MAX_SUMMARY_CHARS {
            return Err(format!(
                "Activity summaries must contain between 1 and {MAX_SUMMARY_CHARS} characters."
            ));
        }
        let tool_name = tool_name.map(str::trim).filter(|value| !value.is_empty());
        if tool_name.is_some_and(|value| value.chars().count() > 200) {
            return Err("Tool names must be 200 characters or fewer.".to_string());
        }
        let session_id = session_id.map(str::trim).filter(|value| !value.is_empty());
        if session_id.is_some_and(|value| value.chars().count() > 200) {
            return Err("Session IDs must be 200 characters or fewer.".to_string());
        }

        fs::create_dir_all(&self.events_directory)
            .map_err(|error| format!("Could not create agent activity storage: {error}"))?;
        let created_at_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("Could not read the system clock: {error}"))?
            .as_millis();
        let event = AgentActivityEvent {
            id: Uuid::new_v4().to_string(),
            event_type: event_type.to_string(),
            summary: summary.to_string(),
            created_at_millis,
            tool_name: tool_name.map(str::to_string),
            session_id: session_id.map(str::to_string),
            source: source.to_string(),
        };
        let mut temporary = NamedTempFile::new_in(&self.events_directory)
            .map_err(|error| format!("Could not create an activity event file: {error}"))?;
        serde_json::to_writer(&mut temporary, &event)
            .map_err(|error| format!("Could not serialize an activity event: {error}"))?;
        temporary
            .flush()
            .map_err(|error| format!("Could not write an activity event: {error}"))?;
        let file_name = format!("{created_at_millis}-{}.json", event.id);
        temporary
            .persist(self.events_directory.join(file_name))
            .map_err(|error| format!("Could not save an activity event: {}", error.error))?;
        self.prune()?;
        Ok(event)
    }

    fn list(&self) -> Result<Vec<AgentActivityEvent>, String> {
        let entries = match fs::read_dir(&self.events_directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("Could not read agent activity: {error}")),
        };
        let mut files = entries
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| format!("Could not inspect agent activity: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        files.retain(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        });
        files.sort();

        files
            .iter()
            .rev()
            .take(100)
            .map(|path| {
                let bytes = fs::read(path)
                    .map_err(|error| format!("Could not read agent activity event: {error}"))?;
                serde_json::from_slice(&bytes)
                    .map_err(|error| format!("Could not parse agent activity event: {error}"))
            })
            .collect()
    }

    fn prune(&self) -> Result<(), String> {
        let entries = fs::read_dir(&self.events_directory)
            .map_err(|error| format!("Could not inspect agent activity storage: {error}"))?;
        let mut files = entries
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| format!("Could not inspect agent activity storage: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        files.retain(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        });
        files.sort();
        for path in files.iter().take(files.len().saturating_sub(MAX_EVENTS)) {
            fs::remove_file(path)
                .map_err(|error| format!("Could not prune old agent activity: {error}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallMcpInput {
    client: String,
    workspace_path: Option<String>,
}

#[tauri::command]
pub fn list_agent_activity(
    activity: State<'_, AgentActivityStore>,
) -> Result<Vec<AgentActivityEvent>, String> {
    activity.list()
}

#[tauri::command]
pub fn get_agent_mcp_config(activity: State<'_, AgentActivityStore>) -> Result<Value, String> {
    mcp_server_config(&activity)
}

#[tauri::command]
pub fn install_agent_mcp(
    activity: State<'_, AgentActivityStore>,
    input: InstallMcpInput,
) -> Result<String, String> {
    configure_agent_mcp(&activity, &input)
}

fn configure_agent_mcp(
    activity: &AgentActivityStore,
    input: &InstallMcpInput,
) -> Result<String, String> {
    let home = home_directory()?;
    configure_agent_mcp_with_home(&activity, &input, &home)
}

fn configure_agent_mcp_with_home(
    activity: &AgentActivityStore,
    input: &InstallMcpInput,
    home: &Path,
) -> Result<String, String> {
    let (client_name, config_path, servers_key, restart_instructions) = match input.client.as_str()
    {
        "vscode" | "claude-code" => {
            let workspace_path = input
                .workspace_path
                .as_deref()
                .ok_or_else(|| "Choose a workspace folder for this MCP client.".to_string())?;
            let workspace = Path::new(workspace_path)
                .canonicalize()
                .map_err(|error| format!("Could not open the selected workspace: {error}"))?;
            if !workspace.is_dir() {
                return Err("Choose an existing workspace folder.".to_string());
            }
            if input.client == "vscode" {
                let vscode_directory = workspace.join(".vscode");
                fs::create_dir_all(&vscode_directory).map_err(|error| {
                    format!("Could not create the workspace .vscode folder: {error}")
                })?;
                let canonical_directory = vscode_directory.canonicalize().map_err(|error| {
                    format!("Could not inspect the workspace .vscode folder: {error}")
                })?;
                if !canonical_directory.starts_with(&workspace) {
                    return Err(
                        "The workspace .vscode folder must remain inside the selected workspace."
                            .to_string(),
                    );
                }
                (
                    "VS Code Copilot",
                    canonical_directory.join("mcp.json"),
                    "servers",
                    "Reload VS Code or restart the Copilot agent session",
                )
            } else {
                (
                    "Claude Code",
                    workspace.join(".mcp.json"),
                    "mcpServers",
                    "Restart Claude Code in this workspace",
                )
            }
        }
        "claude-desktop" => {
            #[cfg(target_os = "macos")]
            let path = home
                .join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json");
            #[cfg(target_os = "windows")]
            let path = env::var_os("APPDATA")
                .map(PathBuf::from)
                .ok_or_else(|| "Could not determine the application data folder.".to_string())?
                .join("Claude")
                .join("claude_desktop_config.json");
            #[cfg(target_os = "linux")]
            let path = home
                .join(".config")
                .join("Claude")
                .join("claude_desktop_config.json");
            (
                "Claude Desktop",
                path,
                "mcpServers",
                "Completely quit and restart Claude Desktop",
            )
        }
        "gemini-cli" => (
            "Gemini CLI",
            home.join(".gemini").join("settings.json"),
            "mcpServers",
            "Restart Gemini CLI",
        ),
        _ => return Err("Choose a supported local MCP client.".to_string()),
    };

    configure_mcp_file(activity, &config_path, servers_key)?;
    Ok(format!(
        "Connected {client_name} to the Rsync Desktop local MCP server in {}. {restart_instructions} to load it.",
        config_path.display()
    ))
}

fn configure_mcp_file(
    activity: &AgentActivityStore,
    config_path: &Path,
    servers_key: &str,
) -> Result<(), String> {
    let parent = config_path
        .parent()
        .ok_or_else(|| "Could not determine the MCP configuration folder.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create the MCP configuration folder: {error}"))?;
    let mut config = if config_path.exists() {
        let bytes = fs::read(&config_path)
            .map_err(|error| format!("Could not read the existing MCP configuration: {error}"))?;
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|error| format!("The existing MCP configuration is not valid JSON: {error}"))?
    } else {
        json!({})
    };
    if !config.is_object() {
        return Err("The existing MCP configuration must be a JSON object.".to_string());
    }
    match config.get(servers_key) {
        None => config[servers_key] = json!({}),
        Some(servers) if servers.is_object() => {}
        Some(_) => {
            return Err(format!(
                "The existing MCP configuration has a non-object \"{servers_key}\" value."
            )
            .to_string());
        }
    }

    let server = mcp_server_config(activity)?;
    let servers = config[servers_key]
        .as_object_mut()
        .ok_or_else(|| "Could not update the VS Code MCP server list.".to_string())?;
    if let Some(existing) = servers.get("rsync-desktop") {
        let is_ours = existing
            .get("args")
            .and_then(Value::as_array)
            .is_some_and(|args| args.iter().any(|arg| arg.as_str() == Some("--mcp")))
            && existing.get("command").and_then(Value::as_str)
                == server.get("command").and_then(Value::as_str);
        if !is_ours {
            return Err(
                "A different MCP server already uses the name \"rsync-desktop\" in this workspace. Rename that entry first."
                    .to_string(),
            );
        }
    }
    servers.insert("rsync-desktop".to_string(), server);
    write_json_atomically(config_path, &config)
}

fn mcp_server_config(activity: &AgentActivityStore) -> Result<Value, String> {
    let executable = env::current_exe()
        .map_err(|error| format!("Could not locate the Rsync Desktop executable: {error}"))?;
    Ok(json!({
        "type": "stdio",
        "command": executable.to_string_lossy(),
        "args": ["--mcp"],
        "env": {
            "RSYNC_DESKTOP_DATA_DIR": activity.data_directory.to_string_lossy()
        }
    }))
}

#[tauri::command]
pub fn install_copilot_hooks(activity: State<'_, AgentActivityStore>) -> Result<String, String> {
    if cfg!(target_os = "windows") {
        return Err(
            "Automatic Copilot hooks are not yet supported on Windows. The MCP progress tools remain available."
                .to_string(),
        );
    }
    let home = home_directory()
        .map_err(|_| "Could not determine the user home folder for Copilot hooks.".to_string())?;
    install_copilot_hooks_for_home(&activity, &home)
}

fn install_copilot_hooks_for_home(
    activity: &AgentActivityStore,
    home: &Path,
) -> Result<String, String> {
    let hooks_directory = home.join(".copilot").join("hooks");
    fs::create_dir_all(&hooks_directory)
        .map_err(|error| format!("Could not create the Copilot hooks folder: {error}"))?;
    let config_path = hooks_directory.join("rsync-desktop.json");
    if config_path.exists() {
        return Err(format!(
            "{} already exists. Review or remove that file before installing Rsync Desktop hooks.",
            config_path.display()
        ));
    }

    let executable = env::current_exe()
        .map_err(|error| format!("Could not locate the Rsync Desktop executable: {error}"))?;
    let executable = executable.to_string_lossy().into_owned();
    let event_types = [
        "sessionStart",
        "userPromptSubmitted",
        "preToolUse",
        "postToolUse",
        "sessionEnd",
    ];
    let hooks = event_types
        .into_iter()
        .map(|event_type| {
            (
                event_type.to_string(),
                json!([{
                    "type": "command",
                    "command": format!("{} --copilot-hook", shell_quote(&executable)),
                    "env": {
                        "RSYNC_DESKTOP_DATA_DIR": activity.data_directory.to_string_lossy()
                    },
                    "timeoutSec": 10
                }]),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let config = json!({
        "version": 1,
        "hooks": hooks
    });
    write_json_atomically(&config_path, &config)?;
    Ok(format!(
        "Installed user-level Copilot hooks at {}. They record event types and tool names only, not prompt text, tool arguments, results, or reasoning. Start a new Copilot session to load them.",
        config_path.display()
    ))
}

fn write_json_atomically(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Could not determine the configuration directory.".to_string())?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("Could not create a temporary configuration: {error}"))?;
    serde_json::to_writer_pretty(&mut temporary, value)
        .map_err(|error| format!("Could not serialize the configuration: {error}"))?;
    temporary
        .write_all(b"\n")
        .map_err(|error| format!("Could not finish writing the configuration: {error}"))?;
    temporary
        .flush()
        .map_err(|error| format!("Could not flush the configuration: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Could not install the configuration: {}", error.error))?;
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub fn standalone_store() -> Result<AgentActivityStore, String> {
    let data_directory = match env::var_os("RSYNC_DESKTOP_DATA_DIR") {
        Some(path) => PathBuf::from(path),
        None => default_data_directory()?,
    };
    Ok(AgentActivityStore::new(data_directory))
}

fn default_data_directory() -> Result<PathBuf, String> {
    let home = home_directory()?;
    #[cfg(target_os = "macos")]
    {
        Ok(home
            .join("Library")
            .join("Application Support")
            .join("com.nexifyup.rsync-desktop"))
    }
    #[cfg(target_os = "windows")]
    {
        let app_data = env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "Could not determine the application data folder.".to_string())?;
        Ok(app_data.join("com.nexifyup.rsync-desktop"))
    }
    #[cfg(target_os = "linux")]
    {
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"));
        Ok(data_home.join("com.nexifyup.rsync-desktop"))
    }
}

fn home_directory() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| "Could not determine the user home folder.".to_string())
}

pub fn run_copilot_hook() -> Result<(), String> {
    let mut payload = String::new();
    io::stdin()
        .take(2 * 1024 * 1024)
        .read_to_string(&mut payload)
        .map_err(|error| format!("Could not read Copilot hook input: {error}"))?;
    let event: Value = serde_json::from_str(&payload)
        .map_err(|error| format!("Could not parse Copilot hook input: {error}"))?;
    record_hook_event(&standalone_store()?, &event)?;
    println!("{{}}");
    Ok(())
}

fn record_hook_event(store: &AgentActivityStore, event: &Value) -> Result<(), String> {
    let hook_name = event
        .get("hookEventName")
        .and_then(Value::as_str)
        .ok_or_else(|| "Copilot hook input did not include hookEventName.".to_string())?;
    let (event_type, summary) = match hook_name {
        "sessionStart" => ("session.started", "Copilot agent session started"),
        "userPromptSubmitted" => ("prompt.submitted", "Copilot agent received a prompt"),
        "preToolUse" => ("tool.started", "Copilot agent started a tool"),
        "postToolUse" => ("tool.completed", "Copilot agent completed a tool"),
        "sessionEnd" => ("session.ended", "Copilot agent session ended"),
        _ => return Err(format!("Unsupported Copilot hook event: {hook_name}")),
    };
    let tool_name = event.get("toolName").and_then(Value::as_str);
    let summary = if matches!(hook_name, "preToolUse" | "postToolUse") {
        match tool_name {
            Some(name) if !name.is_empty() => format!("{summary}: {name}"),
            _ => summary.to_string(),
        }
    } else {
        summary.to_string()
    };
    store.record(event_type, &summary, tool_name, None, "copilot-hook")?;
    Ok(())
}

pub fn run_mcp_stdio() -> Result<(), String> {
    let store = standalone_store()?;
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("Could not read MCP request: {error}"))?;
        if line.len() > 1024 * 1024 {
            return Err("MCP messages must be no larger than 1 MiB.".to_string());
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                writeln!(
                    stdout,
                    "{}",
                    json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": { "code": -32700, "message": format!("Parse error: {error}") }
                    })
                )
                .map_err(|error| format!("Could not write MCP response: {error}"))?;
                stdout
                    .flush()
                    .map_err(|error| format!("Could not flush MCP response: {error}"))?;
                continue;
            }
        };
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            continue;
        };
        if method.starts_with("notifications/") {
            continue;
        }
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        let response = match method {
            "initialize" => {
                let requested_version = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("2025-03-26");
                let protocol_version = if matches!(requested_version, "2024-11-05" | "2025-03-26") {
                    requested_version
                } else {
                    "2025-03-26"
                };
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": protocol_version,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": {
                            "name": "rsync-desktop-agent-activity",
                            "version": env!("CARGO_PKG_VERSION")
                        },
                        "instructions": "Reports explicit agent progress and provides a sanitized activity feed. Copilot hooks report tool names and lifecycle events only; private reasoning, prompt text, tool arguments, and tool results are not collected."
                    }
                })
            }
            "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": mcp_tools() }
            }),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let result = call_mcp_tool(&store, name, &arguments);
                match result {
                    Ok(text) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": text }],
                            "isError": false
                        }
                    }),
                    Err(error) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": error }],
                            "isError": true
                        }
                    }),
                }
            }
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {method}") }
            }),
        };
        serde_json::to_writer(&mut stdout, &response)
            .map_err(|error| format!("Could not encode MCP response: {error}"))?;
        stdout
            .write_all(b"\n")
            .map_err(|error| format!("Could not write MCP response: {error}"))?;
        stdout
            .flush()
            .map_err(|error| format!("Could not flush MCP response: {error}"))?;
    }
    Ok(())
}

fn mcp_tools() -> Value {
    json!([
        {
            "name": "report_agent_activity",
            "description": "Publish a concise, user-visible task status or current step to Rsync Desktop. Do not include private reasoning, secrets, or sensitive user data.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["working", "completed", "blocked"],
                        "description": "The current task state."
                    },
                    "summary": {
                        "type": "string",
                        "maxLength": MAX_SUMMARY_CHARS,
                        "description": "Brief user-facing description of progress; not chain-of-thought."
                    },
                    "currentStep": {
                        "type": "string",
                        "maxLength": MAX_SUMMARY_CHARS,
                        "description": "Optional concise step label."
                    },
                    "sessionId": {
                        "type": "string",
                        "maxLength": 200,
                        "description": "Optional agent session identifier."
                    }
                },
                "required": ["status", "summary"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_agent_activity",
            "description": "Read the most recent sanitized Copilot activity events displayed in Rsync Desktop.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }
    ])
}

fn call_mcp_tool(
    store: &AgentActivityStore,
    name: &str,
    arguments: &Value,
) -> Result<String, String> {
    match name {
        "report_agent_activity" => {
            let status = arguments
                .get("status")
                .and_then(Value::as_str)
                .ok_or_else(|| "Choose a status: working, completed, or blocked.".to_string())?;
            if !matches!(status, "working" | "completed" | "blocked") {
                return Err("Choose a status: working, completed, or blocked.".to_string());
            }
            let summary = arguments
                .get("currentStep")
                .and_then(Value::as_str)
                .filter(|step| !step.trim().is_empty())
                .map(|step| {
                    format!(
                        "{status}: {step} — {}",
                        arguments["summary"].as_str().unwrap_or_default()
                    )
                })
                .or_else(|| {
                    arguments
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(|summary| format!("{status}: {summary}"))
                })
                .ok_or_else(|| "Provide a concise progress summary.".to_string())?;
            if summary.chars().count() > MAX_SUMMARY_CHARS {
                return Err(format!(
                    "The combined status and step must be {MAX_SUMMARY_CHARS} characters or fewer."
                ));
            }
            let event = store.record(
                &format!("agent.{status}"),
                &summary,
                None,
                arguments.get("sessionId").and_then(Value::as_str),
                "mcp",
            )?;
            serde_json::to_string(&event)
                .map_err(|error| format!("Could not serialize activity response: {error}"))
        }
        "get_agent_activity" => serde_json::to_string(&store.list()?)
            .map_err(|error| format!("Could not serialize agent activity: {error}")),
        _ => Err(format!("Unknown Rsync Desktop MCP tool: {name}")),
    }
}

pub fn exit_with_error(error: String) -> ! {
    eprintln!("{error}");
    process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_progress_reports_are_saved_and_listed() {
        let directory = tempfile::tempdir().expect("create data folder");
        let store = AgentActivityStore::new(directory.path().to_path_buf());
        let args = json!({
            "status": "working",
            "summary": "Implementing the activity view",
            "currentStep": "Adding the MCP event list",
            "sessionId": "session-1"
        });

        let response =
            call_mcp_tool(&store, "report_agent_activity", &args).expect("record progress");
        let event: AgentActivityEvent =
            serde_json::from_str(&response).expect("parse recorded event");
        assert_eq!(event.event_type, "agent.working");
        assert_eq!(
            event.summary,
            "working: Adding the MCP event list — Implementing the activity view"
        );
        assert_eq!(event.session_id.as_deref(), Some("session-1"));

        let events = store.list().expect("list activity");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
    }

    #[test]
    fn automatic_hook_records_tool_names_without_arguments_or_results() {
        let directory = tempfile::tempdir().expect("create data folder");
        let store = AgentActivityStore::new(directory.path().to_path_buf());
        let hook = json!({
            "hookEventName": "postToolUse",
            "toolName": "read_file",
            "toolArgs": { "path": "/private/file" },
            "toolResult": "private output",
            "sessionId": "session-2"
        });

        record_hook_event(&store, &hook).expect("record Copilot event");
        let events = store.list().expect("list activity");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].summary,
            "Copilot agent completed a tool: read_file"
        );
        assert_eq!(events[0].tool_name.as_deref(), Some("read_file"));
        assert!(events[0].session_id.is_none());
        let serialized = serde_json::to_string(&events[0]).expect("serialize event");
        assert!(!serialized.contains("/private/file"));
        assert!(!serialized.contains("private output"));
    }

    #[test]
    fn workspace_mcp_install_preserves_other_servers() {
        let directory = tempfile::tempdir().expect("create workspace");
        let vscode = directory.path().join(".vscode");
        fs::create_dir_all(&vscode).expect("create vscode settings folder");
        fs::write(
            vscode.join("mcp.json"),
            r#"{"servers":{"other":{"type":"stdio","command":"other-mcp"}}}"#,
        )
        .expect("write existing MCP configuration");
        let data_directory = directory.path().join("app-data");
        let store = AgentActivityStore::new(data_directory.clone());

        configure_agent_mcp_with_home(
            &store,
            &InstallMcpInput {
                client: "vscode".to_string(),
                workspace_path: Some(directory.path().to_string_lossy().to_string()),
            },
            directory.path(),
        )
        .expect("install MCP config");

        let config: Value = serde_json::from_slice(
            &fs::read(vscode.join("mcp.json")).expect("read installed MCP configuration"),
        )
        .expect("parse installed MCP configuration");
        assert_eq!(config["servers"]["other"]["command"], "other-mcp");
        assert_eq!(config["servers"]["rsync-desktop"]["args"][0], "--mcp");
        assert_eq!(
            config["servers"]["rsync-desktop"]["env"]["RSYNC_DESKTOP_DATA_DIR"],
            data_directory.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn claude_code_and_gemini_cli_use_their_native_stdio_config_shapes() {
        let directory = tempfile::tempdir().expect("create test home");
        let workspace = directory.path().join("workspace");
        fs::create_dir_all(&workspace).expect("create workspace");
        let store = AgentActivityStore::new(directory.path().join("app-data"));

        configure_agent_mcp_with_home(
            &store,
            &InstallMcpInput {
                client: "claude-code".to_string(),
                workspace_path: Some(workspace.to_string_lossy().to_string()),
            },
            directory.path(),
        )
        .expect("install Claude Code configuration");
        configure_agent_mcp_with_home(
            &store,
            &InstallMcpInput {
                client: "gemini-cli".to_string(),
                workspace_path: None,
            },
            directory.path(),
        )
        .expect("install Gemini CLI configuration");

        let claude: Value = serde_json::from_slice(
            &fs::read(workspace.join(".mcp.json")).expect("read Claude Code config"),
        )
        .expect("parse Claude Code config");
        assert_eq!(claude["mcpServers"]["rsync-desktop"]["args"][0], "--mcp");

        let gemini: Value = serde_json::from_slice(
            &fs::read(directory.path().join(".gemini/settings.json"))
                .expect("read Gemini CLI settings"),
        )
        .expect("parse Gemini CLI settings");
        assert_eq!(gemini["mcpServers"]["rsync-desktop"]["args"][0], "--mcp");
    }

    #[test]
    fn copilot_hook_install_creates_user_level_tool_and_session_hooks() {
        let home = tempfile::tempdir().expect("create test home");
        let data_directory = home.path().join("app-data");
        let store = AgentActivityStore::new(data_directory.clone());

        install_copilot_hooks_for_home(&store, home.path()).expect("install Copilot hooks");

        let config: Value = serde_json::from_slice(
            &fs::read(home.path().join(".copilot/hooks/rsync-desktop.json"))
                .expect("read installed hooks"),
        )
        .expect("parse installed hooks");
        for event_type in [
            "sessionStart",
            "userPromptSubmitted",
            "preToolUse",
            "postToolUse",
            "sessionEnd",
        ] {
            let hook = &config["hooks"][event_type][0];
            assert_eq!(hook["type"], "command");
            assert!(
                hook["command"]
                    .as_str()
                    .unwrap()
                    .ends_with("--copilot-hook")
            );
            assert_eq!(
                hook["env"]["RSYNC_DESKTOP_DATA_DIR"],
                data_directory.to_string_lossy().as_ref()
            );
        }
    }

    #[test]
    fn copilot_hook_install_does_not_replace_an_existing_hook_config() {
        let home = tempfile::tempdir().expect("create test home");
        let hooks_directory = home.path().join(".copilot/hooks");
        fs::create_dir_all(&hooks_directory).expect("create hooks folder");
        let config_path = hooks_directory.join("rsync-desktop.json");
        fs::write(&config_path, "existing config").expect("write existing config");
        let store = AgentActivityStore::new(home.path().join("app-data"));

        assert!(install_copilot_hooks_for_home(&store, home.path()).is_err());
        assert_eq!(
            fs::read_to_string(config_path).expect("read preserved config"),
            "existing config"
        );
    }

    #[test]
    fn mcp_progress_rejects_invalid_status_and_overlong_summaries() {
        let directory = tempfile::tempdir().expect("create data folder");
        let store = AgentActivityStore::new(directory.path().to_path_buf());
        assert!(
            call_mcp_tool(
                &store,
                "report_agent_activity",
                &json!({ "status": "secret", "summary": "status" })
            )
            .is_err()
        );
        assert!(
            call_mcp_tool(
                &store,
                "report_agent_activity",
                &json!({ "status": "working", "summary": "x".repeat(MAX_SUMMARY_CHARS + 1) })
            )
            .is_err()
        );
        assert!(store.list().expect("list activity").is_empty());
    }
}
