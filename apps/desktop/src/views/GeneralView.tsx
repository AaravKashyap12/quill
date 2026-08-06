import { ShieldCheck } from "lucide-react";
import { ModeShortcut } from "../components/ModeShortcut";
import { SettingRow } from "../components/SettingRow";
import { Switch } from "../components/Switch";
import { detectPlatform } from "../platform";
import type { AppSettings, HotkeyConfig, Mode } from "../types";

interface GeneralViewProps {
  settings: AppSettings;
  update: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void;
  onHotkey: (mode: Mode, value: HotkeyConfig) => void;
  onPreview: (mode: Mode, active: boolean) => void;
}

export function GeneralView({ settings, update, onHotkey, onPreview }: GeneralViewProps) {
  const pasteShortcut = detectPlatform() === "mac" ? "Command+V" : "Ctrl+V";
  return (
    <div className="view-stack">
      <header className="view-heading">
        <h1>Two shortcuts. Always ready.</h1>
      </header>

      <section aria-labelledby="hotkeys-title">
        <div className="section-heading">
          <h2 id="hotkeys-title">Global hotkeys</h2>
          <span>Click a shortcut, then press a new combination.</span>
        </div>
        <div className="shortcut-list">
          <ModeShortcut
            mode="dictation"
            settings={settings}
            onChange={(hotkey) => onHotkey("dictation", hotkey)}
            onPreview={(active) => onPreview("dictation", active)}
          />
          <ModeShortcut
            mode="scribe"
            settings={settings}
            onChange={(hotkey) => onHotkey("scribe", hotkey)}
            onPreview={(active) => onPreview("scribe", active)}
          />
        </div>
      </section>

      <section aria-labelledby="behavior-title">
        <div className="section-heading">
          <h2 id="behavior-title">App behavior</h2>
        </div>
        <div className="settings-group">
          <SettingRow
            label="Dictation activation"
            description="Hold the shortcut, or tap once to start and again to stop."
          >
            <div className="activation-toggle" role="group" aria-label="Dictation activation">
              {(["hold", "tap-to-lock"] as const).map((behavior) => (
                <button
                  key={behavior}
                  type="button"
                  className={settings.dictationHotkey.behavior === behavior ? "is-active" : ""}
                  aria-pressed={settings.dictationHotkey.behavior === behavior}
                  onClick={() =>
                    onHotkey("dictation", { ...settings.dictationHotkey, behavior })
                  }
                >
                  {behavior === "hold" ? "Hold" : "Tap"}
                </button>
              ))}
            </div>
          </SettingRow>
          <SettingRow
            label="Scribe activation"
            description="Configure Scribe independently from Dictation."
          >
            <div className="activation-toggle" role="group" aria-label="Scribe activation">
              {(["hold", "tap-to-lock"] as const).map((behavior) => (
                <button
                  key={behavior}
                  type="button"
                  className={settings.scribeHotkey.behavior === behavior ? "is-active" : ""}
                  aria-pressed={settings.scribeHotkey.behavior === behavior}
                  onClick={() =>
                    onHotkey("scribe", { ...settings.scribeHotkey, behavior })
                  }
                >
                  {behavior === "hold" ? "Hold" : "Tap"}
                </button>
              ))}
            </div>
          </SettingRow>
          <SettingRow
            label="Default style"
            description="Used when Quill can't identify the app you're typing into. Scribe only â€” Dictation types verbatim."
          >
            <select
              value={settings.defaultRegister}
              onChange={(event) =>
                update("defaultRegister", event.target.value as AppSettings["defaultRegister"])
              }
              aria-label="Default style"
            >
              <option value="generic">General text</option>
              <option value="email">Email</option>
              <option value="chat">Chat message</option>
              <option value="prompt">AI prompt</option>
              <option value="notes">Notes</option>
            </select>
          </SettingRow>
          <SettingRow
            label="Launch at startup"
            description="Start Quill in the tray when you sign in."
          >
            <Switch
              label="Launch at startup"
              checked={settings.launchAtStartup}
              onChange={(checked) => update("launchAtStartup", checked)}
            />
          </SettingRow>
          <SettingRow
            label="Crash recovery"
            description="Keep the current audio locally until its text is safely committed."
          >
            <Switch
              label="Keep recovery audio"
              checked={settings.keepRecoveryAudio}
              onChange={(checked) => update("keepRecoveryAudio", checked)}
            />
          </SettingRow>
          <SettingRow
            label="Text insertion"
            description={
              settings.injectionMode === "clipboard"
                ? `Quill briefly copies the text and pastes it with ${pasteShortcut}. Fastest and most reliable — recommended for long passages.`
                : "Quill types each character directly, one keystroke at a time. Works in editors that block paste (some terminals, sandboxed apps) but is slower."
            }
          >
            <select
              value={settings.injectionMode}
              onChange={(event) =>
                update("injectionMode", event.target.value as AppSettings["injectionMode"])
              }
            >
              <option value="clipboard">Clipboard paste (recommended)</option>
              <option value="keystrokes">Simulated typing</option>
            </select>
          </SettingRow>
        </div>
      </section>

      <div className="privacy-note">
        <ShieldCheck size={18} aria-hidden="true" />
        <span>
          <strong>Local means local.</strong> Quill has no analytics client and makes no
          telemetry requests.
        </span>
      </div>
    </div>
  );
}
