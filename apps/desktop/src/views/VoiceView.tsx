import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Check,
  Download,
  ExternalLink,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import { LanguageCombobox } from "../components/LanguageCombobox";
import { ProviderBadge } from "../components/ProviderBadge";
import { SettingRow } from "../components/SettingRow";
import {
  allLanguages,
  broadLanguages,
  englishOnly,
  majorLanguages,
} from "../data/languages";
import type { AppSettings, ProviderStatus } from "../types";
import {
  cancelOllamaPull,
  cancelWhisperDownload,
  deleteWhisperModel,
  downloadWhisperModel,
  listAudioInputDevices,
  listInstalledWhisperModels,
  openExternal,
  pullOllamaModel,
} from "../tauri";

interface WhisperModelInfo {
  id: string;
  label: string;
  fileSize: string;
  bytes: number;
  hardware: string;
  /** One-line "who is this for" caption for the row. */
  fit: string;
  /** Relative speed on CUDA: 1 = tiny (fastest), 5 = large-v3-turbo. */
  speed: "Fastest" | "Fast" | "Moderate" | "Slow" | "Slowest";
  /** Rough accuracy tier for its target language(s). */
  accuracy: "Basic" | "Good" | "Strong" | "Excellent" | "Best";
  /** English-only models hallucinate when pointed at other languages.
   *  Multilingual variants cover some or all supported languages. */
  multilingual: boolean;
  /** Which languages this model can transcribe with usable quality. */
  languages: readonly string[];
}

const whisperModels: readonly WhisperModelInfo[] = [
  {
    id: "tiny.en",
    label: "tiny.en",
    fileSize: "75 MB",
    bytes: 77_691_713,
    hardware: "1 GB VRAM or 2 GB free RAM",
    fit: "Fastest English — quick notes, drafts, memory-tight machines",
    speed: "Fastest",
    accuracy: "Basic",
    multilingual: false,
    languages: englishOnly,
  },
  {
    id: "tiny",
    label: "tiny",
    fileSize: "75 MB",
    bytes: 77_691_713,
    hardware: "1 GB VRAM or 2 GB free RAM",
    fit: "Fastest multilingual — usable on 23 major European + East-Asian languages",
    speed: "Fastest",
    accuracy: "Basic",
    multilingual: true,
    languages: majorLanguages,
  },
  {
    id: "base.en",
    label: "base.en",
    fileSize: "142 MB",
    bytes: 147_951_465,
    hardware: "1 GB VRAM or 2 GB free RAM",
    fit: "Bundled default — good English on any laptop",
    speed: "Fast",
    accuracy: "Good",
    multilingual: false,
    languages: englishOnly,
  },
  {
    id: "base",
    label: "base",
    fileSize: "142 MB",
    bytes: 147_951_465,
    hardware: "1 GB VRAM or 2 GB free RAM",
    fit: "Balanced multilingual — 23 major languages, fair accuracy",
    speed: "Fast",
    accuracy: "Good",
    multilingual: true,
    languages: majorLanguages,
  },
  {
    id: "small.en",
    label: "small.en",
    fileSize: "466 MB",
    bytes: 487_593_953,
    hardware: "2 GB VRAM or 4 GB free RAM",
    fit: "Strong English — recommended sweet spot for pro dictation",
    speed: "Moderate",
    accuracy: "Strong",
    multilingual: false,
    languages: englishOnly,
  },
  {
    id: "small",
    label: "small",
    fileSize: "466 MB",
    bytes: 487_593_953,
    hardware: "2 GB VRAM or 4 GB free RAM",
    fit: "Strong across ~68 languages incl. Hindi, Arabic, Thai, Vietnamese",
    speed: "Moderate",
    accuracy: "Strong",
    multilingual: true,
    languages: broadLanguages,
  },
  {
    id: "medium.en",
    label: "medium.en",
    fileSize: "1.5 GB",
    bytes: 1_533_763_425,
    hardware: "5 GB VRAM or 8 GB free RAM",
    fit: "Excellent English — meetings, long-form, difficult accents",
    speed: "Slow",
    accuracy: "Excellent",
    multilingual: false,
    languages: englishOnly,
  },
  {
    id: "medium",
    label: "medium",
    fileSize: "1.5 GB",
    bytes: 1_533_763_425,
    hardware: "5 GB VRAM or 8 GB free RAM",
    fit: "Excellent across all 99 Whisper languages",
    speed: "Slow",
    accuracy: "Excellent",
    multilingual: true,
    languages: allLanguages,
  },
  {
    id: "distil-large-v3",
    label: "distil-large-v3",
    fileSize: "1.5 GB",
    bytes: 1_520_000_000,
    hardware: "5 GB VRAM or 8 GB free RAM",
    fit: "High-accuracy English — distilled for fast long-form transcription",
    speed: "Moderate",
    accuracy: "Best",
    multilingual: false,
    languages: englishOnly,
  },
  {
    id: "large-v3-turbo",
    label: "large-v3-turbo",
    fileSize: "1.6 GB",
    bytes: 1_624_555_275,
    hardware: "6 GB VRAM or 10 GB free RAM",
    fit: "Best quality/speed tradeoff — Whisper's current top model",
    speed: "Moderate",
    accuracy: "Best",
    multilingual: true,
    languages: allLanguages,
  },
];

