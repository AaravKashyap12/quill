export type Mode = "dictation" | "scribe";
export type Register = "email" | "chat" | "prompt" | "notes" | "generic";
export type NavigationSection = "general" | "voice" | "dictionary" | "about";

export interface HotkeyConfig {
  modifiers: string[];
  key: string;
  behavior: "hold" | "tap-to-lock";
}

export interface DictionaryEntry {
  id: string;
  spoken: string;
  replacement: string;
  kind: "word" | "snippet";
}

export interface DictionarySuggestion {
  spoken: string;
  replacement: string;
}

export interface AppSettings {
  dictationHotkey: HotkeyConfig;
  scribeHotkey: HotkeyConfig;
  audioInputDevice: string | null;
  whisperModel: string;
  backend: "auto" | "cpu" | "cuda" | "metal";
  language: string;
  transcriptionProvider: "local" | "groq";
  defaultRegister: Register;
  cleanupProvider: "auto" | "ollama" | "openai-compatible" | "gemini" | "disabled";
  cleanupModel: string;
  cleanupBaseUrl: string;
  trailingBufferMs: number;
  launchAtStartup: boolean;
  keepRecoveryAudio: boolean;
  injectionMode: "clipboard" | "keystrokes";
  dictionary: DictionaryEntry[];
  dismissedSuggestions: DictionarySuggestion[];
  speechModelSetupAttempted: boolean;
  scribeSetupDismissed: boolean;
  onboardingCompleted: boolean;
}

export type CloudProvider = "groq" | "gemini";
export interface ProviderKeyStatus {
  provider: CloudProvider;
  configured: boolean;
  status: "missing" | "configured" | "connected" | "error";
  message: string | null;
}

export interface SystemProfile {
  totalMemoryBytes: number;
  availableMemoryBytes: number;
  logicalCpuCount: number;
  platform: string;
  architecture: string;
  speechAcceleration: "cpu" | "metal" | "cuda";
}

export interface ProviderStatus {
  kind: "ollama" | "openai-compatible";
  baseUrl: string;
  available: boolean;
  models: string[];
}

export interface CudaRuntimeStatus {
  state: "missing" | "installed" | "invalid";
  expectedRevision: string;
  downloadBytes: number;
  error: string | null;
}

export interface RuntimeStatus {
  state: "ready" | "listening" | "processing" | "error";
  mode: Mode | null;
  message: string;
  provider: string | null;
}

export interface AppUpdateInfo {
  version: string;
  currentVersion: string;
}

export type AppUpdateEvent =
  | { event: "Started"; data: { contentLength: number | null } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Downloaded" };

export interface ScribeReviewDraft {
  id: number;
  source: string;
  draft: string;
  warning: string | null;
  register: Register;
}

export interface RecoveryManifest {
  id: string;
  updatedAtUnixMs: number;
  mode: "dictation" | "scribe";
  transcript: string;
  failedProvider: string | null;
  audioPath: string | null;
}
