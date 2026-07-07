import { invoke } from "@tauri-apps/api/core";
import type {
  AgentProviderStatus,
  ChromeProfile,
  ContextDoc,
  ProjectState,
  RunState,
} from "./types";

function hasTauriBridge() {
  return "__TAURI_INTERNALS__" in window;
}

function call<T>(command: string, args?: Record<string, unknown>) {
  if (!hasTauriBridge()) {
    return Promise.reject(
      new Error("Tauri backend unavailable in browser preview."),
    );
  }
  return invoke<T>(command, args);
}

export const api = {
  detectAgentProvider() {
    if (!hasTauriBridge()) {
      return Promise.resolve<AgentProviderStatus>({
        id: "desktop",
        title: "Agent",
        command: "",
        args: [],
        enabled: false,
        selected: false,
        available: false,
        error: "Open the desktop app to use an agent.",
      });
    }
    return call<AgentProviderStatus>("detect_agent_provider");
  },
  listAgentProviders() {
    if (!hasTauriBridge()) {
      return Promise.resolve<AgentProviderStatus[]>([
        {
          id: "codex",
          title: "Codex",
          command: "npx",
          args: ["-y", "@agentclientprotocol/codex-acp"],
          enabled: true,
          selected: true,
          available: false,
          error: "Open the desktop app to use an agent.",
        },
        {
          id: "claude",
          title: "Claude Code",
          command: "npx",
          args: ["-y", "@agentclientprotocol/claude-agent-acp"],
          enabled: true,
          selected: false,
          available: false,
          error: "Open the desktop app to use an agent.",
        },
        {
          id: "cursor",
          title: "Cursor",
          command: "cursor-agent",
          args: ["acp"],
          enabled: true,
          selected: false,
          available: false,
          error: "Open the desktop app to use an agent.",
        },
        {
          id: "devin",
          title: "Devin",
          command: "devin",
          args: ["acp"],
          enabled: true,
          selected: false,
          available: false,
          error: "Open the desktop app to use an agent.",
        },
        {
          id: "gemini",
          title: "Gemini",
          command: "gemini",
          args: ["--acp"],
          enabled: true,
          selected: false,
          available: false,
          error: "Open the desktop app to use an agent.",
        },
        {
          id: "copilot",
          title: "Copilot",
          command: "copilot",
          args: ["--acp", "--stdio"],
          enabled: true,
          selected: false,
          available: false,
          error: "Open the desktop app to use an agent.",
        },
      ]);
    }
    return call<AgentProviderStatus[]>("list_agent_providers");
  },
  selectAgentProvider(providerId: string) {
    return call<AgentProviderStatus>("select_agent_provider", { providerId });
  },
  defaultProjectPath(websiteUrl: string) {
    return call<string>("default_project_path", { websiteUrl });
  },
  createProject(websiteUrl: string) {
    return call<ProjectState>("create_project", {
      request: { websiteUrl },
    });
  },
  loadLastProject() {
    if (!hasTauriBridge()) {
      return Promise.resolve<ProjectState | null>(null);
    }
    return call<ProjectState | null>("load_last_project");
  },
  loadProject(projectPath: string) {
    return call<ProjectState>("load_project", { projectPath });
  },
  loadChannelContextDoc(
    projectPath: string,
    channelId: string,
    fileName: string,
  ) {
    return call<ContextDoc>("load_channel_context_doc", {
      projectPath,
      channelId,
      fileName,
    });
  },
  runInitialAnalysis(projectPath: string) {
    return call<RunState>("run_initial_analysis", { projectPath });
  },
  configureChannel(projectPath: string, channelId: string) {
    return call<ProjectState>("configure_channel", { projectPath, channelId });
  },
  listChromeProfiles() {
    return call<ChromeProfile[]>("list_chrome_profiles");
  },
  selectChromeProfile(projectPath: string, profileId: string) {
    return call<ProjectState>("select_chrome_profile", {
      projectPath,
      profileId,
    });
  },
  setSelectedChannels(projectPath: string, channelIds: string[]) {
    return call<ProjectState>("set_selected_channels", {
      projectPath,
      channelIds,
    });
  },
  verifyChannelLogin(
    projectPath: string,
    channelId: string,
    profileId?: string | null,
  ) {
    return call<ProjectState>("verify_channel_login", {
      projectPath,
      channelId,
      profileId,
    });
  },
  runChannelAnalysis(projectPath: string, channelIds: string[]) {
    return call<RunState>("run_channel_analysis", { projectPath, channelIds });
  },
  openProjectInCodex(projectPath: string) {
    return call<void>("open_project_in_codex", { projectPath });
  },
  openExternalUrl(url: string) {
    return call<void>("open_external_url", { url });
  },
  openChromeUrl(url: string) {
    return call<void>("open_chrome_url", { url });
  },
  openChannelLogin(channelId: string, profileId?: string | null) {
    return call<void>("open_channel_login", { channelId, profileId });
  },
};
