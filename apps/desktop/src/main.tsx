import React from "react";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { api } from "./lib/api";
import type { CodexDetection, ProjectState } from "./lib/types";
import "./styles.css";

function App() {
  const [codex, setCodex] = React.useState<CodexDetection | null>(null);
  const [project, setProject] = React.useState<ProjectState | null>(null);
  const [websiteUrl, setWebsiteUrl] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const refreshProject = React.useCallback(async () => {
    if (!project) return;
    setProject(await api.loadProject(project.config.path));
  }, [project]);

  React.useEffect(() => {
    api.detectCodex().then(setCodex).catch((err) => {
      setCodex({ available: false, error: String(err) });
    });
  }, []);

  React.useEffect(() => {
    if (!project) return;
    const timer = window.setInterval(() => {
      void refreshProject().catch((err) => setError(String(err)));
    }, 2500);
    let unlisten: (() => void) | undefined;
    listen("project-updated", () => {
      void refreshProject().catch((err) => setError(String(err)));
    }).then((dispose) => {
      unlisten = dispose;
    }).catch(() => undefined);
    return () => {
      window.clearInterval(timer);
      unlisten?.();
    };
  }, [project, refreshProject]);

  async function createProject() {
    setBusy(true);
    setError(null);
    try {
      const next = await api.createProject(websiteUrl);
      setProject(next);
      await api.runInitialAnalysis(next.config.path);
      setProject(await api.loadProject(next.config.path));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function rerunAnalysis() {
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      await api.runInitialAnalysis(project.config.path);
      setProject(await api.loadProject(project.config.path));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="shell">
      <section className="topbar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true" />
          <span>GTM Agent</span>
        </div>
        <CodexBadge codex={project?.codex ?? codex} />
      </section>

      {error ? <div className="error">{error}</div> : null}

      {!project ? (
        <section className="onboarding">
          <div className="onboarding-copy">
            <p className="eyebrow">Codex workspace</p>
            <h1>Website analysis</h1>
          </div>
          <div className="url-bar">
            <UrlIcon websiteUrl={websiteUrl} />
            <input
              autoFocus
              value={websiteUrl}
              onChange={(event) => setWebsiteUrl(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !busy) void createProject();
              }}
              placeholder="website.com"
            />
            <button onClick={createProject} disabled={busy || !websiteUrl.trim()}>
              {busy ? "Creating..." : "Analyze"}
            </button>
          </div>
        </section>
      ) : (
        <ProjectView
          project={project}
          busy={busy}
          onRun={rerunAnalysis}
          onOpen={() => api.openProjectInCodex(project.config.path)}
        />
      )}
    </main>
  );
}

function UrlIcon({ websiteUrl }: { websiteUrl: string }) {
  const [failedFor, setFailedFor] = React.useState<string | null>(null);
  const faviconUrl = faviconForUrl(websiteUrl);
  const showFavicon = faviconUrl && failedFor !== faviconUrl;

  return (
    <span className="url-icon" aria-hidden="true">
      {showFavicon ? (
        <img
          key={faviconUrl}
          src={faviconUrl}
          alt=""
          onError={() => setFailedFor(faviconUrl)}
        />
      ) : (
        <svg viewBox="0 0 16 16" focusable="false">
          <circle cx="8" cy="8" r="6" />
          <path d="M2.5 8h11M8 2c1.7 1.6 2.5 3.6 2.5 6s-.8 4.4-2.5 6M8 2C6.3 3.6 5.5 5.6 5.5 8s.8 4.4 2.5 6" />
        </svg>
      )}
    </span>
  );
}

function CodexBadge({ codex }: { codex: CodexDetection | null | undefined }) {
  if (!codex) return <div className="badge neutral">Checking Codex</div>;
  return (
    <div className={codex.available ? "badge success" : "badge danger"}>
      <strong>{codex.available ? "Codex ready" : "Codex missing"}</strong>
      <span>{codex.version || codex.error || "No version found"}</span>
    </div>
  );
}

function ProjectView({
  project,
  busy,
  onRun,
  onOpen,
}: {
  project: ProjectState;
  busy: boolean;
  onRun: () => Promise<void>;
  onOpen: () => Promise<void>;
}) {
  const run = project.latestRun;
  const activity = project.runActivity.length
    ? project.runActivity
    : [{ kind: "idle", title: "Waiting", message: "Codex activity will appear here." }];

  return (
    <section className="workspace">
      <div className="workspace-header">
        <div>
          <p className="eyebrow">Workspace</p>
          <h2>{project.config.name}</h2>
          <code>{project.config.path}</code>
        </div>
        <div className="actions">
          <button className="secondary" onClick={onOpen}>Open in Codex</button>
          <button onClick={onRun} disabled={busy || run?.status === "running"}>
            {run?.status === "running" ? "Running..." : "Run analysis"}
          </button>
        </div>
      </div>

      <section className="panel activity-card">
        <div className="activity-head">
          <div>
            <p className="eyebrow">Codex session</p>
            <h3>{run?.status === "running" ? "Running analysis" : "Latest activity"}</h3>
          </div>
          <span className={`status-pill ${run?.status ?? "idle"}`}>
            {run?.status ?? "idle"}
          </span>
        </div>
        <div className="activity-list">
          {activity.map((item, index) => (
            <article className="activity-item" key={`${item.title}-${index}`}>
              <span className={`activity-dot ${item.kind}`} />
              <div>
                <strong>{item.title}</strong>
                <p>{item.message}</p>
              </div>
            </article>
          ))}
        </div>
        {run?.codexThreadId ? (
          <div className="activity-meta">
            <span>Thread</span>
            <code>{run.codexThreadId}</code>
          </div>
        ) : null}
        {run?.error ? <p className="run-error">{run.error}</p> : null}
      </section>

      <section className="docs-section">
        <div className="section-head">
          <p className="eyebrow">Generated files</p>
          <span>{project.docs.length} markdown docs</span>
        </div>
        <section className="docs">
          {project.docs.map((doc) => (
            <article className="panel doc" key={doc.key}>
              <p className="label">{doc.fileName}</p>
              <h3>{doc.title}</h3>
              <pre>{doc.content}</pre>
            </article>
          ))}
        </section>
      </section>
    </section>
  );
}

function faviconForUrl(value: string) {
  const trimmed = value.trim();
  if (!trimmed || !trimmed.includes(".")) return null;
  try {
    const url = new URL(
      trimmed.startsWith("http://") || trimmed.startsWith("https://")
        ? trimmed
        : `https://${trimmed}`,
    );
    return `https://www.google.com/s2/favicons?sz=64&domain_url=${encodeURIComponent(url.origin)}`;
  } catch {
    return null;
  }
}

createRoot(document.getElementById("root")!).render(<App />);
