use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::Emitter;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
enum AppError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("agent error: {0}")]
    Agent(String),
    #[error("open error: {0}")]
    Open(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentProvider {
    id: String,
    title: String,
    command: String,
    args: Vec<String>,
    env: Vec<AgentEnvVar>,
    enabled: bool,
    selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentEnvVar {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentProviderStatus {
    id: String,
    title: String,
    command: String,
    args: Vec<String>,
    available: bool,
    path: Option<String>,
    version: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectRequest {
    website_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    last_project_path: Option<String>,
    #[serde(default)]
    providers: Vec<AgentProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectConfig {
    id: String,
    name: String,
    website_url: String,
    path: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectState {
    config: ProjectConfig,
    agent_provider: AgentProviderStatus,
    docs: Vec<ContextDoc>,
    channel_setups: Vec<ChannelSetup>,
    latest_run: Option<RunState>,
    run_activity: Vec<RunActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextDoc {
    key: String,
    file_name: String,
    title: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunState {
    id: String,
    kind: String,
    status: RunStatus,
    #[serde(default)]
    provider_id: Option<String>,
    #[serde(default)]
    provider_title: Option<String>,
    #[serde(default)]
    external_session_id: Option<String>,
    #[serde(default)]
    codex_thread_id: Option<String>,
    started_at: String,
    completed_at: Option<String>,
    log_path: String,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunActivity {
    kind: String,
    title: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelSetup {
    id: String,
    name: String,
    status: ChannelSetupStatus,
    login_status: XLoginStatus,
    analysis_status: XAnalysisStatus,
    account_label: Option<String>,
    path: String,
    files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ChannelSetupStatus {
    NotConfigured,
    NeedsLogin,
    Analyzing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum XLoginStatus {
    Unknown,
    NeedsLogin,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum XAnalysisStatus {
    NotStarted,
    Running,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct XChannelStatus {
    login_status: XLoginStatus,
    analysis_status: XAnalysisStatus,
    account_label: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RunStatus {
    Running,
    Completed,
    Failed,
}

const DOCS: [(&str, &str, &str); 4] = [
    (
        "product_information",
        "product-information.md",
        "Product Information",
    ),
    (
        "marketing_strategy",
        "marketing-strategy.md",
        "Marketing Strategy",
    ),
    (
        "competitor_analysis",
        "competitor-analysis.md",
        "Competitor Analysis",
    ),
    ("brand_voice", "brand-voice.md", "Brand Voice"),
];

const RPC_INITIALIZE_ID: i64 = 1;
const RPC_SESSION_NEW_ID: i64 = 2;
const RPC_SESSION_PROMPT_ID: i64 = 3;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            detect_agent_provider,
            default_project_path,
            create_project,
            load_last_project,
            load_project,
            run_initial_analysis,
            configure_channel,
            run_x_account_analysis,
            open_project_in_codex,
            open_external_url,
            open_chrome_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running GTM Agent");
}

#[tauri::command]
fn detect_agent_provider() -> AppResult<AgentProviderStatus> {
    selected_provider_status()
}

#[tauri::command]
fn default_project_path(website_url: String) -> AppResult<String> {
    let url = normalize_url(&website_url)?;
    Ok(projects_root()?
        .join(slugify(&project_name_from_url(&url)))
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
fn create_project(request: CreateProjectRequest) -> AppResult<ProjectState> {
    let website_url = normalize_url(&request.website_url)?;
    let project_path = PathBuf::from(default_project_path(website_url.clone())?);
    let config_path = project_path.join(".gtm-agent/config.json");

    if config_path.exists() {
        save_last_project_path(&project_path)?;
        return load_project(project_path.to_string_lossy().to_string());
    }

    let name = project_name_from_url(&website_url);
    fs::create_dir_all(project_path.join(".gtm-agent/runs"))?;

    let now = Utc::now().to_rfc3339();
    let config = ProjectConfig {
        id: format!("project_{}", Uuid::new_v4().simple()),
        name,
        website_url,
        path: project_path.to_string_lossy().to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    write_json_pretty(&project_path.join(".gtm-agent/config.json"), &config)?;
    write_workspace_files(&project_path, &config)?;
    save_last_project_path(&project_path)?;
    load_project(config.path)
}

#[tauri::command]
fn load_last_project() -> AppResult<Option<ProjectState>> {
    let settings = read_app_settings()?;
    if let Some(project_path) = settings.last_project_path {
        let path = PathBuf::from(project_path);
        if path.join(".gtm-agent/config.json").exists() {
            return load_project(path.to_string_lossy().to_string()).map(Some);
        }
    }

    let Some(path) = latest_project_path()? else {
        return Ok(None);
    };
    save_last_project_path(&path)?;
    load_project(path.to_string_lossy().to_string()).map(Some)
}

#[tauri::command]
fn load_project(project_path: String) -> AppResult<ProjectState> {
    let path = PathBuf::from(project_path);
    let config: ProjectConfig = read_json(&path.join(".gtm-agent/config.json"))?;
    let latest_run = latest_run(&path)?;
    let run_activity = latest_run
        .as_ref()
        .map(|run| read_run_activity(&PathBuf::from(&run.log_path)))
        .transpose()?
        .unwrap_or_default();
    Ok(ProjectState {
        config,
        agent_provider: selected_provider_status()?,
        docs: read_docs(&path)?,
        channel_setups: read_channel_setups(&path),
        latest_run,
        run_activity,
    })
}

#[tauri::command]
fn configure_channel(project_path: String, channel_id: String) -> AppResult<ProjectState> {
    let path = PathBuf::from(project_path);
    if channel_id != "x" {
        return Err(AppError::Invalid(format!(
            "{channel_id} setup is not implemented yet"
        )));
    }
    write_x_channel_setup(&path)?;
    append_event(
        &path,
        "channel.configured",
        "X channel setup initialized",
        serde_json::json!({
            "channelId": "x",
            "mode": "chrome_session_draft_first",
        }),
    )?;
    load_project(path.to_string_lossy().to_string())
}

#[tauri::command]
fn run_x_account_analysis(app: tauri::AppHandle, project_path: String) -> AppResult<RunState> {
    let path = PathBuf::from(project_path);
    let config: ProjectConfig = read_json(&path.join(".gtm-agent/config.json"))?;
    write_x_channel_setup(&path)?;
    write_x_channel_status(&path, XLoginStatus::Unknown, XAnalysisStatus::Running, None)?;
    let run_id = format!("x_run_{}", Utc::now().format("%Y%m%d%H%M%S"));
    let run_path = run_manifest_path(&path, &run_id);
    let log_path = path.join(".gtm-agent/runs").join(format!("{run_id}.jsonl"));
    let run = RunState {
        id: run_id.clone(),
        kind: "x_account_analysis".into(),
        status: RunStatus::Running,
        codex_thread_id: None,
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
        log_path: log_path.to_string_lossy().to_string(),
        error: None,
    };
    write_json_pretty(&run_path, &run)?;
    append_event(
        &path,
        "channel.analysis_started",
        "X account analysis started",
        serde_json::json!({ "runId": run.id, "channelId": "x" }),
    )?;

    let app_handle = app.clone();
    let run_for_thread = run.clone();
    thread::spawn(move || {
        let result = execute_codex_turn(
            &path,
            &config,
            &run_for_thread,
            &x_account_analysis_prompt(&config),
            "X account analysis",
        );
        let status_result = match &result {
            Ok(()) => {
                let reported = read_x_channel_status(&path);
                if reported.login_status == XLoginStatus::NeedsLogin {
                    write_x_channel_status(
                        &path,
                        XLoginStatus::NeedsLogin,
                        XAnalysisStatus::NotStarted,
                        reported.account_label,
                    )
                } else {
                    write_x_channel_status(
                        &path,
                        XLoginStatus::Verified,
                        XAnalysisStatus::Ready,
                        read_x_account_label(&path),
                    )
                }
            }
            Err(_) => write_x_channel_status(
                &path,
                XLoginStatus::Unknown,
                XAnalysisStatus::Failed,
                read_x_account_label(&path),
            ),
        };
        let _ = status_result;
        let _ = app_handle.emit(
            "project-updated",
            serde_json::json!({ "projectPath": path.to_string_lossy(), "runId": run_id }),
        );
        if let Err(err) = result {
            let _ = append_event(
                &path,
                "channel.analysis_failed",
                &format!("X account analysis failed: {err}"),
                serde_json::json!({ "runId": run_id, "channelId": "x" }),
            );
        }
    });

    Ok(run)
}

#[tauri::command]
fn run_initial_analysis(app: tauri::AppHandle, project_path: String) -> AppResult<RunState> {
    let path = PathBuf::from(project_path);
    let config: ProjectConfig = read_json(&path.join(".gtm-agent/config.json"))?;
    let provider = selected_provider()?;
    let provider_status = provider_status(&provider);
    if !provider_status.available {
        return Err(AppError::Agent(
            provider_status
                .error
                .unwrap_or_else(|| format!("{} is not available", provider.title)),
        ));
    }
    let run_id = format!("run_{}", Utc::now().format("%Y%m%d%H%M%S"));
    let run_path = run_manifest_path(&path, &run_id);
    let log_path = path.join(".gtm-agent/runs").join(format!("{run_id}.jsonl"));
    let run = RunState {
        id: run_id.clone(),
        kind: "initial_analysis".into(),
        status: RunStatus::Running,
        provider_id: Some(provider.id.clone()),
        provider_title: Some(provider.title.clone()),
        external_session_id: None,
        codex_thread_id: None,
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
        log_path: log_path.to_string_lossy().to_string(),
        error: None,
    };
    write_json_pretty(&run_path, &run)?;
    append_event(
        &path,
        "task.started",
        "Initial analysis started",
        serde_json::json!({ "runId": run.id }),
    )?;

    let app_handle = app.clone();
    let run_for_thread = run.clone();
    thread::spawn(move || {
        let result = execute_initial_analysis(&path, &config, &run_for_thread, &provider);
        let _ = app_handle.emit(
            "project-updated",
            serde_json::json!({ "projectPath": path.to_string_lossy(), "runId": run_id }),
        );
        if let Err(err) = result {
            let _ = append_event(
                &path,
                "task.failed",
                &format!("Initial analysis failed: {err}"),
                serde_json::json!({ "runId": run_id }),
            );
        }
    });

    Ok(run)
}

#[tauri::command]
fn open_project_in_codex(project_path: String) -> AppResult<()> {
    let binary =
        resolve_command("codex").ok_or_else(|| AppError::Open("codex binary not found".into()))?;
    Command::new(binary)
        .arg("app")
        .arg(project_path)
        .spawn()
        .map_err(|err| AppError::Open(err.to_string()))?;
    Ok(())
}

#[tauri::command]
fn open_external_url(url: String) -> AppResult<()> {
    let normalized = normalize_url(&url)?;
    opener::open(normalized).map_err(|err| AppError::Open(err.to_string()))?;
    Ok(())
}

#[tauri::command]
fn open_chrome_url(url: String) -> AppResult<()> {
    let normalized = normalize_url(&url)?;
    Command::new("open")
        .arg("-a")
        .arg("Google Chrome")
        .arg(normalized)
        .spawn()
        .map_err(|err| AppError::Open(err.to_string()))?;
    Ok(())
}

fn execute_initial_analysis(
    project: &Path,
    config: &ProjectConfig,
    initial: &RunState,
    provider: &AgentProvider,
) -> AppResult<()> {
    execute_agent_turn(
        project,
        initial,
        provider,
        &initial_analysis_prompt(config),
        "Initial analysis",
    )
}

fn execute_agent_turn(
    project: &Path,
    initial: &RunState,
    provider: &AgentProvider,
    prompt: &str,
    task_label: &str,
) -> AppResult<()> {
    let binary = resolve_command(&provider.command)
        .ok_or_else(|| AppError::Agent(format!("{} command not found", provider.command)))?;
    let log_path = PathBuf::from(&initial.log_path);
    let log_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?,
    ));

    let mut child = Command::new(binary)
        .args(&provider.args)
        .envs(provider.env.iter().map(|item| (&item.name, &item.value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AppError::Agent(format!("failed to launch {}: {err}", provider.title)))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Agent("missing agent stdin".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Agent("missing agent stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Agent("missing agent stderr".into()))?;

    let stderr_log = Arc::clone(&log_file);
    let stderr_handle = thread::spawn(move || -> AppResult<()> {
        for line in BufReader::new(stderr).lines() {
            log_jsonl(
                &stderr_log,
                &serde_json::json!({ "stream": "stderr", "message": line? }),
            )?;
        }
        Ok(())
    });

    send_rpc(&mut stdin, &acp_initialize_request(), &log_file)?;

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut session_id = None::<String>;
    let mut turn_error = None::<String>;
    let mut turn_completed = false;

    while reader.read_line(&mut line)? > 0 {
        let raw_line = line.trim_end_matches(['\r', '\n']).to_string();
        line.clear();
        if raw_line.is_empty() {
            continue;
        }
        log_raw_line(&log_file, &raw_line)?;
        let message: Value = serde_json::from_str(&raw_line)?;

        if message.get("error").is_some() {
            turn_error = rpc_error_message(&message);
            break;
        }

        if let Some(request_id) = server_request_id(&message) {
            let response = if message.get("method").and_then(Value::as_str)
                == Some("session/request_permission")
            {
                permission_rejected_response(request_id, &message)
            } else {
                unsupported_server_request_response(request_id)
            };
            send_rpc(&mut stdin, &response, &log_file)?;
            continue;
        }

        if rpc_id_is(&message, RPC_INITIALIZE_ID) {
            send_rpc(&mut stdin, &acp_session_new_request(project), &log_file)?;
            continue;
        }

        if rpc_id_is(&message, RPC_SESSION_NEW_ID) {
            let id = acp_session_id_from_message(&message)
                .ok_or_else(|| AppError::Agent("session/new response missing session id".into()))?;
            let mut started = initial.clone();
            started.external_session_id = Some(id.clone());
            session_id = Some(id.clone());
            append_event(
                project,
                "agent.session_started",
                "Agent session started",
                serde_json::json!({
                    "runId": initial.id,
                    "providerId": provider.id,
                    "providerTitle": provider.title,
                    "externalSessionId": id,
                }),
            )?;
            send_rpc(
                &mut stdin,
                &acp_session_prompt_request(&id, prompt),
                &log_file,
            )?;
            write_json_pretty(&run_manifest_path(project, &initial.id), &started)?;
            continue;
        }

        if rpc_id_is(&message, RPC_SESSION_PROMPT_ID) {
            match acp_prompt_stop_reason(&message).as_deref() {
                Some("end_turn") => turn_completed = true,
                Some("max_tokens") | Some("max_turn_requests") => turn_completed = true,
                Some("cancelled") => turn_error = Some("Agent turn was cancelled".into()),
                Some("refusal") => turn_error = Some("Agent refused the prompt".into()),
                Some(other) => turn_error = Some(format!("Agent stopped with reason {other}")),
                None => turn_error = Some("session/prompt response missing stop reason".into()),
            }
            break;
        }
    }

    drop(stdin);
    if turn_completed || turn_error.is_some() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    join_reader(stderr_handle)?;

    let mut finished = initial.clone();
    finished.completed_at = Some(Utc::now().to_rfc3339());
    finished.external_session_id = session_id;

    if turn_completed {
        finished.status = RunStatus::Completed;
        write_json_pretty(&run_manifest_path(project, &initial.id), &finished)?;
        append_event(
            project,
            "task.completed",
            &format!("{task_label} completed"),
            serde_json::json!({
                "runId": initial.id,
                "providerId": finished.provider_id,
                "externalSessionId": finished.external_session_id,
            }),
        )?;
        Ok(())
    } else {
        finished.status = RunStatus::Failed;
        finished.error =
            Some(turn_error.unwrap_or_else(|| format!("agent exited before completion: {status}")));
        write_json_pretty(&run_manifest_path(project, &initial.id), &finished)?;
        Err(AppError::Agent(
            finished
                .error
                .clone()
                .unwrap_or_else(|| "agent run failed".into()),
        ))
    }
}

fn join_reader(handle: thread::JoinHandle<AppResult<()>>) -> AppResult<()> {
    handle
        .join()
        .map_err(|_| AppError::Agent("agent output reader panicked".into()))?
}

fn acp_initialize_request() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": RPC_INITIALIZE_ID,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {},
            "clientInfo": {
                "name": "gtm-agent",
                "title": "GTM Agent",
                "version": env!("CARGO_PKG_VERSION"),
            }
        },
    })
}

fn acp_session_new_request(project: &Path) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": RPC_SESSION_NEW_ID,
        "method": "session/new",
        "params": {
            "cwd": project.to_string_lossy(),
            "mcpServers": [],
        },
    })
}

fn acp_session_prompt_request(session_id: &str, prompt: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": RPC_SESSION_PROMPT_ID,
        "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [{
                "type": "text",
                "text": prompt,
            }],
        },
    })
}

fn permission_rejected_response(id: Value, request: &Value) -> Value {
    let reject_option = request
        .pointer("/params/options")
        .and_then(Value::as_array)
        .and_then(|options| {
            options.iter().find_map(|option| {
                let kind = option.get("kind").and_then(Value::as_str);
                let option_id = option.get("optionId").and_then(Value::as_str);
                matches!(kind, Some("reject_once") | Some("reject_always")).then_some(option_id)?
            })
        });

    if let Some(option_id) = reject_option {
        return serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "outcome": {
                    "outcome": "selected",
                    "optionId": option_id,
                }
            }
        });
    }

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "outcome": {
                "outcome": "cancelled"
            }
        }
    })
}

fn send_rpc(stdin: &mut ChildStdin, request: &Value, log_file: &Arc<Mutex<File>>) -> AppResult<()> {
    log_jsonl(
        log_file,
        &serde_json::json!({ "stream": "stdin", "message": request }),
    )?;
    writeln!(stdin, "{request}")?;
    stdin.flush()?;
    Ok(())
}

fn log_raw_line(log_file: &Arc<Mutex<File>>, line: &str) -> AppResult<()> {
    writeln!(
        log_file
            .lock()
            .map_err(|_| AppError::Agent("log lock poisoned".into()))?,
        "{line}"
    )?;
    Ok(())
}

fn log_jsonl(log_file: &Arc<Mutex<File>>, value: &Value) -> AppResult<()> {
    writeln!(
        log_file
            .lock()
            .map_err(|_| AppError::Agent("log lock poisoned".into()))?,
        "{value}"
    )?;
    Ok(())
}

fn rpc_id_is(message: &Value, id: i64) -> bool {
    message.get("id").and_then(Value::as_i64) == Some(id)
}

fn rpc_error_message(message: &Value) -> Option<String> {
    message
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn server_request_id(message: &Value) -> Option<Value> {
    if message.get("method").is_some()
        && message.get("id").is_some()
        && message.get("result").is_none()
        && message.get("error").is_none()
    {
        return message.get("id").cloned();
    }
    None
}

fn unsupported_server_request_response(id: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32000,
            "message": "GTM Agent only handles ACP permission requests during initial analysis",
        },
    })
}

