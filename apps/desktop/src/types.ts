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
  defaultRegister: Register;
  cleanupProvider: "auto" | "ollama" | "openai-compatible" | "disabled";
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
  audioPath: string | null;
}
