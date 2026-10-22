import type { AppSettings, HotkeyConfig, RuntimeStatus } from "./types";
import { detectPlatform } from "./platform";

const mac = detectPlatform() === "mac";

export const defaultSettings: AppSettings = {
  dictationHotkey: {
    modifiers: mac ? [] : ["Ctrl"],
    key: mac ? "AltRight" : "Space",
    behavior: "hold",
  },
  scribeHotkey: {
    modifiers: mac ? [] : ["Ctrl", "Shift"],
    key: mac ? "MetaRight" : "Space",
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
  dictionary: [],
  dismissedSuggestions: [],
  speechModelSetupAttempted: false,
  scribeSetupDismissed: false,
  onboardingCompleted: false,
  scribeContextEnabled: false,
  styleProfiles: [],
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
    AltRight: "Right Option",
    MetaRight: "Right Command",
    Meta: "⌘",
    Ctrl: "⌃",
    Alt: "⌥",
    Shift: "⇧",
  };
  return [...hotkey.modifiers, hotkey.key]
    .map((part) => (detectPlatform() === "mac" ? macLabels[part] ?? part : part))
    .join(" + ");
}

export function isStandaloneMacHotkey(hotkey: HotkeyConfig): boolean {
  return detectPlatform() === "mac" && hotkey.modifiers.length === 0
    && (hotkey.key === "AltRight" || hotkey.key === "MetaRight");
}