fn acp_session_id_from_message(message: &Value) -> Option<String> {
    message
        .pointer("/result/sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn acp_prompt_stop_reason(message: &Value) -> Option<String> {
    message
        .pointer("/result/stopReason")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn selected_provider() -> AppResult<AgentProvider> {
    read_app_settings()?
        .providers
        .into_iter()
        .find(|provider| provider.enabled && provider.selected)
        .ok_or_else(|| AppError::Agent("no enabled agent provider selected".into()))
}

fn selected_provider_status() -> AppResult<AgentProviderStatus> {
    Ok(provider_status(&selected_provider()?))
}

fn provider_status(provider: &AgentProvider) -> AgentProviderStatus {
    match resolve_command(&provider.command) {
        Some(path) => AgentProviderStatus {
            id: provider.id.clone(),
            title: provider.title.clone(),
            command: provider.command.clone(),
            args: provider.args.clone(),
            available: true,
            path: Some(path.to_string_lossy().to_string()),
            version: command_version(&path),
            error: None,
        },
        None => AgentProviderStatus {
            id: provider.id.clone(),
            title: provider.title.clone(),
            command: provider.command.clone(),
            args: provider.args.clone(),
            available: false,
            path: None,
            version: None,
            error: Some(format!("{} command not found", provider.command)),
        },
    }
}

fn default_agent_providers() -> Vec<AgentProvider> {
    vec![
        provider(
            "codex",
            "Codex",
            "npx",
            &["-y", "@agentclientprotocol/codex-acp"],
            true,
        ),
        provider(
            "claude",
            "Claude Code",
            "npx",
            &["-y", "@agentclientprotocol/claude-agent-acp"],
            false,
        ),
        provider("cursor", "Cursor", "cursor-agent", &["acp"], false),
        provider("gemini", "Gemini", "gemini", &["--acp"], false),
        provider(
            "copilot",
            "Copilot",
            "copilot",
            &["--acp", "--stdio"],
            false,
        ),
        provider("custom", "Custom", "", &[], false),
    ]
}

fn provider(id: &str, title: &str, command: &str, args: &[&str], selected: bool) -> AgentProvider {
    AgentProvider {
        id: id.into(),
        title: title.into(),
        command: command.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        env: Vec::new(),
        enabled: !command.is_empty(),
        selected,
    }
}

fn merge_agent_providers(configured: Vec<AgentProvider>) -> Vec<AgentProvider> {
    if configured.is_empty() {
        return default_agent_providers();
    }

    let mut providers = configured;
    for default_provider in default_agent_providers() {
        if !providers
            .iter()
            .any(|provider| provider.id == default_provider.id)
        {
            providers.push(default_provider);
        }
    }

    if !providers
        .iter()
        .any(|provider| provider.enabled && provider.selected)
    {
        if let Some(provider) = providers.iter_mut().find(|provider| provider.enabled) {
            provider.selected = true;
        }
    }

    providers
}

fn resolve_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    let candidate = PathBuf::from(command);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        return candidate.exists().then_some(candidate);
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|path| path.join(command))
            .find(|path| path.exists())
    })
}