export function modelsForLanguage(
  language: string,
): readonly WhisperModelInfo[] {
  if (language === "en") {
    return whisperModels.filter((model) => !model.multilingual);
  }
  return whisperModels.filter(
    (model) =>
      model.multilingual &&
      (language === "auto" || model.languages.includes(language)),
  );
}

function pairedModelId(modelId: string, language: string): string | null {
  if (language === "en") {
    if (["tiny", "base", "small", "medium"].includes(modelId)) {
      return `${modelId}.en`;
    }
    return null;
  }
  if (modelId.endsWith(".en")) return modelId.slice(0, -3);
  if (modelId === "distil-large-v3") return "large-v3-turbo";
  return null;
}

export function preferredModelForLanguage(
  language: string,
  currentModelId: string,
  installedModels: readonly string[],
  excludedModelId?: string,
): WhisperModelInfo {
  const eligible = modelsForLanguage(language).filter(
    (model) => model.id !== excludedModelId,
  );
  const current = eligible.find((model) => model.id === currentModelId);
  if (current) return current;

  const pairedId = pairedModelId(currentModelId, language);
  const paired = eligible.find((model) => model.id === pairedId);
  if (paired && installedModels.includes(paired.id)) return paired;

  const installed = eligible.find((model) => installedModels.includes(model.id));
  if (installed) return installed;
  if (paired) return paired;

  const fallback =
    (language === "en"
      ? eligible.find((model) => model.id === "base.en")
      : eligible.find((model) => model.id === "base")) ?? eligible[0];
  if (!fallback) {
    throw new Error(`No Whisper model supports language '${language}'`);
  }
  return fallback;
}

type BackendId = AppSettings["backend"];

interface BackendOption {
  id: BackendId;
  title: string;
  requires: string;
  platforms: Array<"win" | "mac" | "linux">;
}

const backendOptions: BackendOption[] = [
  {
    id: "auto",
    title: "Auto-detect",
    requires: "picks the fastest chip you have",
    platforms: ["win", "mac", "linux"],
  },
  {
    id: "cpu",
    title: "CPU",
    requires: "any computer, no GPU needed",
    platforms: ["win", "mac", "linux"],
  },
  {
    id: "cuda",
    title: "CUDA",
    requires: "NVIDIA GPU with CUDA 12+ drivers",
    platforms: ["win", "linux"],
  },
  {
    id: "metal",
    title: "Metal",
    requires: "Apple Silicon Mac (M1–M4)",
    platforms: ["mac"],
  },
];

interface CleanupTier {
  id: string;
  tier: "Light" | "Polish" | "Compose";
  fileSize: string;
  hardware: string;
  quality: string;
}

/** Curated Ollama models covering basic polish through register-aware Compose.
 *  The 7B floor reflects Quill's correction-resolution evaluation; the
 *  deterministic commitment guard still applies to every tier. */
