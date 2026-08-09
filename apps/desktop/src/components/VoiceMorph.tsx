import { motion, useReducedMotion, type TargetAndTransition } from "motion/react";
import { useEffect, useMemo, useRef } from "react";
import type { Mode } from "../types";
import type { VoicePillPhase } from "./RecordingOverlay";

const PRIMITIVE_COUNT = 18;
const STAGE_WIDTH = 168;
const BAR_WIDTH = 3;
const BAR_STEP = 8.45;
const BAR_START = (STAGE_WIDTH - ((PRIMITIVE_COUNT - 1) * BAR_STEP + BAR_WIDTH)) / 2;
const DOT_STEP = 7.4;
const PROCESSING_START = 7.75;
const RING_X = 136;
const GENERATION_START = 31;
const MIN_SCALE = 0.06;
const EASE = [0.22, 1, 0.36, 1] as const;
const DEFAULT_WAVE = Array.from({ length: PRIMITIVE_COUNT }, (_, index) => {
  const centre = Math.sin(((index + 0.5) / PRIMITIVE_COUNT) * Math.PI);
  return 0.16 + centre * (0.42 + 0.12 * Math.sin(index * 1.45));
});

function projectedLevels(levels?: number[] | null) {
  if (!levels?.length) return new Array(PRIMITIVE_COUNT).fill(MIN_SCALE);
  return Array.from({ length: PRIMITIVE_COUNT }, (_, index) => {
    const position = ((index + 0.5) / PRIMITIVE_COUNT) * (levels.length - 1);
    const lower = Math.floor(position);
    const upper = Math.min(levels.length - 1, lower + 1);
    const mix = position - lower;
    const sample =
      Math.abs(levels[lower] ?? 0) * (1 - mix) +
      Math.abs(levels[upper] ?? 0) * mix;
    const centre = 0.45 + 0.55 * Math.sin(((index + 0.5) / PRIMITIVE_COUNT) * Math.PI);
    return Math.max(
      MIN_SCALE,
      Math.min(1, Math.pow(Math.min(1, sample * 5.2), 0.62) * centre),
    );
  });
}

function refinementPeak(index: number) {
  const centre = 0.42 + 0.58 * Math.sin(((index + 0.5) / PRIMITIVE_COUNT) * Math.PI);
  const ripple = 0.78 + 0.22 * Math.sin(index * 0.92);
  return Math.max(0.16, centre * ripple);
}

interface PrimitiveTarget {
  x: number;
  y: number;
  scaleX: number | number[];
  scaleY: number | number[];
  opacity: number | number[];
  borderRadius: number;
  backgroundColor: string;
}

function targetFor(
  phase: VoicePillPhase,
  mode: Mode,
  index: number,
  levels: number[],
): PrimitiveTarget {
  const accent = mode === "scribe" ? "#d0b98e" : "#afc8c4";
  const base: PrimitiveTarget = {
    x: BAR_START + index * BAR_STEP,
    y: 0,
    scaleX: 1,
    scaleY: MIN_SCALE,
    opacity: 0.34,
    borderRadius: 999,
    backgroundColor: accent,
  };

  if (phase === "recording") {
    return { ...base, scaleY: levels[index], opacity: 0.94 };
  }
  if (phase === "stopping") {
    const held = Math.max(0.1, levels[index]);
    return {
      ...base,
      scaleX: [1, 1.2, 1.7],
      scaleY: [held, Math.max(0.1, held * 0.52), MIN_SCALE],
      opacity: [0.94, 0.62, 0.27],
    };
  }
  if (phase === "transcribing" || phase === "processing") {
    if (index === PRIMITIVE_COUNT - 1) {
      return {
        ...base,
        x: RING_X + 10.5,
        scaleX: 0.92,
        scaleY: 0.075,
        opacity: 0,
      };
    }
    return {
      ...base,
      x: PROCESSING_START + index * DOT_STEP,
      scaleX: 0.92,
      scaleY: 0.075,
      opacity: [0.22, 0.88, 0.22],
      backgroundColor: "#efebe2",
    };
  }
  if (phase === "refining") {
    return {
      ...base,
      scaleY: [MIN_SCALE, refinementPeak(index), MIN_SCALE],
      opacity: 0.9,
    };
  }
  if (phase === "generating") {
    const row = Math.floor(index / 6);
    const column = index % 6;
    const lineWidths = [6, 4, 5];
    return {
      ...base,
      x: GENERATION_START + column * 17.5,
      y: (row - 1) * 9,
      scaleX: column < lineWidths[row] ? 5.25 : 0.25,
      scaleY: 0.07,
      opacity: column < lineWidths[row] ? [0.28, 0.9, 0.46] : 0.06,
    };
  }
  if (phase === "idle") {
    return {
      ...base,
      scaleX: 1.65,
      scaleY: [MIN_SCALE, 0.1, MIN_SCALE],
      opacity: [0.2, 0.48, 0.2],
    };
  }
  return {
    ...base,
    scaleY: 0.06,
    opacity: 0,
  };
}