fn command_version(path: &Path) -> Option<String> {
    Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|version| !version.is_empty())
}

fn write_workspace_files(project_path: &Path, config: &ProjectConfig) -> AppResult<()> {
    fs::write(
        project_path.join("AGENTS.md"),
        format!(
            "# GTM Agent Project: {}\n\nThis folder is managed by GTM Agent for brand and GTM research.\n\n## Source of truth\n\n- `product-information.md`: product, features, proof points, and use cases.\n- `marketing-strategy.md`: ICP, personas, pain points, positioning, and channels.\n- `competitor-analysis.md`: competitors, alternatives, and positioning gaps.\n- `brand-voice.md`: voice, tone, vocabulary, and public messaging rules.\n\n## Event protocol\n\nWhen GTM Agent asks you to report progress, append JSON lines to `.gtm-agent/events.jsonl` with `eventType`, `summary`, `payload`, and `createdAt`.\n\nWebsite: {}\n",
            config.name, config.website_url
        ),
    )?;
    for (_, file_name, title) in DOCS {
        let body = format!("# {title}\n\n");
        fs::write(project_path.join(file_name), body)?;
    }
    append_event(
        project_path,
        "project.created",
        &format!("Created GTM workspace for {}", config.name),
        serde_json::json!({ "websiteUrl": config.website_url }),
    )?;
    Ok(())
}

