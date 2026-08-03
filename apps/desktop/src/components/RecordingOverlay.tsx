import { Mic, PenLine, LockKeyhole } from "lucide-react";
import type { Mode } from "../types";
import { useAudioLevels } from "../hooks/useAudioLevels";
import { Waveform } from "./Waveform";

interface RecordingOverlayProps {
  mode: Mode;
  locked?: boolean;
}

export function RecordingOverlay({ mode, locked = false }: RecordingOverlayProps) {
  const isDictation = mode === "dictation";
  const Icon = isDictation ? Mic : PenLine;
  const levels = useAudioLevels();
  return (
    <div className="pill-wrap">
      <span className="pill-halo" aria-hidden="true" />
      <div
        className={`pill mode-${mode}`}
        role="status"
        aria-live="polite"
        aria-label={`${isDictation ? "Dictation" : "Scribe"} is listening${locked ? ", locked" : ""}`}
      >
        <span className="pill__badge" aria-hidden="true">
          <Icon size={14} strokeWidth={2} />
          {locked ? <LockKeyhole className="pill__lock" size={9} strokeWidth={2.2} /> : null}
        </span>
        <Waveform active levels={levels} />
      </div>
    </div>
  );
}
