use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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
    #[error("codex error: {0}")]
    Codex(String),
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
struct CodexDetection {
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
    codex: CodexDetection,
    docs: Vec<ContextDoc>,
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
const RPC_THREAD_START_ID: i64 = 2;
const RPC_THREAD_NAME_ID: i64 = 3;
const RPC_TURN_START_ID: i64 = 4;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            detect_codex,
            default_project_path,
            create_project,
            load_project,
            run_initial_analysis,
            open_project_in_codex,
            open_external_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running GTM Agent");
}

#[tauri::command]
fn detect_codex() -> CodexDetection {
    detect_codex_impl()
}

#[tauri::command]
fn default_project_path(website_url: String) -> AppResult<String> {
    let url = normalize_url(&website_url)?;
    let home =
        dirs::home_dir().ok_or_else(|| AppError::Invalid("cannot locate home directory".into()))?;
    Ok(home
        .join("GTM Agent Projects")
        .join(slugify(&project_name_from_url(&url)))
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
fn create_project(request: CreateProjectRequest) -> AppResult<ProjectState> {
    let website_url = normalize_url(&request.website_url)?;
    let name = project_name_from_url(&website_url);
    let project_path = PathBuf::from(default_project_path(website_url.clone())?);
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
    load_project(config.path)
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
        codex: detect_codex_impl(),
        docs: read_docs(&path)?,
        latest_run,
        run_activity,
    })
}

#[tauri::command]
fn run_initial_analysis(app: tauri::AppHandle, project_path: String) -> AppResult<RunState> {
    let path = PathBuf::from(project_path);
    let config: ProjectConfig = read_json(&path.join(".gtm-agent/config.json"))?;
    let run_id = format!("run_{}", Utc::now().format("%Y%m%d%H%M%S"));
    let run_path = run_manifest_path(&path, &run_id);
    let log_path = path.join(".gtm-agent/runs").join(format!("{run_id}.jsonl"));
    let run = RunState {
        id: run_id.clone(),
        kind: "initial_analysis".into(),
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
        "task.started",
        "Initial analysis started",
        serde_json::json!({ "runId": run.id }),
    )?;

    let app_handle = app.clone();
    let run_for_thread = run.clone();
    thread::spawn(move || {
        let result = execute_initial_analysis(&path, &config, &run_for_thread);
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
    let detection = detect_codex_impl();
    let binary = detection
        .path
        .ok_or_else(|| AppError::Open("codex binary not found".into()))?;
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

fn execute_initial_analysis(
    project: &Path,
    config: &ProjectConfig,
    initial: &RunState,
) -> AppResult<()> {
    let detection = detect_codex_impl();
    let binary = detection.path.ok_or_else(|| {
        AppError::Codex(
            detection
                .error
                .unwrap_or_else(|| "codex binary not found".into()),
        )
    })?;
    let prompt = initial_analysis_prompt(config);
    let log_path = PathBuf::from(&initial.log_path);
    let log_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?,
    ));

    let mut child = Command::new(binary)
        .args(codex_app_server_args())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AppError::Codex(format!("failed to launch codex: {err}")))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Codex("missing codex stdin".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Codex("missing codex stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Codex("missing codex stderr".into()))?;

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

    send_rpc(&mut stdin, &initialize_request(), &log_file)?;

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut thread_id = None::<String>;
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
            if rpc_id_is(&message, RPC_THREAD_NAME_ID) {
                continue;
            }
            turn_error = rpc_error_message(&message);
            break;
        }

        if let Some(request_id) = server_request_id(&message) {
            send_rpc(
                &mut stdin,
                &unsupported_server_request_response(request_id),
                &log_file,
            )?;
            continue;
        }

        if rpc_id_is(&message, RPC_INITIALIZE_ID) {
            send_rpc(&mut stdin, &thread_start_request(project), &log_file)?;
            continue;
        }

        if rpc_id_is(&message, RPC_THREAD_START_ID) {
            let id = codex_thread_id_from_app_server_message(&message)
                .ok_or_else(|| AppError::Codex("thread/start response missing thread id".into()))?;
            let started = if thread_id.as_deref() == Some(id.as_str()) {
                let mut started = initial.clone();
                started.codex_thread_id = Some(id.clone());
                started
            } else {
                record_thread_id(project, initial, &id)?
            };
            thread_id = Some(id.clone());
            send_rpc(
                &mut stdin,
                &thread_name_request(&id, &config.name),
                &log_file,
            )?;
            send_rpc(
                &mut stdin,
                &turn_start_request(project, &id, &prompt),
                &log_file,
            )?;
            write_json_pretty(&run_manifest_path(project, &initial.id), &started)?;
            continue;
        }

        if thread_id.is_none() {
            if let Some(id) = codex_thread_id_from_app_server_message(&message) {
                thread_id = Some(id.clone());
                let started = record_thread_id(project, initial, &id)?;
                write_json_pretty(&run_manifest_path(project, &initial.id), &started)?;
            }
        }

        if message.get("method").and_then(Value::as_str) == Some("turn/completed") {
            if message.pointer("/params/threadId").and_then(Value::as_str) == thread_id.as_deref() {
                match completed_turn_error(&message) {
                    Some(error) => turn_error = Some(error),
                    None => turn_completed = true,
                }
                break;
            }
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
    finished.codex_thread_id = thread_id;

    if turn_completed {
        finished.status = RunStatus::Completed;
        write_json_pretty(&run_manifest_path(project, &initial.id), &finished)?;
        append_event(
            project,
            "task.completed",
            "Initial analysis completed",
            serde_json::json!({
                "runId": initial.id,
                "codexThreadId": finished.codex_thread_id,
            }),
        )?;
        Ok(())
    } else {
        finished.status = RunStatus::Failed;
        finished.error = Some(
            turn_error
                .unwrap_or_else(|| format!("codex app-server exited before completion: {status}")),
        );
        write_json_pretty(&run_manifest_path(project, &initial.id), &finished)?;
        Err(AppError::Codex(
            finished
                .error
                .clone()
                .unwrap_or_else(|| "codex app-server failed".into()),
        ))
    }
}

fn join_reader(handle: thread::JoinHandle<AppResult<()>>) -> AppResult<()> {
    handle
        .join()
        .map_err(|_| AppError::Codex("codex output reader panicked".into()))?
}

fn codex_app_server_args() -> Vec<String> {
    vec!["app-server".into(), "--stdio".into()]
}

fn initialize_request() -> Value {
    serde_json::json!({
        "id": RPC_INITIALIZE_ID,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "gtm-agent",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "experimentalApi": true,
            },
        },
    })
}

