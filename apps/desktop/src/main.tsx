import React from "react";
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
} from "./lib/types";
import "./styles.css";

const logoBlack = new URL(
  "./assets/brand/two-wedge-logo-black-transparent.png",
  import.meta.url,
).href;

const ONBOARDING_STEPS = [
  "url",
  "agent",
  "brand",
  "browser",
  "channels",
  "analysis",
] as const;

type OnboardingStep = (typeof ONBOARDING_STEPS)[number];
type AppStep = OnboardingStep | "workspace";

type ChannelOption = {
  id: string;
  name: string;
  faviconUrl: string;
  description: string;
};

const CHANNEL_OPTIONS: ChannelOption[] = [
  {
    id: "x",
    name: "X",
    faviconUrl: "https://x.com",
    description:
      "Founder-led posts and draft-first replies where builders follow builders.",
  },
  {
    id: "reddit",
    name: "Reddit",
    faviconUrl: "https://reddit.com",
    description:
      "Community research and careful draft-first replies in niche subreddits.",
  },
  {
    id: "hacker-news",
    name: "Hacker News",
    faviconUrl: "https://news.ycombinator.com",
    description:
      "Launches and technical discussion for a founder or developer audience.",
  },
];

const CHANNEL_DOCS = [
  { fileName: "profile.md", title: "Profile" },
  { fileName: "voice.md", title: "Voice" },
  { fileName: "rules.md", title: "Rules" },
  { fileName: "examples.md", title: "Examples" },
];

const AGENT_PROVIDER_ICONS: Record<string, string> = {
  codex: "https://www.google.com/s2/favicons?domain=openai.com&sz=64",
  claude: "https://www.google.com/s2/favicons?domain=claude.ai&sz=64",
  cursor: "https://www.google.com/s2/favicons?domain=cursor.com&sz=64",
  devin: "https://www.google.com/s2/favicons?domain=devin.ai&sz=64",
  gemini: "https://www.google.com/s2/favicons?domain=gemini.google.com&sz=64",
  copilot: "https://www.google.com/s2/favicons?domain=github.com&sz=64",
};

function channelOption(channelId: string) {
  return CHANNEL_OPTIONS.find((channel) => channel.id === channelId);
}

