import { AlertTriangle, Check, LockKeyhole, Mic, PenLine } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { dismissVoiceOverlay } from "../tauri";
import type { Mode, ScribeReviewDraft } from "../types";
import { useAudioLevels } from "../hooks/useAudioLevels";
import { Waveform, type WaveformVariant } from "./Waveform";
import { ScribeReviewWindow } from "./ScribeReviewWindow";

export type VoicePillPhase =
  | "idle"
  | "recording"
  | "stopping"
  | "transcribing"
  | "processing"
  | "refining"
  | "generating"
  | "reviewing"
  | "collapsing"
  | "complete"
  | "error"
  | "dismissing";

interface VoicePillEvent {
  state: Exclude<VoicePillPhase, "idle" | "dismissing">;
  mode?: Mode | null;
  message?: string | null;
  preview?: string | null;
}

interface RecordingOverlayProps {
  mode: Mode;
  locked?: boolean;
  initialPhase?: VoicePillPhase;
  demoReview?: ScribeReviewDraft | null;
}

const PROCESSING_PHASES: VoicePillPhase[] = [
  "transcribing",
  "processing",
  "refining",
];

function phaseLabel(phase: VoicePillPhase, mode: Mode, message?: string | null) {
  if (phase === "recording") return `${mode === "dictation" ? "Dictation" : "Scribe"} is listening`;
  if (phase === "stopping") return "Recording stopped";
  if (phase === "transcribing") return "Transcribing speech";
  if (phase === "refining") return "Refining dictation";
  if (phase === "generating") return "Composing Scribe draft";
  if (phase === "reviewing") return "Review Scribe draft";
  if (phase === "collapsing") return "Closing Scribe review";
  if (phase === "complete") return mode === "dictation" ? "Text inserted" : "Scribe draft ready";
  if (phase === "error") return message || "Couldn't process audio";
  return `${mode === "dictation" ? "Dictation" : "Scribe"} ready`;
}

function previewText(value?: string | null) {
  const normalized = value?.replace(/\s+/g, " ").trim();
  if (!normalized) return "Done";
  return normalized.length > 76 ? `${normalized.slice(0, 73).trimEnd()}…` : normalized;
}

