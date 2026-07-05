export interface AgentProviderStatus {
  id: string;
  title: string;
  command: string;
  args: string[];
  enabled: boolean;
  selected: boolean;
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
  kind: "initial_analysis" | "x_account_analysis";
  status: "running" | "completed" | "failed";
  providerId?: string | null;
  providerTitle?: string | null;
  externalSessionId?: string | null;
  codexThreadId?: string | null;
  startedAt: string;
  completedAt?: string | null;
  logPath: string;
  error?: string | null;
}

export interface RunActivity {
  kind: string;
  title: string;
  message: string;
}

export interface ChannelSetup {
  id: string;
  name: string;
  status: "not_configured" | "needs_login" | "analyzing" | "ready" | "failed";
  accountStatus:
    "not_configured" | "checking" | "authenticated" | "needs_login" | "unknown";
  loginStatus: "unknown" | "needs_login" | "verified";
  analysisStatus: "not_started" | "running" | "ready" | "failed";
  accountLabel?: string | null;
  accountHandle?: string | null;
  accountAvatarUrl?: string | null;
  chromeProfileId?: string | null;
  checkMethod?: string | null;
  checkedAt?: string | null;
  path: string;
  files: string[];
}

export interface ChromeProfile {
  id: string;
  name: string;
  email?: string | null;
  accountName?: string | null;
  avatarPath?: string | null;
  avatarDataUrl?: string | null;
  profileColor?: number | null;
  hasXSession: boolean;
  isRecommended: boolean;
  isDefault: boolean;
}

export interface ProjectState {
  config: ProjectConfig;
  agentProvider: AgentProviderStatus;
  docs: ContextDoc[];
  channelSetups: ChannelSetup[];
  latestRun?: RunState | null;
  runActivity: RunActivity[];
}
