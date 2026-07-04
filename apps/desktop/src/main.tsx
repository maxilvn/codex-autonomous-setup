import React from "react";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { api } from "./lib/api";
import type { CodexDetection, ContextDoc, ProjectState } from "./lib/types";
import "./styles.css";

const logoBlack = new URL("./assets/brand/two-wedge-logo-black-transparent.png", import.meta.url).href;

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

  return (
    <main className="shell">
      {!project ? (
        <section className="topbar">
          <div className="brand">
            <BrandMark />
            <span>GTM Agent</span>
          </div>
          <CodexBadge codex={codex} />
        </section>
      ) : null}

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
        <ProjectView project={project} />
      )}
    </main>
  );
}

function BrandMark() {
  return (
    <span className="brand-mark" aria-hidden="true">
      <img src={logoBlack} alt="" />
    </span>
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

function ProjectView({ project }: { project: ProjectState }) {
  const [selectedDoc, setSelectedDoc] = React.useState<ContextDoc | null>(null);
  const run = project.latestRun;
  const activity = project.runActivity.length
    ? project.runActivity
    : [{ kind: "idle", title: "Waiting", message: "Codex text output will appear here." }];
  const isRunning = run?.status === "running";
  const host = displayHost(project.config.websiteUrl);
  const productDescription = extractProductDescription(project.docs);
  const competitors = extractCompetitors(project.docs, host);

  return (
    <section className="workspace">
      <div className="analysis-grid">
        <aside className="panel documents-card">
          <div className="company-lockup">
            <UrlIcon websiteUrl={project.config.websiteUrl} />
            <div>
              <strong>{project.config.name}</strong>
            </div>
          </div>

          <p className="product-description">{productDescription}</p>

          <div className="documents-section">
            <p className="eyebrow">Documents</p>
          </div>
          <div className="document-list">
            {project.docs.map((doc) => (
              <button
                className="document-row"
                key={doc.key}
                type="button"
                onClick={() => setSelectedDoc(doc)}
              >
                <span className="document-icon" aria-hidden="true">
                  <svg viewBox="0 0 16 16" focusable="false">
                    <path d="M4 1.75h5.2L12.75 5.3v8.95H4z" />
                    <path d="M9 1.9v3.6h3.55M6 8h4M6 10.5h4" />
                  </svg>
                </span>
                <span>{doc.title}</span>
                <span className="document-chevron" aria-hidden="true">›</span>
              </button>
            ))}
          </div>

          <div className="competitors-section">
            <p className="eyebrow">Competitors</p>
            {competitors.length ? (
              <div className="competitor-list">
                {competitors.map((competitor) => (
                  <button
                    className="competitor-row"
                    key={competitor.url}
                    type="button"
                    onClick={() => void api.openExternalUrl(competitor.url)}
                  >
                    <UrlIcon websiteUrl={competitor.url} />
                    <span>{competitor.host}</span>
                  </button>
                ))}
              </div>
            ) : (
              <p className="empty-note">Verified competitor links will appear here after analysis.</p>
            )}
          </div>
        </aside>

        <section className="panel activity-card">
          <div className="activity-head">
            <h2>Brand Analysis</h2>
          </div>
          <div className="activity-list">
            {activity.map((item, index) => (
              <article className="activity-item" key={`${item.title}-${index}`}>
                <p>{item.message}</p>
              </article>
            ))}
          </div>
          {isRunning ? <div className="analyzing-shimmer">Analyzing...</div> : null}
          {run?.error ? <p className="run-error">{run.error}</p> : null}
        </section>
      </div>

      {selectedDoc ? (
        <div className="doc-modal-backdrop" onClick={() => setSelectedDoc(null)}>
          <article className="doc-modal" onClick={(event) => event.stopPropagation()}>
            <button
              className="modal-close"
              type="button"
              aria-label="Close"
              onClick={() => setSelectedDoc(null)}
            >
              ×
            </button>
            <p className="label">{selectedDoc.fileName}</p>
            <h2>{selectedDoc.title}</h2>
            <RenderedDoc content={selectedDoc.content} full />
          </article>
        </div>
      ) : null}
    </section>
  );
}

function RenderedDoc({ content, full = false }: { content: string; full?: boolean }) {
  const blocks = markdownBlocks(content);

  return (
    <div className={full ? "doc-content full" : "doc-content"}>
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          return <h4 key={index}>{block.text}</h4>;
        }
        if (block.type === "list") {
          return (
            <ul key={index}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{item}</li>
              ))}
            </ul>
          );
        }
        if (block.type === "ordered-list") {
          return (
            <ol key={index}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{item}</li>
              ))}
            </ol>
          );
        }
        return <p key={index}>{block.text}</p>;
      })}
    </div>
  );
}

type MarkdownBlock =
  | { type: "heading"; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; items: string[] }
  | { type: "ordered-list"; items: string[] };

type Competitor = {
  host: string;
  url: string;
};

function docByKey(docs: ContextDoc[], key: string) {
  const fileKey = key.replaceAll("_", "-");
  return docs.find((doc) => doc.key === key || doc.fileName.includes(fileKey));
}

