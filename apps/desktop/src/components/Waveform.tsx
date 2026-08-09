import { useEffect, useRef } from "react";

const BAR_COUNT = 18;
const BAR_WIDTH = 2.2;
const BAR_GAP = 2.15;
const WIDTH = BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP;
const HEIGHT = 24;
const MIN_SCALE = 0.1;

export type WaveformVariant = "idle" | "recording" | "settling" | "refining";

function projectLevels(levels: number[]): number[] {
  if (levels.length === 0) return new Array(BAR_COUNT).fill(MIN_SCALE);

  return Array.from({ length: BAR_COUNT }, (_, index) => {
    const position = ((index + 0.5) / BAR_COUNT) * (levels.length - 1);
    const lower = Math.floor(position);
    const upper = Math.min(levels.length - 1, lower + 1);
    const mix = position - lower;
    const sample =
      Math.abs(levels[lower] ?? 0) * (1 - mix) + Math.abs(levels[upper] ?? 0) * mix;
    const centreEnvelope = 0.48 + 0.52 * Math.sin(((index + 0.5) / BAR_COUNT) * Math.PI);
    return Math.max(
      MIN_SCALE,
      Math.min(1, Math.pow(Math.min(1, sample * 3.4), 0.7) * centreEnvelope),
    );
  });
}

function refinementLevels(time: number): number[] {
  const progress = Math.min(1, time / 720);
  const envelope = Math.sin(progress * Math.PI);
  return Array.from({ length: BAR_COUNT }, (_, index) => {
    const centreEnvelope = 0.46 + 0.54 * Math.sin(((index + 0.5) / BAR_COUNT) * Math.PI);
    const ripple = 0.72 + 0.28 * Math.sin(index * 0.9 + progress * Math.PI * 2);
    return Math.max(MIN_SCALE, envelope * centreEnvelope * ripple);
  });
}

/**
 * A centre-origin voice meter. Audio frames only update a ref; a single RAF
 * loop performs attack/decay smoothing and writes compositor-friendly SVG
 * transforms. Non-recording variants settle and stop their RAF once complete.
 */
export function Waveform({
  variant,
  levels,
}: {
  variant: WaveformVariant;
  levels?: number[] | null;
}) {
  const barsRef = useRef<Array<SVGRectElement | null>>([]);
  const levelsRef = useRef<number[] | null>(levels ?? null);
  const currentRef = useRef<number[]>(new Array(BAR_COUNT).fill(MIN_SCALE));

  useEffect(() => {
    levelsRef.current = levels ?? null;
  }, [levels]);

  useEffect(() => {
    let frame = 0;
    let startedAt: number | null = null;
    let settledFrames = 0;

    const loop = (time: number) => {
      startedAt ??= time;
      const elapsed = time - startedAt;
      const target =
        variant === "recording" && levelsRef.current
          ? projectLevels(levelsRef.current)
          : variant === "refining"
            ? refinementLevels(elapsed)
            : new Array(BAR_COUNT).fill(MIN_SCALE);
      const current = currentRef.current;
      let largestDelta = 0;

      for (let index = 0; index < BAR_COUNT; index += 1) {
        const difference = target[index] - current[index];
        const smoothing = difference > 0 ? 0.36 : 0.19;
        current[index] += difference * smoothing;
        largestDelta = Math.max(largestDelta, Math.abs(difference));
        const bar = barsRef.current[index];
        if (bar) bar.style.transform = `scaleY(${current[index].toFixed(3)})`;
      }

      const canSettle = variant !== "recording";
      settledFrames = canSettle && largestDelta < 0.004 ? settledFrames + 1 : 0;
      if (settledFrames < 8 && !(variant === "refining" && elapsed > 820)) {
        frame = requestAnimationFrame(loop);
      }
    };

    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
  }, [variant]);

  return (
    <span className="waveform" aria-hidden="true">
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} preserveAspectRatio="none">
        {Array.from({ length: BAR_COUNT }, (_, index) => {
          const x = index * (BAR_WIDTH + BAR_GAP);
          return (
            <rect
              key={index}
              ref={(node) => {
                barsRef.current[index] = node;
              }}
              className="waveform__bar"
              x={x}
              y={0}
              width={BAR_WIDTH}
              height={HEIGHT}
              rx={BAR_WIDTH / 2}
              style={{ transform: `scaleY(${MIN_SCALE})` }}
            />
          );
        })}
      </svg>
    </span>
  );
}
