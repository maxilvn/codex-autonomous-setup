use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
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
    enabled: bool,
    selected: bool,
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
    chrome_profile_id: Option<String>,
    selected_channels: Vec<String>,
    latest_run: Option<RunState>,
    run_activity: Vec<RunActivity>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSettings {
    chrome_profile_id: Option<String>,
    #[serde(default)]
    selected_channels: Vec<String>,
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
    channels: Vec<String>,
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
    account_status: ChannelAccountStatus,
    login_status: ChannelLoginStatus,
    analysis_status: ChannelAnalysisStatus,
    account_label: Option<String>,
    account_handle: Option<String>,
    account_avatar_url: Option<String>,
    chrome_profile_id: Option<String>,
    check_method: Option<String>,
    checked_at: Option<String>,
    path: String,
    files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChromeProfile {
    id: String,
    name: String,
    email: Option<String>,
    account_name: Option<String>,
    avatar_path: Option<String>,
    avatar_data_url: Option<String>,
    profile_color: Option<i64>,
    sessions: HashMap<String, bool>,
    is_recommended: bool,
    is_default: bool,
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
enum ChannelAccountStatus {
    NotConfigured,
    Checking,
    Authenticated,
    NeedsLogin,
    Unknown,
}

impl Default for ChannelAccountStatus {
    fn default() -> Self {
        Self::NotConfigured
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ChannelLoginStatus {
    Unknown,
    NeedsLogin,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ChannelAnalysisStatus {
    NotStarted,
    Running,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelStatus {
    #[serde(default)]
    account_status: ChannelAccountStatus,
    login_status: ChannelLoginStatus,
    analysis_status: ChannelAnalysisStatus,
    account_label: Option<String>,
    account_handle: Option<String>,
    account_avatar_url: Option<String>,
    chrome_profile_id: Option<String>,
    check_method: Option<String>,
    checked_at: Option<String>,
    updated_at: String,
}

impl ChannelStatus {
    fn empty() -> Self {
        Self {
            account_status: ChannelAccountStatus::NotConfigured,
            login_status: ChannelLoginStatus::Unknown,
            analysis_status: ChannelAnalysisStatus::NotStarted,
            account_label: None,
            account_handle: None,
            account_avatar_url: None,
            chrome_profile_id: None,
            check_method: None,
            checked_at: None,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

struct ChannelLoginCheck {
    account_status: ChannelAccountStatus,
    account_label: Option<String>,
    account_handle: Option<String>,
    account_avatar_url: Option<String>,
    chrome_profile_id: Option<String>,
    check_method: String,
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

const CHANNEL_CONTEXT_DOCS: [(&str, &str, &str); 4] = [
    ("profile", "profile.md", "Profile"),
    ("voice", "voice.md", "Voice"),
    ("rules", "rules.md", "Rules"),
    ("examples", "examples.md", "Examples"),
];

struct ChannelDef {
    id: &'static str,
    name: &'static str,
    login_url: &'static str,
    home_url: &'static str,
    cookie_hosts: &'static [&'static str],
    cookie_names: &'static [&'static str],
}

const CHANNELS: [ChannelDef; 3] = [
    ChannelDef {
        id: "x",
        name: "X",
        login_url: "https://x.com/i/flow/login",
        home_url: "https://x.com/home",
        cookie_hosts: &[".x.com", "x.com", ".twitter.com", "twitter.com"],
        cookie_names: &["auth_token"],
    },
    ChannelDef {
        id: "reddit",
        name: "Reddit",
        login_url: "https://www.reddit.com/login/",
        home_url: "https://www.reddit.com/",
        cookie_hosts: &[".reddit.com", "reddit.com", ".www.reddit.com"],
        cookie_names: &["reddit_session", "token_v2"],
    },
    ChannelDef {
        id: "hacker-news",
        name: "Hacker News",
        login_url: "https://news.ycombinator.com/login",
        home_url: "https://news.ycombinator.com/",
        cookie_hosts: &[
            "news.ycombinator.com",
            ".news.ycombinator.com",
            ".ycombinator.com",
        ],
        cookie_names: &["user"],
    },
];

fn channel_def(channel_id: &str) -> AppResult<&'static ChannelDef> {
    CHANNELS
        .iter()
        .find(|channel| channel.id == channel_id)
        .ok_or_else(|| AppError::Invalid(format!("unsupported channel: {channel_id}")))
}

const RPC_INITIALIZE_ID: i64 = 1;
const RPC_SESSION_NEW_ID: i64 = 2;
const RPC_SESSION_PROMPT_ID: i64 = 3;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_agent_providers,
            select_agent_provider,
            default_project_path,
            create_project,
            load_last_project,
            load_project,
            load_channel_context_doc,
            run_initial_analysis,
            configure_channel,
            list_chrome_profiles,
            select_chrome_profile,
            set_selected_channels,
            verify_channel_login,
            run_channel_analysis,
            open_project_in_codex,
            open_external_url,
            open_chrome_url,
            open_channel_login
        ])
        .run(tauri::generate_context!())
        .expect("error while running GTM Agent");
}

#[tauri::command]
fn list_agent_providers() -> AppResult<Vec<AgentProviderStatus>> {
    Ok(read_app_settings()?
        .providers
        .iter()
        .map(provider_status)
        .collect())
}

#[tauri::command]
fn select_agent_provider(provider_id: String) -> AppResult<AgentProviderStatus> {
    let mut settings = read_app_settings()?;
    let candidate = settings
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::Invalid(format!("unknown agent provider: {provider_id}")))?;
    let candidate_status = provider_status(candidate);
    if !candidate_status.available {
        return Err(AppError::Invalid(format!(
            "{} is not installed on this machine. Install `{}` first.",
            candidate.title, candidate.command
        )));
    }

    for provider in &mut settings.providers {
        let is_selected = provider.id == provider_id;
        provider.selected = is_selected;
        if is_selected {
            provider.enabled = true;
        }
    }
    write_app_settings(&settings)?;
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
    let settings = read_project_settings(&path);
    Ok(ProjectState {
        config,
        agent_provider: selected_provider_status()?,
        docs: read_docs(&path)?,
        channel_setups: read_channel_setups(&path),
        chrome_profile_id: settings.chrome_profile_id,
        selected_channels: settings.selected_channels,
        latest_run,
        run_activity,
    })
}

#[tauri::command]
fn load_channel_context_doc(
    project_path: String,
    channel_id: String,
    file_name: String,
) -> AppResult<ContextDoc> {
    let channel = channel_def(&channel_id)?;
    let (key, file_name, title) = channel_context_doc(&file_name)
        .ok_or_else(|| AppError::Invalid("unsupported channel file".into()))?;
    let path = PathBuf::from(project_path)
        .join(".gtm-agent/channels")
        .join(channel.id)
        .join(file_name);
    Ok(ContextDoc {
        key: format!("{}_{key}", channel.id.replace('-', "_")),
        file_name: file_name.into(),
        title: title.into(),
        content: fs::read_to_string(path).unwrap_or_default(),
    })
}

#[tauri::command]
fn configure_channel(project_path: String, channel_id: String) -> AppResult<ProjectState> {
    let path = PathBuf::from(project_path);
    let channel = channel_def(&channel_id)?;
    write_channel_setup(&path, channel)?;
    append_event(
        &path,
        "channel.configured",
        &format!("{} channel setup initialized", channel.name),
        serde_json::json!({
            "channelId": channel.id,
            "mode": "chrome_session_draft_first",
        }),
    )?;
    load_project(path.to_string_lossy().to_string())
}

#[tauri::command]
fn list_chrome_profiles() -> AppResult<Vec<ChromeProfile>> {
    read_chrome_profiles()
}

#[tauri::command]
fn select_chrome_profile(project_path: String, profile_id: String) -> AppResult<ProjectState> {
    let path = PathBuf::from(project_path);
    let mut settings = read_project_settings(&path);
    settings.chrome_profile_id = Some(profile_id.clone());
    write_project_settings(&path, &settings)?;
    append_event(
        &path,
        "browser.profile_selected",
        "Chrome profile selected",
        serde_json::json!({ "chromeProfileId": profile_id }),
    )?;
    load_project(path.to_string_lossy().to_string())
}

#[tauri::command]
fn set_selected_channels(
    project_path: String,
    channel_ids: Vec<String>,
) -> AppResult<ProjectState> {
    let path = PathBuf::from(project_path);
    for channel_id in &channel_ids {
        channel_def(channel_id)?;
    }
    let mut settings = read_project_settings(&path);
    settings.selected_channels = channel_ids;
    write_project_settings(&path, &settings)?;
    load_project(path.to_string_lossy().to_string())
}

#[tauri::command]
fn verify_channel_login(
    project_path: String,
    channel_id: String,
    profile_id: Option<String>,
) -> AppResult<ProjectState> {
    let path = PathBuf::from(project_path);
    let channel = channel_def(&channel_id)?;
    write_channel_setup(&path, channel)?;
    let login = check_channel_login_in_chrome(channel, profile_id.as_deref())?;
    let login_status = match login.account_status {
        ChannelAccountStatus::Authenticated => ChannelLoginStatus::Verified,
        ChannelAccountStatus::NeedsLogin => ChannelLoginStatus::NeedsLogin,
        _ => ChannelLoginStatus::Unknown,
    };
    let current = read_channel_status(&path, channel.id);
    write_channel_status(
        &path,
        channel.id,
        ChannelStatus {
            account_status: login.account_status,
            login_status,
            analysis_status: current.analysis_status,
            account_label: login.account_label,
            account_handle: login.account_handle,
            account_avatar_url: login.account_avatar_url,
            chrome_profile_id: login.chrome_profile_id,
            check_method: Some(login.check_method),
            checked_at: Some(Utc::now().to_rfc3339()),
            updated_at: Utc::now().to_rfc3339(),
        },
    )?;
    load_project(path.to_string_lossy().to_string())
}

#[tauri::command]
fn run_channel_analysis(
    app: tauri::AppHandle,
    project_path: String,
    channel_ids: Vec<String>,
) -> AppResult<RunState> {
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
    if channel_ids.is_empty() {
        return Err(AppError::Invalid("select at least one channel".into()));
    }
    let channels = channel_ids
        .iter()
        .map(|channel_id| channel_def(channel_id))
        .collect::<AppResult<Vec<_>>>()?;
    for channel in &channels {
        write_channel_setup(&path, channel)?;
        let status = read_channel_status(&path, channel.id);
        if status.account_status != ChannelAccountStatus::Authenticated {
            return Err(AppError::Invalid(format!(
                "sign in to {} in the selected Chrome profile before starting analysis",
                channel.name
            )));
        }
    }
    for channel in &channels {
        let current = read_channel_status(&path, channel.id);
        write_channel_status(
            &path,
            channel.id,
            ChannelStatus {
                analysis_status: ChannelAnalysisStatus::Running,
                updated_at: Utc::now().to_rfc3339(),
                ..current
            },
        )?;
    }

    let run_id = format!("channels_run_{}", Utc::now().format("%Y%m%d%H%M%S"));
    let run_path = run_manifest_path(&path, &run_id);
    let log_path = path.join(".gtm-agent/runs").join(format!("{run_id}.jsonl"));
    let run = RunState {
        id: run_id.clone(),
        kind: "channel_analysis".into(),
        status: RunStatus::Running,
        channels: channel_ids.clone(),
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
        "channel.analysis_started",
        "Channel account analysis started",
        serde_json::json!({ "runId": run.id, "channelIds": channel_ids }),
    )?;

    let app_handle = app.clone();
    let run_for_thread = run.clone();
    let chrome_profile_id = read_project_settings(&path).chrome_profile_id;
    thread::spawn(move || {
        let browser_dir = chrome_profile_id
            .as_deref()
            .filter(|_| resolve_command("npx").is_some())
            .and_then(|profile_id| match seed_browser_user_data_dir(profile_id) {
                Ok(dir) => {
                    let _ = append_event(
                        &path,
                        "browser.session_prepared",
                        "Browser session prepared from the selected Chrome profile",
                        serde_json::json!({ "runId": run_id, "chromeProfileId": profile_id }),
                    );
                    Some(dir)
                }
                Err(err) => {
                    let _ = append_event(
                        &path,
                        "browser.session_unavailable",
                        &format!("Browser session unavailable: {err}"),
                        serde_json::json!({ "runId": run_id, "chromeProfileId": profile_id }),
                    );
                    None
                }
            });
        let mcp_servers = browser_dir
            .as_deref()
            .map(|dir| vec![browser_mcp_server(dir)])
            .unwrap_or_default();

        let mut first_error: Option<String> = None;
        for channel in &channels {
            let status = read_channel_status(&path, channel.id);
            let result = execute_agent_turn(
                &path,
                &run_for_thread,
                &provider,
                &channel_analysis_prompt(
                    &config,
                    channel,
                    &status,
                    chrome_profile_id.as_deref(),
                    !mcp_servers.is_empty(),
                ),
                &format!("{} account analysis", channel.name),
                &mcp_servers,
            );
            if let Some(dir) = browser_dir.as_deref() {
                kill_browser_processes(dir);
            }
            let reported = read_channel_status(&path, channel.id);
            let next_status = match &result {
                Ok(()) => ChannelStatus {
                    account_status: ChannelAccountStatus::Authenticated,
                    login_status: ChannelLoginStatus::Verified,
                    analysis_status: ChannelAnalysisStatus::Ready,
                    account_label: read_channel_account_label(&path, channel.id)
                        .or(reported.account_label.clone()),
                    updated_at: Utc::now().to_rfc3339(),
                    ..reported
                },
                Err(_) => ChannelStatus {
                    analysis_status: ChannelAnalysisStatus::Failed,
                    updated_at: Utc::now().to_rfc3339(),
                    ..reported
                },
            };
            let _ = write_channel_status(&path, channel.id, next_status);
            if let Err(err) = result {
                let _ = append_event(
                    &path,
                    "channel.analysis_failed",
                    &format!("{} account analysis failed: {err}", channel.name),
                    serde_json::json!({ "runId": run_id, "channelId": channel.id }),
                );
                first_error.get_or_insert(err.to_string());
            }
            let _ = write_json_pretty(
                &run_manifest_path(&path, &run_for_thread.id),
                &run_for_thread,
            );
            let _ = app_handle.emit(
                "project-updated",
                serde_json::json!({ "projectPath": path.to_string_lossy(), "runId": run_id }),
            );
        }

        if let Some(dir) = browser_dir.as_deref() {
            cleanup_browser_user_data_dir(dir);
        }

        let mut finished = run_for_thread.clone();
        finished.completed_at = Some(Utc::now().to_rfc3339());
        finished.status = if first_error.is_some() {
            RunStatus::Failed
        } else {
            RunStatus::Completed
        };
        finished.error = first_error;
        let _ = write_json_pretty(&run_manifest_path(&path, &run_for_thread.id), &finished);
        let _ = app_handle.emit(
            "project-updated",
            serde_json::json!({ "projectPath": path.to_string_lossy(), "runId": run_id }),
        );
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
        channels: Vec::new(),
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

#[tauri::command]
fn open_channel_login(channel_id: String, profile_id: Option<String>) -> AppResult<()> {
    let channel = channel_def(&channel_id)?;
    open_chrome_url_in_profile(channel.login_url, profile_id.as_deref())
}

fn open_chrome_url_in_profile(url: &str, profile_id: Option<&str>) -> AppResult<()> {
    let normalized = normalize_url(url)?;
    if let Some(profile_id) = profile_id.filter(|value| !value.trim().is_empty()) {
        Command::new("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
            .arg(format!("--profile-directory={profile_id}"))
            .arg(normalized)
            .spawn()
            .map_err(|err| AppError::Open(err.to_string()))?;
        return Ok(());
    }
    open_chrome_url(normalized)
}

fn chrome_user_data_dir() -> AppResult<PathBuf> {
    Ok(dirs::home_dir()
        .ok_or_else(|| AppError::Open("could not locate the home directory".into()))?
        .join("Library/Application Support/Google/Chrome"))
}

fn read_chrome_profiles() -> AppResult<Vec<ChromeProfile>> {
    let user_data_dir = chrome_user_data_dir()?;
    let local_state_path = user_data_dir.join("Local State");
    if !local_state_path.exists() {
        return Ok(Vec::new());
    }
    let local_state: Value = read_json(&local_state_path)?;
    let Some(info_cache) = local_state
        .get("profile")
        .and_then(|profile| profile.get("info_cache"))
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };

    let mut profiles = info_cache
        .iter()
        .map(|(id, profile)| ChromeProfile {
            id: id.clone(),
            name: profile
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(id)
                .to_string(),
            email: profile
                .get("user_name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            account_name: profile
                .get("gaia_name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            avatar_path: chrome_profile_avatar_path(&user_data_dir, id),
            avatar_data_url: chrome_profile_avatar_data_url(&user_data_dir, id),
            profile_color: profile
                .get("profile_highlight_color")
                .and_then(Value::as_i64),
            sessions: check_profile_channel_sessions(id).unwrap_or_default(),
            is_recommended: false,
            is_default: id == "Default",
        })
        .collect::<Vec<_>>();
    let session_count =
        |profile: &ChromeProfile| profile.sessions.values().filter(|value| **value).count();
    let recommended_id = profiles
        .iter()
        .filter(|profile| session_count(profile) > 0 && !profile.is_default)
        .max_by(|a, b| {
            session_count(a)
                .cmp(&session_count(b))
                .then_with(|| chrome_profile_sort_key(&b.id).cmp(&chrome_profile_sort_key(&a.id)))
        })
        .or_else(|| profiles.iter().find(|profile| session_count(profile) > 0))
        .or_else(|| profiles.iter().find(|profile| profile.is_default))
        .map(|profile| profile.id.clone());
    for profile in &mut profiles {
        profile.is_recommended = recommended_id.as_deref() == Some(profile.id.as_str());
    }
    profiles.sort_by(|a, b| {
        b.is_recommended
            .cmp(&a.is_recommended)
            .then_with(|| session_count(b).cmp(&session_count(a)))
            .then_with(|| chrome_profile_sort_key(&a.id).cmp(&chrome_profile_sort_key(&b.id)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(profiles)
}

fn chrome_profile_avatar_path(user_data_dir: &Path, profile_id: &str) -> Option<String> {
    [
        "Google Profile Picture.png",
        "Account Avatar.png",
        "Profile Picture.png",
    ]
    .iter()
    .map(|file_name| user_data_dir.join(profile_id).join(file_name))
    .find(|path| path.exists())
    .map(|path| path.to_string_lossy().to_string())
}

fn chrome_profile_avatar_data_url(user_data_dir: &Path, profile_id: &str) -> Option<String> {
    let path = chrome_profile_avatar_path(user_data_dir, profile_id)?;
    let bytes = fs::read(path).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:image/png;base64,{encoded}"))
}

fn chrome_profile_sort_key(profile_id: &str) -> i64 {
    if profile_id == "Default" {
        return i64::MAX;
    }
    profile_id
        .strip_prefix("Profile ")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(i64::MAX - 1)
}

fn check_channel_login_in_chrome(
    channel: &ChannelDef,
    profile_id: Option<&str>,
) -> AppResult<ChannelLoginCheck> {
    let unknown = |profile_id: Option<String>| ChannelLoginCheck {
        account_status: ChannelAccountStatus::Unknown,
        account_label: None,
        account_handle: None,
        account_avatar_url: None,
        chrome_profile_id: profile_id,
        check_method: "chrome_cookie_probe".into(),
    };
    let Some(profile_id) = profile_id
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
    else {
        return Ok(unknown(None));
    };
    let has_session = match check_channel_session_cookies(channel, &profile_id) {
        Ok(has_session) => has_session,
        Err(_) => return Ok(unknown(Some(profile_id))),
    };
    if !has_session {
        return Ok(ChannelLoginCheck {
            account_status: ChannelAccountStatus::NeedsLogin,
            account_label: None,
            account_handle: None,
            account_avatar_url: None,
            chrome_profile_id: Some(profile_id),
            check_method: "chrome_cookie_probe".into(),
        });
    }

    Ok(ChannelLoginCheck {
        account_status: ChannelAccountStatus::Authenticated,
        account_label: Some(format!("{} account detected", channel.name)),
        account_handle: None,
        account_avatar_url: None,
        chrome_profile_id: Some(profile_id),
        check_method: "chrome_cookie_probe".into(),
    })
}

fn sqlite_string_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(",")
}

fn channel_cookie_count_query(channel: &ChannelDef) -> String {
    format!(
        "select count(*) from cookies where host_key in ({}) and name in ({});",
        sqlite_string_list(channel.cookie_hosts),
        sqlite_string_list(channel.cookie_names),
    )
}

fn profile_cookies_path(profile_id: &str) -> AppResult<Option<PathBuf>> {
    let user_data_dir = chrome_user_data_dir()?;
    Ok([
        user_data_dir.join(profile_id).join("Network/Cookies"),
        user_data_dir.join(profile_id).join("Cookies"),
    ]
    .into_iter()
    .find(|path| path.exists()))
}

fn copy_profile_cookies(profile_id: &str) -> AppResult<Option<PathBuf>> {
    let Some(cookies_path) = profile_cookies_path(profile_id)? else {
        return Ok(None);
    };
    let temp_path = std::env::temp_dir().join(format!(
        "gtm-agent-chrome-cookies-{}.sqlite",
        Uuid::new_v4().simple()
    ));
    fs::copy(&cookies_path, &temp_path)?;
    Ok(Some(temp_path))
}

fn seed_browser_user_data_dir(profile_id: &str) -> AppResult<PathBuf> {
    let Some(cookies_path) = profile_cookies_path(profile_id)? else {
        return Err(AppError::Open(format!(
            "no Chrome cookies found for profile {profile_id}"
        )));
    };
    let user_data_dir =
        std::env::temp_dir().join(format!("gtm-agent-browser-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(user_data_dir.join("Default/Network"))?;
    fs::copy(&cookies_path, user_data_dir.join("Default/Network/Cookies"))?;
    fs::copy(&cookies_path, user_data_dir.join("Default/Cookies"))?;
    fs::write(user_data_dir.join("First Run"), "")?;
    Ok(user_data_dir)
}

fn kill_browser_processes(user_data_dir: &Path) {
    if let Some(marker) = user_data_dir.file_name().and_then(|name| name.to_str()) {
        if marker.starts_with("gtm-agent-browser-") {
            let _ = Command::new("pkill").arg("-f").arg(marker).status();
        }
    }
}

fn cleanup_browser_user_data_dir(user_data_dir: &Path) {
    kill_browser_processes(user_data_dir);
    thread::sleep(std::time::Duration::from_millis(500));
    let _ = fs::remove_dir_all(user_data_dir);
}

fn browser_mcp_server(user_data_dir: &Path) -> Value {
    serde_json::json!({
        "name": "chrome-devtools",
        "command": "npx",
        "args": [
            "-y",
            "chrome-devtools-mcp@latest",
            "--userDataDir",
            user_data_dir.to_string_lossy(),
            "--viewport",
            "1280x900",
            "--ignore-default-chrome-arg=--use-mock-keychain",
        ],
        "env": [],
    })
}

fn run_cookie_queries(cookies_db: &Path, queries: &str) -> AppResult<Vec<i64>> {
    let output = Command::new("sqlite3")
        .arg(cookies_db)
        .arg(queries)
        .output()
        .map_err(|err| AppError::Open(format!("failed to run sqlite3: {err}")))?;
    if !output.status.success() {
        return Err(AppError::Open(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().parse::<i64>().unwrap_or(0))
        .collect())
}

fn check_channel_session_cookies(channel: &ChannelDef, profile_id: &str) -> AppResult<bool> {
    let Some(temp_path) = copy_profile_cookies(profile_id)? else {
        return Ok(false);
    };
    let result = run_cookie_queries(&temp_path, &channel_cookie_count_query(channel));
    let _ = fs::remove_file(&temp_path);
    Ok(result?.first().copied().unwrap_or(0) > 0)
}

fn check_profile_channel_sessions(profile_id: &str) -> AppResult<HashMap<String, bool>> {
    let mut sessions: HashMap<String, bool> = CHANNELS
        .iter()
        .map(|channel| (channel.id.to_string(), false))
        .collect();
    let Some(temp_path) = copy_profile_cookies(profile_id)? else {
        return Ok(sessions);
    };
    let queries = CHANNELS
        .iter()
        .map(channel_cookie_count_query)
        .collect::<Vec<_>>()
        .join(" ");
    let result = run_cookie_queries(&temp_path, &queries);
    let _ = fs::remove_file(&temp_path);
    let counts = result?;
    for (channel, count) in CHANNELS.iter().zip(counts) {
        sessions.insert(channel.id.to_string(), count > 0);
    }
    Ok(sessions)
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
        &[],
    )
}

fn execute_agent_turn(
    project: &Path,
    initial: &RunState,
    provider: &AgentProvider,
    prompt: &str,
    task_label: &str,
    mcp_servers: &[Value],
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
        .env("PATH", command_env_path())
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
                permission_response(request_id, &message)
            } else {
                unsupported_server_request_response(request_id)
            };
            send_rpc(&mut stdin, &response, &log_file)?;
            continue;
        }

        if rpc_id_is(&message, RPC_INITIALIZE_ID) {
            send_rpc(
                &mut stdin,
                &acp_session_new_request(project, mcp_servers),
                &log_file,
            )?;
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

        if initial.kind == "initial_analysis"
            && initial_analysis_completion_time(project, &initial.started_at)?.is_some()
        {
            turn_completed = true;
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

fn acp_session_new_request(project: &Path, mcp_servers: &[Value]) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": RPC_SESSION_NEW_ID,
        "method": "session/new",
        "params": {
            "cwd": project.to_string_lossy(),
            "mcpServers": mcp_servers,
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

fn permission_response(id: Value, request: &Value) -> Value {
    let allow_option = request
        .pointer("/params/options")
        .and_then(Value::as_array)
        .and_then(|options| {
            options.iter().find_map(|option| {
                let kind = option.get("kind").and_then(Value::as_str);
                let option_id = option.get("optionId").and_then(Value::as_str);
                matches!(kind, Some("allow_once") | Some("allow_always")).then_some(option_id)?
            })
        });

    if let Some(option_id) = allow_option {
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
            "message": "GTM Agent only handles ACP permission requests during analysis",
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
            enabled: provider.enabled,
            selected: provider.selected,
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
            enabled: provider.enabled,
            selected: provider.selected,
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
        provider("devin", "Devin", "devin", &["acp"], false),
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

    command_search_paths()
        .into_iter()
        .map(|path| path.join(command))
        .find(|path| path.exists())
}

fn command_search_paths() -> Vec<PathBuf> {
    let mut paths = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    for path in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        let path = PathBuf::from(path);
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    }
    paths
}

fn command_env_path() -> OsString {
    env::join_paths(command_search_paths())
        .unwrap_or_else(|_| env::var_os("PATH").unwrap_or_default())
}

fn command_version(path: &Path) -> Option<String> {
    Command::new(path)
        .arg("--version")
        .env("PATH", command_env_path())
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

fn reconcile_initial_analysis_completion(
    project_path: &Path,
    run_path: &Path,
    run: &mut RunState,
) -> AppResult<()> {
    if run.kind != "initial_analysis" || run.status != RunStatus::Running {
        return Ok(());
    }

    let Some(completed_at) = initial_analysis_completion_time(project_path, &run.started_at)?
    else {
        return Ok(());
    };

    run.status = RunStatus::Completed;
    run.completed_at = Some(completed_at);
    run.error = None;
    write_json_pretty(run_path, run)
}

fn initial_analysis_completion_time(
    project_path: &Path,
    run_started_at: &str,
) -> AppResult<Option<String>> {
    if !source_docs_have_content(project_path) {
        return Ok(None);
    }
    source_docs_completion_event_time(project_path, run_started_at)
}

fn source_docs_have_content(project_path: &Path) -> bool {
    DOCS.iter().all(|(_, file_name, title)| {
        fs::read_to_string(project_path.join(file_name))
            .map(|content| document_has_body_content(&content, title))
            .unwrap_or(false)
    })
}

fn document_has_body_content(content: &str, title: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && trimmed != format!("# {title}")
    })
}

fn source_docs_completion_event_time(
    project_path: &Path,
    run_started_at: &str,
) -> AppResult<Option<String>> {
    let path = project_path.join(".gtm-agent/events.jsonl");
    if !path.exists() {
        return Ok(None);
    }

    let file = File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if is_source_docs_completion_event(&value) {
            let completed_at = value
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            if event_happened_after_run_start(&completed_at, run_started_at) {
                return Ok(Some(completed_at));
            }
        }
    }
    Ok(None)
}

fn event_happened_after_run_start(event_at: &str, run_started_at: &str) -> bool {
    let Ok(event_at) = DateTime::parse_from_rfc3339(event_at) else {
        return false;
    };
    let Ok(run_started_at) = DateTime::parse_from_rfc3339(run_started_at) else {
        return false;
    };
    event_at >= run_started_at
}

fn is_source_docs_completion_event(value: &Value) -> bool {
    if value.get("eventType").and_then(Value::as_str) != Some("task.completed") {
        return false;
    }

    let completed_docs = value
        .pointer("/payload/documents")
        .and_then(Value::as_array)
        .map(|documents| {
            documents
                .iter()
                .filter_map(Value::as_str)
                .collect::<HashSet<_>>()
        });
    if let Some(completed_docs) = completed_docs {
        return DOCS
            .iter()
            .all(|(_, file_name, _)| completed_docs.contains(file_name));
    }

    value
        .get("summary")
        .and_then(Value::as_str)
        .map(|summary| summary.eq_ignore_ascii_case("GTM source documents completed"))
        .unwrap_or(false)
}

fn channel_context_doc(file_name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    CHANNEL_CONTEXT_DOCS
        .iter()
        .copied()
        .find(|(_, candidate, _)| *candidate == file_name)
}

fn read_channel_setups(project_path: &Path) -> Vec<ChannelSetup> {
    CHANNELS
        .iter()
        .map(|channel| read_channel_setup(project_path, channel))
        .collect()
}

fn read_channel_setup(project_path: &Path, channel: &ChannelDef) -> ChannelSetup {
    let channel_path = project_path.join(".gtm-agent/channels").join(channel.id);
    if !channel_path.exists() {
        return ChannelSetup {
            id: channel.id.into(),
            name: channel.name.into(),
            status: ChannelSetupStatus::NotConfigured,
            account_status: ChannelAccountStatus::NotConfigured,
            login_status: ChannelLoginStatus::Unknown,
            analysis_status: ChannelAnalysisStatus::NotStarted,
            account_label: None,
            account_handle: None,
            account_avatar_url: None,
            chrome_profile_id: None,
            check_method: None,
            checked_at: None,
            path: channel_path.to_string_lossy().to_string(),
            files: Vec::new(),
        };
    }

    let channel_status = read_channel_status(project_path, channel.id);
    let setup_status = match (
        &channel_status.account_status,
        &channel_status.analysis_status,
    ) {
        (ChannelAccountStatus::Authenticated, ChannelAnalysisStatus::Running) => {
            ChannelSetupStatus::Analyzing
        }
        (ChannelAccountStatus::Authenticated, ChannelAnalysisStatus::Ready) => {
            ChannelSetupStatus::Ready
        }
        (_, ChannelAnalysisStatus::Failed) => ChannelSetupStatus::Failed,
        (ChannelAccountStatus::NeedsLogin, _) => ChannelSetupStatus::NeedsLogin,
        _ => ChannelSetupStatus::NotConfigured,
    };
    let files = ["profile.md", "rules.md", "examples.md", "voice.md"]
        .iter()
        .filter(|file_name| channel_path.join(file_name).exists())
        .map(|file_name| (*file_name).to_string())
        .collect::<Vec<_>>();

    ChannelSetup {
        id: channel.id.into(),
        name: channel.name.into(),
        status: setup_status,
        account_status: channel_status.account_status,
        login_status: channel_status.login_status,
        analysis_status: channel_status.analysis_status,
        account_label: channel_status.account_label,
        account_handle: channel_status.account_handle,
        account_avatar_url: channel_status.account_avatar_url,
        chrome_profile_id: channel_status.chrome_profile_id,
        check_method: channel_status.check_method,
        checked_at: channel_status.checked_at,
        path: channel_path.to_string_lossy().to_string(),
        files,
    }
}

fn write_channel_setup(project_path: &Path, channel: &ChannelDef) -> AppResult<()> {
    let channel_path = project_path.join(".gtm-agent/channels").join(channel.id);
    fs::create_dir_all(channel_path.join("drafts"))?;
    let name = channel.name;

    write_if_missing(
        &channel_path.join("profile.md"),
        &format!("# {name} Channel Profile\n\n## Connection\n\n- Mode: Existing Chrome session\n- Posting: Browser-assisted after explicit user approval\n- API: Not required for MVP\n\n## Account voice to learn\n\nThe agent should learn this from the signed-in {name} account before recurring runs:\n\n- Profile bio and positioning\n- Recent posts and replies\n- Topics the account naturally discusses\n- Phrases, pacing, and formatting that sound native to the account\n- Posts or replies the user marks as strong examples\n\n## Operating posture\n\nUse global brand voice as the base, then adapt it for {name}: concise, specific, and conversational. Avoid generic launch hype and avoid posting without review.\n"),
    )?;
    write_if_missing(
        &channel_path.join("rules.md"),
        &format!("# {name} Channel Rules\n\n## Approval\n\n- Manual approval is required before every post or reply.\n- The agent may search, rank opportunities, and draft replies.\n- The agent may open Chrome and prepare a post only after the user confirms.\n- The final public action must be visible to the user before submission.\n\n## Draft standards\n\n- Lead with the problem or observation, not a pitch.\n- Prefer useful replies to cold promotion.\n- Use product mentions only when the thread context makes them natural.\n- Do not make unsupported claims beyond the brand source documents.\n- Save strong approved posts back to `examples.md`.\n\n## Avoid\n\n- Spammy reply chains\n- Generic AI/productivity claims\n- Engagement bait\n- Posting into threads where the product is not relevant\n"),
    )?;
    write_if_missing(
        &channel_path.join("examples.md"),
        &format!("# {name} Examples\n\nUse this file as the channel-specific memory for what good looks like.\n\n## Strong examples\n\n_Add approved posts and replies here after the user marks them as good._\n\n## Avoid examples\n\n_Add drafts or posts that felt too salesy, off-tone, or low-signal._\n"),
    )?;
    write_if_missing(
        &channel_path.join("voice.md"),
        &format!("# {name} Account Voice\n\n_Pending account analysis._\n\nThe agent should replace this with account-specific voice guidance after reviewing the signed-in {name} account, recent posts, replies, and strong examples.\n"),
    )?;
    write_if_missing(
        &channel_path.join("searches.md"),
        &format!("# {name} Search Strategy\n\n## Inputs\n\nUse `marketing-strategy.md`, `brand-voice.md`, competitor names, ICP language, and pain-point terms from the brand analysis.\n\n## Opportunity types\n\n- Buyer pain posts\n- Founder/operator discussions\n- Competitor or alternative mentions\n- Category education threads\n- Launch or workflow discussions where a helpful reply fits\n\n## Daily run output\n\nFor every opportunity, capture the source URL, why it matters, suggested angle, draft reply, and review status before any browser-assisted posting.\n"),
    )?;
    write_if_missing(
        &channel_path.join("drafts/schema.md"),
        &format!("# {name} Draft Format\n\nEach draft should include:\n\n- Source post URL\n- Opportunity summary\n- Why this is relevant\n- Suggested reply draft\n- Risk notes\n- Status: `drafted`, `approved`, `posted`, or `skipped`\n\nApproved drafts can later be opened in Chrome, pasted into the reply field, and left for the user to send. A future version may submit after an explicit in-app confirmation.\n"),
    )?;
    write_if_missing(&channel_path.join("opportunities.jsonl"), "")?;
    write_if_missing(&channel_path.join("runs.jsonl"), "")?;
    write_if_missing(
        &channel_path.join("drafts/README.md"),
        &format!("# {name} Draft Queue\n\nDrafts created by daily {name} runs should live here until they are approved, edited, skipped, or posted through Chrome.\n"),
    )?;
    if !channel_path.join("status.json").exists() {
        write_channel_status(project_path, channel.id, ChannelStatus::empty())?;
    }
    Ok(())
}

fn read_channel_status(project_path: &Path, channel_id: &str) -> ChannelStatus {
    let path = project_path
        .join(".gtm-agent/channels")
        .join(channel_id)
        .join("status.json");
    if path.exists() {
        if let Ok(status) = read_json(&path) {
            return status;
        }
    }
    ChannelStatus::empty()
}

fn write_channel_status(
    project_path: &Path,
    channel_id: &str,
    status: ChannelStatus,
) -> AppResult<()> {
    write_json_pretty(
        &project_path
            .join(".gtm-agent/channels")
            .join(channel_id)
            .join("status.json"),
        &status,
    )
}

fn read_channel_account_label(project_path: &Path, channel_id: &str) -> Option<String> {
    let status = read_channel_status(project_path, channel_id);
    status.account_label.or(status.account_handle).or_else(|| {
        let profile = fs::read_to_string(
            project_path
                .join(".gtm-agent/channels")
                .join(channel_id)
                .join("profile.md"),
        )
        .unwrap_or_default();
        profile
            .lines()
            .find_map(|line| line.strip_prefix("- Account:").map(str::trim))
            .filter(|value| !value.is_empty() && !value.contains("Pending"))
            .map(str::to_string)
    })
}

fn project_settings_path(project_path: &Path) -> PathBuf {
    project_path.join(".gtm-agent/settings.json")
}

fn read_project_settings(project_path: &Path) -> ProjectSettings {
    read_json(&project_settings_path(project_path)).unwrap_or_default()
}

fn write_project_settings(project_path: &Path, settings: &ProjectSettings) -> AppResult<()> {
    write_json_pretty(&project_settings_path(project_path), settings)
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
    let Some(path) = manifests.last() else {
        return Ok(None);
    };

    let mut run: RunState = read_json(path)?;
    reconcile_initial_analysis_completion(project_path, path, &mut run)?;
    Ok(Some(run))
}

fn read_run_activity(log_path: &Path) -> AppResult<Vec<RunActivity>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(log_path)?;
    let mut activity = Vec::new();
    let mut message_deltas = HashMap::<String, String>::new();
    let mut message_order = Vec::<String>::new();
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
                &mut message_order,
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
                if !message_deltas.contains_key(message_id) {
                    message_order.push(message_id.to_string());
                }
                message_deltas
                    .entry(message_id.to_string())
                    .or_default()
                    .push_str(text);
            }
            Some("item/completed") => read_legacy_completed_activity(
                value.pointer("/params/item").unwrap_or(&Value::Null),
                &mut activity,
                &mut message_deltas,
                &mut message_order,
                &mut completed_messages,
            ),
            _ => {}
        }
    }

    for item_id in message_order {
        if completed_messages.contains(&item_id) {
            continue;
        }
        let Some(text) = message_deltas.get(&item_id) else {
            continue;
        };
        if should_show_agent_output(text) {
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
    message_order: &mut Vec<String>,
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
            if !message_deltas.contains_key(message_id) {
                message_order.push(message_id.to_string());
            }
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
    message_order: &mut Vec<String>,
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
                message_order.retain(|id| id != message_id);
                if should_show_agent_output(text) {
                    activity.push(RunActivity {
                        kind: "message".into(),
                        title: "Codex output".into(),
                        message: compact_text(text, 520),
                    });
                }
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

fn should_show_agent_output(text: &str) -> bool {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return false;
    }
    let lower = normalized.to_lowercase();
    let hidden_prefixes = [
        "warning: code mode is enabled",
        "warning: skill descriptions were shortened",
        "i’m using the local `gtm-source-doc-rewrite` workflow",
        "i'm using the local `gtm-source-doc-rewrite` workflow",
        "i’m using the local workflow",
        "i'm using the local workflow",
        "i found the recurring gtm rewrite workflow note",
        "i’ll first refresh the project-specific gtm workflow notes",
        "i'll first refresh the project-specific gtm workflow notes",
        "the workspace contract is narrow:",
    ];
    !hidden_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix))
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

fn channel_analysis_prompt(
    config: &ProjectConfig,
    channel: &ChannelDef,
    status: &ChannelStatus,
    selected_chrome_profile_id: Option<&str>,
    has_browser_mcp: bool,
) -> String {
    let channel_id = channel.id;
    let channel_name = channel.name;
    let home_url = channel.home_url;
    let account = status
        .account_label
        .as_deref()
        .or(status.account_handle.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| format!("the currently signed-in {channel_name} account"));
    let account_handle = status.account_handle.as_deref().unwrap_or("unknown");
    let account_avatar_url = status.account_avatar_url.as_deref().unwrap_or("unknown");
    let chrome_profile_id = selected_chrome_profile_id
        .or(status.chrome_profile_id.as_deref())
        .unwrap_or("unknown");
    let checked_at = status.checked_at.as_deref().unwrap_or("unknown");
    let check_method = status
        .check_method
        .as_deref()
        .unwrap_or("chrome_cookie_probe");
    let channel_inspection = match channel.id {
        "x" => "Inspect the visible profile at x.com: account name/handle, bio, recent posts, replies, and account-specific voice.",
        "reddit" => "Inspect the signed-in Reddit account: username, karma, active subreddits, recent posts, and comments (reddit.com/user/<username>).",
        "hacker-news" => "Inspect the signed-in Hacker News account: username, karma, about text, recent submissions, and comments (news.ycombinator.com/user?id=<username>).",
        _ => "Inspect the signed-in account: profile, recent posts, and replies.",
    };
    let browser_instructions = if has_browser_mcp {
        format!(
            "This session includes the `chrome-devtools` MCP server. Its browser tools (new_page, navigate_page, take_snapshot, take_screenshot, click, and related tools) control a dedicated Chrome window that is already signed in with the user's {channel_name} session from the selected Chrome profile. Use these tools as your primary method: open {home_url}, confirm the signed-in account, then inspect it. {channel_inspection} Do not sign out, change account settings, or navigate to unrelated sites. If the browser tools fail repeatedly, fail open: complete the channel setup from the app-provided account metadata and the global brand files, noting in `profile.md`, `voice.md`, and `examples.md` that live account inspection was unavailable."
        )
    } else {
        format!(
            "If your ACP provider exposes browser, Chrome, or computer-control tools, open {home_url} through the signed-in Chrome profile `{chrome_profile_id}` and use it to inspect the account. {channel_inspection} If browser tools are unavailable, fail open: complete the channel setup from the app-provided account metadata and the global brand files, explicitly noting in `profile.md`, `voice.md`, and `examples.md` that live account inspection was unavailable and should be refreshed after browser tools are configured."
        )
    };
    format!(
        r#"Configure {channel_name} outreach for {account} in this GTM workspace.

Website: {url}
Brand: {name}
Channel: {channel_name} ({home_url})
Account verified by app: {account}
Account handle from app: {account_handle}
Selected Chrome profile ID: {chrome_profile_id}
Avatar URL from app: {account_avatar_url}
Verification method: {check_method}
Verified at: {checked_at}

Goal:
Configure the {channel_name} channel for draft-first outreach. Do not post, like, follow, send, or publicly interact. Only inspect the logged-in account and write local channel context files.

The app has already verified the selected Chrome profile has an authenticated {channel_name} session. Do not spend the run checking whether login exists. {browser_instructions}

Then rewrite only these files:
- `.gtm-agent/channels/{channel_id}/profile.md`
- `.gtm-agent/channels/{channel_id}/rules.md`
- `.gtm-agent/channels/{channel_id}/examples.md`
- `.gtm-agent/channels/{channel_id}/voice.md`
- `.gtm-agent/channels/{channel_id}/status.json`

Use the global brand files as base context:
- `product-information.md`
- `marketing-strategy.md`
- `competitor-analysis.md`
- `brand-voice.md`

File requirements:

1. `profile.md`
Capture the signed-in account name/handle, visible bio, positioning, recurring topics, audience clues, and what kind of {channel_name} activity fits the account. Include a line formatted exactly as `- Account: @handle or display name` when known.

2. `voice.md`
Write account-specific voice guidance based on visible posts/replies: tone, pacing, sentence style, vocabulary, formatting habits, and how the global brand voice should adapt for {channel_name}.

3. `examples.md`
Capture only useful patterns, not private data. Include strong post/reply examples as short paraphrased patterns unless quoting is necessary. Add sections for `Strong examples`, `Reusable patterns`, and `Avoid`.

4. `rules.md`
Keep the draft-first operating rules: the agent may find opportunities and draft replies; public posting requires explicit user approval. Include guardrails for spam, unsupported claims, and when not to reply.

5. `status.json`
Write valid JSON with this exact shape:
{{
  "accountStatus": "authenticated",
  "loginStatus": "verified",
  "analysisStatus": "ready",
  "accountLabel": "{account}",
  "accountHandle": "{account_handle}",
  "accountAvatarUrl": "{account_avatar_url}",
  "chromeProfileId": "{chrome_profile_id}",
  "checkMethod": "{check_method}",
  "checkedAt": "{checked_at}",
  "updatedAt": "ISO-8601 timestamp"
}}
If any provided value is `unknown`, write JSON null for that field instead of the word unknown.

Write progress messages for the app user in a clean product-research tone. Keep them short and non-technical. Do not mention local sessions, agent internals, workspace mechanics, tools, implementation details, file operations, event logs, JSONL, or branch/git state.

Do not create outreach drafts in this run. Do not modify backend queue files such as `opportunities.jsonl`, `runs.jsonl`, or files in `drafts/` unless they do not exist and are needed as empty placeholders.
"#,
        url = config.website_url,
        name = config.name,
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

fn write_app_settings(settings: &AppSettings) -> AppResult<()> {
    write_json_pretty(&app_settings_path()?, settings)
}

fn save_last_project_path(project_path: &Path) -> AppResult<()> {
    let mut settings = read_app_settings()?;
    settings.last_project_path = Some(project_path.to_string_lossy().to_string());
    write_app_settings(&settings)
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
        let provider_ids: Vec<_> = providers
            .iter()
            .map(|provider| provider.id.as_str())
            .collect();
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
        assert_eq!(
            provider_ids,
            vec!["codex", "claude", "cursor", "devin", "gemini", "copilot", "custom"]
        );
    }

    #[test]
    fn command_search_path_includes_common_macos_locations() {
        let paths = command_search_paths();
        assert!(paths
            .iter()
            .any(|path| path == Path::new("/opt/homebrew/bin")));
        assert!(paths.iter().any(|path| path == Path::new("/usr/local/bin")));
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
        let request = acp_session_new_request(Path::new("/tmp/project"), &[]);
        assert_eq!(request["method"], "session/new");
        assert_eq!(request["params"]["cwd"], "/tmp/project");
        assert_eq!(request["params"]["mcpServers"], serde_json::json!([]));

        let server = browser_mcp_server(Path::new("/tmp/gtm-agent-browser-test"));
        let request = acp_session_new_request(Path::new("/tmp/project"), &[server]);
        assert_eq!(
            request["params"]["mcpServers"][0]["name"],
            "chrome-devtools"
        );
        assert_eq!(request["params"]["mcpServers"][0]["command"], "npx");
    }

    #[test]
    fn builds_browser_mcp_server_config() {
        let server = browser_mcp_server(Path::new("/tmp/gtm-agent-browser-abc"));
        let args = server["args"].as_array().unwrap();
        assert!(args.iter().any(|arg| arg == "chrome-devtools-mcp@latest"));
        assert!(args.iter().any(|arg| arg == "/tmp/gtm-agent-browser-abc"));
        assert!(args
            .iter()
            .any(|arg| arg == "--ignore-default-chrome-arg=--use-mock-keychain"));
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
    fn approves_acp_permission_requests() {
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
        let response = permission_response(serde_json::json!(8), &request);
        assert_eq!(response["result"]["outcome"]["outcome"], "selected");
        assert_eq!(response["result"]["outcome"]["optionId"], "allow-once");

        let no_allow_option = serde_json::json!({
            "id": 9,
            "method": "session/request_permission",
            "params": {
                "options": [
                    { "optionId": "reject-once", "name": "Reject", "kind": "reject_once" }
                ]
            },
        });
        let response = permission_response(serde_json::json!(9), &no_allow_option);
        assert_eq!(response["result"]["outcome"]["outcome"], "cancelled");
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
                            "messageId": "agent",
                            "content": {
                                "type": "text",
                                "text": "Warning: Code Mode is enabled in configuration, but model `gpt-5.5` does not advertise Code Mode support."
                            }
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
                            "messageId": "msg_internal",
                            "content": {
                                "type": "text",
                                "text": "I’m using the local `gtm-source-doc-rewrite` workflow because this is the exact four-document GTM source rewrite pattern."
                            }
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
                            "messageId": "msg_first",
                            "content": {
                                "type": "text",
                                "text": "Initial search shows a crowded category."
                            }
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
                            "messageId": "msg_second",
                            "content": {
                                "type": "text",
                                "text": "TapTalk's own pages confirm the core boundary."
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
        assert_eq!(activity.len(), 3);
        assert_eq!(activity[0].message, "Searching public sources");
        assert_eq!(activity[1].title, "Agent output");
        assert_eq!(
            activity[1].message,
            "Initial search shows a crowded category."
        );
        assert_eq!(
            activity[2].message,
            "TapTalk's own pages confirm the core boundary."
        );

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
    fn detects_completed_initial_analysis_artifacts() {
        let project_path =
            std::env::temp_dir().join(format!("gtm-agent-test-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(project_path.join(".gtm-agent")).unwrap();
        for (_, file_name, title) in DOCS {
            fs::write(
                project_path.join(file_name),
                format!("# {title}\n\nUseful body content.\n"),
            )
            .unwrap();
        }
        fs::write(
            project_path.join(".gtm-agent/events.jsonl"),
            serde_json::json!({
                "eventType": "task.completed",
                "summary": "GTM source documents completed",
                "payload": {
                    "documents": [
                        "product-information.md",
                        "marketing-strategy.md",
                        "competitor-analysis.md",
                        "brand-voice.md"
                    ]
                },
                "createdAt": "2026-07-06T17:36:34Z"
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(
            initial_analysis_completion_time(&project_path, "2026-07-06T17:32:22Z")
                .unwrap()
                .as_deref(),
            Some("2026-07-06T17:36:34Z")
        );

        fs::remove_dir_all(project_path).unwrap();
    }

    #[test]
    fn keeps_initial_analysis_running_until_docs_are_ready() {
        let project_path =
            std::env::temp_dir().join(format!("gtm-agent-test-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(project_path.join(".gtm-agent")).unwrap();
        for (_, file_name, title) in DOCS {
            fs::write(project_path.join(file_name), format!("# {title}\n\n")).unwrap();
        }
        fs::write(
            project_path.join(".gtm-agent/events.jsonl"),
            serde_json::json!({
                "eventType": "task.completed",
                "summary": "GTM source documents completed",
                "payload": {},
                "createdAt": "2026-07-06T17:36:34Z"
            })
            .to_string(),
        )
        .unwrap();

        assert!(
            initial_analysis_completion_time(&project_path, "2026-07-06T17:32:22Z")
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(project_path).unwrap();
    }

    #[test]
    fn ignores_completion_events_from_previous_runs() {
        let project_path =
            std::env::temp_dir().join(format!("gtm-agent-test-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(project_path.join(".gtm-agent")).unwrap();
        for (_, file_name, title) in DOCS {
            fs::write(
                project_path.join(file_name),
                format!("# {title}\n\nUseful body content.\n"),
            )
            .unwrap();
        }
        fs::write(
            project_path.join(".gtm-agent/events.jsonl"),
            serde_json::json!({
                "eventType": "task.completed",
                "summary": "GTM source documents completed",
                "payload": {
                    "documents": [
                        "product-information.md",
                        "marketing-strategy.md",
                        "competitor-analysis.md",
                        "brand-voice.md"
                    ]
                },
                "createdAt": "2026-07-06T17:36:34Z"
            })
            .to_string(),
        )
        .unwrap();

        assert!(
            initial_analysis_completion_time(&project_path, "2026-07-06T17:40:00Z")
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(project_path).unwrap();
    }

    #[test]
    fn latest_run_reconciles_completed_initial_analysis_manifest() {
        let project_path =
            std::env::temp_dir().join(format!("gtm-agent-test-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(project_path.join(".gtm-agent/runs")).unwrap();
        for (_, file_name, title) in DOCS {
            fs::write(
                project_path.join(file_name),
                format!("# {title}\n\nUseful body content.\n"),
            )
            .unwrap();
        }
        fs::write(
            project_path.join(".gtm-agent/events.jsonl"),
            serde_json::json!({
                "eventType": "task.completed",
                "summary": "GTM source documents completed",
                "payload": {
                    "documents": [
                        "product-information.md",
                        "marketing-strategy.md",
                        "competitor-analysis.md",
                        "brand-voice.md"
                    ]
                },
                "createdAt": "2026-07-06T17:36:34Z"
            })
            .to_string(),
        )
        .unwrap();
        let run = RunState {
            id: "run_test".into(),
            kind: "initial_analysis".into(),
            status: RunStatus::Running,
            channels: Vec::new(),
            provider_id: Some("codex".into()),
            provider_title: Some("Codex".into()),
            external_session_id: Some("session_test".into()),
            codex_thread_id: None,
            started_at: "2026-07-06T17:32:22Z".into(),
            completed_at: None,
            log_path: project_path
                .join(".gtm-agent/runs/run_test.jsonl")
                .to_string_lossy()
                .to_string(),
            error: None,
        };
        let run_path = project_path.join(".gtm-agent/runs/run_test.json");
        write_json_pretty(&run_path, &run).unwrap();

        let reconciled = latest_run(&project_path).unwrap().unwrap();
        assert_eq!(reconciled.status, RunStatus::Completed);
        assert_eq!(
            reconciled.completed_at.as_deref(),
            Some("2026-07-06T17:36:34Z")
        );

        let persisted: RunState = read_json(&run_path).unwrap();
        assert_eq!(persisted.status, RunStatus::Completed);

        fs::remove_dir_all(project_path).unwrap();
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
    fn writes_channel_setup_files_for_all_channels() {
        let project_path =
            std::env::temp_dir().join(format!("gtm-agent-x-test-{}", Uuid::new_v4().simple()));

        for channel in &CHANNELS {
            write_channel_setup(&project_path, channel).unwrap();
        }

        for channel in &CHANNELS {
            let channel_path = project_path.join(".gtm-agent/channels").join(channel.id);
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
        }
        let setups = read_channel_setups(&project_path);
        assert_eq!(setups.len(), CHANNELS.len());
        assert_eq!(setups[0].id, "x");
        assert_eq!(setups[1].id, "reddit");
        assert_eq!(setups[2].id, "hacker-news");
        for setup in &setups {
            assert_eq!(setup.status, ChannelSetupStatus::NotConfigured);
            assert_eq!(setup.account_status, ChannelAccountStatus::NotConfigured);
            assert_eq!(setup.login_status, ChannelLoginStatus::Unknown);
            assert_eq!(setup.analysis_status, ChannelAnalysisStatus::NotStarted);
            assert!(setup.files.contains(&"voice.md".into()));
            assert!(!setup.files.contains(&"drafts/schema.md".into()));
        }

        fs::remove_dir_all(project_path).unwrap();
    }

    #[test]
    fn resolves_channel_definitions() {
        assert_eq!(channel_def("x").unwrap().name, "X");
        assert_eq!(channel_def("reddit").unwrap().name, "Reddit");
        assert_eq!(channel_def("hacker-news").unwrap().name, "Hacker News");
        assert!(channel_def("linkedin").is_err());
    }

    #[test]
    fn builds_channel_cookie_queries() {
        let query = channel_cookie_count_query(channel_def("x").unwrap());
        assert!(query.contains("'.x.com'"));
        assert!(query.contains("'auth_token'"));
        let query = channel_cookie_count_query(channel_def("hacker-news").unwrap());
        assert!(query.contains("'news.ycombinator.com'"));
        assert!(query.contains("'user'"));
    }

    #[test]
    fn channel_prompt_mentions_chrome_extension_and_files() {
        let config = ProjectConfig {
            id: "project_test".into(),
            name: "Example".into(),
            website_url: "https://example.com".into(),
            path: "/tmp/example".into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        let prompt = channel_analysis_prompt(
            &config,
            channel_def("reddit").unwrap(),
            &ChannelStatus::empty(),
            Some("Profile 3"),
            true,
        );
        assert!(prompt.contains("`chrome-devtools` MCP server"));
        assert!(prompt.contains(".gtm-agent/channels/reddit/profile.md"));
        assert!(prompt.contains("https://www.reddit.com/"));

        let fallback_prompt = channel_analysis_prompt(
            &config,
            channel_def("reddit").unwrap(),
            &ChannelStatus::empty(),
            Some("Profile 3"),
            false,
        );
        assert!(fallback_prompt.contains("Chrome profile `Profile 3`"));
        assert!(fallback_prompt.contains("fail open"));
    }

    #[test]
    fn round_trips_project_settings() {
        let project_path = std::env::temp_dir().join(format!(
            "gtm-agent-settings-test-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&project_path).unwrap();

        assert_eq!(read_project_settings(&project_path).chrome_profile_id, None);
        write_project_settings(
            &project_path,
            &ProjectSettings {
                chrome_profile_id: Some("Profile 1".into()),
                selected_channels: vec!["x".into(), "hacker-news".into()],
            },
        )
        .unwrap();
        let settings = read_project_settings(&project_path);
        assert_eq!(settings.chrome_profile_id.as_deref(), Some("Profile 1"));
        assert_eq!(settings.selected_channels, vec!["x", "hacker-news"]);

        fs::remove_dir_all(project_path).unwrap();
    }
}
