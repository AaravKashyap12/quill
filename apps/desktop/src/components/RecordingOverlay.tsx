import { AlertTriangle, Check } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { dismissVoiceOverlay } from "../tauri";
import type { Mode, ScribeReviewDraft } from "../types";
import { useAudioLevels } from "../hooks/useAudioLevels";
import { ScribeReviewWindow } from "./ScribeReviewWindow";
import { VoiceMorph } from "./VoiceMorph";

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
  controlledPhase?: VoicePillPhase;
  demoLevels?: number[] | null;
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
  initialPhase = "recording",
  controlledPhase,
  demoLevels,
  demoReview = null,
}: RecordingOverlayProps) {
  const [phase, setPhase] = useState<VoicePillPhase>(initialPhase);
  const [message, setMessage] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const liveLevels = useAudioLevels(mode);
  const levels = demoLevels ?? liveLevels;
  const reducedMotion = useReducedMotion();
  const phaseRef = useRef(phase);
  const transitionTimer = useRef<number | null>(null);
  const dismissTimer = useRef<number | null>(null);
  const hideTimer = useRef<number | null>(null);

  useEffect(() => {
    phaseRef.current = phase;
  }, [phase]);

  useEffect(() => {
    if (controlledPhase) setPhase(controlledPhase);
  }, [controlledPhase]);

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

  const isProcessing = PROCESSING_PHASES.includes(phase);
  const composerVisible = phase === "reviewing";

  const showInserted = () => {
    setPreview("Inserted");
    setMessage("Scribe draft inserted");
    setPhase("collapsing");
    transitionTimer.current = window.setTimeout(() => setPhase("complete"), 280);
    dismissTimer.current = window.setTimeout(() => setPhase("dismissing"), 1000);
    hideTimer.current = window.setTimeout(() => void dismissVoiceOverlay(), 1210);
  };

  const closeComposer = () => {
    setPhase("dismissing");
    hideTimer.current = window.setTimeout(() => void dismissVoiceOverlay(), 220);
  };

  const expanded = composerVisible;
  const completed = phase === "complete" || phase === "collapsing";
  const showStatusBadge = completed || phase === "error";
  const resolvedPreview = previewText(preview);
  const successWidth = Math.min(300, Math.max(120, 84 + resolvedPreview.length * 6.2));
  const compactWidth = phase === "error" ? 300 : completed ? successWidth : 198;
  const surfaceTransition = reducedMotion
    ? { duration: 0 }
    : { duration: expanded ? 0.42 : 0.3, ease: [0.22, 1, 0.36, 1] as const };

  return (
    <motion.div
      className={`voice-pill-wrap is-${phase}${composerVisible ? " has-composer" : ""}`}
      animate={{ width: expanded ? 480 : 238, height: expanded ? 390 : 72 }}
      transition={surfaceTransition}
    >
      <motion.section
        layout
        className={`voice-pill mode-${mode} is-${phase}`}
        initial={reducedMotion ? false : { opacity: 0, scale: 0.98 }}
        animate={{
          width: expanded ? 456 : compactWidth,
          height: expanded ? 338 : 58,
          borderRadius: expanded ? 20 : 999,
          opacity: phase === "dismissing" ? 0 : 1,
          y: phase === "dismissing" ? 4 : 0,
          scale: phase === "dismissing" ? 0.98 : 1,
        }}
        transition={surfaceTransition}
        role={composerVisible ? undefined : "status"}
        aria-hidden={composerVisible || undefined}
        aria-live={composerVisible ? undefined : phase === "error" ? "assertive" : "polite"}
        aria-atomic="true"
        aria-busy={phase === "stopping" || isProcessing || phase === "generating"}
      >
        <AnimatePresence initial={false} mode="popLayout">
          {composerVisible && mode === "scribe" ? (
            <motion.div
              key="composer"
              className="voice-pill__composer-content"
              initial={reducedMotion ? false : { opacity: 0, y: 3 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 2 }}
              transition={{ duration: reducedMotion ? 0 : 0.22, delay: reducedMotion ? 0 : 0.14 }}
            >
              <ScribeReviewWindow
                embedded
                initialReview={demoReview}
                onInserted={showInserted}
                onDiscarded={closeComposer}
              />
            </motion.div>
          ) : (
            <motion.div
              key="compact"
              className={`voice-pill__compact${showStatusBadge ? " has-status-badge" : ""}`}
              initial={reducedMotion ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: reducedMotion ? 0 : 0.16 }}
            >
              <AnimatePresence initial={false}>
                {showStatusBadge ? (
                  <motion.span
                    className="voice-pill__badge"
                    layout
                    aria-hidden="true"
                    initial={reducedMotion ? false : { opacity: 0, scale: 0.85 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.92 }}
                  >
                  <motion.span
                    className="voice-pill__badge-icon"
                    initial={reducedMotion ? false : { opacity: 0, scale: 0.85 }}
                    animate={{ opacity: 1, scale: 1 }}
                    transition={
                      completed && !reducedMotion
                        ? { duration: 0.25, ease: [0.34, 1.25, 0.64, 1] }
                        : { duration: reducedMotion ? 0 : 0.14 }
                    }
                  >
                    {completed ? (
                      <Check size={18} strokeWidth={2.1} />
                    ) : (
                      <AlertTriangle size={16} strokeWidth={2} />
                    )}
                  </motion.span>
                  </motion.span>
                ) : null}
              </AnimatePresence>

              <span className="voice-pill__stage" aria-hidden="true">
                <VoiceMorph phase={phase} mode={mode} levels={levels} />
                <AnimatePresence initial={false}>
                  {completed ? (
                    <motion.span
                      key="complete-copy"
                      className="voice-pill__resolved-copy"
                      initial={reducedMotion ? false : { opacity: 0, y: 3 }}
                      animate={{ opacity: 1, y: 0 }}
                      exit={{ opacity: 0 }}
                      transition={{ duration: reducedMotion ? 0 : 0.26, delay: reducedMotion ? 0 : 0.08 }}
                    >
                      {resolvedPreview}
                    </motion.span>
                  ) : phase === "error" ? (
                    <motion.span
                      key="error-copy"
                      className="voice-pill__resolved-copy is-error"
                      initial={reducedMotion ? false : { opacity: 0, y: 2 }}
                      animate={{ opacity: 1, y: 0 }}
                    >
                      {message || "Couldn't process audio"}<small>Retry</small>
                    </motion.span>
                  ) : null}
                </AnimatePresence>
              </span>
              <span className="sr-only">{phaseLabel(phase, mode, message)}</span>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.section>
    </motion.div>
  );
}
