import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";

interface AudioLevelPayload {
  mode?: string;
  levels: number[];
}

/**
 * Live microphone amplitudes (one value per visualiser bar) streamed from the
 * backend while a session is recording. Returns `null` until the first frame
 * arrives, and again once the stream goes quiet, so the waveform can fall back
 * to its idle animation instead of drawing a flat line.
 */
export function useAudioLevels(): number[] | null {
  const [levels, setLevels] = useState<number[] | null>(null);
  const idleTimer = useRef<number | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    const unlisten = listen<AudioLevelPayload>("runtime://audio-level", (event) => {
      if (!alive) return;
      const next = event.payload?.levels;
      if (!Array.isArray(next)) return;
      setLevels(next);
      if (idleTimer.current) window.clearTimeout(idleTimer.current);
      // If frames stop arriving, release the live state after a short grace
      // period so the pill returns to its idle shimmer.
      idleTimer.current = window.setTimeout(() => setLevels(null), 320);
    });
    return () => {
      alive = false;
      if (idleTimer.current) window.clearTimeout(idleTimer.current);
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  return levels;
}
