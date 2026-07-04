import { invoke } from "@tauri-apps/api/core";
import type { CodexDetection, ProjectState, RunState } from "./types";

function hasTauriBridge() {
  return "__TAURI_INTERNALS__" in window;
}

function call<T>(command: string, args?: Record<string, unknown>) {
  if (!hasTauriBridge()) {
    return Promise.reject(new Error("Tauri backend unavailable in browser preview."));
  }
  return invoke<T>(command, args);
}

export const api = {
  detectCodex() {
    if (!hasTauriBridge()) {
      return Promise.resolve<CodexDetection>({
        available: false,
        error: "Open the desktop app to use Codex.",
      });
    }
    return call<CodexDetection>("detect_codex");
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
  openProjectInCodex(projectPath: string) {
    return call<void>("open_project_in_codex", { projectPath });
  },
  openExternalUrl(url: string) {
    return call<void>("open_external_url", { url });
  },
};
