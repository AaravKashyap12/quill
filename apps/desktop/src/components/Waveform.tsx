import { useEffect, useRef } from "react";

/*
 * A voice-shaped equaliser. 22 vertical bars mirrored around the centre line,
 * with soft rounded caps so it reads like real speech rather than a synthetic
 * zigzag. Live: interpolated from the backend's per-frame audio levels. Idle:
 * a slow sine-wave breathing so the pill never looks dead.
 *
 * Transforms run through `scaleY` on the bars so animation stays on the GPU.
 */

const BAR_COUNT = 22;
const BAR_WIDTH = 2.4;
const BAR_GAP = 1.2;
const W = BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP;
const H = 22;
const MIN_SCALE = 0.14;
const SMOOTHING = 0.32; // 0..1, higher = snappier

/** Distribute a small number of backend levels across all bars, weighted
 *  toward the centre so a voice pill visually peaks in the middle. */
function projectLevels(levels: number[]): number[] {
  const source = levels.map((level) => Math.min(1, Math.abs(level) * 3.2));
  const out = new Array<number>(BAR_COUNT);
  for (let i = 0; i < BAR_COUNT; i += 1) {
    const t = (i + 0.5) / BAR_COUNT;
    const position = t * (source.length - 1);
    const lo = Math.floor(position);
    const hi = Math.min(source.length - 1, lo + 1);
    const frac = position - lo;
    const sampled = source[lo] * (1 - frac) + source[hi] * frac;
    // Bell-shaped envelope: 1 at centre, ~0.55 at the edges.
    const envelope = 0.55 + 0.45 * Math.sin(Math.PI * t);
    out[i] = Math.max(MIN_SCALE, Math.min(1, Math.pow(sampled, 0.72) * envelope));
  }
  return out;
}

/** Idle breathing when no live audio is streaming. */
function idleLevels(time: number): number[] {
  const out = new Array<number>(BAR_COUNT);
  for (let i = 0; i < BAR_COUNT; i += 1) {
    const t = (i + 0.5) / BAR_COUNT;
    const envelope = 0.55 + 0.45 * Math.sin(Math.PI * t);
    // Two travelling sines at different frequencies + phase offsets, so the
    // pattern never repeats obviously.
    const wave =
      0.5 +
      0.28 * Math.sin(time * 0.0034 + i * 0.62) +
      0.16 * Math.sin(time * 0.0071 - i * 0.31);
    out[i] = Math.max(MIN_SCALE, Math.min(1, wave * envelope));
  }
  return out;
}

export function Waveform({
  active,
  levels,
}: {
  active: boolean;
  levels?: number[] | null;
}) {
  const barsRef = useRef<Array<SVGRectElement | null>>([]);
  const stateRef = useRef<number[]>(new Array(BAR_COUNT).fill(MIN_SCALE));
  const targetRef = useRef<number[]>(new Array(BAR_COUNT).fill(MIN_SCALE));

  // Whenever a fresh audio frame arrives, retarget the bars. The RAF loop
  // below eases the current state toward the target for smoothness.
  useEffect(() => {
    if (!active) {
      targetRef.current = new Array(BAR_COUNT).fill(MIN_SCALE);
      return;
    }
    if (levels && levels.length > 0) {
      targetRef.current = projectLevels(levels);
    }
  }, [active, levels]);

  useEffect(() => {
    let frame = 0;
    const loop = (time: number) => {
      const useIdle = active && (!levels || levels.length === 0);
      const target = useIdle ? idleLevels(time) : targetRef.current;
      const current = stateRef.current;
      for (let i = 0; i < BAR_COUNT; i += 1) {
        current[i] = current[i] + (target[i] - current[i]) * SMOOTHING;
        const bar = barsRef.current[i];
        if (bar) bar.setAttribute("transform", `scale(1 ${current[i].toFixed(3)})`);
      }
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
  }, [active, levels]);

  return (
    <span className="waveform" aria-hidden="true">
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet">
        {Array.from({ length: BAR_COUNT }, (_, i) => {
          const x = i * (BAR_WIDTH + BAR_GAP);
          return (
            <g
              key={i}
              transform={`translate(${x + BAR_WIDTH / 2} ${H / 2})`}
            >
              <rect
                ref={(node) => {
                  barsRef.current[i] = node;
                }}
                className="waveform__bar"
                x={-BAR_WIDTH / 2}
                y={-H / 2}
                width={BAR_WIDTH}
                height={H}
                rx={BAR_WIDTH / 2}
                transform={`scale(1 ${MIN_SCALE})`}
              />
            </g>
          );
        })}
      </svg>
    </span>
  );
}
