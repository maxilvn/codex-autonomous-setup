import React from "react";
import { createPortal } from "react-dom";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { api } from "./lib/api";
import type {
  AgentProviderStatus,
  ChannelSetup,
  ChromeProfile,
  ContextDoc,
  ProjectState,
  RunActivity,
  RunState,
} from "./lib/types";
import "./styles.css";

const logoBlack = new URL(
  "./assets/brand/two-wedge-logo-black-transparent.png",
  import.meta.url,
).href;

function App() {
  const [agentProvider, setAgentProvider] =
    React.useState<AgentProviderStatus | null>(null);
  const [agentProviders, setAgentProviders] = React.useState<
    AgentProviderStatus[]
  >([]);
  const [project, setProject] = React.useState<ProjectState | null>(null);
  const [websiteUrl, setWebsiteUrl] = React.useState("");
  const [onboardingStep, setOnboardingStep] = React.useState<"url" | "agent">(
    "url",
  );
  const [busy, setBusy] = React.useState(false);
  const [restoring, setRestoring] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const refreshProject = React.useCallback(async () => {
    if (!project) return;
    setProject(await api.loadProject(project.config.path));
  }, [project]);

  React.useEffect(() => {
    api
      .detectAgentProvider()
      .then(setAgentProvider)
      .catch((err) => {
        setAgentProvider({
          id: "agent",
          title: "Agent",
          command: "",
          args: [],
          enabled: false,
          selected: false,
          available: false,
          error: String(err),
        });
      });
    api
      .listAgentProviders()
      .then((providers) => {
        setAgentProviders(providers);
        setAgentProvider(
          providers.find((provider) => provider.selected) ??
            providers[0] ??
            null,
        );
      })
      .catch(() => undefined);
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

  async function continueToAgentSelection() {
    if (!websiteUrl.trim()) return;
    setError(null);
    setOnboardingStep("agent");
  }

  async function createProject(providerId = "codex") {
    setBusy(true);
    setError(null);
    try {
      const selected = await api.selectAgentProvider(providerId);
      setAgentProvider(selected);
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
        </section>
      ) : null}

      {error ? <div className="error">{error}</div> : null}

      {!project && !restoring && onboardingStep === "url" ? (
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
                if (event.key === "Enter" && !busy)
                  void continueToAgentSelection();
              }}
              placeholder="website.com"
            />
            <button
              onClick={continueToAgentSelection}
              disabled={busy || !websiteUrl.trim()}
            >
              Analyze
            </button>
          </div>
        </section>
      ) : !project && !restoring && onboardingStep === "agent" ? (
        <AgentSelectionStep
          providers={agentProviders}
          selectedProvider={agentProvider}
          websiteUrl={websiteUrl}
          busy={busy}
          onBack={() => setOnboardingStep("url")}
          onUnavailable={(title) =>
            setError(`${title} is not available yet. Use Codex for now.`)
          }
          onSelect={(providerId) => void createProject(providerId)}
        />
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

function AgentSelectionStep({
  providers,
  selectedProvider,
  websiteUrl,
  busy,
  onBack,
  onSelect,
  onUnavailable,
}: {
  providers: AgentProviderStatus[];
  selectedProvider: AgentProviderStatus | null;
  websiteUrl: string;
  busy: boolean;
  onBack: () => void;
  onSelect: (providerId: string) => void;
  onUnavailable: (title: string) => void;
}) {
  const [selectedAgentId, setSelectedAgentId] = React.useState(
    selectedProvider?.id ?? "codex",
  );
  const providerIds = new Set(providers.map((provider) => provider.id));
  const providerOptions = AGENT_PROVIDER_OPTIONS.filter(
    (option) => option.id === "codex" || providerIds.has(option.id),
  );
  const selectedOption =
    providerOptions.find((option) => option.id === selectedAgentId) ??
    providerOptions.find((option) => option.id === "codex");

  React.useEffect(() => {
    setSelectedAgentId(selectedProvider?.id ?? "codex");
  }, [selectedProvider?.id]);

  return (
    <section className="agent-onboarding">
      <div className="onboarding-copy">
        <p className="eyebrow">Agent provider</p>
        <h1>Select your agent</h1>
        <p className="agent-onboarding-subtitle">
          {displayHost(websiteUrl)} will be analyzed through the selected ACP
          provider.
        </p>
      </div>

      <div className="agent-provider-list">
        {providerOptions.map((option) => {
          const isCodex = option.id === "codex";
          const isSelected = selectedAgentId === option.id;
          return (
            <button
              className={[
                "agent-provider-row",
                isSelected ? "is-selected" : "",
                !isCodex ? "is-disabled" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              key={option.id}
              type="button"
              onClick={() => {
                if (!isCodex) {
                  onUnavailable(option.title);
                  return;
                }
                setSelectedAgentId(option.id);
              }}
              disabled={busy}
            >
              <img
                alt=""
                className="agent-provider-icon"
                src={option.faviconUrl}
              />
              <strong>{option.title}</strong>
              <span className={isCodex ? "agent-ready" : "agent-unavailable"}>
                {isCodex ? "Available" : "Not available yet"}
              </span>
            </button>
          );
        })}
      </div>

      <div className="agent-onboarding-actions">
        <button className="secondary" type="button" onClick={onBack}>
          Back
        </button>
        <button
          type="button"
          onClick={() => onSelect(selectedOption?.id ?? "codex")}
          disabled={busy || selectedOption?.id !== "codex"}
        >
          {busy ? "Starting..." : "Select"}
        </button>
      </div>
    </section>
  );
}

const AGENT_PROVIDER_OPTIONS = [
  {
    id: "codex",
    title: "Codex",
    faviconUrl: "https://www.google.com/s2/favicons?domain=openai.com&sz=64",
  },
  {
    id: "claude",
    title: "Claude Code",
    faviconUrl: "https://www.google.com/s2/favicons?domain=claude.ai&sz=64",
  },
  {
    id: "cursor",
    title: "Cursor",
    faviconUrl: "https://www.google.com/s2/favicons?domain=cursor.com&sz=64",
  },
  {
    id: "devin",
    title: "Devin",
    faviconUrl: "https://www.google.com/s2/favicons?domain=devin.ai&sz=64",
  },
  {
    id: "gemini",
    title: "Gemini",
    faviconUrl:
      "https://www.google.com/s2/favicons?domain=gemini.google.com&sz=64",
  },
  {
    id: "copilot",
    title: "Copilot",
    faviconUrl: "https://www.google.com/s2/favicons?domain=github.com&sz=64",
  },
];

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
  const [checkingChannelId, setCheckingChannelId] = React.useState<
    string | null
  >(null);
  const [analyzingChannelId, setAnalyzingChannelId] = React.useState<
    string | null
  >(null);
  const [chromeProfiles, setChromeProfiles] = React.useState<ChromeProfile[]>(
    [],
  );
  const [selectedChromeProfileId, setSelectedChromeProfileId] = React.useState<
    string | null
  >(null);
  const [isLoadingChromeProfiles, setIsLoadingChromeProfiles] =
    React.useState(false);
  const [channelError, setChannelError] = React.useState<string | null>(null);
  const run = project.latestRun;
  const isInitialAnalysisRun = run?.kind === "initial_analysis";
  const isInitialAnalysisRunning =
    isInitialAnalysisRun && run?.status === "running";
  const activity =
    isInitialAnalysisRun && project.runActivity.length
      ? project.runActivity
      : [
          {
            kind: "idle",
            title: "Waiting",
            message: "Analysis updates will appear here.",
          },
        ];
  const agentOutput = agentOutputActivity(activity);
  const isAnalysisComplete =
    !isInitialAnalysisRunning && project.docs.every(hasDocumentContent);
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
    setChannelError(null);
    if (channelId !== "x") {
      setActiveChannelId(null);
      setChannelError(`${channelName(channelId)} setup is coming next.`);
      return;
    }
    setActiveChannelId(channelId);
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

  async function analyzeXAccount() {
    setChannelError(null);
    setAnalyzingChannelId("x");
    try {
      await api.runXAccountAnalysis(project.config.path);
      onProjectUpdate(await api.loadProject(project.config.path));
    } catch (err) {
      setChannelError(String(err));
    } finally {
      setAnalyzingChannelId(null);
    }
  }

  async function selectXProfileForAnalysis(
    profileId = selectedChromeProfileId,
  ) {
    if (!profileId) {
      setChannelError("Choose a Chrome profile first.");
      return;
    }
    setChannelError(null);
    setCheckingChannelId("x");
    try {
      const checkedProject = await api.verifyXLogin(
        project.config.path,
        profileId,
      );
      onProjectUpdate(checkedProject);
      const xSetup = checkedProject.channelSetups.find(
        (setup) => setup.id === "x",
      );
      if (xSetup?.accountStatus === "authenticated") {
        await analyzeXAccount();
      }
    } catch (err) {
      setChannelError(String(err));
    } finally {
      setCheckingChannelId(null);
    }
  }

  async function openXLogin(profileId = selectedChromeProfileId) {
    setChannelError(null);
    try {
      await api.openXLogin(profileId);
    } catch (err) {
      setChannelError(String(err));
    }
  }

  async function openXChannelDoc(fileName: string) {
    setChannelError(null);
    try {
      setSelectedDoc(
        await api.loadChannelContextDoc(project.config.path, "x", fileName),
      );
    } catch (err) {
      setChannelError(String(err));
    }
  }

  React.useEffect(() => {
    setOnboardingStep(isAnalysisComplete ? "channels" : "analysis");
    setSelectedDoc(null);
    setIsCompanyPanelOpen(!isAnalysisComplete);
  }, [project.config.id]);

  React.useEffect(() => {
    window.scrollTo({ top: 0, left: 0 });
  }, [onboardingStep]);

  React.useEffect(() => {
    if (activeChannelId !== "x") {
      return;
    }
    let isCancelled = false;
    setIsLoadingChromeProfiles(true);
    void api
      .listChromeProfiles()
      .then((profiles) => {
        if (isCancelled) {
          return;
        }
        setChromeProfiles(profiles);
        setSelectedChromeProfileId((current) =>
          current && profiles.some((profile) => profile.id === current)
            ? current
            : null,
        );
      })
      .catch((err) => {
        if (!isCancelled) {
          setChannelError(String(err));
        }
      })
      .finally(() => {
        if (!isCancelled) {
          setIsLoadingChromeProfiles(false);
        }
      });
    return () => {
      isCancelled = true;
    };
  }, [activeChannelId, activeChannelSetup?.chromeProfileId]);

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

        {!showChannels ? (
          <AnalysisProgressCard
            agentOutput={agentOutput}
            channels={channels}
            competitors={competitors}
            docs={project.docs}
            isComplete={isAnalysisComplete}
            isRunning={isInitialAnalysisRunning}
            productDescription={productDescription}
            runError={run?.error ?? null}
            onContinue={() => {
              setOnboardingStep("channels");
              setIsCompanyPanelOpen(false);
            }}
          />
        ) : null}

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

          <div className="channel-list">
            {channels
              .filter((channel) => !showChannelDetail || channel.id === "x")
              .map((channel) => (
                <article
                  className={[
                    "channel-card",
                    `channel-card-${channel.id}`,
                    showChannelDetail && channel.id === "x"
                      ? "is-expanded"
                      : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  key={channel.id}
                >
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
                  {showChannelDetail && channel.id === "x" ? null : (
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
                  )}
                  {showChannelDetail && channel.id === "x" ? (
                    <div className="channel-card-expansion">
                      <XChannelSetupPanel
                        channel={activeChannel ?? channel}
                        setup={activeChannelSetup}
                        run={run?.kind === "x_account_analysis" ? run : null}
                        activity={
                          run?.kind === "x_account_analysis"
                            ? project.runActivity
                            : []
                        }
                        isConfiguring={configuringChannelId === channel.id}
                        isChecking={checkingChannelId === channel.id}
                        isAnalyzing={analyzingChannelId === channel.id}
                        chromeProfiles={chromeProfiles}
                        selectedChromeProfileId={selectedChromeProfileId}
                        isLoadingChromeProfiles={isLoadingChromeProfiles}
                        onSelectChromeProfile={setSelectedChromeProfileId}
                        onSelectForAnalysis={(profileId) =>
                          void selectXProfileForAnalysis(profileId)
                        }
                        onOpenLogin={(profileId) => void openXLogin(profileId)}
                        onOpenFile={(fileName) =>
                          void openXChannelDoc(fileName)
                        }
                        embedded
                      />
                    </div>
                  ) : null}
                </article>
              ))}
          </div>

          {channelError ? (
            <p className="channel-error">{channelError}</p>
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

type AnalysisStepStatus = "done" | "active" | "pending";

type AnalysisStep = {
  title: string;
  detail: string;
  statusLabel: string;
  status: AnalysisStepStatus;
};

function AnalysisProgressCard({
  agentOutput,
  channels,
  competitors,
  docs,
  isComplete,
  isRunning,
  productDescription,
  runError,
  onContinue,
}: {
  agentOutput: RunActivity[];
  channels: MarketingChannel[];
  competitors: Competitor[];
  docs: ContextDoc[];
  isComplete: boolean;
  isRunning: boolean;
  productDescription: string;
  runError: string | null;
  onContinue: () => void;
}) {
  const [isLogOpen, setIsLogOpen] = React.useState(false);
  const logRef = React.useRef<HTMLDivElement | null>(null);
  const steps = analysisSteps(docs, competitors, isRunning, isComplete);
  const activeStep = steps.find((step) => step.status === "active");
  const readyDocCount = docs.filter(hasDocumentContent).length;
  const resultStats = [
    { label: "Documents", value: `${readyDocCount}/${docs.length}` },
    { label: "Competitors", value: String(competitors.length) },
    { label: "Channels", value: String(channels.length) },
  ];

  React.useEffect(() => {
    if (!isLogOpen || !logRef.current) return;
    logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [isLogOpen, agentOutput.length, agentOutput.at(-1)?.message]);

  return (
    <section className="panel activity-card">
      <div className="activity-head">
        <div>
          <p className="eyebrow">Brand workspace</p>
          <h2>{isComplete ? "Brand analysis ready" : "Analyzing brand"}</h2>
        </div>
        <button
          className="secondary agent-log-toggle"
          type="button"
          onClick={() => setIsLogOpen((open) => !open)}
        >
          {isLogOpen ? "Hide agent log" : "Show agent log"}
        </button>
      </div>

      <div className="analysis-card-body">
        <div className="analysis-summary">
          <p>
            {isComplete
              ? "The source documents are ready. Review the brand context, then continue into channel setup."
              : (activeStep?.detail ??
                "GTM Agent is preparing the source documents from the selected ACP agent.")}
          </p>
        </div>

        <div className="analysis-step-list" aria-label="Analysis progress">
          {steps.map((step) => (
            <div className={`analysis-step is-${step.status}`} key={step.title}>
              <span className="analysis-step-icon" aria-hidden="true" />
              <div>
                <strong>{step.title}</strong>
                <p>{step.statusLabel}</p>
              </div>
            </div>
          ))}
        </div>

        {isComplete && !runError ? (
          <div className="analysis-result" aria-label="Analysis result">
            <p>{productDescription}</p>
            <div className="analysis-result-grid">
              {resultStats.map((stat) => (
                <div key={stat.label}>
                  <strong>{stat.value}</strong>
                  <span>{stat.label}</span>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {isLogOpen ? (
          <div className="agent-log-panel">
            <div className="activity-list agent-log-list" ref={logRef}>
              {agentOutput.length ? (
                agentOutput.map((item, index) => (
                  <article
                    className={`activity-item ${activityClass(item.kind)}`}
                    key={`${item.title}-${index}`}
                  >
                    <p>{item.message}</p>
                  </article>
                ))
              ) : (
                <article className="activity-item">
                  <p>No visible agent messages yet.</p>
                </article>
              )}
            </div>
          </div>
        ) : null}
      </div>

      {runError ? <p className="run-error">{runError}</p> : null}
      {isComplete && !runError ? (
        <div className="activity-actions">
          <button type="button" onClick={onContinue}>
            Continue
          </button>
        </div>
      ) : null}
    </section>
  );
}

function analysisSteps(
  docs: ContextDoc[],
  competitors: Competitor[],
  isRunning: boolean,
  isComplete: boolean,
): AnalysisStep[] {
  const productDoc = docByKey(docs, "product_information");
  const strategyDoc = docByKey(docs, "marketing_strategy");
  const competitorsDoc = docByKey(docs, "competitor_analysis");
  const brandDoc = docByKey(docs, "brand_voice");
  const productDocReady = Boolean(productDoc && hasDocumentContent(productDoc));
  const strategyDocReady = Boolean(
    strategyDoc && hasDocumentContent(strategyDoc),
  );
  const competitorsDocReady = Boolean(
    competitorsDoc && hasDocumentContent(competitorsDoc),
  );
  const brandDocReady = Boolean(brandDoc && hasDocumentContent(brandDoc));

  const stepInputs = [
    {
      title: "Website review",
      detail:
        "Reading the public website and extracting the core product context.",
      pendingLabel: "Waiting",
      activeLabel: "Reading website",
      doneLabel: "Website reviewed",
      done: productDocReady || isComplete,
    },
    {
      title: "Positioning",
      detail: "Turning the evidence into product positioning and brand voice.",
      pendingLabel: "Waiting",
      activeLabel: "Extracting positioning",
      doneLabel: "Positioning captured",
      done: (productDocReady && brandDocReady) || isComplete,
    },
    {
      title: "Market context",
      detail:
        "Checking competitors and category alternatives before writing recommendations.",
      pendingLabel: "Waiting",
      activeLabel: "Checking market context",
      doneLabel: "Market context ready",
      done: competitorsDocReady || competitors.length > 0 || isComplete,
    },
    {
      title: "Source documents",
      detail:
        "Writing the GTM source documents used by the rest of the workspace.",
      pendingLabel: "Waiting",
      activeLabel: "Writing source documents",
      doneLabel: "Source documents ready",
      done:
        isComplete ||
        (productDocReady &&
          strategyDocReady &&
          competitorsDocReady &&
          brandDocReady),
    },
  ];
  let activeAssigned = false;
  return stepInputs.map((step) => {
    if (step.done) {
      return { ...step, status: "done", statusLabel: step.doneLabel };
    }
    if (isRunning && !activeAssigned) {
      activeAssigned = true;
      return { ...step, status: "active", statusLabel: step.activeLabel };
    }
    return { ...step, status: "pending", statusLabel: step.pendingLabel };
  });
}

function XChannelSetupPanel({
  channel,
  setup,
  run,
  activity,
  isConfiguring,
  isChecking,
  isAnalyzing,
  chromeProfiles,
  selectedChromeProfileId,
  isLoadingChromeProfiles,
  onSelectChromeProfile,
  onSelectForAnalysis,
  onOpenLogin,
  onOpenFile,
  embedded = false,
}: {
  channel: MarketingChannel;
  setup: ChannelSetup | null;
  run: RunState | null;
  activity: RunActivity[];
  isConfiguring: boolean;
  isChecking: boolean;
  isAnalyzing: boolean;
  chromeProfiles: ChromeProfile[];
  selectedChromeProfileId: string | null;
  isLoadingChromeProfiles: boolean;
  onSelectChromeProfile: (profileId: string) => void;
  onSelectForAnalysis: (profileId?: string | null) => void;
  onOpenLogin: (profileId?: string | null) => void;
  onOpenFile: (fileName: string) => void;
  embedded?: boolean;
}) {
  const [isProfilePickerOpen, setIsProfilePickerOpen] = React.useState(false);
  const [hasSelectedProfileForRun, setHasSelectedProfileForRun] =
    React.useState(false);
  const isReady = setup?.status === "ready";
  const isVerified = setup?.accountStatus === "authenticated";
  const hasVerifiedSelectedProfile =
    isVerified && setup?.chromeProfileId === selectedChromeProfileId;
  const needsLogin = setup?.accountStatus === "needs_login";
  const isUnknown = setup?.accountStatus === "unknown";
  const hasConfirmedSelectedProfile =
    hasSelectedProfileForRun || Boolean(run?.status === "running");
  const isRunActive =
    hasConfirmedSelectedProfile &&
    hasVerifiedSelectedProfile &&
    (isAnalyzing || run?.status === "running");
  const isLoginActionBusy = isChecking || isRunActive || isConfiguring;
  const selectedChromeProfile = chromeProfiles.find(
    (profile) => profile.id === selectedChromeProfileId,
  );
  const selectedProfileHasXSession = Boolean(
    selectedChromeProfile?.hasXSession,
  );
  const accountName = xAccountDisplayName(setup);
  const loginLabel = hasVerifiedSelectedProfile
    ? `Signed in as ${accountName}`
    : needsLogin || (selectedChromeProfile && !selectedProfileHasXSession)
      ? "No signed-in X account found in this Chrome profile."
      : isUnknown
        ? "GTM Agent could not verify this Chrome profile. Check again or choose another profile."
        : selectedChromeProfile
          ? selectedProfileHasXSession
            ? "X session found in this Chrome profile. The username appears here once analysis can read it."
            : "No signed-in X account found in this Chrome profile."
          : "Choose the Chrome profile GTM Agent should check.";
  const actionLabel = isChecking
    ? "Checking..."
    : isRunActive
      ? "Analyzing..."
      : needsLogin || (selectedChromeProfile && !selectedProfileHasXSession)
        ? "Sign in to X"
        : hasVerifiedSelectedProfile && !hasSelectedProfileForRun
          ? "Select"
          : hasVerifiedSelectedProfile
            ? isReady
              ? "Ready"
              : "Check again"
            : "Select";
  const shouldShowAnalysisOutput =
    hasSelectedProfileForRun &&
    hasVerifiedSelectedProfile &&
    (isRunActive || agentOutputActivity(activity).length > 0 || isReady);
  const agentOutput = agentOutputActivity(activity);
  function useChromeProfile(profileId: string) {
    onSelectChromeProfile(profileId);
    setHasSelectedProfileForRun(false);
    setIsProfilePickerOpen(false);
  }
  return (
    <div
      className={
        embedded ? "x-setup-panel x-setup-panel-embedded" : "x-setup-panel"
      }
      aria-label="X channel setup"
    >
      {embedded ? null : (
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
      )}

      {!shouldShowAnalysisOutput ? (
        <div className="x-login-card">
          <div className="x-login-copy">
            <strong>Selected Chrome profile</strong>
            {selectedChromeProfile ? (
              <div className="x-selected-profile">
                <ChromeProfileAvatar profile={selectedChromeProfile} />
                <div>
                  <span>{selectedChromeProfile.name}</span>
                  <p>{profileSubtitle(selectedChromeProfile)}</p>
                  {selectedProfileHasXSession ? (
                    <p className="x-selected-account">
                      {hasVerifiedSelectedProfile
                        ? xAccountDisplayName(setup)
                        : "X session found"}
                    </p>
                  ) : null}
                </div>
              </div>
            ) : (
              <p>
                {isLoadingChromeProfiles
                  ? "Loading Chrome profiles..."
                  : "No Chrome profile selected yet."}
              </p>
            )}
            <p className="x-profile-note">{loginLabel}</p>
          </div>
          <div className="x-login-actions">
            <button
              className="secondary"
              type="button"
              onClick={() => setIsProfilePickerOpen(true)}
              disabled={isLoginActionBusy || isLoadingChromeProfiles}
            >
              Choose profile
            </button>
            {!selectedChromeProfile ? null : needsLogin ||
              !selectedProfileHasXSession ? (
              <button
                className="secondary"
                type="button"
                onClick={() => onOpenLogin(selectedChromeProfileId)}
                disabled={isLoginActionBusy}
              >
                Sign in to X
              </button>
            ) : (
              <button
                type="button"
                onClick={() => {
                  setHasSelectedProfileForRun(true);
                  onSelectForAnalysis(selectedChromeProfileId);
                }}
                disabled={isLoginActionBusy}
              >
                {actionLabel}
              </button>
            )}
          </div>
        </div>
      ) : null}

      {isProfilePickerOpen
        ? createPortal(
            <div
              className="profile-picker-backdrop"
              onClick={() => setIsProfilePickerOpen(false)}
            >
              <article
                className="profile-picker-modal"
                onClick={(event) => event.stopPropagation()}
              >
                <div className="profile-picker-head">
                  <div>
                    <p className="eyebrow">Chrome profile</p>
                    <h3>Choose the profile GTM Agent should use</h3>
                  </div>
                  <button
                    className="modal-close"
                    type="button"
                    aria-label="Close"
                    onClick={() => setIsProfilePickerOpen(false)}
                  >
                    ×
                  </button>
                </div>
                <div className="profile-picker-list">
                  {chromeProfiles.map((profile) => (
                    <button
                      className={[
                        "profile-picker-row",
                        profile.id === selectedChromeProfileId
                          ? "is-selected"
                          : "",
                      ]
                        .filter(Boolean)
                        .join(" ")}
                      key={profile.id}
                      type="button"
                      onClick={() => useChromeProfile(profile.id)}
                    >
                      <ChromeProfileAvatar profile={profile} />
                      <div className="profile-picker-main">
                        <div className="profile-picker-title">
                          <strong>{profile.name}</strong>
                          {profile.isRecommended ? (
                            <span className="profile-recommended">
                              Suggested
                            </span>
                          ) : null}
                        </div>
                        <span className="profile-picker-email">
                          {profileSubtitle(profile)}
                        </span>
                        <span
                          className={
                            profile.hasXSession
                              ? "profile-x-status is-found"
                              : "profile-x-status"
                          }
                        >
                          {profileXAccountLabel(profile, setup)}
                        </span>
                      </div>
                      <em className="profile-picker-action">
                        {profile.id === selectedChromeProfileId
                          ? "Selected"
                          : "Choose"}
                      </em>
                    </button>
                  ))}
                </div>
                <p className="profile-picker-note">
                  Pick the Chrome profile first. GTM Agent only checks X and
                  starts analysis after you confirm with Select.
                </p>
              </article>
            </div>,
            document.body,
          )
        : null}

      {shouldShowAnalysisOutput ? (
        <div className="x-analysis-grid">
          <div className="x-analysis-files">
            <p className="eyebrow">Writing</p>
            {X_CHANNEL_DOCS.map((doc) => (
              <button
                className="x-analysis-file-row"
                key={doc.fileName}
                type="button"
                onClick={() => onOpenFile(doc.fileName)}
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
          <div className="x-codex-card">
            <div className="x-codex-output">
              {agentOutput.length ? (
                agentOutput.slice(-6).map((item, index) => (
                  <article
                    className="x-codex-item"
                    key={`${item.title}-${index}`}
                  >
                    <p>{item.message}</p>
                  </article>
                ))
              ) : (
                <article className="x-codex-item">
                  <p>
                    Codex will inspect the selected X account in Chrome and
                    write profile, rules, examples, and voice.
                  </p>
                </article>
              )}
              {isRunActive ? (
                <div className="analyzing-shimmer">Analyzing...</div>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}

      {embedded ? null : (
        <div className="x-setup-steps">
          <SetupStep
            title="Learn account voice"
            description="Profile, recent posts, replies, strong examples, and avoid patterns become channel memory."
            status={
              setup?.analysisStatus === "running"
                ? "Running"
                : isReady
                  ? "Ready"
                  : "Next"
            }
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
      )}
    </div>
  );
}

const X_CHANNEL_DOCS = [
  { fileName: "profile.md", title: "Profile" },
  { fileName: "voice.md", title: "Voice" },
  { fileName: "rules.md", title: "Rules" },
  { fileName: "examples.md", title: "Examples" },
];

function ChromeProfileAvatar({ profile }: { profile: ChromeProfile }) {
  if (profile.avatarDataUrl) {
    return (
      <img
        className="chrome-profile-avatar chrome-profile-avatar-image"
        src={profile.avatarDataUrl}
        alt=""
      />
    );
  }
  return (
    <span
      className="chrome-profile-avatar"
      style={{ backgroundColor: chromeProfileColor(profile) }}
      aria-hidden="true"
    >
      {profileInitials(profile)}
    </span>
  );
}

function profileSubtitle(profile: ChromeProfile) {
  return (
    profile.email ??
    profile.accountName ??
    (profile.isDefault ? "Default Chrome profile" : profile.id)
  );
}

function profileXAccountLabel(
  profile: ChromeProfile,
  setup: ChannelSetup | null,
) {
  if (
    setup?.accountStatus === "authenticated" &&
    setup.chromeProfileId === profile.id
  ) {
    return xAccountDisplayName(setup);
  }
  return profile.hasXSession ? "X session found" : "Not signed in";
}

function xAccountDisplayName(setup: ChannelSetup | null) {
  return setup?.accountHandle ?? setup?.accountLabel ?? "X session found";
}

function profileInitials(profile: ChromeProfile) {
  const source =
    profile.name || profile.accountName || profile.email || "Chrome";
  return (
    source
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase())
      .join("") || "C"
  );
}

function chromeProfileColor(profile: ChromeProfile) {
  if (typeof profile.profileColor === "number") {
    const rgb = profile.profileColor >>> 0;
    return `#${(rgb & 0xffffff).toString(16).padStart(6, "0")}`;
  }
  const palette = ["#1a73e8", "#188038", "#d93025", "#f9ab00", "#9334e6"];
  const seed = Array.from(profile.id).reduce(
    (sum, character) => sum + character.charCodeAt(0),
    0,
  );
  return palette[seed % palette.length];
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

function agentOutputActivity(activity: RunActivity[]) {
  return activity.filter(
    (item) =>
      item.kind !== "tool" &&
      item.kind !== "idle" &&
      shouldShowAgentOutput(item.message),
  );
}

function shouldShowAgentOutput(message: string) {
  const text = message.trim().replace(/\s+/g, " ");
  if (!text) return false;
  const lower = text.toLowerCase();
  return ![
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
  ].some((prefix) => lower.startsWith(prefix));
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