const cleanupTiers: readonly CleanupTier[] = [
  {
    id: "qwen2.5:0.5b",
    tier: "Light",
    fileSize: "~400 MB",
    hardware: "4 GB RAM, any CPU",
    quality: "Basic polish only — punctuation and filler removal.",
  },
  {
    id: "qwen2.5:3b",
    tier: "Polish",
    fileSize: "~1.9 GB",
    hardware: "8 GB RAM, CPU or any GPU",
    quality: "Polish only — not reliable for complex self-corrections.",
  },
  {
    id: "qwen2.5:7b",
    tier: "Compose",
    fileSize: "~4.7 GB",
    hardware: "16 GB RAM or dedicated GPU",
    quality: "Recommended minimum for corrections and register-aware rewriting.",
  },
];

const providerInstallers = [
  {
    id: "ollama",
    title: "Ollama",
    detail: "Simplest local LLM manager. Pull a model, Quill auto-detects.",
    requires: "8 GB RAM",
    url: "https://ollama.com/download",
  },
  {
    id: "lmstudio",
    title: "LM Studio",
    detail: "Desktop UI for open models. Enable Local Server to connect.",
    requires: "8 GB RAM",
    url: "https://lmstudio.ai",
  },
];

function detectPlatform(): "win" | "mac" | "linux" {
  const p = (
    typeof navigator !== "undefined" ? navigator.platform ?? "" : ""
  ).toLowerCase();
  if (p.startsWith("mac")) return "mac";
  if (p.startsWith("linux")) return "linux";
  return "win";
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
  if (bytes >= 1_000_000) return `${Math.round(bytes / 1_000_000)} MB`;
  return `${Math.round(bytes / 1000)} KB`;
}

interface DownloadProgress {
  bytesDownloaded: number;
  bytesTotal: number;
}

interface VoiceViewProps {
  settings: AppSettings;
  update: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void;
  providers: ProviderStatus[];
  onDetectProviders: () => Promise<void>;
}

