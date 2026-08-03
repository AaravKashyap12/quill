import {
  AudioLines,
  BookOpen,
  Check,
  CircleHelp,
  LockKeyhole,
  Settings2,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import { BrandMark } from "./components/BrandMark";
import { RecordingOverlay } from "./components/RecordingOverlay";
import { RecoveryBanner } from "./components/RecoveryBanner";
import { ScribeReviewWindow } from "./components/ScribeReviewWindow";
import { defaultSettings, initialRuntimeStatus } from "./defaults";
import { detectProviders, loadSettings, persistSettings, previewMode } from "./tauri";
import type {
  AppSettings,
  HotkeyConfig,
  Mode,
  NavigationSection,
  ProviderStatus,
  RuntimeStatus,
} from "./types";
import { AboutView } from "./views/AboutView";
import { DictionaryView } from "./views/DictionaryView";
import { GeneralView } from "./views/GeneralView";
import { VoiceView } from "./views/VoiceView";

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
  const [runtime, setRuntime] = useState<RuntimeStatus>(initialRuntimeStatus);
  const [dirty, setDirty] = useState(false);
  const [saved, setSaved] = useState(false);
  const overlayOnly = new URLSearchParams(window.location.search).has("overlay");
  const reviewOnly = new URLSearchParams(window.location.search).has("review");
  const overlayMode =
    (new URLSearchParams(window.location.search).get("mode") as Mode | null) ?? "dictation";

  /* Last persisted settings, so Discard can restore them without a reload. */
  const savedSettings = useRef<AppSettings>(defaultSettings);

  useEffect(() => {
    void loadSettings().then((loaded) => {
      savedSettings.current = loaded;
      setSettings(loaded);
    });
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

  if (reviewOnly) {
    return <ScribeReviewWindow />;
  }

  if (overlayOnly) {
    return (
      <main className="overlay-page">
        <RecordingOverlay mode={overlayMode} />
      </main>
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
          <div className={`engine-state is-${runtime.state}`} role="status">
            <i aria-hidden="true" />
            <span>{runtime.message}</span>
          </div>
          <div className="sidebar-privacy">
            <LockKeyhole size={14} />
            <span>Processing stays on this device</span>
          </div>
        </div>
      </aside>

      <section className="workspace">
        <div className="content-scroll">
          <RecoveryBanner />
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
              onDetectProviders={refreshProviders}
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
