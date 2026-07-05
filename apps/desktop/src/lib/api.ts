import { invoke } from "@tauri-apps/api/core";
import type {
  AgentProviderStatus,
  ChromeProfile,
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
        available: false,
        error: "Open the desktop app to use an agent.",
      });
    }
    return call<AgentProviderStatus>("detect_agent_provider");
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
  runInitialAnalysis(projectPath: string) {
    return call<RunState>("run_initial_analysis", { projectPath });
  },
  configureChannel(projectPath: string, channelId: string) {
    return call<ProjectState>("configure_channel", { projectPath, channelId });
  },
  listChromeProfiles() {
    return call<ChromeProfile[]>("list_chrome_profiles");
  },
  verifyXLogin(projectPath: string, profileId?: string | null) {
    return call<ProjectState>("verify_x_login", { projectPath, profileId });
  },
  runXAccountAnalysis(projectPath: string) {
    return call<RunState>("run_x_account_analysis", { projectPath });
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
  openXLogin(profileId?: string | null) {
    return call<void>("open_x_login", { profileId });
  },
};