fn thread_start_request(project: &Path) -> Value {
    serde_json::json!({
        "id": RPC_THREAD_START_ID,
        "method": "thread/start",
        "params": {
            "cwd": project.to_string_lossy(),
            "ephemeral": false,
            "approvalPolicy": "never",
            "sandbox": "workspace-write",
            "personality": "pragmatic",
            "threadSource": "gtm-agent",
            "sessionStartSource": "startup",
        },
    })
}

fn thread_name_request(thread_id: &str, project_name: &str) -> Value {
    serde_json::json!({
        "id": RPC_THREAD_NAME_ID,
        "method": "thread/name/set",
        "params": {
            "threadId": thread_id,
            "name": format!("{} GTM analysis", project_name),
        },
    })
}

fn turn_start_request(project: &Path, thread_id: &str, prompt: &str) -> Value {
    serde_json::json!({
        "id": RPC_TURN_START_ID,
        "method": "turn/start",
        "params": {
            "threadId": thread_id,
            "cwd": project.to_string_lossy(),
            "approvalPolicy": "never",
            "personality": "pragmatic",
            "sandboxPolicy": {
                "type": "workspaceWrite",
                "writableRoots": [project.to_string_lossy()],
                "networkAccess": true,
                "excludeTmpdirEnvVar": false,
                "excludeSlashTmp": false,
            },
            "input": [{
                "type": "text",
                "text": prompt,
                "text_elements": [],
            }],
        },
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
            .map_err(|_| AppError::Codex("log lock poisoned".into()))?,
        "{line}"
    )?;
    Ok(())
}

fn log_jsonl(log_file: &Arc<Mutex<File>>, value: &Value) -> AppResult<()> {
    writeln!(
        log_file
            .lock()
            .map_err(|_| AppError::Codex("log lock poisoned".into()))?,
        "{value}"
    )?;
    Ok(())
}

fn record_thread_id(project: &Path, initial: &RunState, thread_id: &str) -> AppResult<RunState> {
    let mut started = initial.clone();
    started.codex_thread_id = Some(thread_id.to_string());
    append_event(
        project,
        "codex.thread_started",
        "Codex thread started",
        serde_json::json!({
            "runId": initial.id,
            "codexThreadId": thread_id,
        }),
    )?;
    Ok(started)
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
        "id": id,
        "error": {
            "code": -32000,
            "message": "GTM Agent does not handle interactive server requests during initial analysis",
        },
    })
}

