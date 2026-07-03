export interface CodexDetection {
  available: boolean;
  path?: string | null;
  version?: string | null;
  error?: string | null;
}

export interface ProjectConfig {
  id: string;
  name: string;
  websiteUrl: string;
  path: string;
  createdAt: string;
  updatedAt: string;
}

export interface ContextDoc {
  key: string;
  fileName: string;
  title: string;
  content: string;
}

export interface RunState {
  id: string;
  kind: "initial_analysis";
  status: "running" | "completed" | "failed";
  codexThreadId?: string | null;
  startedAt: string;
  completedAt?: string | null;
  logPath: string;
  error?: string | null;
}

export interface ProjectState {
  config: ProjectConfig;
  codex: CodexDetection;
  docs: ContextDoc[];
  latestRun?: RunState | null;
}