fn read_docs(project_path: &Path) -> AppResult<Vec<ContextDoc>> {
    DOCS.iter()
        .map(|(key, file_name, title)| {
            Ok(ContextDoc {
                key: (*key).into(),
                file_name: (*file_name).into(),
                title: (*title).into(),
                content: fs::read_to_string(project_path.join(file_name)).unwrap_or_default(),
            })
        })
        .collect()
}

fn read_channel_setups(project_path: &Path) -> Vec<ChannelSetup> {
    let x_path = project_path.join(".gtm-agent/channels/x");
    if !x_path.exists() {
        return vec![ChannelSetup {
            id: "x".into(),
            name: "X".into(),
            status: ChannelSetupStatus::NotConfigured,
            login_status: XLoginStatus::Unknown,
            analysis_status: XAnalysisStatus::NotStarted,
            account_label: None,
            path: x_path.to_string_lossy().to_string(),
            files: Vec::new(),
        }];
    }

    let channel_status = read_x_channel_status(project_path);
    let setup_status = match channel_status.analysis_status {
        XAnalysisStatus::Running => ChannelSetupStatus::Analyzing,
        XAnalysisStatus::Ready => ChannelSetupStatus::Ready,
        XAnalysisStatus::Failed => ChannelSetupStatus::Failed,
        XAnalysisStatus::NotStarted => ChannelSetupStatus::NeedsLogin,
    };
    let files = ["profile.md", "rules.md", "examples.md", "voice.md"]
        .iter()
        .filter(|file_name| x_path.join(file_name).exists())
        .map(|file_name| (*file_name).to_string())
        .collect::<Vec<_>>();

    vec![ChannelSetup {
        id: "x".into(),
        name: "X".into(),
        status: setup_status,
        login_status: channel_status.login_status,
        analysis_status: channel_status.analysis_status,
        account_label: channel_status.account_label,
        path: x_path.to_string_lossy().to_string(),
        files,
    }]
}

fn write_x_channel_setup(project_path: &Path) -> AppResult<()> {
    let channel_path = project_path.join(".gtm-agent/channels/x");
    fs::create_dir_all(channel_path.join("drafts"))?;

    write_if_missing(
        &channel_path.join("profile.md"),
        "# X Channel Profile\n\n## Connection\n\n- Mode: Existing Chrome session\n- Posting: Browser-assisted after explicit user approval\n- API: Not required for MVP\n\n## Account voice to learn\n\nCodex should learn this from the signed-in X account before recurring runs:\n\n- Profile bio and positioning\n- Recent posts and replies\n- Topics the account naturally discusses\n- Phrases, pacing, and formatting that sound native to the account\n- Posts or replies the user marks as strong examples\n\n## Operating posture\n\nUse global brand voice as the base, then adapt it for X: concise, founder-led, specific, and conversational. Avoid generic launch hype and avoid posting without review.\n",
    )?;
    write_if_missing(
        &channel_path.join("rules.md"),
        "# X Channel Rules\n\n## Approval\n\n- Manual approval is required before every post or reply.\n- Codex may search, rank opportunities, and draft replies.\n- Codex may open Chrome and prepare a post only after the user confirms.\n- The final public action must be visible to the user before submission.\n\n## Draft standards\n\n- Lead with the problem or observation, not a pitch.\n- Prefer useful replies to cold promotion.\n- Use product mentions only when the thread context makes them natural.\n- Do not make unsupported claims beyond the brand source documents.\n- Save strong approved posts back to `examples.md`.\n\n## Avoid\n\n- Spammy reply chains\n- Generic AI/productivity claims\n- Engagement bait\n- Posting into threads where the product is not relevant\n",
    )?;
    write_if_missing(
        &channel_path.join("examples.md"),
        "# X Examples\n\nUse this file as the channel-specific memory for what good looks like.\n\n## Strong examples\n\n_Add approved posts and replies here after the user marks them as good._\n\n## Avoid examples\n\n_Add drafts or posts that felt too salesy, off-tone, or low-signal._\n",
    )?;
    write_if_missing(
        &channel_path.join("voice.md"),
        "# X Account Voice\n\n_Pending account analysis._\n\nCodex should replace this with account-specific voice guidance after reviewing the signed-in X profile, recent posts, replies, and strong examples.\n",
    )?;
    write_if_missing(
        &channel_path.join("searches.md"),
        "# X Search Strategy\n\n## Inputs\n\nUse `marketing-strategy.md`, `brand-voice.md`, competitor names, ICP language, and pain-point terms from the brand analysis.\n\n## Opportunity types\n\n- Buyer pain posts\n- Founder/operator discussions\n- Competitor or alternative mentions\n- Category education threads\n- Launch or workflow discussions where a helpful reply fits\n\n## Daily run output\n\nFor every opportunity, capture the source URL, why it matters, suggested angle, draft reply, and review status before any browser-assisted posting.\n",
    )?;
    write_if_missing(
        &channel_path.join("drafts/schema.md"),
        "# X Draft Format\n\nEach draft should include:\n\n- Source post URL\n- Opportunity summary\n- Why this is relevant\n- Suggested reply draft\n- Risk notes\n- Status: `drafted`, `approved`, `posted`, or `skipped`\n\nApproved drafts can later be opened in Chrome, pasted into the reply field, and left for the user to send. A future version may submit after an explicit in-app confirmation.\n",
    )?;
    write_if_missing(&channel_path.join("opportunities.jsonl"), "")?;
    write_if_missing(&channel_path.join("runs.jsonl"), "")?;
    write_if_missing(
        &channel_path.join("drafts/README.md"),
        "# X Draft Queue\n\nDrafts created by daily X runs should live here until they are approved, edited, skipped, or posted through Chrome.\n",
    )?;
    if !channel_path.join("status.json").exists() {
        write_x_channel_status(
            project_path,
            XLoginStatus::Unknown,
            XAnalysisStatus::NotStarted,
            None,
        )?;
    }
    Ok(())
}