export function VoiceMorph({
  phase,
  mode,
  levels,
}: {
  phase: VoicePillPhase;
  mode: Mode;
  levels?: number[] | null;
}) {
  const reducedMotion = useReducedMotion();
  const projected = useMemo(() => projectedLevels(levels), [levels]);
  const lastActiveLevels = useRef(DEFAULT_WAVE);

  useEffect(() => {
    if (phase === "recording" && projected.some((level) => level > 0.12)) {
      lastActiveLevels.current = projected;
    }
  }, [phase, projected]);

  const displayedLevels = phase === "stopping" ? lastActiveLevels.current : projected;
  const processing = phase === "transcribing" || phase === "processing";

  return (
    <span className="voice-morph" aria-hidden="true">
      {Array.from({ length: PRIMITIVE_COUNT }, (_, index) => {
        const target = targetFor(phase, mode, index, displayedLevels);
        const processingDot =
          processing &&
          index < PRIMITIVE_COUNT - 1;
        const refining = phase === "refining";
        const idle = phase === "idle";
        const stopping = phase === "stopping";
        const generating = phase === "generating";
        return (
          <motion.i
            key={index}
            className="voice-morph__primitive"
            animate={target as TargetAndTransition}
            transition={
              reducedMotion
                ? { duration: 0 }
                : processingDot
                    ? {
                        x: { duration: 0.42, ease: EASE },
                        scaleX: { duration: 0.42, ease: EASE },
                        scaleY: { duration: 0.42, ease: EASE },
                        opacity: {
                          duration: 1.08,
                          delay: index * 0.045,
                          times: [0, 0.35, 1],
                          ease: "easeInOut",
                          repeat: Infinity,
                        },
                      }
                    : stopping
                      ? {
                          scaleX: {
                            duration: 0.42,
                            times: [0, 0.58, 1],
                            ease: EASE,
                            repeat: Infinity,
                            repeatDelay: 0.18,
                          },
                          scaleY: {
                            duration: 0.42,
                            times: [0, 0.58, 1],
                            ease: EASE,
                            repeat: Infinity,
                            repeatDelay: 0.18,
                          },
                          opacity: {
                            duration: 0.42,
                            times: [0, 0.58, 1],
                            ease: EASE,
                            repeat: Infinity,
                            repeatDelay: 0.18,
                          },
                          x: { duration: 0.32, ease: EASE },
                        }
                      : idle
                        ? {
                            scaleY: {
                              duration: 1.65,
                              delay: index * 0.03,
                              times: [0, 0.5, 1],
                              ease: "easeInOut",
                              repeat: Infinity,
                            },
                            opacity: {
                              duration: 1.65,
                              delay: index * 0.03,
                              times: [0, 0.5, 1],
                              ease: "easeInOut",
                              repeat: Infinity,
                            },
                            default: { duration: 0.32, ease: EASE },
                          }
                        : generating
                          ? {
                              x: { duration: 0.42, ease: EASE },
                              y: { duration: 0.42, ease: EASE },
                              scaleX: { duration: 0.42, ease: EASE },
                              scaleY: { duration: 0.42, ease: EASE },
                              opacity: Array.isArray(target.opacity)
                                ? {
                                    duration: 0.96,
                                    delay: (Math.floor(index / 6) * 6 + (index % 6)) * 0.032,
                                    times: [0, 0.42, 1],
                                    ease: "easeInOut",
                                    repeat: Infinity,
                                  }
                                : { duration: 0.24, ease: EASE },
                            }
                  : refining
                    ? {
                        scaleY: {
                          duration: 0.72,
                          delay: index * 0.012,
                          times: [0, 0.48, 1],
                          ease: EASE,
                          repeat: Infinity,
                          repeatDelay: 0.16,
                        },
                        default: { duration: 0.28, ease: EASE },
                      }
                    : phase === "recording"
                      ? { type: "spring", stiffness: 620, damping: 42, mass: 0.16 }
                      : { duration: 0.42, ease: EASE }
            }
          />
        );
      })}
      <motion.b
        className="voice-morph__ring"
        initial={false}
        animate={{
          opacity: processing ? 1 : 0,
          scale: processing ? 1 : 0.08,
          rotate: processing ? 360 : 0,
        }}
        transition={
          reducedMotion
            ? { duration: 0 }
            : {
                opacity: { duration: 0.2, ease: EASE },
                scale: { duration: 0.42, ease: EASE },
                rotate: processing
                  ? { duration: 1.05, ease: "linear", repeat: Infinity }
                  : { duration: 0.2, ease: EASE },
              }
        }
      />
    </span>
  );
}
