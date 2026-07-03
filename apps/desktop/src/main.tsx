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
      <section className="header">
        <div>
          <h1>GTM Agent</h1>
          <p>Local Codex workspaces for brand and GTM analysis.</p>
        </div>
        <CodexBadge codex={project?.codex ?? codex} />
      </section>

      {error ? <div className="error">{error}</div> : null}

      {!project ? (
        <section className="panel onboarding">
          <div>
            <h2>Create a Codex GTM workspace</h2>
            <p>
              Enter a URL. GTM Agent creates a local folder, starts a persisted
              Codex session, and writes the initial strategy documents there.
            </p>
          </div>
          <div className="form-row">
            <input
              autoFocus
              value={websiteUrl}
              onChange={(event) => setWebsiteUrl(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !busy) void createProject();
              }}
              placeholder="example.com"
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
  return (
    <section className="workspace">
      <div className="panel project-summary">
        <div>
          <p className="label">Workspace</p>
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

      <div className="grid">
        <section className="panel run-panel">
          <p className="label">Latest Codex run</p>
          {run ? (
            <dl>
              <dt>Status</dt>
              <dd>{run.status}</dd>
              <dt>Thread</dt>
              <dd>{run.codexThreadId || "Waiting for thread id"}</dd>
              <dt>Log</dt>
              <dd><code>{run.logPath}</code></dd>
              {run.error ? (
                <>
                  <dt>Error</dt>
                  <dd>{run.error}</dd>
                </>
              ) : null}
            </dl>
          ) : (
            <p className="muted">No runs yet.</p>
          )}
        </section>

        <section className="docs">
          {project.docs.map((doc) => (
            <article className="panel doc" key={doc.key}>
              <p className="label">{doc.fileName}</p>
              <h3>{doc.title}</h3>
              <pre>{doc.content}</pre>
            </article>
          ))}
        </section>
      </div>
    </section>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
