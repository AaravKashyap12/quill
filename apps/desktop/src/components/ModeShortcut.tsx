import { Mic, PenLine, Play } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { AppSettings, HotkeyConfig, Mode } from "../types";
import { formatHotkey } from "../defaults";
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

function hotkeyFromEvent(
  event: KeyboardEvent<HTMLButtonElement>,
  behavior: HotkeyConfig["behavior"],
): HotkeyResult | null {
  // Modifier-only presses aren't a full chord yet — wait for the trigger key.
  if (["Control", "Shift", "Alt", "Meta"].includes(event.key)) return null;

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

function describeKey(key: string): string {
  if (/^F\d{1,2}$/.test(key)) return `Function key (${key})`;
  if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(key)) return "Arrow keys";
  if (["Home", "End", "PageUp", "PageDown", "Insert", "Delete"].includes(key)) return "Navigation keys";
  return `"${key}"`;
}

function chordFragments(label: string): string[] {
  return label.split(/\s*[+·]\s*/).map((part) => part.trim()).filter(Boolean);
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
    ? "Raw words, live as you speak"
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

  // Clear the rejection message when the user starts a fresh recording.
  useEffect(() => {
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
        onBlur={() => setRecording(false)}
        aria-label={`Change ${title} hotkey — currently ${hotkeyLabel}`}
        title={rejection ?? undefined}
      >
        {recording ? (
          <span className="hotkey-recorder__prompt">
            {rejection ?? "Press a shortcut…"}
          </span>
        ) : (
          <span className="hotkey-recorder__chord">
            {fragments.map((fragment, index) => (
              <span key={`${fragment}-${index}`}>
                {index > 0 ? <em aria-hidden="true">+</em> : null}
                <kbd>{fragment}</kbd>
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