function App() {
  const [project, setProject] = React.useState<ProjectState | null>(null);
  const [step, setStep] = React.useState<AppStep>("url");
  const [websiteUrl, setWebsiteUrl] = React.useState("");
  const [agentProviders, setAgentProviders] = React.useState<
    AgentProviderStatus[]
  >([]);
  const [busy, setBusy] = React.useState(false);
  const [restoring, setRestoring] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const refreshProject = React.useCallback(async () => {
    if (!project) return;
    setProject(await api.loadProject(project.config.path));
  }, [project]);

  React.useEffect(() => {
    api
      .listAgentProviders()
      .then(setAgentProviders)
      .catch(() => undefined);
    let cancelled = false;
    api
      .loadLastProject()
      .then((lastProject) => {
        if (cancelled || !lastProject) return;
        setProject(lastProject);
        setWebsiteUrl(lastProject.config.websiteUrl);
        setStep(resumeStepForProject(lastProject));
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
  }, [project?.config.path, refreshProject]);

  async function startProjectWithAgent(providerId: string) {
    setBusy(true);
    setError(null);
    try {
      await api.selectAgentProvider(providerId);
      const next = await api.createProject(websiteUrl);
      setProject(next);
      setStep("brand");
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

  async function chooseBrowserProfile(profileId: string) {
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      setProject(await api.selectChromeProfile(project.config.path, profileId));
      setStep("channels");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function startChannelAnalysis(channelIds: string[]) {
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      let next = await api.setSelectedChannels(project.config.path, channelIds);
      for (const channelId of channelIds) {
        next = await api.verifyChannelLogin(
          project.config.path,
          channelId,
          next.chromeProfileId,
        );
      }
      setProject(next);
      const unauthenticated = channelIds.filter((channelId) => {
        const setup = next.channelSetups.find(
          (candidate) => candidate.id === channelId,
        );
        return setup?.accountStatus !== "authenticated";
      });
      if (unauthenticated.length) {
        setError(
          `Sign in to ${unauthenticated
            .map((channelId) => channelOption(channelId)?.name ?? channelId)
            .join(", ")} in the selected Chrome profile first.`,
        );
        return;
      }
      await api.runChannelAnalysis(project.config.path, channelIds);
      setProject(await api.loadProject(project.config.path));
      setStep("analysis");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function retryChannelAnalysis(channelId: string) {
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      await api.verifyChannelLogin(
        project.config.path,
        channelId,
        project.chromeProfileId,
      );
      await api.runChannelAnalysis(project.config.path, [channelId]);
      setProject(await api.loadProject(project.config.path));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  if (restoring) {
    return <main className="onboard" />;
  }

  if (step === "workspace" && project) {
    return <Workspace project={project} error={error} onError={setError} />;
  }

  const stepIndex = ONBOARDING_STEPS.indexOf(step as OnboardingStep);

  return (
    <OnboardingShell stepIndex={stepIndex} error={error}>
      {step === "url" ? (
        <UrlStep
          websiteUrl={websiteUrl}
          busy={busy}
          onChange={setWebsiteUrl}
          onContinue={() => {
            if (!websiteUrl.trim()) return;
            setError(null);
            setStep("agent");
          }}
        />
      ) : step === "agent" ? (
        <AgentStep
          providers={agentProviders}
          websiteUrl={websiteUrl}
          busy={busy}
          onBack={() => setStep("url")}
          onSelect={(providerId) => void startProjectWithAgent(providerId)}
        />
      ) : step === "brand" && project ? (
        <BrandAnalysisStep
          project={project}
          onContinue={() => {
            setError(null);
            setStep("browser");
          }}
        />
      ) : step === "browser" && project ? (
        <BrowserStep
          project={project}
          busy={busy}
          onError={setError}
          onSelect={(profileId) => void chooseBrowserProfile(profileId)}
        />
      ) : step === "channels" && project ? (
        <ChannelsStep
          project={project}
          busy={busy}
          onError={setError}
          onBack={() => setStep("browser")}
          onProjectUpdate={setProject}
          onStart={(channelIds) => void startChannelAnalysis(channelIds)}
        />
      ) : step === "analysis" && project ? (
        <ChannelAnalysisStep
          project={project}
          onError={setError}
          onRetry={(channelId) => void retryChannelAnalysis(channelId)}
          onFinish={() => {
            setError(null);
            setStep("workspace");
          }}
        />
      ) : null}
    </OnboardingShell>
  );
}

function resumeStepForProject(project: ProjectState): AppStep {
  if (!isBrandAnalysisComplete(project)) return "brand";
  if (!project.chromeProfileId) return "browser";
  const selected = project.selectedChannels;
  if (!selected.length) return "channels";
  const setups = selected.map((channelId) =>
    project.channelSetups.find((setup) => setup.id === channelId),
  );
  if (setups.every((setup) => setup?.analysisStatus === "ready")) {
    return "workspace";
  }
  if (
    setups.some(
      (setup) =>
        setup?.analysisStatus === "running" ||
        setup?.analysisStatus === "ready" ||
        setup?.analysisStatus === "failed",
    )
  ) {
    return "analysis";
  }
  return "channels";
}

function OnboardingShell({
  stepIndex,
  error,
  children,
}: {
  stepIndex: number;
  error: string | null;
  children: React.ReactNode;
}) {
  return (
    <main className="onboard">
      <section className="onboard-pane">
        <div className="onboard-brand">
          <BrandMark />
          <span>GTM Agent</span>
        </div>
        <div className="onboard-content">
          {children}
          {error ? <p className="onboard-error">{error}</p> : null}
        </div>
        <div className="onboard-dots" aria-label="Onboarding progress">
          {ONBOARDING_STEPS.map((step, index) => (
            <span
              className={[
                "onboard-dot",
                index === stepIndex ? "is-active" : "",
                index < stepIndex ? "is-done" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              key={step}
            />
          ))}
        </div>
      </section>
      <aside className="onboard-preview" aria-hidden="true">
        <div className="onboard-preview-mock">
          <div className="mock-topbar">
            <span className="mock-pill" />
            <span className="mock-pill mock-pill-short" />
          </div>
          <div className="mock-grid">
            <div className="mock-sidebar">
              <span className="mock-line mock-line-strong" />
              <span className="mock-line" />
              <span className="mock-line" />
              <span className="mock-line mock-line-short" />
            </div>
            <div className="mock-main">
              <span className="mock-line mock-line-strong" />
              <div className="mock-cards">
                <span className="mock-card" />
                <span className="mock-card" />
                <span className="mock-card" />
              </div>
              <span className="mock-line" />
              <span className="mock-line mock-line-short" />
            </div>
          </div>
        </div>
      </aside>
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

function UrlStep({
  websiteUrl,
  busy,
  onChange,
  onContinue,
}: {
  websiteUrl: string;
  busy: boolean;
  onChange: (value: string) => void;
  onContinue: () => void;
}) {
  return (
    <div className="onboard-step">
      <div className="onboard-copy">
        <h1>Analyze your brand</h1>
        <p>
          Enter the website you want to market. GTM Agent turns it into source
          documents for every channel.
        </p>
      </div>
      <div className="url-bar">
        <UrlIcon websiteUrl={websiteUrl} />
        <input
          autoFocus
          value={websiteUrl}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !busy) onContinue();
          }}
          placeholder="website.com"
        />
        <button onClick={onContinue} disabled={busy || !websiteUrl.trim()}>
          Continue
        </button>
      </div>
    </div>
  );
}

function AgentStep({
  providers,
  websiteUrl,
  busy,
  onBack,
  onSelect,
}: {
  providers: AgentProviderStatus[];
  websiteUrl: string;
  busy: boolean;
  onBack: () => void;
  onSelect: (providerId: string) => void;
}) {
  const visibleProviders = providers.filter(
    (provider) => provider.id !== "custom",
  );
  const [selectedAgentId, setSelectedAgentId] = React.useState<string | null>(
    null,
  );

  React.useEffect(() => {
    if (selectedAgentId) return;
    const preferred =
      visibleProviders.find(
        (provider) => provider.selected && provider.available,
      ) ?? visibleProviders.find((provider) => provider.available);
    if (preferred) setSelectedAgentId(preferred.id);
  }, [visibleProviders, selectedAgentId]);

  const selectedProvider = visibleProviders.find(
    (provider) => provider.id === selectedAgentId && provider.available,
  );

  return (
    <div className="onboard-step">
      <div className="onboard-copy">
        <h1>Select your agent</h1>
        <p>
          {displayHost(websiteUrl)} will be analyzed through the selected ACP
          agent. Any installed agent works.
        </p>
      </div>

      <div className="agent-provider-list">
        {visibleProviders.map((provider) => {
          const isSelected = selectedAgentId === provider.id;
          return (
            <button
              className={[
                "agent-provider-row",
                isSelected && provider.available ? "is-selected" : "",
                !provider.available ? "is-disabled" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              key={provider.id}
              type="button"
              onClick={() => {
                if (provider.available) setSelectedAgentId(provider.id);
              }}
              disabled={busy || !provider.available}
            >
              <img
                alt=""
                className="agent-provider-icon"
                src={
                  AGENT_PROVIDER_ICONS[provider.id] ??
                  AGENT_PROVIDER_ICONS.codex
                }
              />
              <span className="agent-provider-main">
                <strong>{provider.title}</strong>
                {!provider.available ? (
                  <em>Install `{provider.command}` to enable</em>
                ) : provider.version ? (
                  <em>{compactVersion(provider.version)}</em>
                ) : null}
              </span>
              <span
                className={
                  provider.available ? "agent-ready" : "agent-unavailable"
                }
              >
                {provider.available ? "Installed" : "Not installed"}
              </span>
            </button>
          );
        })}
      </div>

      <div className="onboard-actions">
        <button className="secondary" type="button" onClick={onBack}>
          Back
        </button>
        <button
          type="button"
          onClick={() => selectedProvider && onSelect(selectedProvider.id)}
          disabled={busy || !selectedProvider}
        >
          {busy ? "Starting..." : "Continue"}
        </button>
      </div>
    </div>
  );
}

function BrandAnalysisStep({
  project,
  onContinue,
}: {
  project: ProjectState;
  onContinue: () => void;
}) {
  const [isLogOpen, setIsLogOpen] = React.useState(false);
  const logRef = React.useRef<HTMLDivElement | null>(null);
  const run = project.latestRun;
  const isRunning =
    run?.kind === "initial_analysis" && run?.status === "running";
  const isComplete = isBrandAnalysisComplete(project);
  const runError =
    run?.kind === "initial_analysis" ? (run?.error ?? null) : null;
  const competitors = extractCompetitors(
    project.docs,
    displayHost(project.config.websiteUrl),
  );
  const steps = brandAnalysisSteps(
    project.docs,
    competitors,
    isRunning,
    isComplete,
  );
  const agentOutput = agentOutputActivity(project.runActivity);
  const productDescription = extractProductDescription(project.docs);

  React.useEffect(() => {
    if (!isLogOpen || !logRef.current) return;
    logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [isLogOpen, agentOutput.length, agentOutput.at(-1)?.message]);

  return (
    <div className="onboard-step">
      <div className="onboard-copy">
        <h1>{isComplete ? "Brand analysis ready" : "Analyzing your brand"}</h1>
        <p>
          {isComplete
            ? productDescription
            : `${project.agentProvider.title} is researching ${displayHost(
                project.config.websiteUrl,
              )} and writing the GTM source documents.`}
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

      {runError ? <p className="run-error">{runError}</p> : null}

      <div className="onboard-actions">
        <button
          className="secondary agent-log-toggle"
          type="button"
          onClick={() => setIsLogOpen((open) => !open)}
        >
          {isLogOpen ? "Hide agent log" : "Show agent log"}
        </button>
        <button type="button" onClick={onContinue} disabled={!isComplete}>
          Continue
        </button>
      </div>

      {isLogOpen ? (
        <div className="agent-log-panel">
          <div className="activity-list agent-log-list" ref={logRef}>
            {agentOutput.length ? (
              agentOutput.map((item, index) => (
                <article
                  className="activity-item"
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
  );
}

function BrowserStep({
  project,
  busy,
  onError,
  onSelect,
}: {
  project: ProjectState;
  busy: boolean;
  onError: (error: string | null) => void;
  onSelect: (profileId: string) => void;
}) {
  const [profiles, setProfiles] = React.useState<ChromeProfile[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [selectedProfileId, setSelectedProfileId] = React.useState<
    string | null
  >(project.chromeProfileId ?? null);

  React.useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    api
      .listChromeProfiles()
      .then((next) => {
        if (cancelled) return;
        setProfiles(next);
        setSelectedProfileId(
          (current) =>
            current ??
            next.find((profile) => profile.isRecommended)?.id ??
            next[0]?.id ??
            null,
        );
      })
      .catch((err) => {
        if (!cancelled) onError(String(err));
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="onboard-step">
      <div className="onboard-copy">
        <h1>Choose your browser profile</h1>
        <p>
          GTM Agent works through your signed-in Chrome profile. Pick the one
          with the accounts you use for {project.config.name}.
        </p>
      </div>

      <div className="browser-profile-list">
        {isLoading ? (
          <p className="empty-note">Loading Chrome profiles...</p>
        ) : profiles.length === 0 ? (
          <p className="empty-note">
            No Chrome profiles found. Install Google Chrome and sign in first.
          </p>
        ) : (
          profiles.map((profile) => {
            const detectedChannels = CHANNEL_OPTIONS.filter(
              (channel) => profile.sessions?.[channel.id],
            );
            return (
              <button
                className={[
                  "browser-profile-row",
                  profile.id === selectedProfileId ? "is-selected" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                key={profile.id}
                type="button"
                onClick={() => setSelectedProfileId(profile.id)}
                disabled={busy}
              >
                <ChromeProfileAvatar profile={profile} />
                <span className="browser-profile-main">
                  <strong>{profile.name}</strong>
                  <em>{profileSubtitle(profile)}</em>
                </span>
                <span className="browser-profile-sessions">
                  {detectedChannels.length ? (
                    detectedChannels.map((channel) => (
                      <span
                        className="browser-session-chip"
                        key={channel.id}
                        title={`${channel.name} account detected`}
                      >
                        <UrlIcon websiteUrl={channel.faviconUrl} />
                      </span>
                    ))
                  ) : (
                    <em>No accounts detected</em>
                  )}
                </span>
              </button>
            );
          })
        )}
      </div>

      <div className="onboard-actions">
        <span />
        <button
          type="button"
          onClick={() => selectedProfileId && onSelect(selectedProfileId)}
          disabled={busy || !selectedProfileId}
        >
          {busy ? "Saving..." : "Continue"}
        </button>
      </div>
    </div>
  );
}

function ChannelsStep({
  project,
  busy,
  onError,
  onBack,
  onProjectUpdate,
  onStart,
}: {
  project: ProjectState;
  busy: boolean;
  onError: (error: string | null) => void;
  onBack: () => void;
  onProjectUpdate: (project: ProjectState) => void;
  onStart: (channelIds: string[]) => void;
}) {
  const [profiles, setProfiles] = React.useState<ChromeProfile[]>([]);
  const [checkingChannelId, setCheckingChannelId] = React.useState<
    string | null
  >(null);
  const [selectedChannelIds, setSelectedChannelIds] = React.useState<string[]>(
    project.selectedChannels,
  );
  const profileId = project.chromeProfileId ?? null;
  const selectedProfile = profiles.find((profile) => profile.id === profileId);

  const refreshProfiles = React.useCallback(() => {
    api
      .listChromeProfiles()
      .then(setProfiles)
      .catch((err) => onError(String(err)));
  }, [onError]);

  React.useEffect(() => {
    refreshProfiles();
  }, [refreshProfiles]);

  React.useEffect(() => {
    if (selectedChannelIds.length || !selectedProfile) return;
    const detected = CHANNEL_OPTIONS.filter(
      (channel) => selectedProfile.sessions?.[channel.id],
    ).map((channel) => channel.id);
    if (detected.length) setSelectedChannelIds(detected);
  }, [selectedProfile]);

  function channelDetected(channelId: string) {
    const setup = project.channelSetups.find(
      (candidate) => candidate.id === channelId,
    );
    if (
      setup?.accountStatus === "authenticated" &&
      setup?.chromeProfileId === profileId
    ) {
      return true;
    }
    return Boolean(selectedProfile?.sessions?.[channelId]);
  }

  async function checkChannel(channelId: string) {
    setCheckingChannelId(channelId);
    onError(null);
    try {
      onProjectUpdate(
        await api.verifyChannelLogin(project.config.path, channelId, profileId),
      );
      refreshProfiles();
    } catch (err) {
      onError(String(err));
    } finally {
      setCheckingChannelId(null);
    }
  }

  async function signIn(channelId: string) {
    onError(null);
    try {
      await api.openChannelLogin(channelId, profileId);
    } catch (err) {
      onError(String(err));
    }
  }

  function toggleChannel(channelId: string) {
    setSelectedChannelIds((current) =>
      current.includes(channelId)
        ? current.filter((candidate) => candidate !== channelId)
        : [...current, channelId],
    );
  }

  const missingLogins = selectedChannelIds.filter(
    (channelId) => !channelDetected(channelId),
  );

  return (
    <div className="onboard-step">
      <div className="onboard-copy">
        <h1>Pick your channels</h1>
        <p>
          Choose where {project.config.name} should show up. GTM Agent checks
          each account in the selected Chrome profile
          {selectedProfile ? ` (${selectedProfile.name})` : ""}.
        </p>
      </div>

      <div className="channel-select-list">
        {CHANNEL_OPTIONS.map((channel) => {
          const isSelected = selectedChannelIds.includes(channel.id);
          const detected = channelDetected(channel.id);
          const isChecking = checkingChannelId === channel.id;
          return (
            <div
              className={["channel-select-row", isSelected ? "is-selected" : ""]
                .filter(Boolean)
                .join(" ")}
              key={channel.id}
            >
              <button
                className="channel-select-main"
                type="button"
                onClick={() => toggleChannel(channel.id)}
                disabled={busy}
                aria-pressed={isSelected}
              >
                <span className="channel-select-check" aria-hidden="true" />
                <UrlIcon websiteUrl={channel.faviconUrl} />
                <span className="channel-select-copy">
                  <strong>{channel.name}</strong>
                  <em>{channel.description}</em>
                </span>
              </button>
              <div className="channel-select-status">
                <span
                  className={
                    detected
                      ? "channel-account-status is-detected"
                      : "channel-account-status"
                  }
                >
                  {isChecking
                    ? "Checking..."
                    : detected
                      ? "Account detected"
                      : "No account detected"}
                </span>
                {!detected ? (
                  <>
                    <button
                      className="secondary channel-inline-action"
                      type="button"
                      onClick={() => void signIn(channel.id)}
                      disabled={busy || isChecking}
                    >
                      Sign in
                    </button>
                    <button
                      className="secondary channel-inline-action"
                      type="button"
                      onClick={() => void checkChannel(channel.id)}
                      disabled={busy || isChecking}
                    >
                      Check
                    </button>
                  </>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>

      <div className="onboard-actions">
        <button className="secondary" type="button" onClick={onBack}>
          Back
        </button>
        <button
          type="button"
          onClick={() => onStart(selectedChannelIds)}
          disabled={
            busy || !selectedChannelIds.length || missingLogins.length > 0
          }
        >
          {busy
            ? "Starting..."
            : missingLogins.length
              ? "Sign in to continue"
              : "Start analysis"}
        </button>
      </div>
    </div>
  );
}

function ChannelAnalysisStep({
  project,
  onError,
  onRetry,
  onFinish,
}: {
  project: ProjectState;
  onError: (error: string | null) => void;
  onRetry: (channelId: string) => void;
  onFinish: () => void;
}) {
  const [selectedDoc, setSelectedDoc] = React.useState<ContextDoc | null>(null);
  const selectedChannels = project.selectedChannels.length
    ? project.selectedChannels
    : project.channelSetups
        .filter((setup) => setup.analysisStatus !== "not_started")
        .map((setup) => setup.id);
  const setups = selectedChannels
    .map((channelId) =>
      project.channelSetups.find((setup) => setup.id === channelId),
    )
    .filter((setup): setup is ChannelSetup => Boolean(setup));
  const allReady =
    setups.length > 0 &&
    setups.every((setup) => setup.analysisStatus === "ready");
  const runError =
    project.latestRun?.kind === "channel_analysis"
      ? (project.latestRun?.error ?? null)
      : null;

  async function openChannelDoc(channelId: string, fileName: string) {
    onError(null);
    try {
      setSelectedDoc(
        await api.loadChannelContextDoc(
          project.config.path,
          channelId,
          fileName,
        ),
      );
    } catch (err) {
      onError(String(err));
    }
  }

  return (
    <div className="onboard-step">
      <div className="onboard-copy">
        <h1>{allReady ? "Channels ready" : "Preparing your channels"}</h1>
        <p>
          {allReady
            ? "Channel memory is ready for draft-first outreach. You can review each file below."
            : `${project.agentProvider.title} opens a dedicated Chrome window with your signed-in accounts and writes the channel memory files. You can watch it work.`}
        </p>
      </div>

      <div className="channel-analysis-list">
        {setups.map((setup) => {
          const option = channelOption(setup.id);
          const isRunning = setup.analysisStatus === "running";
          const isReady = setup.analysisStatus === "ready";
          const isFailed = setup.analysisStatus === "failed";
          return (
            <article className="channel-analysis-card" key={setup.id}>
              <div className="channel-analysis-head">
                <UrlIcon websiteUrl={option?.faviconUrl ?? ""} />
                <strong>{setup.name}</strong>
                <span
                  className={[
                    "channel-analysis-chip",
                    isReady ? "is-ready" : "",
                    isFailed ? "is-failed" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                >
                  {isRunning
                    ? "Analyzing..."
                    : isReady
                      ? "Ready"
                      : isFailed
                        ? "Failed"
                        : "Queued"}
                </span>
              </div>
              <div className="channel-analysis-files">
                {CHANNEL_DOCS.map((doc) => {
                  const exists = setup.files.includes(doc.fileName);
                  return (
                    <button
                      className="channel-file-chip"
                      key={doc.fileName}
                      type="button"
                      onClick={() =>
                        void openChannelDoc(setup.id, doc.fileName)
                      }
                      disabled={!exists}
                    >
                      <span className="document-icon" aria-hidden="true">
                        <svg viewBox="0 0 16 16" focusable="false">
                          <path d="M4 1.75h5.2L12.75 5.3v8.95H4z" />
                          <path d="M9 1.9v3.6h3.55M6 8h4M6 10.5h4" />
                        </svg>
                      </span>
                      {doc.title}
                    </button>
                  );
                })}
              </div>
              {isRunning ? (
                <div className="analyzing-shimmer">Analyzing account...</div>
              ) : isFailed ? (
                <button
                  className="secondary channel-inline-action"
                  type="button"
                  onClick={() => onRetry(setup.id)}
                >
                  Retry
                </button>
              ) : null}
            </article>
          );
        })}
      </div>

      {runError ? <p className="run-error">{runError}</p> : null}

      <div className="onboard-actions">
        <span />
        <button type="button" onClick={onFinish} disabled={!allReady}>
          Finish
        </button>
      </div>

      {selectedDoc ? (
        <DocModal doc={selectedDoc} onClose={() => setSelectedDoc(null)} />
      ) : null}
    </div>
  );
}

function Workspace({
  project,
  error,
  onError,
}: {
  project: ProjectState;
  error: string | null;
  onError: (error: string | null) => void;
}) {
  const [selectedDoc, setSelectedDoc] = React.useState<ContextDoc | null>(null);
  const host = displayHost(project.config.websiteUrl);
  const productDescription = extractProductDescription(project.docs);
  const competitors = extractCompetitors(project.docs, host);
  const readyChannels = project.channelSetups.filter(
    (setup) =>
      project.selectedChannels.includes(setup.id) &&
      setup.analysisStatus === "ready",
  );

  async function openChannelDoc(channelId: string, fileName: string) {
    onError(null);
    try {
      setSelectedDoc(
        await api.loadChannelContextDoc(
          project.config.path,
          channelId,
          fileName,
        ),
      );
    } catch (err) {
      onError(String(err));
    }
  }

  return (
    <main className="shell">
      {error ? <div className="error">{error}</div> : null}
      <section className="workspace workspace-final">
        <div className="analysis-grid">
          <aside className="panel documents-card" aria-label="Company context">
            <div className="company-lockup">
              <UrlIcon websiteUrl={project.config.websiteUrl} />
              <div>
                <strong>{project.config.name}</strong>
              </div>
            </div>

            <div className="documents-body">
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

          <section className="workspace-main" aria-label="GTM workspace">
            <div className="channel-header">
              <p className="eyebrow">Dashboard</p>
              <h2>Daily GTM workspace</h2>
              <p>
                Your configured channels will turn into recurring research,
                drafts, approvals, and browser-assisted posting runs.
              </p>
            </div>

            <div className="dashboard-status-list">
              {readyChannels.length ? (
                readyChannels.map((setup) => {
                  const option = channelOption(setup.id);
                  return (
                    <article className="dashboard-status-card" key={setup.id}>
                      <div className="dashboard-status-head">
                        <UrlIcon websiteUrl={option?.faviconUrl ?? ""} />
                        <strong>{setup.name}</strong>
                        <span>Ready for draft-first runs</span>
                      </div>
                      <div className="channel-analysis-files">
                        {CHANNEL_DOCS.map((doc) => (
                          <button
                            className="channel-file-chip"
                            key={doc.fileName}
                            type="button"
                            onClick={() =>
                              void openChannelDoc(setup.id, doc.fileName)
                            }
                            disabled={!setup.files.includes(doc.fileName)}
                          >
                            <span className="document-icon" aria-hidden="true">
                              <svg viewBox="0 0 16 16" focusable="false">
                                <path d="M4 1.75h5.2L12.75 5.3v8.95H4z" />
                                <path d="M9 1.9v3.6h3.55M6 8h4M6 10.5h4" />
                              </svg>
                            </span>
                            {doc.title}
                          </button>
                        ))}
                      </div>
                    </article>
                  );
                })
              ) : (
                <p className="empty-note">
                  No channels are ready yet. Finish channel analysis first.
                </p>
              )}
            </div>
          </section>
        </div>

        {selectedDoc ? (
          <DocModal doc={selectedDoc} onClose={() => setSelectedDoc(null)} />
        ) : null}
      </section>
    </main>
  );
}

function DocModal({ doc, onClose }: { doc: ContextDoc; onClose: () => void }) {
  return (
    <div className="doc-modal-backdrop" onClick={onClose}>
      <article
        className="doc-modal"
        onClick={(event) => event.stopPropagation()}
      >
        <button
          className="modal-close"
          type="button"
          aria-label="Close"
          onClick={onClose}
        >
          ×
        </button>
        <p className="label">{doc.fileName}</p>
        <h2>{doc.title}</h2>
        <RenderedDoc content={doc.content} full />
      </article>
    </div>
  );
}

type AnalysisStepStatus = "done" | "active" | "pending";

type AnalysisStep = {
  title: string;
  detail: string;
  statusLabel: string;
  status: AnalysisStepStatus;
};

function brandAnalysisSteps(
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

function docByKey(docs: ContextDoc[], key: string) {
  const fileKey = key.replaceAll("_", "-");
  return docs.find((doc) => doc.key === key || doc.fileName.includes(fileKey));
}

function shouldRunInitialAnalysis(project: ProjectState) {
  if (!project.latestRun) return true;
  if (project.latestRun.status === "failed") return true;
  return project.docs.some((doc) => !hasDocumentContent(doc));
}

function isBrandAnalysisComplete(project: ProjectState) {
  const isRunning =
    project.latestRun?.kind === "initial_analysis" &&
    project.latestRun?.status === "running";
  return !isRunning && project.docs.every(hasDocumentContent);
}

function hasDocumentContent(doc: ContextDoc) {
  return doc.content.trim() !== `# ${doc.title}`;
}

function compactVersion(version: string) {
  return version.split("\n")[0]?.slice(0, 40) ?? version;
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
