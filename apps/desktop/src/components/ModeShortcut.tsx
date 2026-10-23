import { Globe, Mic, PenLine, Play } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { AppSettings, HotkeyConfig, Mode } from "../types";
import { formatHotkey, macShortcutPreset } from "../defaults";
import { isMac } from "../platform";
import { setHotkeyCapture } from "../tauri";

interface ModeShortcutProps {
  mode: Mode;
  settings: AppSettings;
  onChange: (hotkey: HotkeyConfig) => void;
  onPreview: (active: boolean) => void;
}

/** Keep the recorder inside the key vocabulary understood by the native
 * pollers. Command is valid on macOS but remains rejected as the Windows key. */
type HotkeyResult = { ok: true; hotkey: HotkeyConfig } | { ok: false; reason: string };

export function hotkeyFromEvent(
  event: Pick<KeyboardEvent<HTMLButtonElement>, "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">,
  behavior: HotkeyConfig["behavior"],
): HotkeyResult | null {
  // Modifier-only presses aren't a full chord yet — wait for the trigger key.
  if (["Control", "Shift", "Alt", "Meta", "Fn"].includes(event.key)) return null;

  const mac = isMac();
  if (event.metaKey && !mac) {
    return {
      ok: false,
      reason: "Windows key isn't supported yet — try Ctrl, Shift, or Alt.",
    };
  }

  const modifiers = [
    event.metaKey && mac ? "Meta" : "",
    event.ctrlKey ? "Ctrl" : "",
    event.shiftKey ? "Shift" : "",
    event.altKey ? "Alt" : "",
  ].filter(Boolean);

  let key: string;
  if (event.key === " ") {
    key = "Space";
  } else if (event.key.length === 1 && /^[A-Za-z0-9]$/.test(event.key)) {
    key = event.key.toUpperCase();
  } else {
    return {
      ok: false,
      reason: `${describeKey(event.key)} isn't supported yet — pick a letter, digit, or Space.`,
    };
  }

  if (modifiers.length === 0 && key !== "Space") {
    return {
      ok: false,
      reason: mac
        ? "Add at least one modifier (Command, Control, Shift, or Option)."
        : "Add at least one modifier (Ctrl, Shift, or Alt).",
    };
  }

  return { ok: true, hotkey: { modifiers, key, behavior } };
}

type ModifierEvent = Pick<KeyboardEvent<HTMLButtonElement>, "code" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">;

export function standaloneModifierKey(event: ModifierEvent): string | null {
  if (!isMac() || event.ctrlKey || event.shiftKey) return null;
  if (event.code === "AltRight" && !event.metaKey) return "AltRight";
  if (event.code === "MetaRight" && !event.altKey) return "MetaRight";
  return null;
}

export function releasedModifierHotkey(candidate: string | null, event: ModifierEvent): HotkeyConfig | null {
  if (!candidate || candidate !== standaloneModifierKey(event)) return null;
  return { modifiers: [], key: candidate, behavior: "hold" };
}

function describeKey(key: string): string {
  if (/^F\d{1,2}$/.test(key)) return `Function key (${key})`;
  if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(key)) return "Arrow keys";
  if (["Home", "End", "PageUp", "PageDown", "Insert", "Delete"].includes(key)) return "Navigation keys";
  return `"${key}"`;
}

function chordFragments(label: string): string[] {
  return label.split(/\s*[+·]\s*/).map((part) => part.trim()).filter(Boolean);
}

function Keycap({ label }: { label: string }) {
  if (isMac() && label === "Fn") {
    return <kbd title="Fn / Globe"><span>fn</span><Globe size={14} strokeWidth={1.7} aria-hidden="true" /></kbd>;
  }
  if (isMac() && label === "Ctrl ⌃") {
    return <kbd title="Control"><svg width="16" height="14" viewBox="0 0 16 14" fill="none" aria-hidden="true">
      <path d="M3 9.5 8 4.5l5 5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
    </svg></kbd>;
  }
  return <kbd>{label}</kbd>;
}

/** Presets do not depend on WebKit emitting the Apple Fn key event. */
export function MacShortcutPreset({ mode, settings, onChange }: Pick<ModeShortcutProps, "mode" | "settings" | "onChange">) {
  const current = mode === "dictation" ? settings.dictationHotkey : settings.scribeHotkey;
  const presets = ["apple-fn", "no-fn"] as const;
  const selected = presets.find((preset) => {
    const hotkey = macShortcutPreset(mode, preset);
    return hotkey.key === current.key
      && hotkey.modifiers.length === current.modifiers.length
      && hotkey.modifiers.every((modifier) => current.modifiers.includes(modifier));
  }) ?? "custom";
  return (
    <select
      aria-label={`${mode === "dictation" ? "Dictation" : "Scribe"} shortcut preset`}
      value={selected}
      onChange={(event) => {
        const preset = event.target.value;
        if (preset === "apple-fn" || preset === "no-fn") onChange({ ...macShortcutPreset(mode, preset), behavior: current.behavior });
      }}
    >
      <option value="custom" disabled>Custom shortcut</option>
      {presets.map((preset) => (
        <option key={preset} value={preset}>
          {preset === "apple-fn" ? "Apple Fn" : "No Fn"}: {formatHotkey({
            ...settings,
            [mode === "dictation" ? "dictationHotkey" : "scribeHotkey"]: macShortcutPreset(mode, preset),
          }, mode)}
        </option>
      ))}
    </select>
  );
}

