import React from "react";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { api } from "./lib/api";
import type {
  AgentProviderStatus,
  ChannelSetup,
  ContextDoc,
  ProjectState,
} from "./lib/types";
import "./styles.css";

const logoBlack = new URL(
  "./assets/brand/two-wedge-logo-black-transparent.png",
  import.meta.url,
).href;

function App() {
  const [agentProvider, setAgentProvider] = React.useState<AgentProviderStatus | null>(null);
  const [project, setProject] = React.useState<ProjectState | null>(null);
  const [websiteUrl, setWebsiteUrl] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [restoring, setRestoring] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const refreshProject = React.useCallback(async () => {
    if (!project) return;
    setProject(await api.loadProject(project.config.path));
  }, [project]);

  React.useEffect(() => {
    api.detectAgentProvider().then(setAgentProvider).catch((err) => {
      setAgentProvider({
        id: "agent",
        title: "Agent",
        command: "",
        args: [],
        available: false,
        error: String(err),
      });
    });
    let cancelled = false;
    api
      .loadLastProject()
      .then((lastProject) => {
        if (!cancelled && lastProject) setProject(lastProject);
      })
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setRestoring(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  React.useEffect(() => {
    if (!project) return;
    const timer = window.setInterval(() => {
      void refreshProject().catch((err) => setError(String(err)));
    }, 2500);
    let unlisten: (() => void) | undefined;
    listen("project-updated", () => {
      void refreshProject().catch((err) => setError(String(err)));
    })
      .then((dispose) => {
        unlisten = dispose;
      })
      .catch(() => undefined);
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
      if (shouldRunInitialAnalysis(next)) {
        await api.runInitialAnalysis(next.config.path);
        setProject(await api.loadProject(next.config.path));
      }
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
          <AgentBadge provider={agentProvider} />
        </section>
      ) : null}

      {error ? <div className="error">{error}</div> : null}

      {!project && !restoring ? (
        <section className="onboarding">
          <div className="onboarding-copy">
            <p className="eyebrow">Brand workspace</p>
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
            <button
              onClick={createProject}
              disabled={busy || !websiteUrl.trim()}
            >
              {busy ? "Creating..." : "Analyze"}
            </button>
          </div>
        </section>
      ) : project ? (
        <ProjectView project={project} onProjectUpdate={setProject} />
      ) : null}
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
  const [candidateIndex, setCandidateIndex] = React.useState(0);
  const faviconUrls = faviconUrlsForUrl(websiteUrl);
  const faviconKey = faviconUrls.join("|");
  const faviconUrl = faviconUrls[candidateIndex] ?? null;

  React.useEffect(() => {
    setCandidateIndex(0);
  }, [faviconKey]);

  return (
    <span className="url-icon" aria-hidden="true">
      {faviconUrl ? (
        <img
          key={faviconUrl}
          src={faviconUrl}
          alt=""
          onError={() => setCandidateIndex((index) => index + 1)}
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

function AgentBadge({ provider }: { provider: AgentProviderStatus | null | undefined }) {
  if (!provider) return <div className="badge neutral">Checking agent</div>;
  return (
    <div className={provider.available ? "badge success" : "badge danger"}>
      <strong>{provider.available ? "Agent ready" : "Agent missing"}</strong>
      <span>{provider.available ? provider.title : provider.error || "No agent found"}</span>
    </div>
  );
}

function ProjectView({
  project,
  onProjectUpdate,
}: {
  project: ProjectState;
  onProjectUpdate: (project: ProjectState) => void;
}) {
  const [selectedDoc, setSelectedDoc] = React.useState<ContextDoc | null>(null);
  const [onboardingStep, setOnboardingStep] = React.useState<
    "analysis" | "channels" | "dashboard"
  >("analysis");
  const [isCompanyPanelOpen, setIsCompanyPanelOpen] = React.useState(true);
  const [activeChannelId, setActiveChannelId] = React.useState<string | null>(
    null,
  );
  const [configuringChannelId, setConfiguringChannelId] = React.useState<
    string | null
  >(null);
  const [channelError, setChannelError] = React.useState<string | null>(null);
  const run = project.latestRun;
  const activity = project.runActivity.length
    ? project.runActivity
    : [
        {
          kind: "idle",
          title: "Waiting",
          message: "Analysis updates will appear here.",
        },
      ];
  const isRunning = run?.status === "running";
  const isAnalysisComplete =
    !isRunning && project.docs.every(hasDocumentContent);
  const runLabel = project.docs.some(hasDocumentContent)
    ? "Writing..."
    : "Analyzing...";
  const host = displayHost(project.config.websiteUrl);
  const productDescription = extractProductDescription(project.docs);
  const competitors = extractCompetitors(project.docs, host);
  const channels = extractMarketingChannels(project.docs);
  const channelSetups = new Map(
    project.channelSetups.map((setup) => [setup.id, setup]),
  );
  const activeChannel = channels.find(
    (channel) => channel.id === activeChannelId,
  );
  const activeChannelSetup = activeChannelId
    ? (channelSetups.get(activeChannelId) ?? null)
    : null;
  const showChannelDetail = activeChannel?.id === "x";
  const hasConfiguredChannel = project.channelSetups.some(
    (setup) => setup.status === "ready",
  );

  async function configureChannel(channelId: string) {
    setActiveChannelId(channelId);
    setChannelError(null);
    if (channelId !== "x") {
      setChannelError(`${channelName(channelId)} setup is coming next.`);
      return;
    }
    setConfiguringChannelId(channelId);
    try {
      const next = await api.configureChannel(project.config.path, channelId);
      onProjectUpdate(next);
    } catch (err) {
      setChannelError(String(err));
    } finally {
      setConfiguringChannelId(null);
    }
  }

  React.useEffect(() => {
    setOnboardingStep(isAnalysisComplete ? "channels" : "analysis");
    setSelectedDoc(null);
    setIsCompanyPanelOpen(!isAnalysisComplete);
  }, [isAnalysisComplete, project.config.id]);

  React.useEffect(() => {
    window.scrollTo({ top: 0, left: 0 });
  }, [onboardingStep]);

  const showChannels = onboardingStep === "channels";
  const showDashboard = onboardingStep === "dashboard";
  const workspaceClassName = [
    "workspace",
    showChannels || showDashboard ? "workspace-channels" : "workspace-analysis",
    showDashboard ? "workspace-dashboard" : "",
    showChannels || showDashboard ? "workspace-distribution" : "",
    (showChannels || showDashboard) && isCompanyPanelOpen
      ? "workspace-company-open"
      : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <section className={workspaceClassName}>
      <div className="analysis-grid">
        <aside className="panel documents-card" aria-label="Company context">
          <button
            className="company-lockup"
            type="button"
            aria-expanded={isCompanyPanelOpen}
            onClick={() => {
              if (showChannels) {
                setIsCompanyPanelOpen((open) => !open);
              }
            }}
          >
            <UrlIcon websiteUrl={project.config.websiteUrl} />
            <div>
              <strong>{project.config.name}</strong>
            </div>
          </button>

          <div
            className="documents-body"
            aria-hidden={showChannels && !isCompanyPanelOpen}
          >
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
                  <span className="document-chevron" aria-hidden="true">
                    ›
                  </span>
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
                <p className="empty-note">
                  Verified competitor links will appear here after analysis.
                </p>
              )}
            </div>
          </div>
        </aside>

        <section className="panel activity-card" aria-hidden={showChannels}>
          <div className="activity-head">
            <h2>Brand Analysis</h2>
          </div>
          <div className="activity-list">
            {activity.map((item, index) => (
              <article
                className={`activity-item ${activityClass(item.kind)}`}
                key={`${item.title}-${index}`}
              >
                <span className="activity-title">{item.title}</span>
                <p>{item.message}</p>
              </article>
            ))}
            {isRunning ? (
              <div className="analyzing-shimmer">{runLabel}</div>
            ) : null}
          </div>
          {run?.error ? <p className="run-error">{run.error}</p> : null}
          {isAnalysisComplete && !run?.error ? (
            <div className="activity-actions">
              <button
                type="button"
                onClick={() => {
                  setOnboardingStep("channels");
                  setIsCompanyPanelOpen(false);
                }}
              >
                Continue
              </button>
            </div>
          ) : null}
        </section>

        <section
          className="channel-setup"
          aria-hidden={!showChannels || showDashboard}
          aria-label="Marketing channel setup"
        >
          <div className="channel-header">
            <p className="eyebrow">
              {showChannelDetail ? "X setup" : "Distribution setup"}
            </p>
            <h2>
              {showChannelDetail
                ? "Configure X outreach"
                : "Recommended marketing channels"}
            </h2>
            <p>
              {showChannelDetail
                ? "Prepare draft-first X outreach through the existing Chrome session. Codex will find posts, create reply drafts, and wait for approval."
                : "Start with the channels that matched the strategy analysis. Connect or configure each one before moving into recurring GTM work."}
            </p>
          </div>

          {showChannelDetail ? null : (
            <div className="channel-list">
              {channels.map((channel) => (
                <article className="channel-card" key={channel.id}>
                  <UrlIcon websiteUrl={channel.faviconUrl} />
                  <div>
                    <div className="channel-card-head">
                      <h3>{channel.name}</h3>
                      <span>
                        {channelSetups.get(channel.id)?.status === "ready"
                          ? "Ready"
                          : channel.priority}
                      </span>
                    </div>
                    <p>{channel.reason}</p>
                  </div>
                  <button
                    className="secondary"
                    type="button"
                    onClick={() => void configureChannel(channel.id)}
                    disabled={configuringChannelId === channel.id}
                  >
                    {configuringChannelId === channel.id
                      ? "Setting up..."
                      : channelSetups.get(channel.id)?.status === "ready"
                        ? "Open"
                        : "Configure"}
                  </button>
                </article>
              ))}
            </div>
          )}

          {channelError ? (
            <p className="channel-error">{channelError}</p>
          ) : null}

          {activeChannel?.id === "x" ? (
            <XChannelSetupPanel
              channel={activeChannel}
              setup={activeChannelSetup}
              isConfiguring={configuringChannelId === activeChannel.id}
            />
          ) : null}

          <div
            className={
              showChannelDetail
                ? "channel-actions channel-actions-detail"
                : "channel-actions"
            }
          >
            {showChannelDetail ? (
              <button
                className="secondary"
                type="button"
                onClick={() => setActiveChannelId(null)}
              >
                Back
              </button>
            ) : (
              <p>
                {hasConfiguredChannel
                  ? "At least one distribution channel is ready."
                  : "Configure one channel to continue."}
              </p>
            )}
            <button
              type="button"
              disabled={!hasConfiguredChannel}
              onClick={() => setOnboardingStep("dashboard")}
            >
              Continue
            </button>
          </div>
        </section>

        <section
          className="dashboard-preview"
          aria-hidden={!showDashboard}
          aria-label="GTM dashboard"
        >
          <div className="channel-header">
            <p className="eyebrow">Dashboard</p>
            <h2>Daily GTM workspace</h2>
            <p>
              Your configured channels will turn into recurring research,
              drafts, approvals, and browser-assisted posting runs.
            </p>
          </div>

          <div className="dashboard-status-list">
            {project.channelSetups
              .filter((setup) => setup.status === "ready")
              .map((setup) => (
                <article className="dashboard-status-card" key={setup.id}>
                  <strong>{setup.name}</strong>
                  <span>Ready for draft-first runs</span>
                </article>
              ))}
          </div>
        </section>
      </div>

      {selectedDoc ? (
        <div
          className="doc-modal-backdrop"
          onClick={() => setSelectedDoc(null)}
        >
          <article
            className="doc-modal"
            onClick={(event) => event.stopPropagation()}
          >
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

function XChannelSetupPanel({
  channel,
  setup,
  isConfiguring,
}: {
  channel: MarketingChannel;
  setup: ChannelSetup | null;
  isConfiguring: boolean;
}) {
  const isReady = setup?.status === "ready";
  return (
    <article className="x-setup-panel" aria-label="X channel setup">
      <div className="x-setup-head">
        <UrlIcon websiteUrl={channel.faviconUrl} />
        <div>
          <p className="eyebrow">X setup</p>
          <h3>Draft-first outreach through Chrome</h3>
        </div>
        <span>
          {isReady ? "Ready" : isConfiguring ? "Creating" : "Not set up"}
        </span>
      </div>

      <div className="x-setup-steps">
        <SetupStep
          title="Use existing Chrome login"
          description="Codex will use the signed-in browser session instead of the paid X API."
          status={isReady ? "Ready" : "Next"}
        />
        <SetupStep
          title="Learn account voice"
          description="Profile, recent posts, replies, strong examples, and avoid patterns become channel memory."
          status={isReady ? "Ready" : "Planned"}
        />
        <SetupStep
          title="Create daily draft queue"
          description="Codex stores each source post link with a matching reply draft, risk notes, and review status."
          status={isReady ? "Ready" : "Planned"}
        />
        <SetupStep
          title="Prepare reply in Chrome"
          description="Approved drafts can later open the source post and paste the reply so the user only has to review and send."
          status={isReady ? "Next" : "Planned"}
        />
      </div>

      {isReady ? (
        <div className="x-setup-files">
          <p className="eyebrow">Created channel files</p>
          <div>
            {setup.files.map((file) => (
              <code key={file}>{file}</code>
            ))}
          </div>
        </div>
      ) : null}
    </article>
  );
}

function SetupStep({
  title,
  description,
  status,
}: {
  title: string;
  description: string;
  status: string;
}) {
  return (
    <div className="x-setup-step">
      <div>
        <strong>{title}</strong>
        <p>{description}</p>
      </div>
      <span>{status}</span>
    </div>
  );
}

function RenderedDoc({
  content,
  full = false,
}: {
  content: string;
  full?: boolean;
}) {
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

type MarketingChannel = {
  id: string;
  name: string;
  faviconUrl: string;
  priority: "Recommended" | "Optional" | "Not now";
  reason: string;
};

function channelName(channelId: string) {
  return (
    SUPPORTED_CHANNELS.find((channel) => channel.id === channelId)?.name ??
    channelId
  );
}

const SUPPORTED_CHANNELS: MarketingChannel[] = [
  {
    id: "x",
    name: "X",
    faviconUrl: "https://x.com",
    priority: "Optional",
    reason:
      "Use founder-led posts and product narratives when the audience follows builders.",
  },
  {
    id: "reddit",
    name: "Reddit",
    faviconUrl: "https://reddit.com",
    priority: "Optional",
    reason:
      "Use community research and draft replies when niche problem discussions are active.",
  },
  {
    id: "hacker-news",
    name: "Hacker News",
    faviconUrl: "https://news.ycombinator.com",
    priority: "Optional",
    reason:
      "Use launches and technical discussion when the product has a founder or developer angle.",
  },
  {
    id: "seo",
    name: "SEO",
    faviconUrl: "https://search.google.com/search-console",
    priority: "Optional",
    reason:
      "Use search demand and website content when organic intent is visible.",
  },
];

function docByKey(docs: ContextDoc[], key: string) {
  const fileKey = key.replaceAll("_", "-");
  return docs.find((doc) => doc.key === key || doc.fileName.includes(fileKey));
}

function shouldRunInitialAnalysis(project: ProjectState) {
  if (!project.latestRun) return true;
  if (project.latestRun.status === "failed") return true;
  return project.docs.some((doc) => !hasDocumentContent(doc));
}

function hasDocumentContent(doc: ContextDoc) {
  return doc.content.trim() !== `# ${doc.title}`;
}

function activityClass(kind: string) {
  if (kind === "message" || kind === "tool" || kind === "idle") return kind;
  return "other";
}

function extractProductDescription(docs: ContextDoc[]) {
  const doc = docByKey(docs, "product_information");
  if (!doc) return "Product description will appear here after analysis.";

  const paragraph = markdownBlocks(doc.content).find((block) => {
    if (block.type !== "paragraph") return false;
    const text = block.text.toLowerCase();
    const urlCount = (
      block.text.match(/https?:\/\/|\b(?:[a-z0-9-]+\.)+[a-z]{2,}\b/gi) ?? []
    ).length;
    return (
      block.text.length > 60 &&
      urlCount < 2 &&
      !text.includes("status:") &&
      !text.includes("source url") &&
      !text.includes("urls checked") &&
      !text.includes("sources checked")
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
    if (
      !key ||
      key.endsWith(".md") ||
      key === own ||
      key.endsWith(`.${own}`) ||
      competitors.has(key)
    )
      return;
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

function extractMarketingChannels(docs: ContextDoc[]) {
  const doc = docByKey(docs, "marketing_strategy");
  if (!doc || !hasDocumentContent(doc)) return SUPPORTED_CHANNELS;

  const content = doc.content.toLowerCase();
  const detected = SUPPORTED_CHANNELS.map((channel) => {
    const score = channelKeywordScore(channel.id, content);
    const priority = extractChannelPriority(channel, doc.content);
    return {
      ...channel,
      priority:
        priority ?? (score > 0 ? ("Recommended" as const) : channel.priority),
      reason: score > 0 ? recommendedChannelReason(channel.id) : channel.reason,
      score,
    };
  })
    .filter((channel) => channel.score > 0 && channel.priority !== "Not now")
    .sort((a, b) => channelDisplayRank(a.id) - channelDisplayRank(b.id));

  return detected.length
    ? detected.map(({ score: _, ...channel }) => channel)
    : SUPPORTED_CHANNELS;
}

function channelDisplayRank(channelId: string) {
  const ranks: Record<string, number> = {
    x: 10,
    reddit: 20,
    "hacker-news": 30,
    seo: 40,
  };
  return ranks[channelId] ?? 100;
}

function extractChannelPriority(channel: MarketingChannel, content: string) {
  const headingNames: Record<string, string[]> = {
    seo: ["seo"],
    x: ["x", "twitter"],
    reddit: ["reddit"],
    "hacker-news": ["hacker news", "hn"],
  };
  const names = headingNames[channel.id] ?? [channel.name.toLowerCase()];
  const lines = content.split(/\r?\n/);
  const headingIndex = lines.findIndex((line) => {
    const normalized = cleanInline(line)
      .replace(/^#+\s*/, "")
      .trim()
      .toLowerCase();
    return names.some(
      (name) => normalized === name || normalized.startsWith(`${name} `),
    );
  });
  if (headingIndex === -1) return null;

  const section = lines
    .slice(headingIndex + 1, headingIndex + 8)
    .join("\n")
    .toLowerCase();
  if (section.includes("priority: not now")) return "Not now" as const;
  if (section.includes("priority: optional")) return "Optional" as const;
  if (section.includes("priority: recommended")) return "Recommended" as const;
  return null;
}

function channelKeywordScore(channelId: string, content: string) {
  const patterns: Record<string, RegExp[]> = {
    seo: [
      /\bseo\b/g,
      /\bsearch\b/g,
      /\borganic\b/g,
      /\bkeyword\b/g,
      /\bcontent\b/g,
      /\bgoogle\b/g,
    ],
    x: [
      /(^|\s)x(\s|$|[.,;:])/g,
      /\btwitter\b/g,
      /\bfounder-led\b/g,
      /\bfounder led\b/g,
      /\bbuild in public\b/g,
    ],
    reddit: [/\breddit\b/g, /\bsubreddit\b/g, /\bcommunity\b/g],
    "hacker-news": [
      /\bhacker news\b/g,
      /(^|\s)hn(\s|$|[.,;:])/g,
      /\by combinator\b/g,
      /\btechnical audience\b/g,
    ],
  };
  return (patterns[channelId] ?? []).reduce(
    (score, pattern) => score + (content.match(pattern)?.length ?? 0),
    0,
  );
}

function recommendedChannelReason(channelId: string) {
  const reasons: Record<string, string> = {
    seo: "The strategy points to search intent, website content, or category education as a useful acquisition path.",
    x: "The strategy points to founder-led distribution, product narrative, or an audience that can be reached through short-form posts.",
    reddit:
      "The strategy points to niche communities where the problem can be researched and joined through careful draft-first replies.",
    "hacker-news":
      "The strategy points to a technical or founder audience where launches and product discussions can create early signal.",
  };
  return (
    reasons[channelId] ?? "Recommended by the marketing strategy analysis."
  );
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
      blocks.push({
        type: "ordered-list",
        items: orderedList.map(cleanInline),
      });
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
  return blocks.length
    ? blocks
    : [{ type: "paragraph", text: "No content yet." }];
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

function faviconUrlsForUrl(value: string) {
  const trimmed = value.trim();
  if (!trimmed || !trimmed.includes(".")) return [];
  try {
    const url = new URL(
      trimmed.startsWith("http://") || trimmed.startsWith("https://")
        ? trimmed
        : `https://${trimmed}`,
    );
    return [
      `https://icons.duckduckgo.com/ip3/${url.hostname}.ico`,
      `${url.origin}/favicon.ico`,
    ];
  } catch {
    return [];
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
    return value
      .replace(/^https?:\/\//, "")
      .replace(/^www\./, "")
      .split("/")[0];
  }
}

createRoot(document.getElementById("root")!).render(<App />);