function extractProductDescription(docs: ContextDoc[]) {
  const doc = docByKey(docs, "product_information");
  if (!doc) return "Product description will appear here after analysis.";

  const paragraph = markdownBlocks(doc.content).find((block) => {
    if (block.type !== "paragraph") return false;
    const text = block.text.toLowerCase();
    const urlCount = (block.text.match(/https?:\/\/|\b(?:[a-z0-9-]+\.)+[a-z]{2,}\b/gi) ?? []).length;
    return (
      block.text.length > 60
      && urlCount < 2
      && !text.includes("status:")
      && !text.includes("source url")
      && !text.includes("urls checked")
      && !text.includes("sources checked")
    );
  });

  return paragraph?.type === "paragraph"
    ? stripMarkdownLinks(paragraph.text)
    : "Product description will appear here after analysis.";
}

function extractCompetitors(docs: ContextDoc[], ownHost: string) {
  const doc = docByKey(docs, "competitor_analysis");
  if (!doc) return [];

  const competitors = new Map<string, Competitor>();
  const own = ownHost.toLowerCase();
  const markdownLink = /\[([^\]]+)]\((https?:\/\/[^)\s]+)\)/g;
  const plainUrl = /https?:\/\/[^\s),]+/g;
  const heading = /^###\s+(.+)$/gm;
  const examples = /examples? found:\s*([^\n]+)/gi;

  function add(value: string) {
    const url = normalizeDisplayUrl(value);
    if (!url) return;
    const host = displayHost(url);
    const key = host.toLowerCase();
    if (!key || key.endsWith(".md") || key === own || key.endsWith(`.${own}`) || competitors.has(key)) return;
    competitors.set(key, { host, url });
  }

  function addName(value: string) {
    const name = cleanCompetitorName(value);
    if (!name || isGenericCompetitorCategory(name)) return;
    const knownUrl = knownCompetitorUrl(name);
    if (knownUrl) add(knownUrl);
  }

  for (const match of doc.content.matchAll(markdownLink)) add(match[2]);
  for (const match of doc.content.matchAll(plainUrl)) add(match[0]);
  for (const match of doc.content.matchAll(heading)) addName(match[1]);
  for (const match of doc.content.matchAll(examples)) {
    for (const item of match[1].split(/,|\band\b/gi)) {
      addName(item);
    }
  }

  return Array.from(competitors.values()).slice(0, 6);
}

function cleanCompetitorName(value: string) {
  return cleanInline(value)
    .replace(/^[\s:;/,-]+|[\s:;/,-]+$/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function isGenericCompetitorCategory(value: string) {
  const lower = value.toLowerCase();
  return [
    "services",
    "advisors",
    "lawyers",
    "spreadsheets",
    "manual folders",
    "cloud storage",
    "notes apps",
    "property management",
    "landlord tools",
    "alternatives",
  ].some((term) => lower.includes(term));
}

function knownCompetitorUrl(value: string) {
  const key = value.toLowerCase();
  const known: Record<string, string> = {
    "wispr flow": "https://wisprflow.ai",
    superwhisper: "https://superwhisper.com",
    "apple dictation and apple intelligence writing tools": "https://apple.com",
    "aqua voice": "https://app.aquavoice.com",
  };
  return known[key] ?? null;
}

function markdownBlocks(content: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  const lines = content.split(/\r?\n/);
  let paragraph: string[] = [];
  let list: string[] = [];
  let orderedList: string[] = [];

  function flushParagraph() {
    if (!paragraph.length) return;
    blocks.push({ type: "paragraph", text: cleanInline(paragraph.join(" ")) });
    paragraph = [];
  }

  function flushList() {
    if (list.length) {
      blocks.push({ type: "list", items: list.map(cleanInline) });
      list = [];
    }
    if (orderedList.length) {
      blocks.push({ type: "ordered-list", items: orderedList.map(cleanInline) });
      orderedList = [];
    }
  }

  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) {
      flushParagraph();
      flushList();
      continue;
    }

    const heading = line.match(/^#{1,6}\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      blocks.push({ type: "heading", text: cleanInline(heading[1]) });
      continue;
    }

    const bullet = line.match(/^[-*]\s+(.+)$/);
    if (bullet) {
      flushParagraph();
      orderedList = [];
      list.push(bullet[1]);
      continue;
    }

    const numbered = line.match(/^\d+\.\s+(.+)$/);
    if (numbered) {
      flushParagraph();
      list = [];
      orderedList.push(numbered[1]);
      continue;
    }

    flushList();
    paragraph.push(line);
  }

  flushParagraph();
  flushList();
  return blocks.length ? blocks : [{ type: "paragraph", text: "No content yet." }];
}

function cleanInline(value: string) {
  return value
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/_([^_]+)_/g, "$1")
    .trim();
}

function stripMarkdownLinks(value: string) {
  return value
    .replace(/\[([^\]]+)]\((https?:\/\/[^)]+)\)/g, "$1")
    .replace(/https?:\/\/\S+/g, "")
    .replace(/\s{2,}/g, " ")
    .trim();
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

function normalizeDisplayUrl(value: string) {
  const trimmed = value.trim().replace(/[.,;:]+$/, "");
  if (!trimmed || !trimmed.includes(".")) return null;
  try {
    return new URL(
      trimmed.startsWith("http://") || trimmed.startsWith("https://")
        ? trimmed
        : `https://${trimmed}`,
    ).toString();
  } catch {
    return null;
  }
}

function displayHost(value: string) {
  try {
    return new URL(value).host.replace(/^www\./, "");
  } catch {
    return value.replace(/^https?:\/\//, "").replace(/^www\./, "").split("/")[0];
  }
}

createRoot(document.getElementById("root")!).render(<App />);
