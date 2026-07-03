use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            detect_codex,
            default_project_path,
            create_project,
            load_project,
            run_initial_analysis,
            open_project_in_codex
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
    Ok(ProjectState {
        config,
        codex: detect_codex_impl(),
        docs: read_docs(&path)?,
        latest_run: latest_run(&path)?,
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
    let thread_id = Arc::new(Mutex::new(None::<String>));

    let mut child = Command::new(binary)
        .args(codex_exec_args(project, &prompt))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AppError::Codex(format!("failed to launch codex: {err}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Codex("missing codex stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Codex("missing codex stderr".into()))?;

    let stdout_log = Arc::clone(&log_file);
    let stdout_thread_id = Arc::clone(&thread_id);
    let stdout_handle = thread::spawn(move || -> AppResult<()> {
        for line in BufReader::new(stdout).lines() {
            let line = line?;
            if let Some(id) = codex_thread_id_from_line(&line) {
                *stdout_thread_id
                    .lock()
                    .map_err(|_| AppError::Codex("thread id lock poisoned".into()))? = Some(id);
            }
            writeln!(
                stdout_log
                    .lock()
                    .map_err(|_| AppError::Codex("log lock poisoned".into()))?,
                "{line}"
            )?;
        }
        Ok(())
    });

    let stderr_log = Arc::clone(&log_file);
    let stderr_handle = thread::spawn(move || -> AppResult<()> {
        for line in BufReader::new(stderr).lines() {
            writeln!(
                stderr_log
                    .lock()
                    .map_err(|_| AppError::Codex("log lock poisoned".into()))?,
                "{}",
                serde_json::json!({ "type": "stderr", "message": line? })
            )?;
        }
        Ok(())
    });

    let status = child.wait()?;
    join_reader(stdout_handle)?;
    join_reader(stderr_handle)?;

    let mut finished = initial.clone();
    finished.completed_at = Some(Utc::now().to_rfc3339());
    finished.codex_thread_id = thread_id
        .lock()
        .map_err(|_| AppError::Codex("thread id lock poisoned".into()))?
        .clone();

    if status.success() {
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
        finished.error = Some(format!("codex exited with {status}"));
        write_json_pretty(&run_manifest_path(project, &initial.id), &finished)?;
        Err(AppError::Codex(format!("codex exited with {status}")))
    }
}

fn join_reader(handle: thread::JoinHandle<AppResult<()>>) -> AppResult<()> {
    handle
        .join()
        .map_err(|_| AppError::Codex("codex output reader panicked".into()))?
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

fn codex_exec_args(project: &Path, prompt: &str) -> Vec<String> {
    vec![
        "-C".into(),
        project.to_string_lossy().to_string(),
        "exec".into(),
        "--json".into(),
        "--skip-git-repo-check".into(),
        "--ignore-user-config".into(),
        prompt.into(),
    ]
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

fn codex_thread_id_from_line(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) == Some("thread.started") {
        return value
            .get("thread_id")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    None
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
    fn parses_codex_thread_started_event() {
        let line =
            r#"{"type":"thread.started","thread_id":"019f2957-b734-7211-9bbc-f74f5980d6f3"}"#;
        assert_eq!(
            codex_thread_id_from_line(line).as_deref(),
            Some("019f2957-b734-7211-9bbc-f74f5980d6f3")
        );
    }

    #[test]
    fn ignores_non_json_and_other_events() {
        assert!(codex_thread_id_from_line("warning").is_none());
        assert!(codex_thread_id_from_line(r#"{"type":"turn.started"}"#).is_none());
    }

    #[test]
    fn builds_codex_exec_args_with_global_cd_before_exec() {
        let args = codex_exec_args(Path::new("/tmp/project"), "analyze");
        assert_eq!(
            args,
            vec![
                "-C",
                "/tmp/project",
                "exec",
                "--json",
                "--skip-git-repo-check",
                "--ignore-user-config",
                "analyze"
            ]
        );
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
