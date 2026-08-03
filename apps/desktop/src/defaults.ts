import type { AppSettings, RuntimeStatus } from "./types";

export const defaultSettings: AppSettings = {
  dictationHotkey: {
    modifiers: ["Ctrl"],
    key: "Space",
    behavior: "hold",
  },
  scribeHotkey: {
    modifiers: ["Ctrl", "Shift"],
    key: "Space",
    behavior: "hold",
  },
  audioInputDevice: null,
  whisperModel: "base.en",
  backend: "cuda",
  language: "en",
  defaultRegister: "generic",
  cleanupProvider: "auto",
  cleanupModel: "",
  cleanupBaseUrl: "http://127.0.0.1:11434",
  trailingBufferMs: 1500,
  launchAtStartup: false,
  keepRecoveryAudio: true,
  injectionMode: "clipboard",
  dictionary: [],
  dismissedSuggestions: [],
};

export const initialRuntimeStatus: RuntimeStatus = {
  state: "ready",
  mode: null,
  message: "Ready",
  provider: null,
};

export function formatHotkey(settings: AppSettings, mode: "dictation" | "scribe") {
  const hotkey = mode === "dictation" ? settings.dictationHotkey : settings.scribeHotkey;
  return [...hotkey.modifiers, hotkey.key].join(" + ");
}