fn codex_thread_id_from_app_server_message(message: &Value) -> Option<String> {
    message
        .pointer("/result/thread/id")
        .or_else(|| message.pointer("/params/thread/id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn completed_turn_error(message: &Value) -> Option<String> {
    let status = message
        .pointer("/params/turn/status")
        .and_then(Value::as_str);
    match status {
        Some("completed") => None,
        Some("failed") | Some("interrupted") => Some(
            message
                .pointer("/params/turn/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Codex turn did not complete")
                .to_string(),
        ),
        Some(other) => Some(format!("Codex turn ended with status {other}")),
        None => Some("Codex turn completion was missing status".into()),
    }
}

fn detect_codex_impl() -> CodexDetection {
    match codex_binary() {
        Some(path) => match Command::new(&path).arg("--version").output() {
            Ok(output) if output.status.success() => CodexDetection {
                available: true,
                path: Some(path.to_string_lossy().to_string()),
                version: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
                error: None,
            },
            Ok(output) => CodexDetection {
                available: false,
                path: Some(path.to_string_lossy().to_string()),
                version: None,
                error: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            },
            Err(err) => CodexDetection {
                available: false,
                path: Some(path.to_string_lossy().to_string()),
                version: None,
                error: Some(err.to_string()),
            },
        },
        None => CodexDetection {
            available: false,
            path: None,
            version: None,
            error: Some("codex binary not found".into()),
        },
    }
}

fn codex_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CODEX_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    for path in [
        "/Users/maxi.lvn/.local/bin/codex",
        "/opt/homebrew/bin/codex",
        "/usr/local/bin/codex",
        "/usr/bin/codex",
    ] {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    Command::new("sh")
        .arg("-lc")
        .arg("command -v codex")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (!path.is_empty()).then(|| PathBuf::from(path))
        })
}