fn read_x_channel_status(project_path: &Path) -> XChannelStatus {
    let path = project_path.join(".gtm-agent/channels/x/status.json");
    if path.exists() {
        if let Ok(status) = read_json(&path) {
            return status;
        }
    }
    XChannelStatus {
        login_status: XLoginStatus::Unknown,
        analysis_status: XAnalysisStatus::NotStarted,
        account_label: None,
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn write_x_channel_status(
    project_path: &Path,
    login_status: XLoginStatus,
    analysis_status: XAnalysisStatus,
    account_label: Option<String>,
) -> AppResult<()> {
    write_json_pretty(
        &project_path.join(".gtm-agent/channels/x/status.json"),
        &XChannelStatus {
            login_status,
            analysis_status,
            account_label,
            updated_at: Utc::now().to_rfc3339(),
        },
    )
}

fn read_x_account_label(project_path: &Path) -> Option<String> {
    let status = read_x_channel_status(project_path);
    status.account_label.or_else(|| {
        let profile = fs::read_to_string(project_path.join(".gtm-agent/channels/x/profile.md"))
            .unwrap_or_default();
        profile
            .lines()
            .find_map(|line| line.strip_prefix("- Account:").map(str::trim))
            .filter(|value| !value.is_empty() && !value.contains("Pending"))
            .map(str::to_string)
    })
}

fn write_if_missing(path: &Path, content: &str) -> AppResult<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn latest_run(project_path: &Path) -> AppResult<Option<RunState>> {
    let dir = project_path.join(".gtm-agent/runs");
    if !dir.exists() {
        return Ok(None);
    }
    let mut manifests = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    manifests.sort();
    manifests.last().map(|path| read_json(path)).transpose()
}

fn read_run_activity(log_path: &Path) -> AppResult<Vec<RunActivity>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(log_path)?;
    let mut activity = Vec::new();
    let mut message_deltas = HashMap::<String, String>::new();
    let mut completed_messages = HashSet::<String>::new();
    let mut tool_titles = HashMap::<String, String>::new();

    for line in BufReader::new(file).lines() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = value.get("method").and_then(Value::as_str);

        match method {
            Some("session/update") => read_acp_activity_update(
                value.pointer("/params/update").unwrap_or(&Value::Null),
                &mut activity,
                &mut message_deltas,
                &mut tool_titles,
            ),
            Some("item/agentMessage/delta") => {
                let text = value
                    .pointer("/params/delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                let message_id = value
                    .pointer("/params/itemId")
                    .and_then(Value::as_str)
                    .unwrap_or("codex");
                message_deltas
                    .entry(message_id.to_string())
                    .or_default()
                    .push_str(text);
            }
            Some("item/completed") => read_legacy_completed_activity(
                value.pointer("/params/item").unwrap_or(&Value::Null),
                &mut activity,
                &mut message_deltas,
                &mut completed_messages,
            ),
            _ => {}
        }
    }

    for (item_id, text) in message_deltas {
        if completed_messages.contains(&item_id) {
            continue;
        }
        if !text.trim().is_empty() {
            activity.push(RunActivity {
                kind: "message".into(),
                title: "Agent output".into(),
                message: compact_text(&text, 520),
            });
        }
    }

    let keep_from = activity.len().saturating_sub(12);
    Ok(activity.into_iter().skip(keep_from).collect())
}

fn read_acp_activity_update(
    update: &Value,
    activity: &mut Vec<RunActivity>,
    message_deltas: &mut HashMap<String, String>,
    tool_titles: &mut HashMap<String, String>,
) {
    match update.get("sessionUpdate").and_then(Value::as_str) {
        Some("agent_message_chunk") => {
            let text = update
                .pointer("/content/text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if text.trim().is_empty() {
                return;
            }
            let message_id = update
                .get("messageId")
                .and_then(Value::as_str)
                .unwrap_or("agent");
            message_deltas
                .entry(message_id.to_string())
                .or_default()
                .push_str(text);
        }
        Some("tool_call") => {
            if let (Some(tool_id), Some(title)) = (
                update.get("toolCallId").and_then(Value::as_str),
                update.get("title").and_then(Value::as_str),
            ) {
                tool_titles.insert(tool_id.to_string(), title.to_string());
                activity.push(RunActivity {
                    kind: "tool".into(),
                    title: "Agent tool".into(),
                    message: compact_text(title, 220),
                });
            }
        }
        Some("tool_call_update") => {
            if let Some(message) = acp_tool_update_message(update, tool_titles) {
                activity.push(RunActivity {
                    kind: "tool".into(),
                    title: "Agent tool".into(),
                    message,
                });
            }
        }
        _ => {}
    }
}

fn read_legacy_completed_activity(
    item: &Value,
    activity: &mut Vec<RunActivity>,
    message_deltas: &mut HashMap<String, String>,
    completed_messages: &mut HashSet<String>,
) {
    match item.get("type").and_then(Value::as_str) {
        Some("agentMessage") => {
            let message_id = item.get("id").and_then(Value::as_str).unwrap_or("codex");
            if let Some(text) = item
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
            {
                completed_messages.insert(message_id.to_string());
                message_deltas.remove(message_id);
                activity.push(RunActivity {
                    kind: "message".into(),
                    title: "Codex output".into(),
                    message: compact_text(text, 520),
                });
            }
        }
        Some("commandExecution") => {
            if let Some(message) = legacy_command_message(item) {
                activity.push(RunActivity {
                    kind: "tool".into(),
                    title: "Tool progress".into(),
                    message,
                });
            }
        }
        Some("webSearch") => {
            if let Some(message) = legacy_web_search_message(item) {
                activity.push(RunActivity {
                    kind: "tool".into(),
                    title: "Research".into(),
                    message,
                });
            }
        }
        _ => {}
    }
}

fn legacy_command_message(item: &Value) -> Option<String> {
    let actions = item
        .get("commandActions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let summaries = actions
        .iter()
        .filter_map(legacy_command_action_summary)
        .collect::<Vec<_>>();
    if !summaries.is_empty() {
        return Some(compact_text(&summaries.join(", "), 260));
    }

    item.get("command")
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .map(|command| compact_text(&format!("Ran command: {command}"), 260))
}

fn legacy_command_action_summary(action: &Value) -> Option<String> {
    let action_type = action.get("type").and_then(Value::as_str)?;
    match action_type {
        "read" => action
            .get("name")
            .or_else(|| action.get("path"))
            .and_then(Value::as_str)
            .map(|name| format!("Read {name}")),
        "listFiles" => action
            .get("path")
            .and_then(Value::as_str)
            .map(|path| format!("Listed {path}"))
            .or_else(|| Some("Listed project files".into())),
        "search" => action
            .get("query")
            .and_then(Value::as_str)
            .map(|query| format!("Searched {query}")),
        "write" | "edit" => action
            .get("name")
            .or_else(|| action.get("path"))
            .and_then(Value::as_str)
            .map(|name| format!("Updated {name}")),
        _ => None,
    }
}

fn legacy_web_search_message(item: &Value) -> Option<String> {
    let queries = item
        .pointer("/action/queries")
        .and_then(Value::as_array)
        .map(|queries| {
            queries
                .iter()
                .filter_map(Value::as_str)
                .filter(|query| !query.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !queries.is_empty() {
        return Some(compact_text(
            &format!("Searched: {}", queries.join(", ")),
            260,
        ));
    }

    item.get("query")
        .and_then(Value::as_str)
        .filter(|query| !query.trim().is_empty())
        .map(|query| compact_text(&format!("Searched: {query}"), 260))
        .or_else(|| Some("Searched public sources".into()))
}

fn acp_tool_update_message(
    update: &Value,
    tool_titles: &HashMap<String, String>,
) -> Option<String> {
    let tool_id = update.get("toolCallId").and_then(Value::as_str);
    let status = update.get("status").and_then(Value::as_str);
    let title = tool_id
        .and_then(|id| tool_titles.get(id))
        .map(String::as_str)
        .unwrap_or("Agent tool");
    let content_text = update
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.pointer("/content/text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
            })
        });

    content_text
        .map(|text| compact_text(text, 300))
        .or_else(|| status.map(|status| compact_text(&format!("{title}: {status}"), 220)))
}

fn compact_text(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let mut truncated = compact.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn run_manifest_path(project_path: &Path, run_id: &str) -> PathBuf {
    project_path
        .join(".gtm-agent/runs")
        .join(format!("{run_id}.json"))
}

fn append_event(
    project_path: &Path,
    event_type: &str,
    summary: &str,
    payload: Value,
) -> AppResult<()> {
    fs::create_dir_all(project_path.join(".gtm-agent"))?;
    let event = serde_json::json!({
        "eventType": event_type,
        "summary": summary,
        "payload": payload,
        "createdAt": Utc::now().to_rfc3339(),
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(project_path.join(".gtm-agent/events.jsonl"))?;
    writeln!(file, "{event}")?;
    Ok(())
}

fn initial_analysis_prompt(config: &ProjectConfig) -> String {
    format!(
        r#"Analyze this brand from public evidence and turn the findings into four concise GTM source documents.

Website: {url}

Work in this order:

1. Research the product beyond the homepage. Use the source website, visible subpages, pricing/docs/about/support pages, app store listings, comparison pages, and relevant public search results when available.
2. Rewrite `product-information.md` first. Start it with a short, plain-language product description paragraph with no URLs. Then add product scope, audience, use cases, proof points, pricing/business model if found, and source notes.
3. Rewrite `marketing-strategy.md` next with ICP, segments, pain points, offer, positioning, conversion ideas, caveats, and the channel recommendations needed for onboarding. Keep everything in this file; do not create extra channel, ICP, offer, positioning, or plan files. Include a clearly titled `## Recommended marketing channels` section. In that section evaluate only the currently supported channels: SEO, X, Reddit, and Hacker News. For each supported channel, write a short entry with `Priority: Recommended`, `Priority: Optional`, or `Priority: Not now`, followed by `Why`, `First setup step`, and `Operating mode`. Recommend only channels that fit the public evidence and category. Do not recommend every supported channel by default.
4. Rewrite `competitor-analysis.md` next. Make this file especially strong. Do not copy only the landing page's named competitors; treat those as signals, not the final ranking. Research the category from multiple public sources: the source website, comparison pages, search results, app listings, category roundups, and competitor websites. Identify products a real buyer would compare for the same job-to-be-done, not just products with similar wording. Rank competitors by buyer relevance, customer overlap, adoption/visibility, category ownership, product maturity, and GTM threat. The top six should usually mix direct specialist products with larger incumbent or platform-native alternatives when those alternatives shape buyer decisions. Include exactly six top competitors in a `## Verified competitor links` section near the top, ordered strongest first. Use canonical product or company pages, not help-center/support URLs. Avoid obscure, tiny, hobby, or open-source tools in the top six unless public evidence shows they materially influence buyer decisions. For each top competitor, include `Why it matters`, `Customer overlap`, `Strengths`, `Weaknesses`, `Positioning angle`, and `GTM implication`. Add a short `## Secondary alternatives` section for smaller tools that are relevant but not top-six, with one sentence explaining why they were excluded from the main set. Each verified link must be a Markdown link using the official company or product website, for example `- [Competitor Name](https://example.com)`. Only include reachable official links you have verified; omit uncertain competitors instead of guessing domains or inventing websites from names.
5. Rewrite `brand-voice.md` last with tone, vocabulary, messaging rules, claims to avoid, and example language.

Save each file immediately after you finish that file, then append a progress event before moving to the next file. This lets the app show completed documents one by one.

Write progress messages for the app user in a clean product-research tone. Keep them short and non-technical: one or two sentences about what was learned or what is being researched next. Do not mention local sessions, Codex internals, workspace mechanics, tools, priority instructions, implementation details, file operations, placeholders, source files, event logs, JSONL, validation commands, or branch/git state.

Keep the files concise but specific enough that future GTM tasks can use them as source context. Include uncertainty where evidence is weak. Do not create outreach drafts, schedules, plugins, or extra strategy files. Do not post publicly or send messages. Rewrite only the four requested Markdown files and append progress/completion events to `.gtm-agent/events.jsonl` as JSON lines with eventType, summary, payload, and createdAt.
"#,
        url = config.website_url
    )
}

fn x_account_analysis_prompt(config: &ProjectConfig) -> String {
    format!(
        r#"Use [@chrome](plugin://chrome@openai-bundled) to analyze the currently signed-in X account for this GTM workspace.

Website: {url}
Brand: {name}

Goal:
Configure the X channel for draft-first outreach. Do not post, like, follow, send, or publicly interact. Only inspect the logged-in account and write local channel context files.

First action, before reading memory, browsing the web, or reviewing local brand files:
- Use Chrome to open or inspect `https://x.com/home`.
- Verify whether the current Chrome profile is signed into X.
- If no account is signed in, immediately write `.gtm-agent/channels/x/status.json` with `loginStatus: "needs_login"`, `analysisStatus: "not_started"`, `accountLabel: null`, and stop.
- If an account is signed in, identify the visible handle/name from the profile or account switcher, then continue with the account analysis.

Then rewrite only these files:
- `.gtm-agent/channels/x/profile.md`
- `.gtm-agent/channels/x/rules.md`
- `.gtm-agent/channels/x/examples.md`
- `.gtm-agent/channels/x/voice.md`
- `.gtm-agent/channels/x/status.json`

Use the global brand files as base context:
- `product-information.md`
- `marketing-strategy.md`
- `competitor-analysis.md`
- `brand-voice.md`

File requirements:

1. `profile.md`
Capture the signed-in account name/handle, visible bio, positioning, recurring topics, audience clues, and what kind of X activity fits the account. Include a line formatted exactly as `- Account: @handle or display name` when known.

2. `voice.md`
Write account-specific voice guidance based on visible posts/replies: tone, pacing, sentence style, vocabulary, formatting habits, and how the global brand voice should adapt for X.

3. `examples.md`
Capture only useful patterns, not private data. Include strong post/reply examples as short paraphrased patterns unless quoting is necessary. Add sections for `Strong examples`, `Reusable patterns`, and `Avoid`.

4. `rules.md`
Keep the draft-first operating rules: Codex may find opportunities and draft replies; public posting requires explicit user approval. Include guardrails for spam, unsupported claims, and when not to reply.

5. `status.json`
Write valid JSON with this exact shape:
{{
  "loginStatus": "verified",
  "analysisStatus": "ready",
  "accountLabel": "@handle or display name",
  "updatedAt": "ISO-8601 timestamp"
}}

Do not create outreach drafts in this run. Do not modify backend queue files such as `opportunities.jsonl`, `runs.jsonl`, or files in `drafts/` unless they do not exist and are needed as empty placeholders.
"#,
        url = config.website_url,
        name = config.name
    )
}

fn normalize_url(input: &str) -> AppResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::Invalid("website URL is required".into()));
    }
    let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    if host.is_empty() || !host.contains('.') {
        return Err(AppError::Invalid(
            "website URL must include a valid host".into(),
        ));
    }
    Ok(url)
}

fn project_name_from_url(url: &str) -> String {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("product")
        .trim_start_matches("www.");
    let label = host.split('.').next().unwrap_or("product");
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "Product".into(),
    }
}

fn slugify(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> AppResult<T> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn projects_root() -> AppResult<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| AppError::Invalid("cannot locate home directory".into()))?;
    Ok(home.join("GTM Agent Projects"))
}

