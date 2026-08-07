import { Channel, invoke } from "@tauri-apps/api/core";
import { isTauri } from "@tauri-apps/api/core";
import { defaultSettings } from "./defaults";
import { capDismissedSuggestions } from "./dictionary";
import type {
  AppSettings,
  AppUpdateEvent,
  AppUpdateInfo,
  CudaRuntimeStatus,
  DictionarySuggestion,
  Mode,
  ProviderStatus,
  Register,
  RecoveryManifest,
  ScribeReviewDraft,
  SystemProfile,
} from "./types";

export async function checkAppUpdate(): Promise<AppUpdateInfo | null> {
  if (!isTauri()) {
    return new URLSearchParams(window.location.search).has("update")
      ? { version: "0.2.2", currentVersion: "0.2.1" }
      : null;
  }
  return invoke<AppUpdateInfo | null>("check_app_update");
}

export async function installAppUpdate(
  onEvent: (event: AppUpdateEvent) => void,
): Promise<void> {
  if (!isTauri()) {
    onEvent({ event: "Started", data: { contentLength: 12_000_000 } });
    for (const chunkLength of [2_400_000, 3_000_000, 3_600_000, 3_000_000]) {
      await new Promise((resolve) => window.setTimeout(resolve, 90));
      onEvent({ event: "Progress", data: { chunkLength } });
    }
    onEvent({ event: "Downloaded" });
    return;
  }
  const onEventChannel = new Channel<AppUpdateEvent>();
  onEventChannel.onmessage = onEvent;
  await invoke("install_app_update", { onEvent: onEventChannel });
}

export async function loadSettings(): Promise<AppSettings> {
  if (!isTauri()) {
    const stored = window.localStorage.getItem("quill.settings");
    const settings = stored ? { ...defaultSettings, ...JSON.parse(stored) } : defaultSettings;
    return {
      ...settings,
      dismissedSuggestions: capDismissedSuggestions(settings.dismissedSuggestions),
    };
  }
  return invoke<AppSettings>("get_settings");
}

export async function persistSettings(settings: AppSettings): Promise<void> {
  const boundedSettings = {
    ...settings,
    dismissedSuggestions: capDismissedSuggestions(settings.dismissedSuggestions),
  };
  if (!isTauri()) {
    window.localStorage.setItem("quill.settings", JSON.stringify(boundedSettings));
    return;
  }
  await invoke("save_settings", { settings: boundedSettings });
}

export async function detectProviders(): Promise<ProviderStatus[]> {
  if (!isTauri()) {
    if (new URLSearchParams(window.location.search).has("firstRun")) return [];
    return [
      {
        kind: "ollama",
        baseUrl: "http://127.0.0.1:11434",
        available: true,
        models: ["qwen2.5:3b", "qwen2.5:7b"],
      },
    ];
  }
  return invoke<ProviderStatus[]>("detect_local_providers");
}

export async function listAudioInputDevices(): Promise<string[]> {
  if (!isTauri()) return ["Default microphone"];
  return invoke<string[]>("list_audio_input_devices");
}

export async function listInstalledWhisperModels(): Promise<string[]> {
  if (!isTauri()) {
    return new URLSearchParams(window.location.search).has("firstRun")
      ? []
      : ["base.en"];
  }
  return invoke<string[]>("list_installed_whisper_models");
}

export async function getSystemProfile(): Promise<SystemProfile> {
  if (!isTauri()) {
    const memoryGiB = (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? 8;
    return {
      totalMemoryBytes: memoryGiB * 1024 ** 3,
      availableMemoryBytes: memoryGiB * 0.65 * 1024 ** 3,
      logicalCpuCount: navigator.hardwareConcurrency || 4,
      platform: navigator.platform || "browser",
      architecture: "preview",
    };
  }
  return invoke<SystemProfile>("get_system_profile");
}

export async function downloadWhisperModel(id: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("download_whisper_model", { id });
}

export async function cancelWhisperDownload(id: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_whisper_download", { id });
}

export async function deleteWhisperModel(id: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("delete_whisper_model", { id });
}

export async function getCudaRuntimeStatus(): Promise<CudaRuntimeStatus> {
  if (!isTauri()) {
    return {
      state: "missing",
      expectedRevision: "f049fff95a089aa9969deb009cdd4892b3e74916",
      downloadBytes: 700_000_000,
      error: null,
    };
  }
  return invoke<CudaRuntimeStatus>("get_cuda_runtime_status");
}

export async function downloadCudaRuntime(): Promise<void> {
  if (!isTauri()) return;
  await invoke("download_cuda_runtime");
}

export async function cancelCudaRuntimeDownload(): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_cuda_runtime_download");
}

export async function deleteCudaRuntime(): Promise<void> {
  if (!isTauri()) return;
  await invoke("delete_cuda_runtime");
}

export async function pullOllamaModel(name: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("pull_ollama_model", { name });
}

export async function cancelOllamaPull(name: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_ollama_pull", { name });
}

export async function openExternal(url: string): Promise<void> {
  if (isTauri()) {
    try {
      await invoke("plugin:shell|open", { path: url });
      return;
    } catch {
      // Fall through to window.open below.
    }
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function previewMode(mode: Mode, active: boolean): Promise<void> {
  if (!isTauri()) return;
  await invoke("preview_mode", { mode, active });
}

/** Pause the session's hotkey handling while the settings UI is capturing
    a new shortcut, so pressing the current combination is written into the
    field instead of firing the mode. */
export async function setHotkeyCapture(capturing: boolean): Promise<void> {
  if (!isTauri()) return;
  await invoke("set_hotkey_capture", { capturing });
}

export async function getScribeReview(): Promise<ScribeReviewDraft | null> {
  if (!isTauri()) return null;
  return invoke<ScribeReviewDraft | null>("get_scribe_review");
}

export async function regenerateScribeReview(register?: Register): Promise<ScribeReviewDraft> {
  return invoke<ScribeReviewDraft>("regenerate_scribe_review", {
    register: register ?? null,
  });
}

export async function acceptScribeReview(text: string): Promise<DictionarySuggestion | null> {
  return invoke<DictionarySuggestion | null>("accept_scribe_review", { text });
}

export async function discardScribeReview(): Promise<void> {
  await invoke("discard_scribe_review");
}

export async function getPendingRecovery(): Promise<RecoveryManifest | null> {
  if (!isTauri()) return null;
  return invoke<RecoveryManifest | null>("get_pending_recovery");
}

export async function discardRecovery(id: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("discard_recovery", { id });
}