fn write_workspace_files(project_path: &Path, config: &ProjectConfig) -> AppResult<()> {
    fs::write(
        project_path.join("AGENTS.md"),
        format!(
            "# GTM Agent Project: {}\n\nThis folder is a local Codex workspace managed by GTM Agent.\n\n## Source of truth\n\n- `product-information.md`: product, features, proof points, and use cases.\n- `marketing-strategy.md`: ICP, personas, pain points, positioning, and channels.\n- `competitor-analysis.md`: competitors, alternatives, and positioning gaps.\n- `brand-voice.md`: voice, tone, vocabulary, and public messaging rules.\n\n## Event protocol\n\nWhen GTM Agent asks you to report progress, append JSON lines to `.gtm-agent/events.jsonl` with `eventType`, `summary`, `payload`, and `createdAt`.\n\nWebsite: {}\n",
            config.name, config.website_url
        ),
    )?;
    for (_, file_name, title) in DOCS {
        let body = format!(
            "# {title}\n\n_Status: pending Codex analysis_\n\nSource URL: {}\n\n",
            config.website_url
        );
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

    for line in BufReader::new(file).lines() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = value.get("method").and_then(Value::as_str);
        match method {
            Some("item/agentMessage/delta") => {
                if let (Some(item_id), Some(delta)) = (
                    value.pointer("/params/itemId").and_then(Value::as_str),
                    value.pointer("/params/delta").and_then(Value::as_str),
                ) {
                    message_deltas
                        .entry(item_id.to_string())
                        .or_default()
                        .push_str(delta);
                }
            }
            Some("item/completed") => match value
                .pointer("/params/item/type")
                .and_then(Value::as_str)
            {
                Some("agentMessage") => {
                    if let Some(text) = value.pointer("/params/item/text").and_then(Value::as_str) {
                        if let Some(item_id) =
                            value.pointer("/params/item/id").and_then(Value::as_str)
                        {
                            completed_messages.insert(item_id.to_string());
                        }
                        activity.push(RunActivity {
                            kind: "message".into(),
                            title: "Codex".into(),
                            message: compact_text(text, 520),
                        });
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    for (item_id, text) in message_deltas {
        if !completed_messages.contains(&item_id) && !text.trim().is_empty() {
            activity.push(RunActivity {
                kind: "message".into(),
                title: "Codex".into(),
                message: compact_text(&text, 520),
            });
        }
    }

    let keep_from = activity.len().saturating_sub(12);
    Ok(activity.into_iter().skip(keep_from).collect())
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
        r#"# GTM Agent Initial Analysis

You are running inside a local Codex workspace for GTM Agent.

Analyze this website: {url}

Rewrite exactly these files with concrete, useful findings:
- product-information.md
- marketing-strategy.md
- competitor-analysis.md
- brand-voice.md

For `competitor-analysis.md`, include a short `## Verified competitor links` section near the top. Each item must be a Markdown link using the official company or product website, for example `- [Competitor Name](https://example.com)`. Only include links you have verified from the public web or the source website; omit uncertain competitors instead of guessing domains.

Keep the files concise but specific enough that future GTM tasks can use them as source context. Include uncertainty where evidence is weak. Do not create outreach drafts, schedules, plugins, or extra strategy files. Do not post publicly or send messages.

Append progress and completion events to `.gtm-agent/events.jsonl` as JSON lines with eventType, summary, payload, and createdAt.
"#,
        url = config.website_url
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
    fn parses_app_server_thread_started_response() {
        let message = serde_json::json!({
            "id": RPC_THREAD_START_ID,
            "result": {
                "thread": {
                    "id": "019f2957-b734-7211-9bbc-f74f5980d6f3"
                }
            }
        });
        assert_eq!(
            codex_thread_id_from_app_server_message(&message).as_deref(),
            Some("019f2957-b734-7211-9bbc-f74f5980d6f3")
        );
    }

    #[test]
    fn parses_app_server_thread_started_notification() {
        let message = serde_json::json!({
            "method": "thread/started",
            "params": {
                "thread": {
                    "id": "019f2957-b734-7211-9bbc-f74f5980d6f3"
                }
            }
        });
        assert_eq!(
            codex_thread_id_from_app_server_message(&message).as_deref(),
            Some("019f2957-b734-7211-9bbc-f74f5980d6f3")
        );
    }

    #[test]
    fn builds_codex_app_server_args() {
        assert_eq!(codex_app_server_args(), vec!["app-server", "--stdio"]);
    }

    #[test]
    fn builds_thread_start_request_for_persisted_workspace_thread() {
        let request = thread_start_request(Path::new("/tmp/project"));
        assert_eq!(request["method"], "thread/start");
        assert_eq!(request["params"]["cwd"], "/tmp/project");
        assert_eq!(request["params"]["ephemeral"], false);
        assert_eq!(request["params"]["approvalPolicy"], "never");
        assert_eq!(request["params"]["sandbox"], "workspace-write");
        assert_eq!(request["params"]["threadSource"], "gtm-agent");
    }

    #[test]
    fn builds_turn_start_request_with_workspace_write_access() {
        let request = turn_start_request(Path::new("/tmp/project"), "thread_123", "analyze");
        assert_eq!(request["method"], "turn/start");
        assert_eq!(request["params"]["threadId"], "thread_123");
        assert_eq!(
            request["params"]["sandboxPolicy"],
            serde_json::json!({
                "type": "workspaceWrite",
                "writableRoots": ["/tmp/project"],
                "networkAccess": true,
                "excludeTmpdirEnvVar": false,
                "excludeSlashTmp": false,
            })
        );
        assert_eq!(request["params"]["input"][0]["text"], "analyze");
    }

    #[test]
    fn detects_failed_completed_turns() {
        let completed = serde_json::json!({
            "method": "turn/completed",
            "params": {
                "turn": {
                    "status": "completed"
                }
            }
        });
        assert!(completed_turn_error(&completed).is_none());

        let failed = serde_json::json!({
            "method": "turn/completed",
            "params": {
                "turn": {
                    "status": "failed",
                    "error": {
                        "message": "model failed"
                    }
                }
            }
        });
        assert_eq!(
            completed_turn_error(&failed).as_deref(),
            Some("model failed")
        );
    }

    #[test]
    fn reads_run_activity_from_app_server_log() {
        let log_path =
            std::env::temp_dir().join(format!("gtm-agent-log-{}.jsonl", Uuid::new_v4().simple()));
        fs::write(
            &log_path,
            [
                serde_json::json!({
                    "method": "thread/started",
                    "params": { "thread": { "id": "thread_123" } }
                })
                .to_string(),
                serde_json::json!({
                    "method": "item/completed",
                    "params": {
                        "item": {
                            "type": "commandExecution",
                            "status": "completed",
                            "command": "curl https://example.com"
                        }
                    }
                })
                .to_string(),
                serde_json::json!({
                    "method": "item/completed",
                    "params": {
                        "item": {
                            "type": "agentMessage",
                            "id": "msg_123",
                            "text": "I found the source material."
                        }
                    }
                })
                .to_string(),
                serde_json::json!({
                    "method": "turn/completed",
                    "params": { "turn": { "status": "completed" } }
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

        let activity = read_run_activity(&log_path).unwrap();
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].title, "Codex");
        assert_eq!(activity[0].message, "I found the source material.");

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
        }
        assert!(project_path.join(".gtm-agent/events.jsonl").exists());

        fs::remove_dir_all(project_path).unwrap();
    }
}