fn latest_project_path() -> AppResult<Option<PathBuf>> {
    let root = projects_root()?;
    if !root.exists() {
        return Ok(None);
    }

    let mut projects = Vec::<(u64, PathBuf)>::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let config_path = path.join(".gtm-agent/config.json");
        if !config_path.exists() {
            continue;
        }
        let modified = fs::metadata(&config_path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        projects.push((modified, path));
    }

    projects.sort_by_key(|(modified, _)| *modified);
    Ok(projects.pop().map(|(_, path)| path))
}

fn app_settings_path() -> AppResult<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::Invalid("cannot locate config directory".into()))?;
    Ok(config_dir.join("GTM Agent").join("settings.json"))
}

fn read_app_settings() -> AppResult<AppSettings> {
    let path = app_settings_path()?;
    if !path.exists() {
        return Ok(AppSettings {
            last_project_path: None,
            providers: default_agent_providers(),
        });
    }
    let mut settings: AppSettings = read_json(&path)?;
    settings.providers = merge_agent_providers(settings.providers);
    Ok(settings)
}

fn save_last_project_path(project_path: &Path) -> AppResult<()> {
    let mut settings = read_app_settings()?;
    settings.last_project_path = Some(project_path.to_string_lossy().to_string());
    write_json_pretty(&app_settings_path()?, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls() {
        assert_eq!(normalize_url("example.com").unwrap(), "https://example.com");
        assert!(normalize_url("not-a-host").is_err());
    }

    #[test]
    fn derives_project_names_and_slugs() {
        assert_eq!(project_name_from_url("https://www.acme.ai"), "Acme");
        assert_eq!(slugify("Acme GTM Agent!"), "acme-gtm-agent");
    }

    #[test]
    fn seeds_default_agent_providers() {
        let providers = default_agent_providers();
        assert_eq!(providers[0].id, "codex");
        assert_eq!(providers[0].command, "npx");
        assert_eq!(
            providers[0].args,
            vec![
                "-y".to_string(),
                "@agentclientprotocol/codex-acp".to_string()
            ]
        );
        assert!(providers[0].selected);
        assert!(providers.iter().any(|provider| provider.id == "custom"));
    }

    #[test]
    fn merges_provider_defaults_without_losing_selection() {
        let configured = vec![AgentProvider {
            id: "custom".into(),
            title: "Custom Agent".into(),
            command: "/tmp/custom-agent".into(),
            args: vec!["acp".into()],
            env: Vec::new(),
            enabled: true,
            selected: true,
        }];

        let providers = merge_agent_providers(configured);
        assert!(providers.iter().any(|provider| provider.id == "codex"));
        assert!(
            providers
                .iter()
                .find(|provider| provider.id == "custom")
                .unwrap()
                .selected
        );
    }

    #[test]
    fn reads_legacy_settings_without_providers() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({
            "lastProjectPath": "/tmp/project"
        }))
        .unwrap();
        assert_eq!(settings.last_project_path.as_deref(), Some("/tmp/project"));
        assert!(settings.providers.is_empty());
    }

    #[test]
    fn builds_acp_initialize_request() {
        let request = acp_initialize_request();
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["method"], "initialize");
        assert_eq!(request["params"]["protocolVersion"], 1);
        assert_eq!(request["params"]["clientInfo"]["name"], "gtm-agent");
    }

    #[test]
    fn builds_acp_session_new_request() {
        let request = acp_session_new_request(Path::new("/tmp/project"));
        assert_eq!(request["method"], "session/new");
        assert_eq!(request["params"]["cwd"], "/tmp/project");
        assert_eq!(request["params"]["mcpServers"], serde_json::json!([]));
    }

    #[test]
    fn builds_acp_session_prompt_request() {
        let request = acp_session_prompt_request("sess_123", "analyze");
        assert_eq!(request["method"], "session/prompt");
        assert_eq!(request["params"]["sessionId"], "sess_123");
        assert_eq!(request["params"]["prompt"][0]["type"], "text");
        assert_eq!(request["params"]["prompt"][0]["text"], "analyze");
    }

    #[test]
    fn parses_acp_session_and_stop_reason() {
        let message = serde_json::json!({
            "id": RPC_SESSION_NEW_ID,
            "result": {
                "sessionId": "sess_abc123"
            }
        });
        assert_eq!(
            acp_session_id_from_message(&message).as_deref(),
            Some("sess_abc123")
        );

        let completed = serde_json::json!({
            "id": RPC_SESSION_PROMPT_ID,
            "result": {
                "stopReason": "end_turn"
            }
        });
        assert_eq!(
            acp_prompt_stop_reason(&completed).as_deref(),
            Some("end_turn")
        );
    }

    #[test]
    fn rejects_acp_permission_requests() {
        let request = serde_json::json!({
            "id": 8,
            "method": "session/request_permission",
            "params": {
                "options": [
                    { "optionId": "allow-once", "name": "Allow", "kind": "allow_once" },
                    { "optionId": "reject-once", "name": "Reject", "kind": "reject_once" }
                ]
            },
        });
        let response = permission_rejected_response(serde_json::json!(8), &request);
        assert_eq!(response["result"]["outcome"]["outcome"], "selected");
        assert_eq!(response["result"]["outcome"]["optionId"], "reject-once");
    }

    #[test]
    fn reads_legacy_codex_thread_id_run_manifest() {
        let run: RunState = serde_json::from_value(serde_json::json!({
            "id": "run_1",
            "kind": "initial_analysis",
            "status": "completed",
            "codexThreadId": "thread_legacy",
            "startedAt": "2026-01-01T00:00:00Z",
            "completedAt": "2026-01-01T00:01:00Z",
            "logPath": "/tmp/run.jsonl",
            "error": null
        }))
        .unwrap();
        assert_eq!(run.codex_thread_id.as_deref(), Some("thread_legacy"));
        assert!(run.external_session_id.is_none());
    }

    #[test]
    fn reads_run_activity_from_acp_log() {
        let log_path =
            std::env::temp_dir().join(format!("gtm-agent-log-{}.jsonl", Uuid::new_v4().simple()));
        fs::write(
            &log_path,
            [
                serde_json::json!({
                    "method": "session/update",
                    "params": {
                        "sessionId": "sess_123",
                        "update": {
                            "sessionUpdate": "tool_call",
                            "toolCallId": "tool_123",
                            "title": "Searching public sources",
                            "kind": "search",
                            "status": "pending"
                        }
                    }
                })
                .to_string(),
                serde_json::json!({
                    "method": "session/update",
                    "params": {
                        "sessionId": "sess_123",
                        "update": {
                            "sessionUpdate": "agent_message_chunk",
                            "messageId": "msg_123",
                            "content": {
                                "type": "text",
                                "text": "I found the source material."
                            }
                        }
                    }
                })
                .to_string(),
                serde_json::json!({
                    "id": RPC_SESSION_PROMPT_ID,
                    "result": { "stopReason": "end_turn" }
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

        let activity = read_run_activity(&log_path).unwrap();
        assert_eq!(activity.len(), 2);
        assert_eq!(activity[0].message, "Searching public sources");
        assert_eq!(activity[1].title, "Agent output");
        assert_eq!(activity[1].message, "I found the source material.");

        fs::remove_file(log_path).unwrap();
    }

    #[test]
    fn reads_run_activity_from_legacy_codex_log() {
        let log_path =
            std::env::temp_dir().join(format!("gtm-agent-log-{}.jsonl", Uuid::new_v4().simple()));
        fs::write(
            &log_path,
            [
                serde_json::json!({
                    "method": "item/agentMessage/delta",
                    "params": {
                        "itemId": "msg_123",
                        "delta": "I found "
                    }
                })
                .to_string(),
                serde_json::json!({
                    "method": "item/agentMessage/delta",
                    "params": {
                        "itemId": "msg_123",
                        "delta": "the source material."
                    }
                })
                .to_string(),
                serde_json::json!({
                    "method": "item/completed",
                    "params": {
                        "item": {
                            "type": "agentMessage",
                            "id": "msg_123",
                            "text": "I found the source material.",
                            "phase": "commentary"
                        }
                    }
                })
                .to_string(),
                serde_json::json!({
                    "method": "item/completed",
                    "params": {
                        "item": {
                            "type": "commandExecution",
                            "id": "call_123",
                            "commandActions": [
                                {
                                    "type": "read",
                                    "name": "product-information.md",
                                    "path": "/tmp/product-information.md"
                                }
                            ]
                        }
                    }
                })
                .to_string(),
                serde_json::json!({
                    "method": "item/completed",
                    "params": {
                        "item": {
                            "type": "webSearch",
                            "id": "search_123",
                            "action": {
                                "queries": ["TapTalk Mac dictation", "taptlk pricing"]
                            }
                        }
                    }
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

        let activity = read_run_activity(&log_path).unwrap();
        assert_eq!(activity.len(), 3);
        assert_eq!(activity[0].title, "Codex output");
        assert_eq!(activity[0].message, "I found the source material.");
        assert_eq!(activity[1].title, "Tool progress");
        assert_eq!(activity[1].message, "Read product-information.md");
        assert_eq!(activity[2].title, "Research");
        assert_eq!(
            activity[2].message,
            "Searched: TapTalk Mac dictation, taptlk pricing"
        );

        fs::remove_file(log_path).unwrap();
    }

    #[test]
    fn writes_mvp_workspace_files() {
        let project_path =
            std::env::temp_dir().join(format!("gtm-agent-test-{}", Uuid::new_v4().simple()));
        let config = ProjectConfig {
            id: "project_test".into(),
            name: "Example".into(),
            website_url: "https://example.com".into(),
            path: project_path.to_string_lossy().to_string(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        fs::create_dir_all(project_path.join(".gtm-agent/runs")).unwrap();
        write_workspace_files(&project_path, &config).unwrap();

        assert!(project_path.join("AGENTS.md").exists());
        for (_, file_name, _) in DOCS {
            assert!(project_path.join(file_name).exists());
            let content = fs::read_to_string(project_path.join(file_name)).unwrap();
            assert!(!content.contains("pending Codex analysis"));
            assert!(!content.contains("Source URL"));
        }
        assert!(project_path.join(".gtm-agent/events.jsonl").exists());

        fs::remove_dir_all(project_path).unwrap();
    }

    #[test]
    fn writes_x_channel_setup_files() {
        let project_path =
            std::env::temp_dir().join(format!("gtm-agent-x-test-{}", Uuid::new_v4().simple()));

        write_x_channel_setup(&project_path).unwrap();

        let channel_path = project_path.join(".gtm-agent/channels/x");
        for file_name in [
            "profile.md",
            "rules.md",
            "examples.md",
            "voice.md",
            "searches.md",
            "opportunities.jsonl",
            "runs.jsonl",
            "status.json",
            "drafts/README.md",
            "drafts/schema.md",
        ] {
            assert!(channel_path.join(file_name).exists());
        }
        let setups = read_channel_setups(&project_path);
        assert_eq!(setups.len(), 1);
        assert_eq!(setups[0].status, ChannelSetupStatus::NeedsLogin);
        assert_eq!(setups[0].login_status, XLoginStatus::Unknown);
        assert_eq!(setups[0].analysis_status, XAnalysisStatus::NotStarted);
        assert!(setups[0].files.contains(&"voice.md".into()));
        assert!(!setups[0].files.contains(&"drafts/schema.md".into()));

        fs::remove_dir_all(project_path).unwrap();
    }
}
