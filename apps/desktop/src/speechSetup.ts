import type { SpeechSetupState } from "./components/FirstRunSetup";
import { modelDownloadBytes } from "./onboarding";
import type { AppSettings, ProviderKeyStatus } from "./types";

/** Rebind setup feedback when the user changes provider/model during startup. */
export function speechSetupFor(
  settings: AppSettings, installed: string[], statuses: ProviderKeyStatus[],
): SpeechSetupState {
  if (settings.transcriptionProvider === "local") {
    return {
      phase: installed.includes(settings.whisperModel) ? "ready" : "missing",
      modelId: settings.whisperModel,
      bytesDownloaded: 0, bytesTotal: modelDownloadBytes(settings.whisperModel),
      error: null,
    };
  }
  const status = statuses.find((item) => item.provider === settings.transcriptionProvider);
  const unchecked = !status || status.status === "unchecked";
  return {
    phase: unchecked ? "checking" : status.configured && status.status !== "error" ? "ready" : "error",
    modelId: settings.transcriptionProvider === "gemini" ? "Gemini 3.5 Transcribe" : "Groq Cloud",
    bytesDownloaded: 0, bytesTotal: 0,
    error: unchecked ? null : status.message ?? (status.configured ? null : "Connect your selected speech provider in Voice settings."),
  };
}
