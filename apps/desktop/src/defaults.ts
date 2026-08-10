import type { AppSettings, RuntimeStatus } from "./types";
import { detectPlatform } from "./platform";

const mac = detectPlatform() === "mac";

export const defaultSettings: AppSettings = {
  dictationHotkey: {
    modifiers: mac ? ["Meta", "Shift"] : ["Ctrl"],
    key: mac ? "D" : "Space",
    behavior: "hold",
  },
  scribeHotkey: {
    modifiers: mac ? ["Meta", "Shift"] : ["Ctrl", "Shift"],
    key: mac ? "S" : "Space",
    behavior: "hold",
  },
  audioInputDevice: null,
  whisperModel: "base.en",
  backend: "auto",
  language: "en",
  transcriptionProvider: "local",
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
  speechModelSetupAttempted: false,
  scribeSetupDismissed: false,
  onboardingCompleted: false,
};

export const initialRuntimeStatus: RuntimeStatus = {
  state: "ready",
  mode: null,
  message: "Ready",
  provider: null,
};

export function formatHotkey(settings: AppSettings, mode: "dictation" | "scribe") {
  const hotkey = mode === "dictation" ? settings.dictationHotkey : settings.scribeHotkey;
  const macLabels: Record<string, string> = {
    Meta: "⌘",
    Ctrl: "⌃",
    Alt: "⌥",
    Shift: "⇧",
  };
  return [...hotkey.modifiers, hotkey.key]
    .map((part) => (detectPlatform() === "mac" ? macLabels[part] ?? part : part))
    .join(" + ");
}