export function RecordingOverlay({
  mode,
  locked = false,
  initialPhase = "recording",
  demoReview = null,
}: RecordingOverlayProps) {
  const [phase, setPhase] = useState<VoicePillPhase>(initialPhase);
  const [message, setMessage] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const levels = useAudioLevels(mode);
  const phaseRef = useRef(phase);
  const transitionTimer = useRef<number | null>(null);
  const dismissTimer = useRef<number | null>(null);
  const hideTimer = useRef<number | null>(null);

  useEffect(() => {
    phaseRef.current = phase;
  }, [phase]);

  useEffect(() => {
    const clearTimers = () => {
      for (const timer of [transitionTimer, dismissTimer, hideTimer]) {
        if (timer.current !== null) window.clearTimeout(timer.current);
        timer.current = null;
      }
    };

    const scheduleDismiss = (holdMs: number) => {
      if (dismissTimer.current !== null) window.clearTimeout(dismissTimer.current);
      if (hideTimer.current !== null) window.clearTimeout(hideTimer.current);
      dismissTimer.current = window.setTimeout(() => setPhase("dismissing"), holdMs);
      hideTimer.current = window.setTimeout(() => void dismissVoiceOverlay(), holdMs + 210);
    };

    const applyEvent = (event: VoicePillEvent) => {
      if (event.mode && event.mode !== mode) return;
      if (transitionTimer.current !== null) {
        window.clearTimeout(transitionTimer.current);
        transitionTimer.current = null;
      }

      setMessage(event.message ?? null);
      if (event.preview) setPreview(event.preview);

      if (event.state === "recording") {
        clearTimers();
        setPreview(null);
        setPhase("recording");
        return;
      }

      if (event.state === "stopping") {
        setPhase("stopping");
        transitionTimer.current = window.setTimeout(() => setPhase("transcribing"), 360);
        return;
      }

      if (event.state === "complete") {
        if (mode === "scribe") {
          setPreview(event.preview ?? null);
          setPhase("reviewing");
          return;
        }
        const reveal = () => {
          setPhase("complete");
          scheduleDismiss(820);
        };
        if (phaseRef.current === "stopping") {
          transitionTimer.current = window.setTimeout(reveal, 380);
        } else {
          reveal();
        }
        return;
      }

      if (event.state === "error") {
        setPhase("error");
        scheduleDismiss(2400);
        return;
      }

      setPhase(event.state);
    };

    if (!isTauri()) return clearTimers;
    const unlisten = listen<VoicePillEvent>("voice-pill://state", (event) =>
      applyEvent(event.payload),
    );
    return () => {
      clearTimers();
      void unlisten.then((dispose) => dispose());
    };
  }, [mode]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (phaseRef.current === "reviewing") return;
        setPhase("dismissing");
        window.setTimeout(() => void dismissVoiceOverlay(), 200);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const isDictation = mode === "dictation";
  const isProcessing = PROCESSING_PHASES.includes(phase);
  const waveformVariant: WaveformVariant =
    phase === "recording"
      ? "recording"
      : phase === "refining"
        ? "refining"
        : phase === "stopping"
          ? "settling"
          : "idle";
  const Icon = isDictation ? Mic : PenLine;
  const composerVisible = phase === "reviewing" || phase === "collapsing";

  const showInserted = () => {
    setPreview("Inserted");
    setMessage("Scribe draft inserted");
    setPhase("collapsing");
    transitionTimer.current = window.setTimeout(() => setPhase("complete"), 280);
    dismissTimer.current = window.setTimeout(() => setPhase("dismissing"), 1000);
    hideTimer.current = window.setTimeout(() => void dismissVoiceOverlay(), 1210);
  };

  const closeComposer = () => {
    setPhase("collapsing");
    hideTimer.current = window.setTimeout(() => void dismissVoiceOverlay(), 240);
  };

  return (
    <div className={`voice-pill-wrap is-${phase}${composerVisible ? " has-composer" : ""}`}>
      <div
        className={`voice-pill mode-${mode} is-${phase}`}
        role={composerVisible ? undefined : "status"}
        aria-hidden={composerVisible || undefined}
        aria-live={composerVisible ? undefined : phase === "error" ? "assertive" : "polite"}
        aria-atomic="true"
        aria-busy={phase === "stopping" || isProcessing || phase === "generating"}
      >
        <span className="voice-pill__badge" aria-hidden="true">
          <span className="voice-pill__badge-icon">
            {phase === "complete" ? (
              <Check size={18} strokeWidth={2.1} />
            ) : phase === "error" ? (
              <AlertTriangle size={16} strokeWidth={2} />
            ) : (
              <Icon size={17} strokeWidth={1.9} />
            )}
          </span>
          {locked && phase === "recording" ? (
            <LockKeyhole className="voice-pill__lock" size={9} strokeWidth={2.2} />
          ) : null}
        </span>

        <span className="voice-pill__stage" aria-hidden="true">
          <span className="voice-pill__layer voice-pill__wave-layer">
            <Waveform variant={waveformVariant} levels={levels} />
          </span>

          <span className="voice-pill__layer voice-pill__processing-layer">
            <span className="voice-pill__dots">
              {Array.from({ length: 12 }, (_, index) => (
                <i key={index} style={{ animationDelay: `${index * 56}ms` }} />
              ))}
            </span>
            <span className="voice-pill__ring" />
          </span>

          <span className="voice-pill__layer voice-pill__generation-layer">
            <span className="voice-pill__text-lines">
              <i />
              <i />
              <i />
            </span>
          </span>

          <span className="voice-pill__layer voice-pill__complete-layer">
            <span>{previewText(preview)}</span>
          </span>

          <span className="voice-pill__layer voice-pill__error-layer">
            <span>{message || "Couldn't process audio"}</span>
            <small>Try again</small>
          </span>
        </span>

        <span className="sr-only">{phaseLabel(phase, mode, message)}</span>
      </div>
      {composerVisible && mode === "scribe" ? (
        <ScribeReviewWindow
          embedded
          initialReview={demoReview}
          onInserted={showInserted}
          onDiscarded={closeComposer}
        />
      ) : null}
    </div>
  );
}
