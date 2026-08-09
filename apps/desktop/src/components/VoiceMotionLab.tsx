import { Pause, Play, RotateCcw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Mode, ScribeReviewDraft } from "../types";
import { RecordingOverlay, type VoicePillPhase } from "./RecordingOverlay";

const DICTATION_SEQUENCE: Array<{ phase: VoicePillPhase; hold: number }> = [
  { phase: "idle", hold: 650 },
  { phase: "recording", hold: 2500 },
  { phase: "stopping", hold: 420 },
  { phase: "transcribing", hold: 1250 },
  { phase: "refining", hold: 880 },
  { phase: "complete", hold: 1050 },
  { phase: "dismissing", hold: 480 },
];

const SCRIBE_SEQUENCE: Array<{ phase: VoicePillPhase; hold: number }> = [
  { phase: "idle", hold: 650 },
  { phase: "recording", hold: 2500 },
  { phase: "stopping", hold: 420 },
  { phase: "transcribing", hold: 1050 },
  { phase: "generating", hold: 1050 },
  { phase: "reviewing", hold: 2600 },
];

const demoReview: ScribeReviewDraft = {
  id: 1,
  source: "Tell Jordan I can do 5 PM tomorrow and send my calendar link.",
  draft:
    "Hi Jordan,\n\n5 PM tomorrow works for me. You can use my calendar link to schedule it.\n\nTalk soon.",
  warning: null,
  register: "email",
};

