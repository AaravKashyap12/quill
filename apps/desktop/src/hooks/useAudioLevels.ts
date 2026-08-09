import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import type { Mode } from "../types";

interface AudioLevelPayload {
  mode?: string;
  levels: number[];
}

/**
 * Live microphone amplitudes (one value per visualiser bar) streamed from the
 * backend while a session is recording. Returns `null` until the first frame
 * arrives. Silence is represented by real zero-valued frames; a timer must not
 * reinterpret a temporarily delayed transcription loop as an idle UI state.
 */
export function useAudioLevels(mode?: Mode): number[] | null {
  const [levels, setLevels] = useState<number[] | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    let dispose: (() => void) | undefined;
    void listen<AudioLevelPayload>("runtime://audio-level", (event) => {
      if (!alive) return;
      if (mode && event.payload?.mode && event.payload.mode !== mode) return;
      const next = event.payload?.levels;
      if (!Array.isArray(next) || next.some((level) => !Number.isFinite(level))) return;
      setLevels(next);
    }).then((unlisten) => {
      if (alive) dispose = unlisten;
      else unlisten();
    }).catch(() => {
      if (alive) setLevels(null);
    });
    return () => {
      alive = false;
      dispose?.();
    };
  }, [mode]);

  return levels;
}
