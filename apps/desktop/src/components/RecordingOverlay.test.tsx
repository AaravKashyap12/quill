import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mode } from "../types";
import { RecordingOverlay, type VoicePillPhase } from "./RecordingOverlay";

type Surface = {
  mode: Mode;
  visible: boolean;
  interactive: boolean;
  events: Map<string, (event: { payload: unknown }) => void>;
  keys: Set<(event: { key: string }) => void>;
  dispose: () => void;
};

const harness = vi.hoisted(() => ({
  effects: [] as Array<() => void | (() => void)>,
  current: null as Surface | null,
  dismiss: vi.fn(),
}));

// Match the existing App test's effect harness: execute the component's real
// subscriptions/timers without adding a DOM dependency. This does not test paint.
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useEffect: (effect: () => void | (() => void)) => { harness.effects.push(effect); },
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, callback: (event: { payload: unknown }) => void) => {
    const surface = harness.current!;
    surface.events.set(name, callback);
    return () => { surface.events.delete(name); };
  }),
}));
vi.mock("../tauri", () => ({ dismissVoiceOverlay: () => harness.dismiss(harness.current) }));
vi.mock("../hooks/useAudioLevels", () => ({ useAudioLevels: () => [] }));
vi.mock("./VoiceMorph", () => ({ VoiceMorph: () => null }));
vi.mock("./ScribeReviewWindow", () => ({ ScribeReviewWindow: () => null }));
vi.mock("motion/react", () => ({
  AnimatePresence: ({ children }: { children: React.ReactNode }) => children,
  motion: Object.fromEntries(["div", "section", "span"].map((tag) => [tag,
    ({ children }: { children: React.ReactNode }) => children,
  ])),
  useReducedMotion: () => true,
}));

function within<T>(surface: Surface, run: () => T): T {
  const previous = harness.current;
  harness.current = surface;
  try { return run(); } finally { harness.current = previous; }
}

const surfaces: Surface[] = [];
function mount(mode: Mode, initialPhase: VoicePillPhase = "recording") {
  const surface: Surface = {
    mode, visible: true, interactive: false, events: new Map(), keys: new Set(), dispose: () => {},
  };
  within(surface, () => {
    renderToStaticMarkup(<RecordingOverlay mode={mode} initialPhase={initialPhase} />);
    const cleanups = harness.effects.splice(0).map((effect) => effect());
    surface.dispose = () => { for (const cleanup of cleanups) cleanup?.(); };
  });
  surfaces.push(surface);
  return surface;
}

function emit(surface: Surface, state: string, mode: Mode | null = surface.mode) {
  within(surface, () => surface.events.get("voice-pill://state")!({ payload: { state, mode } }));
}

function activate(surface: Surface, state: "starting" | "recording" | "complete") {
  // Native show_overlay/review::present own visibility and hit testing.
  for (const other of surfaces) other.visible = other === surface;
  surface.interactive = state === "complete";
  emit(surface, state);
}

function escape(surface: Surface) {
  within(surface, () => { for (const listener of surface.keys) listener({ key: "Escape" }); });
}

beforeEach(() => {
  vi.useFakeTimers();
  harness.dismiss.mockReset().mockImplementation((surface: Surface) => {
    // Native boundary contract: Tauri injects the invoking WebviewWindow.
    // This models that command's effects; native dispatch needs a native check.
    surface.visible = false;
    surface.interactive = false;
    return Promise.resolve();
  });
  vi.stubGlobal("window", {
    setTimeout: (callback: () => void, ms: number) => {
      const owner = harness.current!;
      return setTimeout(() => within(owner, callback), ms);
    },
    clearTimeout: (timer: ReturnType<typeof setTimeout>) => clearTimeout(timer),
    addEventListener: (_name: string, callback: (event: { key: string }) => void) => {
      harness.current!.keys.add(callback);
    },
    removeEventListener: (_name: string, callback: (event: { key: string }) => void) => {
      for (const surface of surfaces) surface.keys.delete(callback);
    },
  });
});

afterEach(async () => {
  for (const surface of surfaces.splice(0)) surface.dispose();
  await Promise.resolve();
  harness.effects.length = 0;
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("overlay dismissal lifetime", () => {
  it.each([["complete", 1030], ["error", 2610]] as const)(
    "%s still dismisses its originating surface after the exit animation", (state, deadline) => {
      const dictation = mount("dictation");
      emit(dictation, state);
      vi.advanceTimersByTime(deadline - 1);
      expect(dictation.visible).toBe(true);
      vi.advanceTimersByTime(1);
      expect(dictation.visible).toBe(false);
      expect(harness.dismiss).toHaveBeenCalledExactlyOnceWith(dictation);
    },
  );

  it.each(["starting", "recording", "complete"] as const)(
    "an old Dictation completion cannot hide Scribe %s", (state) => {
      const dictation = mount("dictation");
      const scribe = mount("scribe");
      emit(dictation, "complete");
      vi.advanceTimersByTime(900); // Exit animation has begun; native hide is still pending.
      activate(scribe, state);
      vi.advanceTimersByTime(3000);
      expect(harness.dismiss).toHaveBeenCalledExactlyOnceWith(dictation);
      expect(scribe.visible).toBe(true);
      expect(scribe.interactive).toBe(state === "complete");
    },
  );

  it("a broadcast error cannot dismiss a newly opened Scribe review", () => {
    const dictation = mount("dictation");
    const scribe = mount("scribe");
    emit(dictation, "error", null);
    emit(scribe, "error", null);
    vi.advanceTimersByTime(2500);
    activate(scribe, "complete");
    vi.advanceTimersByTime(3000);
    expect(scribe.visible).toBe(true);
    expect(scribe.interactive).toBe(true);
    expect(harness.dismiss).toHaveBeenCalledExactlyOnceWith(dictation);
  });

  it.each(["starting", "recording"] as const)("%s cancels an Escape dismissal", (state) => {
    const dictation = mount("dictation");
    escape(dictation);
    vi.advanceTimersByTime(100);
    activate(dictation, state);
    vi.advanceTimersByTime(3000);
    expect(dictation.visible).toBe(true);
    expect(harness.dismiss).not.toHaveBeenCalled();
  });

  it("a new processing state cancels the previous error's pending hide", () => {
    const dictation = mount("dictation");
    emit(dictation, "error");
    vi.advanceTimersByTime(2500);
    emit(dictation, "transcribing");
    vi.advanceTimersByTime(3000);
    expect(dictation.visible).toBe(true);
    expect(harness.dismiss).not.toHaveBeenCalled();
  });

  it("unmount cancels Escape timers, including repeated Escape presses", () => {
    const dictation = mount("dictation");
    escape(dictation);
    vi.advanceTimersByTime(100);
    escape(dictation);
    dictation.dispose();
    vi.advanceTimersByTime(3000);
    expect(harness.dismiss).not.toHaveBeenCalled();
  });

  it("Escape does not dismiss an open review", () => {
    const scribe = mount("scribe", "reviewing");
    escape(scribe);
    vi.advanceTimersByTime(3000);
    expect(harness.dismiss).not.toHaveBeenCalled();
  });
});