export function ModeShortcut({
  mode,
  settings,
  onChange,
  onPreview,
}: ModeShortcutProps) {
  const [recording, setRecording] = useState(false);
  const [rejection, setRejection] = useState<string | null>(null);
  const isDictation = mode === "dictation";
  const Icon = isDictation ? Mic : PenLine;
  const title = isDictation ? "Dictation" : "Scribe";
  const description = isDictation
    ? "Raw words, inserted when you stop"
    : "Corrections resolved before typing";
  const hotkeyLabel = formatHotkey(settings, mode);
  const fragments = chordFragments(hotkeyLabel);

  // The session polls global keys, so entering recording mode has to pause
  // that polling; otherwise pressing Ctrl+Space to bind it also triggers the
  // mode. Backend keeps its key-state in sync so releasing is clean.
  useEffect(() => {
    void setHotkeyCapture(recording);
    return () => {
      if (recording) void setHotkeyCapture(false);
    };
  }, [recording]);

  const modifierCandidate = useRef<string | null>(null);
  const previewActive = useRef(false);
  function pushPreview(active: boolean) {
    if (previewActive.current === active) return;
    previewActive.current = active;
    onPreview(active);
  }

  function record(event: KeyboardEvent<HTMLButtonElement>) {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecording(false);
      setRejection(null);
      return;
    }
    // Wait for release so a right modifier can still begin a custom chord.
    if (!event.repeat) modifierCandidate.current = standaloneModifierKey(event);
    const current =
      mode === "dictation" ? settings.dictationHotkey : settings.scribeHotkey;
    const next = hotkeyFromEvent(event, current.behavior);
    if (!next) return;
    if (!next.ok) {
      setRejection(next.reason);
      return;
    }
    onChange(next.hotkey);
    setRejection(null);
    setRecording(false);
  }

  function recordRelease(event: KeyboardEvent<HTMLButtonElement>) {
    if (!recording) return;
    const next = releasedModifierHotkey(modifierCandidate.current, event);
    modifierCandidate.current = null;
    if (!next) return;
    event.preventDefault();
    event.stopPropagation();
    const current = mode === "dictation" ? settings.dictationHotkey : settings.scribeHotkey;
    onChange({ ...next, behavior: current.behavior });
    setRejection(null);
    setRecording(false);
  }

  // Clear the rejection message when the user starts a fresh recording.
  useEffect(() => {
    modifierCandidate.current = null;
    if (recording) setRejection(null);
  }, [recording]);

  return (
    <article className={`mode-shortcut mode-${mode}`}>
      <div className="mode-symbol" aria-hidden="true">
        <Icon size={19} strokeWidth={1.9} />
      </div>
      <div className="mode-copy">
        <h3>{title}</h3>
        <p>{description}</p>
      </div>
      <button
        className={`hotkey-recorder ${recording ? "is-recording" : ""}${rejection ? " is-rejected" : ""}`}
        type="button"
        onClick={() => setRecording((state) => !state)}
        onKeyDown={record}
        onKeyUp={recordRelease}
        onBlur={() => setRecording(false)}
        aria-label={`Change ${title} hotkey — currently ${hotkeyLabel}`}
        title={rejection ?? undefined}
      >
        {recording ? (
          <span className="hotkey-recorder__prompt">
            {rejection ?? (isMac() ? "Press a shortcut or release a right modifier…" : "Press a shortcut…")}
          </span>
        ) : (
          <span className="hotkey-recorder__chord">
            {fragments.map((fragment, index) => (
              <span key={`${fragment}-${index}`}>
                {index > 0 ? <em aria-hidden="true">+</em> : null}
                <Keycap label={fragment} />
              </span>
            ))}
          </span>
        )}
      </button>
      <button
        className="preview-button"
        type="button"
        onPointerDown={() => pushPreview(true)}
        onPointerUp={() => pushPreview(false)}
        onPointerLeave={() => pushPreview(false)}
        onPointerCancel={() => pushPreview(false)}
        aria-label={`Preview ${title} overlay`}
        title={`Preview ${title}`}
      >
        <Play size={13} strokeWidth={2} fill="currentColor" />
      </button>
    </article>
  );
}