export function VoiceView({
  settings,
  update,
  providers,
  onDetectProviders,
}: VoiceViewProps) {
  const [checking, setChecking] = useState(false);
  const [inputDevices, setInputDevices] = useState<string[]>([]);
  const [installedModels, setInstalledModels] = useState<string[]>([]);
  const [modelsScanned, setModelsScanned] = useState(false);
  const [downloads, setDownloads] = useState<Record<string, DownloadProgress>>({});
  const [downloadErrors, setDownloadErrors] = useState<Record<string, string>>({});
  const [ollamaPulls, setOllamaPulls] = useState<
    Record<string, { status: string; bytesDownloaded: number; bytesTotal: number }>
  >({});
  const [ollamaErrors, setOllamaErrors] = useState<Record<string, string>>({});
  const platform = useMemo(detectPlatform, []);
  const availableProvider = providers.find((provider) => provider.available);

  const refreshInstalled = useCallback(() => {
    void listInstalledWhisperModels().then((models) => {
      setInstalledModels(models);
      setModelsScanned(true);
    });
  }, []);

  const visibleModels = useMemo(
    () => modelsForLanguage(settings.language),
    [settings.language],
  );

  const activeModel = useMemo(
    () =>
      whisperModels.find((m) => m.id === settings.whisperModel) ??
      preferredModelForLanguage(
        settings.language,
        settings.whisperModel,
        installedModels,
      ),
    [installedModels, settings.language, settings.whisperModel],
  );

  useEffect(() => {
    if (!modelsScanned) return;
    const preferred = preferredModelForLanguage(
      settings.language,
      settings.whisperModel,
      installedModels,
    );
    if (preferred.id !== settings.whisperModel) {
      update("whisperModel", preferred.id);
    }
  }, [installedModels, modelsScanned, settings.language, settings.whisperModel, update]);

  useEffect(() => {
    void listAudioInputDevices().then(setInputDevices);
    refreshInstalled();
  }, [refreshInstalled]);

  useEffect(() => {
    if (!isTauri()) return;
    const unlistenProgress = listen<DownloadProgress & { id: string }>(
      "model-download://progress",
      (event) => {
        const { id, bytesDownloaded, bytesTotal } = event.payload;
        setDownloads((current) => ({
          ...current,
          [id]: { bytesDownloaded, bytesTotal },
        }));
      },
    );
    const unlistenComplete = listen<{ id: string; ok: boolean; error: string | null }>(
      "model-download://complete",
      (event) => {
        const { id, ok, error } = event.payload;
        setDownloads((current) => {
          const next = { ...current };
          delete next[id];
          return next;
        });
        if (ok) {
          setDownloadErrors((current) => {
            const next = { ...current };
            delete next[id];
            return next;
          });
          refreshInstalled();
        } else if (error) {
          setDownloadErrors((current) => ({ ...current, [id]: error }));
        }
      },
    );
    return () => {
      void unlistenProgress.then((dispose) => dispose());
      void unlistenComplete.then((dispose) => dispose());
    };
  }, [refreshInstalled]);

  useEffect(() => {
    if (!isTauri()) return;
    const unlistenProgress = listen<{
      name: string;
      status: string;
      bytesDownloaded: number;
      bytesTotal: number;
    }>("ollama-pull://progress", (event) => {
      const { name, status, bytesDownloaded, bytesTotal } = event.payload;
      setOllamaPulls((current) => ({
        ...current,
        [name]: { status, bytesDownloaded, bytesTotal },
      }));
    });
    const unlistenComplete = listen<{ name: string; ok: boolean; error: string | null }>(
      "ollama-pull://complete",
      (event) => {
        const { name, ok, error } = event.payload;
        setOllamaPulls((current) => {
          const next = { ...current };
          delete next[name];
          return next;
        });
        if (ok) {
          setOllamaErrors((current) => {
            const next = { ...current };
            delete next[name];
            return next;
          });
          void onDetectProviders();
        } else if (error) {
          setOllamaErrors((current) => ({ ...current, [name]: error }));
        }
      },
    );
    return () => {
      void unlistenProgress.then((dispose) => dispose());
      void unlistenComplete.then((dispose) => dispose());
    };
  }, [onDetectProviders]);

  async function startOllamaPull(name: string) {
    setOllamaErrors((current) => {
      const next = { ...current };
      delete next[name];
      return next;
    });
    setOllamaPulls((current) => ({
      ...current,
      [name]: { status: "starting", bytesDownloaded: 0, bytesTotal: 0 },
    }));
    try {
      await pullOllamaModel(name);
    } catch (error) {
      setOllamaPulls((current) => {
        const next = { ...current };
        delete next[name];
        return next;
      });
      setOllamaErrors((current) => ({
        ...current,
        [name]: error instanceof Error ? error.message : String(error),
      }));
    }
  }

  async function cancelPull(name: string) {
    await cancelOllamaPull(name);
  }

  async function startDownload(id: string) {
    setDownloadErrors((current) => {
      const next = { ...current };
      delete next[id];
      return next;
    });
    setDownloads((current) => ({
      ...current,
      [id]: { bytesDownloaded: 0, bytesTotal: 0 },
    }));
    try {
      await downloadWhisperModel(id);
    } catch (error) {
      // Errors also arrive via the complete event, but catch the invoke rejection so React state is clean.
      setDownloads((current) => {
        const next = { ...current };
        delete next[id];
        return next;
      });
      setDownloadErrors((current) => ({
        ...current,
        [id]: error instanceof Error ? error.message : String(error),
      }));
    }
  }

  async function cancelDownload(id: string) {
    await cancelWhisperDownload(id);
  }

  async function removeModel(id: string) {
    await deleteWhisperModel(id);
    if (settings.whisperModel === id) {
      const remainingInstalled = installedModels.filter((model) => model !== id);
      const fallback = preferredModelForLanguage(
        settings.language,
        id,
        remainingInstalled,
        id,
      );
      update("whisperModel", fallback.id);
    }
    refreshInstalled();
  }

  function changeLanguage(language: string) {
    const preferred = preferredModelForLanguage(
      language,
      settings.whisperModel,
      installedModels,
    );
    update("language", language);
    if (preferred.id !== settings.whisperModel) {
      update("whisperModel", preferred.id);
    }
  }

  async function detect() {
    setChecking(true);
    await onDetectProviders();
    setChecking(false);
  }

  const bundledModelId = "base.en";

  return (
    <div className="view-stack">
      <header className="view-heading">
        <h1>Everything runs <em>locally</em>.</h1>
      </header>

      <section aria-labelledby="recognition-title">
        <div className="section-heading">
          <h2 id="recognition-title">Recognition</h2>
          <span>whisper.cpp runs on your machine.</span>
        </div>
        <div className="settings-group">
          <SettingRow
            label="Microphone"
            description="Default follows Windows. Pick a specific device to pin it."
          >
            <select
              value={settings.audioInputDevice ?? ""}
              onChange={(event) =>
                update("audioInputDevice", event.target.value || null)
              }
            >
              <option value="">System default input</option>
              {inputDevices.map((device) => (
                <option value={device} key={device}>
                  {device}
                </option>
              ))}
            </select>
          </SettingRow>
          <div className="model-setting">
            <SettingRow
              label="Speech model"
              description={`${activeModel.hardware}. ${activeModel.fit}`}
            >
              <select
                value={settings.whisperModel}
                onChange={(event) => update("whisperModel", event.target.value)}
              >
                {visibleModels.map((model) => {
                  const installed = installedModels.includes(model.id);
                  return (
                    <option value={model.id} key={model.id} disabled={!installed}>
                      {model.label} · {model.fileSize}
                      {installed ? "" : " · not installed"}
                    </option>
                  );
                })}
              </select>
            </SettingRow>
            <details className="model-guide">
              <summary>Compare and download models</summary>
              <div className="model-guide-scroll">
                <table>
                  <colgroup>
                    <col className="col-model" />
                    <col className="col-memory" />
                    <col className="col-fit" />
                    <col className="col-action" />
                  </colgroup>
                  <thead>
                    <tr>
                      <th>Model</th>
                      <th>Min. memory</th>
                      <th>Best fit</th>
                      <th className="model-guide__action-head">Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visibleModels.map((model) => {
                      const installed = installedModels.includes(model.id);
                      const active = settings.whisperModel === model.id;
                      const progress = downloads[model.id];
                      const error = downloadErrors[model.id];
                      const isDownloading = progress !== undefined;
                      const percent =
                        progress && progress.bytesTotal > 0
                          ? Math.min(
                              100,
                              Math.round(
                                (progress.bytesDownloaded / progress.bytesTotal) * 100,
                              ),
                            )
                          : 0;
                      return (
                        <tr key={model.id} className={active ? "is-selected" : ""}>
                          <th scope="row">
                            {model.label}
                            <span>{model.fileSize}</span>
                          </th>
                          <td>
                            <strong>{model.hardware.split(" or ")[0]}</strong>
                            <span>
                              or {model.hardware.split(" or ")[1] ?? model.hardware}
                            </span>
                          </td>
                          <td>
                            <strong>{model.accuracy} · {model.speed}</strong>
                            <span>{model.fit}</span>
                          </td>
                          <td className="model-guide__action">
                            {isDownloading ? (
                              <div className="model-guide__progress">
                                <div className="progress-bar">
                                  <div
                                    className="progress-bar__fill"
                                    style={{ width: `${percent}%` }}
                                  />
                                </div>
                                <div className="model-guide__progress-row">
                                  <span className="progress-bar__label">
                                    {percent}%
                                    {progress.bytesTotal > 0
                                      ? ` · ${formatBytes(progress.bytesDownloaded)} / ${formatBytes(progress.bytesTotal)}`
                                      : ""}
                                  </span>
                                  <button
                                    type="button"
                                    className="link-button"
                                    onClick={() => cancelDownload(model.id)}
                                  >
                                    Cancel
                                  </button>
                                </div>
                              </div>
                            ) : installed ? (
                              <div className="model-guide__installed">
                                <span className="model-status is-installed">
                                  <Check size={12} strokeWidth={2.4} />
                                  Installed
                                </span>
                                {model.id !== bundledModelId ? (
                                  <button
                                    type="button"
                                    className="icon-only-button"
                                    onClick={() => removeModel(model.id)}
                                    aria-label={`Delete ${model.label}`}
                                    title="Delete model"
                                  >
                                    <Trash2 size={12} strokeWidth={1.8} />
                                  </button>
                                ) : null}
                              </div>
                            ) : (
                              <button
                                type="button"
                                className="row-download-button"
                                onClick={() => startDownload(model.id)}
                              >
                                <Download size={12} strokeWidth={2} />
                                Download
                              </button>
                            )}
                            {error ? (
                              <p className="model-guide__error">{error}</p>
                            ) : null}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
              <p>
                Downloads come from the models' official GGML repositories on
                Hugging Face and are stored under your app data folder. Once
                installed a model runs fully offline.
              </p>
            </details>
          </div>
          <SettingRow
            label="Compute backend"
            description={`Which chip runs the model. ${
              backendOptions.find((o) => o.id === settings.backend)?.title
            } — ${backendOptions.find((o) => o.id === settings.backend)?.requires}.`}
          >
            <select
              value={settings.backend}
              onChange={(event) =>
                update("backend", event.target.value as AppSettings["backend"])
              }
            >
              {backendOptions
                .filter((option) => option.platforms.includes(platform))
                .map((option) => (
                  <option value={option.id} key={option.id}>
                    {option.title}
                  </option>
                ))}
            </select>
          </SettingRow>
          <SettingRow
            label="Language"
            description={
              settings.language === "en"
                ? "English uses dedicated English-only models for the best accuracy."
                : settings.language === "auto"
                  ? "Auto-detect uses multilingual models and adds a small startup delay."
                  : "Non-English languages use compatible multilingual models."
            }
          >
            <LanguageCombobox
              value={settings.language}
              onChange={changeLanguage}
            />
          </SettingRow>
        </div>
      </section>

      <section aria-labelledby="scribe-title">
        <div className="section-heading">
          <h2 id="scribe-title">Scribe cleanup</h2>
          <ProviderBadge
            state={checking ? "checking" : availableProvider ? "available" : "unavailable"}
            label={
              checking
                ? "Checking local servers"
                : availableProvider
                  ? `${availableProvider.kind} connected`
                  : "No local server found"
            }
          />
        </div>

        <div className="settings-group">
          <div className="model-setting">
            <SettingRow
              label="Provider"
              description={
                availableProvider
                  ? "Auto-detect matches whichever local server is running."
                  : "No local server detected yet. Install one below — Quill will find it automatically."
              }
            >
              <div className="inline-control">
                <select
                  value={settings.cleanupProvider}
                  onChange={(event) =>
                    update(
                      "cleanupProvider",
                      event.target.value as AppSettings["cleanupProvider"],
                    )
                  }
                >
                  <option value="auto">Auto-detect local server</option>
                  <option value="ollama">Ollama</option>
                  <option value="openai-compatible">OpenAI-compatible</option>
                  <option value="disabled">Disabled</option>
                </select>
                <button
                  className="icon-button"
                  type="button"
                  onClick={detect}
                  aria-label="Detect local servers"
                  title="Recheck for local servers"
                >
                  <RefreshCw size={16} className={checking ? "is-spinning" : ""} />
                </button>
              </div>
            </SettingRow>
            <details className="model-guide" open={!availableProvider}>
              <summary>Compare and install local servers</summary>
              <div className="model-guide-scroll">
                <table>
                  <colgroup>
                    <col className="col-model" />
                    <col className="col-memory" />
                    <col className="col-fit" />
                    <col className="col-action" />
                  </colgroup>
                  <thead>
                    <tr>
                      <th>Server</th>
                      <th>What it is</th>
                      <th>Requires</th>
                      <th className="model-guide__action-head">Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {providerInstallers.map((installer) => (
                      <tr key={installer.id}>
                        <th scope="row">{installer.title}</th>
                        <td>{installer.detail}</td>
                        <td>
                          <strong>{installer.requires}</strong>
                          <span>GPU optional</span>
                        </td>
                        <td className="model-guide__action">
                          <button
                            type="button"
                            className="row-download-button"
                            onClick={() => void openExternal(installer.url)}
                          >
                            <Download size={12} strokeWidth={2} />
                            Get
                            <ExternalLink size={10} strokeWidth={2} />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <p>
                Opens the installer's official download page in your browser. After
                installing, click the refresh icon on the Provider row and Quill
                connects.
              </p>
            </details>
          </div>
          <div className="model-setting">
            <SettingRow
              label="Cleanup model"
              description={
                availableProvider
                  ? "Use 7B+ for Compose. Smaller models are best limited to Polish."
                  : "Available once a local server is running."
              }
            >
              <select
                value={settings.cleanupModel}
                onChange={(event) => update("cleanupModel", event.target.value)}
                disabled={!availableProvider}
              >
                <option value="">Automatic (prefer Compose 7B+)</option>
                {providers.flatMap((provider) =>
                  provider.models.map((model) => (
                    <option value={model} key={`${provider.kind}-${model}`}>
                      {model}
                    </option>
                  )),
                )}
              </select>
            </SettingRow>
            {(() => {
              const ollamaProvider = providers.find(
                (p) => p.kind === "ollama" && p.available,
              );
              if (!ollamaProvider) return null;
              const installedNames = new Set(ollamaProvider.models);
              const hasAny = cleanupTiers.some((t) => installedNames.has(t.id));
              return (
                <details className="model-guide" open={!hasAny}>
                  <summary>Install a recommended cleanup model</summary>
                  <div className="model-guide-scroll">
                    <table>
                      <colgroup>
                        <col className="col-model" />
                        <col className="col-memory" />
                        <col className="col-fit" />
                        <col className="col-action" />
                      </colgroup>
                      <thead>
                        <tr>
                          <th>Tier</th>
                          <th>Requires</th>
                          <th>Best for</th>
                          <th className="model-guide__action-head">Action</th>
                        </tr>
                      </thead>
                      <tbody>
                        {cleanupTiers.map((tier) => {
                          const installed = installedNames.has(tier.id);
                          const active = settings.cleanupModel === tier.id;
                          const progress = ollamaPulls[tier.id];
                          const error = ollamaErrors[tier.id];
                          const isPulling = progress !== undefined;
                          const percent =
                            progress && progress.bytesTotal > 0
                              ? Math.min(
                                  100,
                                  Math.round(
                                    (progress.bytesDownloaded / progress.bytesTotal) *
                                      100,
                                  ),
                                )
                              : 0;
                          return (
                            <tr
                              key={tier.id}
                              className={active ? "is-selected" : ""}
                            >
                              <th scope="row">
                                {tier.tier}
                                <span>
                                  {tier.id} · {tier.fileSize}
                                </span>
                              </th>
                              <td>
                                <strong>{tier.hardware.split(", ")[0]}</strong>
                                <span>{tier.hardware.split(", ").slice(1).join(", ")}</span>
                              </td>
                              <td>{tier.quality}</td>
                              <td className="model-guide__action">
                                {isPulling ? (
                                  <div className="model-guide__progress">
                                    <div className="progress-bar">
                                      <div
                                        className="progress-bar__fill"
                                        style={{ width: `${percent}%` }}
                                      />
                                    </div>
                                    <div className="model-guide__progress-row">
                                      <span className="progress-bar__label">
                                        {progress.status === "success"
                                          ? "Finalising…"
                                          : progress.bytesTotal > 0
                                            ? `${percent}%`
                                            : progress.status}
                                      </span>
                                      <button
                                        type="button"
                                        className="link-button"
                                        onClick={() => cancelPull(tier.id)}
                                      >
                                        Cancel
                                      </button>
                                    </div>
                                  </div>
                                ) : installed ? (
                                  <div className="model-guide__installed">
                                    <span className="model-status is-installed">
                                      <Check size={12} strokeWidth={2.4} />
                                      Installed
                                    </span>
                                    {!active ? (
                                      <button
                                        type="button"
                                        className="link-button"
                                        onClick={() =>
                                          update("cleanupModel", tier.id)
                                        }
                                      >
                                        Use
                                      </button>
                                    ) : null}
                                  </div>
                                ) : (
                                  <button
                                    type="button"
                                    className="row-download-button"
                                    onClick={() => startOllamaPull(tier.id)}
                                  >
                                    <Download size={12} strokeWidth={2} />
                                    Install
                                  </button>
                                )}
                                {error ? (
                                  <p className="model-guide__error">{error}</p>
                                ) : null}
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                  <p>
                    Runs `ollama pull` in the background. Files are stored inside
                    Ollama's own model folder and run fully offline afterwards.
                  </p>
                </details>
              );
            })()}
          </div>
          <SettingRow
            label="Correction window"
            description="Scribe holds the complete utterance until you stop, then types only the resolved wording."
          >
            <span className="fixed-setting">Until shortcut release</span>
          </SettingRow>
        </div>

        <p className="safety-copy">
          Scribe forwards the raw transcript to your local LLM with a strict "polish,
          don't invent" prompt. The model fixes tone, punctuation, mishearings, and
          self-corrections but must preserve every specific fact you said. Runs
          entirely on your machine — nothing leaves this device.
        </p>
      </section>
    </div>
  );
}
