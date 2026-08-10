import {
  AlertCircle,
  AudioLines,
  BookOpen,
  Check,
  CircleHelp,
  Download,
  LockKeyhole,
  Settings2,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import { BrandMark } from "./components/BrandMark";
import { isScribeReady } from "./scribe";
import { AppUpdateBanner } from "./components/AppUpdateBanner";
import {
  FirstRunSetup,
  type SpeechSetupState,
} from "./components/FirstRunSetup";
import { RecordingOverlay } from "./components/RecordingOverlay";
import { RecoveryBanner } from "./components/RecoveryBanner";
import { ScribeReviewWindow } from "./components/ScribeReviewWindow";
import { Onboarding } from "./components/Onboarding";
import { defaultSettings, initialRuntimeStatus } from "./defaults";
import { modelDownloadBytes } from "./onboarding";
import {
  detectProviders,
  downloadWhisperModel,
  getSystemProfile,
  getProviderKeyStatus,
  listInstalledWhisperModels,
  loadSettings,
  persistSettings,
  previewMode,
  testProviderConnection,
} from "./tauri";
import type {
  AppSettings,
  HotkeyConfig,
  Mode,
  NavigationSection,
  ProviderStatus,
  ProviderKeyStatus,
  RuntimeStatus,
  SystemProfile,
} from "./types";
import { AboutView } from "./views/AboutView";
import { DictionaryView } from "./views/DictionaryView";
import { GeneralView } from "./views/GeneralView";
import { modelsForLanguage, VoiceView } from "./views/VoiceView";

const FALLBACK_MODEL_BYTES = 147_951_465;

const navigation = [
  { id: "general" as const, label: "General", icon: Settings2 },
  { id: "voice" as const, label: "Voice", icon: AudioLines },
  { id: "dictionary" as const, label: "Dictionary", icon: BookOpen },
  { id: "about" as const, label: "About", icon: CircleHelp },
];

export function App() {
  const [section, setSection] = useState<NavigationSection>("general");
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
  const [cloudStatuses, setCloudStatuses] = useState<ProviderKeyStatus[]>([]);
  const [runtime, setRuntime] = useState<RuntimeStatus>(initialRuntimeStatus);
  const [dirty, setDirty] = useState(false);
  const [saved, setSaved] = useState(false);
  const [providersChecked, setProvidersChecked] = useState(false);
  const [initialising, setInitialising] = useState(true);
  const [onboardingRequired, setOnboardingRequired] = useState(false);
  const [systemProfile, setSystemProfile] = useState<SystemProfile | null>(null);
  const [speechSetup, setSpeechSetup] = useState<SpeechSetupState>({
    phase: "checking",
    modelId: "base.en",
    bytesDownloaded: 0,
    bytesTotal: FALLBACK_MODEL_BYTES,
    error: null,
  });
  const overlayOnly = new URLSearchParams(window.location.search).has("overlay");
  const reviewOnly = new URLSearchParams(window.location.search).has("review");
  const overlayMode =
    (new URLSearchParams(window.location.search).get("mode") as Mode | null) ?? "dictation";
  const overlayDemo = new URLSearchParams(window.location.search).get("demo");

  /* Last persisted settings, so Discard can restore them without a reload. */
  const savedSettings = useRef<AppSettings>(defaultSettings);

  useEffect(() => {
    let disposed = false;
    void initialiseSpeechSetup().catch((error) => {
      if (disposed) return;
      setInitialising(false);
      setSpeechSetup((current) => ({
        ...current,
        phase: "error",
        error: error instanceof Error ? error.message : String(error),
      }));
    });

    async function initialiseSpeechSetup() {
      const [loaded, installed, profile, groqStatus, geminiStatus] = await Promise.all([
        loadSettings(),
        listInstalledWhisperModels(),
        getSystemProfile(),
        getProviderKeyStatus("groq"),
        getProviderKeyStatus("gemini"),
      ]);
      if (disposed) return;
      setSystemProfile(profile);
      let checkedGemini = geminiStatus;
      if (loaded.cleanupProvider === "gemini" && geminiStatus.configured) {
        try {
          checkedGemini = await testProviderConnection("gemini");
        } catch (error) {
          checkedGemini = {
            ...geminiStatus,
            status: "error",
            message: error instanceof Error ? error.message : String(error),
          };
        }
      }
      if (disposed) return;
      setCloudStatuses([groqStatus, checkedGemini]);

      if (loaded.transcriptionProvider === "groq" && groqStatus.configured) {
        savedSettings.current = loaded;
        setSettings(loaded);
        setSpeechSetup({
          phase: "ready",
          modelId: "Groq Cloud",
          bytesDownloaded: 0,
          bytesTotal: 0,
          error: null,
        });
        setOnboardingRequired(false);
        setInitialising(false);
        return;
      }

      let next = loaded;
      const languageModels = modelsForLanguage(loaded.language);
      const selectedModelIsUsable =
        installed.includes(loaded.whisperModel) &&
        languageModels.some((model) => model.id === loaded.whisperModel);
      const compatibleInstalled = languageModels.find((model) =>
        installed.includes(model.id),
      );
      const usableModelId = selectedModelIsUsable
        ? loaded.whisperModel
        : compatibleInstalled?.id;
      if (usableModelId) {
        if (loaded.whisperModel !== usableModelId) {
          next = { ...next, whisperModel: usableModelId };
        }
        if (!next.speechModelSetupAttempted || !next.onboardingCompleted) {
          next = {
            ...next,
            speechModelSetupAttempted: true,
            onboardingCompleted: true,
          };
        }
        if (next !== loaded) await persistSettings(next);
        savedSettings.current = next;
        setSettings(next);
        setSpeechSetup({
          phase: "ready",
          modelId: next.whisperModel,
          bytesDownloaded: 0,
          bytesTotal: 0,
          error: null,
        });
        setOnboardingRequired(false);
        setInitialising(false);
        return;
      }

      const modelId = loaded.whisperModel || (loaded.language === "en" ? "base.en" : "base");
      savedSettings.current = loaded;
      setSettings(loaded);
      setSpeechSetup({
        phase: "missing",
        modelId,
        bytesDownloaded: 0,
        bytesTotal: modelDownloadBytes(modelId) || FALLBACK_MODEL_BYTES,
        error: loaded.onboardingCompleted ? "The speech model is not installed." : null,
      });
      setOnboardingRequired(!loaded.onboardingCompleted);
      setInitialising(false);
    }

    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    const unlistenProgress = listen<{
      id: string;
      bytesDownloaded: number;
      bytesTotal: number;
    }>("model-download://progress", (event) => {
      setSpeechSetup((current) =>
        event.payload.id === current.modelId
          ? {
              ...current,
              phase: "downloading",
              bytesDownloaded: event.payload.bytesDownloaded,
              bytesTotal: event.payload.bytesTotal || current.bytesTotal,
              error: null,
            }
          : current,
      );
    });
    const unlistenComplete = listen<{ id: string; ok: boolean; error: string | null }>(
      "model-download://complete",
      (event) => {
        setSpeechSetup((current) => {
          if (event.payload.id !== current.modelId) return current;
          return event.payload.ok
            ? {
                ...current,
                phase: "ready",
                bytesDownloaded: current.bytesTotal,
                error: null,
              }
            : {
                ...current,
                phase: "error",
                error: event.payload.error ?? "The speech model download failed.",
              };
        });
      },
    );
    return () => {
      void unlistenProgress.then((dispose) => dispose());
      void unlistenComplete.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    void refreshProviders()
      .catch(() => setProviders([]))
      .finally(() => setProvidersChecked(true));
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    const unlisten = listen<RuntimeStatus>("runtime://status", (event) => {
      setRuntime(event.payload);
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  const update = useCallback(
    <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
      setSettings((current) => ({ ...current, [key]: value }));
      setDirty(true);
      setSaved(false);
    },
    [],
  );

  function updateHotkey(mode: Mode, value: HotkeyConfig) {
    update(mode === "dictation" ? "dictationHotkey" : "scribeHotkey", value);
  }

  function showPreview(mode: Mode, active: boolean) {
    setRuntime({
      state: active ? "listening" : "ready",
      mode: active ? mode : null,
      message: active ? "Listening" : "Ready",
      provider: null,
    });
    void previewMode(mode, active);
  }

  async function save() {
    await persistSettings(settings);
    savedSettings.current = settings;
    setDirty(false);
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1800);
  }

  function discard() {
    setSettings(savedSettings.current);
    setDirty(false);
    setSaved(false);
  }

  async function refreshProviders() {
    setProviders(await detectProviders());
  }

  async function refreshCloudStatuses() {
    const statuses = await Promise.all([
      getProviderKeyStatus("groq"),
      getProviderKeyStatus("gemini"),
    ]);
    setCloudStatuses(statuses);
    const groq = statuses.find((status) => status.provider === "groq");
    if (settings.transcriptionProvider === "groq" && groq?.configured) {
      setSpeechSetup({
        phase: "ready",
        modelId: "Groq Cloud",
        bytesDownloaded: 0,
        bytesTotal: 0,
        error: null,
      });
    }
  }

  async function startSpeechDownload(modelId = speechSetup.modelId) {
    setSpeechSetup({
      phase: "downloading",
      modelId,
      bytesDownloaded: 0,
      bytesTotal: modelDownloadBytes(modelId) || FALLBACK_MODEL_BYTES,
      error: null,
    });
    try {
      await downloadWhisperModel(modelId);
    } catch (error) {
      setSpeechSetup((current) => ({
        ...current,
        phase: "error",
        error: error instanceof Error ? error.message : String(error),
      }));
    }
  }

  async function startOnboardingSpeech(modelId: string, language: string) {
    const next = {
      ...savedSettings.current,
      language,
      whisperModel: modelId,
      speechModelSetupAttempted: true,
    };
    await persistSettings(next);
    savedSettings.current = next;
    setSettings(next);
    await startSpeechDownload(modelId);
  }

  async function finishOnboarding(openScribe: boolean) {
    const next = {
      ...savedSettings.current,
      onboardingCompleted: true,
      scribeSetupDismissed:
        openScribe || scribeReady ? savedSettings.current.scribeSetupDismissed : true,
    };
    await persistSettings(next);
    savedSettings.current = next;
    setSettings(next);
    setOnboardingRequired(false);
    if (openScribe) openVoiceSetup("scribe");
  }

  async function useGroqFromOnboarding() {
    const next: AppSettings = {
      ...savedSettings.current,
      transcriptionProvider: "groq",
      speechModelSetupAttempted: true,
      onboardingCompleted: true,
    };
    await persistSettings(next);
    savedSettings.current = next;
    setSettings(next);
    setSpeechSetup({
      phase: "missing",
      modelId: "Groq Cloud",
      bytesDownloaded: 0,
      bytesTotal: 0,
      error: "Connect a Groq API key to finish speech setup.",
    });
    setOnboardingRequired(false);
    setSection("voice");
  }

  function openVoiceSetup(target: "speech" | "scribe") {
    setSection("voice");
    if (target === "scribe") void refreshProviders();
    window.setTimeout(() => {
      document
        .getElementById(target === "scribe" ? "scribe-title" : "recognition-title")
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    }, 80);
  }

  function dismissScribeSetup() {
    const persisted = {
      ...savedSettings.current,
      scribeSetupDismissed: true,
    };
    savedSettings.current = persisted;
    setSettings((current) => ({ ...current, scribeSetupDismissed: true }));
    void persistSettings(persisted);
  }

  const scribeReady = isScribeReady(settings, providers, cloudStatuses);
  const showScribeSetup =
    providersChecked &&
    settings.cleanupProvider !== "disabled" &&
    !settings.scribeSetupDismissed &&
    !scribeReady;
  const speechPercent =
    speechSetup.bytesTotal > 0
      ? Math.min(
          100,
          Math.round((speechSetup.bytesDownloaded / speechSetup.bytesTotal) * 100),
        )
      : 0;

  if (reviewOnly) {
    return <ScribeReviewWindow />;
  }

  if (overlayOnly) {
    const demoPhase =
      overlayDemo === "review"
        ? "reviewing"
        : overlayDemo === "processing"
          ? "transcribing"
          : overlayDemo === "generating"
            ? "generating"
            : overlayDemo === "complete"
              ? "complete"
              : "recording";
    const demoReview =
      overlayDemo === "review"
        ? {
            id: 1,
            source: "Tell Jordan I can do 5 PM tomorrow and send my calendar link.",
            draft:
              "Hi Jordan,\n\n5 PM tomorrow works for me. You can use my calendar link to schedule it.\n\nTalk soon.",
            warning: null,
            register: "email" as const,
          }
        : null;
    return (
      <main className="overlay-page">
        <RecordingOverlay
          mode={overlayMode}
          initialPhase={demoPhase}
          demoReview={demoReview}
        />
      </main>
    );
  }

  if (initialising || (onboardingRequired && !systemProfile)) {
    return (
      <main className="onboarding-page is-loading" role="status" aria-live="polite">
        <span className="onboarding-wordmark">Quill</span>
        <p>Checking this computer…</p>
      </main>
    );
  }

  if (onboardingRequired && systemProfile) {
    return (
      <Onboarding
        settings={settings}
        profile={systemProfile}
        speech={speechSetup}
        scribeReady={scribeReady}
        onStartSpeech={startOnboardingSpeech}
        onRetrySpeech={() => void startSpeechDownload()}
        onUseGroq={useGroqFromOnboarding}
        onFinish={finishOnboarding}
      />
    );
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <BrandMark />
        <nav aria-label="Settings">
          {navigation.map((item) => {
            const Icon = item.icon;
            return (
              <button
                type="button"
                key={item.id}
                onClick={() => setSection(item.id)}
                className={section === item.id ? "is-active" : ""}
                aria-current={section === item.id ? "page" : undefined}
              >
                <Icon size={18} strokeWidth={1.8} />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
        <div className="sidebar-foot">
          {/* Engine state lives with the app chrome, not in a full-width band
              across every settings page. */}
          {speechSetup.phase === "ready" ? (
            <div className={`engine-state is-${runtime.state}`} role="status" title={runtime.message}>
              <i aria-hidden="true" />
              <span>{runtime.message}</span>
            </div>
          ) : (
            <button
              type="button"
              className={`sidebar-download is-${speechSetup.phase}`}
              onClick={() => openVoiceSetup("speech")}
              aria-label="Open speech model setup"
            >
              <span className="sidebar-download__title">
                {speechSetup.phase === "error" || speechSetup.phase === "missing" ? (
                  <AlertCircle size={13} />
                ) : (
                  <Download size={13} />
                )}
                {speechSetup.phase === "error" || speechSetup.phase === "missing"
                  ? "Speech model needs attention"
                  : `Downloading ${speechSetup.modelId}`}
              </span>
              {speechSetup.phase === "downloading" ? (
                <span className="sidebar-download__progress" aria-hidden="true">
                  <i style={{ width: `${speechPercent}%` }} />
                </span>
              ) : null}
              <small>
                {speechSetup.phase === "downloading"
                  ? `${speechPercent}% · ${Math.max(
                      0,
                      Math.round((speechSetup.bytesTotal - speechSetup.bytesDownloaded) / 1_000_000),
                    )} MB left`
                  : "Open Voice to retry"}
              </small>
            </button>
          )}
          <div className="sidebar-privacy">
            <LockKeyhole size={14} />
            <span>
              {settings.transcriptionProvider === "groq" || settings.cleanupProvider === "gemini"
                ? "Cloud is used only for selected providers"
                : "Processing stays on this device"}
            </span>
          </div>
        </div>
      </aside>

      <section className="workspace">
        <AppUpdateBanner
          blockedReason={
            dirty
              ? "Save or discard your settings before updating."
              : runtime.state === "listening" || runtime.state === "processing"
                ? "Finish the current session before updating."
                : null
          }
        />
        <div className="content-scroll">
          <RecoveryBanner />
          {section === "general" ? (
            <FirstRunSetup
              speech={speechSetup}
              showScribeSetup={showScribeSetup}
              onOpenVoice={openVoiceSetup}
              onRetrySpeech={() => void startSpeechDownload()}
              onDismissScribe={dismissScribeSetup}
            />
          ) : null}
          {section === "general" ? (
            <GeneralView
              settings={settings}
              update={update}
              onHotkey={updateHotkey}
              onPreview={showPreview}
            />
          ) : null}
          {section === "voice" ? (
            <VoiceView
              settings={settings}
              update={update}
              providers={providers}
              cloudStatuses={cloudStatuses}
              onDetectProviders={refreshProviders}
              onCloudStatusChange={refreshCloudStatuses}
            />
          ) : null}
          {section === "dictionary" ? (
            <DictionaryView settings={settings} update={update} />
          ) : null}
          {section === "about" ? <AboutView /> : null}
        </div>

        {/* Contextual: the save bar exists only while there is something to
            save. Discard restores the last persisted settings. */}
        {dirty ? (
          <footer className="save-bar">
            <span className="save-bar__note">
              <i aria-hidden="true" />
              Unsaved changes
            </span>
            <div className="save-bar__actions">
              <button className="ghost-action" type="button" onClick={discard}>
                Discard
              </button>
              <button className="primary-button" type="button" onClick={save}>
                Save changes
              </button>
            </div>
          </footer>
        ) : null}

        {saved ? (
          <span className="saved-toast" role="status">
            <Check size={14} strokeWidth={2.2} />
            Saved
          </span>
        ) : null}
      </section>

      {/* Inline preview is only rendered in the browser dev harness. In the real
          app the native Tauri overlay window is shown by `previewMode`, and
          rendering this on top of it would duplicate the pill. */}
      {!isTauri() && runtime.state === "listening" && runtime.mode ? (
        <div className="overlay-preview">
          <RecordingOverlay mode={runtime.mode} />
        </div>
      ) : null}
    </main>
  );
}