export function VoiceMotionLab() {
  const [mode, setMode] = useState<Mode>("dictation");
  const [phase, setPhase] = useState<VoicePillPhase>("idle");
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [inputSource, setInputSource] = useState<"demo" | "microphone">("demo");
  const [microphoneError, setMicrophoneError] = useState<string | null>(null);
  const [levels, setLevels] = useState<number[]>(new Array(18).fill(0));
  const stepRef = useRef(0);
  const microphoneStream = useRef<MediaStream | null>(null);
  const audioContext = useRef<AudioContext | null>(null);
  const analyser = useRef<AnalyserNode | null>(null);
  const sequence = mode === "dictation" ? DICTATION_SEQUENCE : SCRIBE_SEQUENCE;

  useEffect(() => {
    if (!playing) return;
    const current = sequence[stepRef.current] ?? sequence[0];
    setPhase(current.phase);
    const timer = window.setTimeout(() => {
      stepRef.current = (stepRef.current + 1) % sequence.length;
      if (stepRef.current === 0) setPlaying(false);
      else setPhase(sequence[stepRef.current].phase);
    }, current.hold / speed);
    return () => window.clearTimeout(timer);
  }, [playing, phase, sequence, speed]);

  useEffect(() => {
    if (phase !== "recording") {
      setLevels(new Array(18).fill(0));
      return;
    }
    let frame = 0;
    const started = performance.now();
    const spectrum = new Uint8Array(analyser.current?.frequencyBinCount ?? 128);
    const tick = (time: number) => {
      const t = (time - started) / 1000;
      if (inputSource === "microphone" && analyser.current) {
        analyser.current.getByteFrequencyData(spectrum);
        setLevels(
          Array.from({ length: 18 }, (_, index) => {
            const normalized = index / 17;
            const bin = Math.min(spectrum.length - 1, Math.round(2 + normalized ** 1.55 * 62));
            const nearby = Math.max(
              spectrum[bin] ?? 0,
              spectrum[Math.max(0, bin - 1)] ?? 0,
              spectrum[Math.min(spectrum.length - 1, bin + 1)] ?? 0,
            );
            return Math.max(0.01, nearby / 255);
          }),
        );
      } else {
        setLevels(
          Array.from({ length: 18 }, (_, index) => {
            const envelope = 0.3 + 0.7 * Math.abs(Math.sin(t * 3.1 + index * 0.34));
            const speech = 0.16 + 0.2 * Math.sin(t * 7.2) + 0.12 * Math.sin(t * 13.7);
            return Math.max(0.025, speech * envelope);
          }),
        );
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [inputSource, phase]);

  useEffect(
    () => () => {
      microphoneStream.current?.getTracks().forEach((track) => track.stop());
      void audioContext.current?.close();
    },
    [],
  );

  async function enableMicrophone() {
    if (inputSource === "microphone") {
      microphoneStream.current?.getTracks().forEach((track) => track.stop());
      microphoneStream.current = null;
      analyser.current = null;
      void audioContext.current?.close();
      audioContext.current = null;
      setInputSource("demo");
      return;
    }
    setMicrophoneError(null);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          autoGainControl: false,
          echoCancellation: false,
          noiseSuppression: false,
        },
      });
      const context = new AudioContext();
      const nextAnalyser = context.createAnalyser();
      nextAnalyser.fftSize = 256;
      nextAnalyser.smoothingTimeConstant = 0.68;
      context.createMediaStreamSource(stream).connect(nextAnalyser);
      microphoneStream.current = stream;
      audioContext.current = context;
      analyser.current = nextAnalyser;
      setInputSource("microphone");
      setPlaying(false);
      stepRef.current = sequence.findIndex((entry) => entry.phase === "recording");
      setPhase("recording");
    } catch (reason) {
      setMicrophoneError(`Microphone unavailable: ${String(reason)}`);
    }
  }

  const phases = useMemo(
    () => Array.from(new Set(sequence.map((item) => item.phase))),
    [sequence],
  );

  function chooseMode(next: Mode) {
    setPlaying(false);
    stepRef.current = 0;
    setMode(next);
    setPhase("idle");
  }

  function play() {
    if (playing) {
      setPlaying(false);
      return;
    }
    if (phase === "dismissing" || phase === "reviewing") {
      stepRef.current = 0;
      setPhase(sequence[0].phase);
    } else {
      const current = sequence.findIndex((item) => item.phase === phase);
      stepRef.current = Math.max(0, current);
    }
    setPlaying(true);
  }

  return (
    <main className="motion-lab">
      <header className="motion-lab__header">
        <div>
          <span>Quill · Motion Lab</span>
          <h1>Voice becomes text without a cut.</h1>
          <p>Production component, deterministic timeline, live geometry.</p>
        </div>
        <a
          href="https://www.figma.com/design/6dTVorYvjVKiV6Ft5zHd8A"
          target="_blank"
          rel="noreferrer"
        >
          Open Figma storyboard
        </a>
      </header>

      <section className="motion-lab__stage" aria-label={`${mode} motion preview`}>
        <RecordingOverlay
          mode={mode}
          controlledPhase={phase}
          demoLevels={levels}
          demoReview={mode === "scribe" ? demoReview : null}
        />
      </section>

      <section className="motion-lab__controls">
        <div className="motion-lab__segmented" aria-label="Voice mode">
          {(["dictation", "scribe"] as Mode[]).map((item) => (
            <button
              key={item}
              type="button"
              className={mode === item ? "is-active" : ""}
              onClick={() => chooseMode(item)}
            >
              {item}
            </button>
          ))}
        </div>

        <div className="motion-lab__transport">
          <button type="button" onClick={play} aria-label={playing ? "Pause" : "Play sequence"}>
            {playing ? <Pause size={15} /> : <Play size={15} />}
          </button>
          <button
            type="button"
            onClick={() => {
              setPlaying(false);
              stepRef.current = 0;
              setPhase("idle");
            }}
            aria-label="Reset sequence"
          >
            <RotateCcw size={14} />
          </button>
          <label>
            Speed
            <select value={speed} onChange={(event) => setSpeed(Number(event.target.value))}>
              <option value={0.5}>0.5×</option>
              <option value={1}>1×</option>
              <option value={2}>2×</option>
            </select>
          </label>
          <button
            type="button"
            className={inputSource === "microphone" ? "is-live" : ""}
            onClick={() => void enableMicrophone()}
          >
            {inputSource === "microphone" ? "Live mic" : "Use mic"}
          </button>
        </div>

        <div className="motion-lab__timeline">
          {phases.map((item, index) => (
            <button
              key={item}
              type="button"
              className={phase === item ? "is-active" : ""}
              onClick={() => {
                setPlaying(false);
                stepRef.current = sequence.findIndex((entry) => entry.phase === item);
                setPhase(item);
              }}
            >
              <i>{String(index + 1).padStart(2, "0")}</i>
              <span>{item}</span>
            </button>
          ))}
        </div>
        {microphoneError ? <p className="motion-lab__input-error">{microphoneError}</p> : null}
      </section>
    </main>
  );
}
