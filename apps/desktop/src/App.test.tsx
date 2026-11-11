import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { defaultSettings } from "./defaults";
import * as native from "./tauri";
import type { AppSettings } from "./types";
import type { SpeechSetupState } from "./components/FirstRunSetup";
import { listen } from "@tauri-apps/api/event";

const { effects, stateChanges, hooks, view } = vi.hoisted(() => ({
  effects: [] as Array<() => unknown>,
  stateChanges: [] as Array<{ initial: unknown; next: unknown }>,
  hooks: { cursor: 0, values: [] as unknown[] },
  view: { update: null as null | (<K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void), speech: null as SpeechSetupState | null, showScribeSetup: false },
}));

// Collect mount effects without a DOM; child surfaces have separate lifecycles.
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useEffect: (effect: () => unknown) => { effects.push(effect); },
  useState: (initial: unknown) => {
    const index = hooks.cursor++;
    if (index >= hooks.values.length) hooks.values.push(initial);
    return [hooks.values[index], (next: unknown) => {
      hooks.values[index] = typeof next === "function" ? next(hooks.values[index]) : next;
      stateChanges.push({ initial, next: hooks.values[index] });
    }];
  },
  useRef: (initial: unknown) => {
    const index = hooks.cursor++;
    if (index >= hooks.values.length) hooks.values.push({ current: initial });
    return hooks.values[index];
  },
}));
vi.mock("./views/GeneralView", () => ({ GeneralView: ({ update }: { update: typeof view.update }) => {
  view.update = update; return <div>General settings</div>;
} }));
vi.mock("./components/FirstRunSetup", () => ({ FirstRunSetup: ({ speech, showScribeSetup }: { speech: SpeechSetupState; showScribeSetup: boolean }) => {
  view.speech = speech; view.showScribeSetup = showScribeSetup; return <div>{speech.modelId}: {speech.phase}</div>;
} }));
vi.mock("@tauri-apps/api/core", async (original) => ({
  ...await original<typeof import("@tauri-apps/api/core")>(),
  isTauri: () => true,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("./components/RecordingOverlay", () => ({
  RecordingOverlay: ({ mode }: { mode: string }) => <div>Recording {mode}</div>,
}));
vi.mock("./components/ScribeReviewWindow", () => ({
  ScribeReviewWindow: () => <div>Review draft</div>,
}));
vi.mock("./tauri", async (original) => ({
  ...await original<typeof native>(),
  loadSettings: vi.fn(),
  persistSettings: vi.fn(async () => {}),
  listInstalledWhisperModels: vi.fn(async () => ["base.en"]),
  getSystemProfile: vi.fn(async () => ({})),
  getProviderKeyStatus: vi.fn(async (provider: string) => ({ provider, configured: false })),
  testProviderConnection: vi.fn(),
  detectProviders: vi.fn(async () => []),
}));

beforeEach(() => {
  effects.length = 0;
  stateChanges.length = 0;
  hooks.cursor = 0; hooks.values = [];
  view.update = null; view.speech = null;
  vi.clearAllMocks();
  vi.mocked(native.loadSettings).mockResolvedValue({ ...defaultSettings });
  vi.mocked(native.getProviderKeyStatus).mockImplementation(async (provider) => ({
    provider, configured: false, status: "unchecked", message: null,
  }));
});
afterEach(() => vi.unstubAllGlobals());

describe("window startup ownership", () => {
  it("keeps a cached missing-key error visible after the runtime restarts", async () => {
    vi.stubGlobal("window", { location: { search: "" } });
    vi.mocked(native.loadSettings).mockResolvedValueOnce({ ...defaultSettings, transcriptionProvider: "groq", onboardingCompleted: true });
    renderToStaticMarkup(<App />);
    const cleanups = effects.map((effect) => effect());
    try {
      await vi.waitFor(() => expect(native.getProviderKeyStatus).toHaveBeenCalledTimes(2));
      vi.mocked(native.getProviderKeyStatus).mockImplementation(async (provider) => ({
        provider, configured: false, status: "missing", message: null,
      }));
      const callback = vi.mocked(listen).mock.calls.find(([event]) => event === "runtime://status")![1];
      callback({ event: "runtime://status", id: 1, payload: { state: "error", provider: null, mode: null, message: "Missing saved key" } });
      await vi.waitFor(() => {
        hooks.cursor = 0;
        renderToStaticMarkup(<App />);
        expect(view.speech?.phase).toBe("error");
      });
      callback({ event: "runtime://status", id: 1, payload: { state: "ready", provider: null, mode: null, message: "Ready with Groq" } });
      hooks.cursor = 0;
      renderToStaticMarkup(<App />);
      expect(view.speech).toMatchObject({ phase: "error", modelId: "Groq Cloud" });
      expect(native.getProviderKeyStatus).toHaveBeenLastCalledWith("gemini");
      expect(native.testProviderConnection).not.toHaveBeenCalled();
    } finally {
      for (const cleanup of cleanups) if (typeof cleanup === "function") cleanup();
    }
  });
  it.each([
    ["?overlay&mode=dictation", "Recording dictation"],
    ["?overlay&mode=scribe", "Recording scribe"],
    ["?review", "Review draft"],
  ])("keeps settings initialization out of %s", (search, content) => {
    vi.stubGlobal("window", { location: { search } });

    expect(renderToStaticMarkup(<App />)).toContain(content);
    expect(effects).toHaveLength(0);
    expect(native.loadSettings).not.toHaveBeenCalled();
    expect(native.getProviderKeyStatus).not.toHaveBeenCalled();
    expect(native.testProviderConnection).not.toHaveBeenCalled();
    expect(native.persistSettings).not.toHaveBeenCalled();
  });

  it("still loads settings, checks providers and migrates installed models in main", async () => {
    vi.stubGlobal("window", { location: { search: "" } });
    expect(renderToStaticMarkup(<App />)).toContain("Checking this computer");
    const cleanups = effects.map((effect) => effect());
    try {
      await vi.waitFor(() => {
        expect(native.loadSettings).toHaveBeenCalledOnce();
        expect(native.getProviderKeyStatus).toHaveBeenCalledWith("groq", false);
        expect(native.getProviderKeyStatus).toHaveBeenCalledWith("gemini", false);
        hooks.cursor = 0;
        renderToStaticMarkup(<App />);
        expect(view.speech?.phase).toBe("ready");
        expect(view.speech?.error).toBeNull();
        expect(native.detectProviders).toHaveBeenCalledOnce();
        expect(native.persistSettings).toHaveBeenCalledWith(expect.objectContaining({
          onboardingCompleted: true,
          speechModelSetupAttempted: true,
        }));
      });
    } finally {
      for (const cleanup of cleanups) if (typeof cleanup === "function") cleanup();
    }
  });

  it("keeps cloud setup unchanged when a credential is inaccessible", async () => {
    vi.stubGlobal("window", { location: { search: "" } });
    vi.mocked(native.loadSettings).mockResolvedValueOnce({ ...defaultSettings, transcriptionProvider: "groq" });
    vi.mocked(native.getProviderKeyStatus).mockResolvedValueOnce({
      provider: "groq", configured: false, status: "error", message: "Keychain access denied",
    });
    renderToStaticMarkup(<App />);
    const cleanups = effects.map((effect) => effect());
    try {
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(native.loadSettings).toHaveBeenCalledOnce();
      expect(native.getProviderKeyStatus).toHaveBeenCalledTimes(2);
      expect(native.persistSettings).not.toHaveBeenCalled();
      expect(native.testProviderConnection).not.toHaveBeenCalled();
    } finally {
      for (const cleanup of cleanups) if (typeof cleanup === "function") cleanup();
    }
  });

  it("never requests either saved key or tests connections on mount", async () => {
    vi.stubGlobal("window", { location: { search: "" } });
    vi.mocked(native.loadSettings).mockResolvedValueOnce({
      ...defaultSettings, transcriptionProvider: "groq", cleanupProvider: "gemini", onboardingCompleted: true,
    });
    vi.mocked(native.getProviderKeyStatus).mockImplementation(async (provider) => ({
      provider, configured: false, status: "unchecked", message: null,
    }));
    renderToStaticMarkup(<App />);
    const cleanups = effects.map((effect) => effect());
    try {
      await vi.waitFor(() => expect(native.getProviderKeyStatus).toHaveBeenCalledTimes(2));
      expect(native.getProviderKeyStatus).toHaveBeenCalledWith("groq", false);
      expect(native.getProviderKeyStatus).toHaveBeenCalledWith("gemini", false);
      hooks.cursor = 0;
      renderToStaticMarkup(<App />);
      expect(view.speech?.phase).toBe("ready");
      expect(view.speech?.error).toBeNull();
      expect(native.testProviderConnection).not.toHaveBeenCalled();
      expect(native.persistSettings).not.toHaveBeenCalled();
    } finally {
      for (const cleanup of cleanups) if (typeof cleanup === "function") cleanup();
    }
  });

  it("renders saved cloud settings while the OS credential decision is pending", async () => {
    vi.stubGlobal("window", { location: { search: "" } });
    const loaded = { ...defaultSettings, transcriptionProvider: "groq" as const, onboardingCompleted: true };
    vi.mocked(native.loadSettings).mockResolvedValueOnce(loaded);
    let finish!: (value: Awaited<ReturnType<typeof native.getProviderKeyStatus>>) => void;
    vi.mocked(native.getProviderKeyStatus).mockImplementation((provider) => provider === "groq"
      ? new Promise((resolve) => { finish = resolve; })
      : Promise.resolve({ provider, configured: false, status: "unchecked", message: null }));
    renderToStaticMarkup(<App />);
    const cleanups = effects.map((effect) => effect());
    try {
      await vi.waitFor(() => expect(native.getProviderKeyStatus).toHaveBeenCalledWith("groq", false));
      expect(stateChanges).toContainEqual({ initial: defaultSettings, next: loaded });
      expect(stateChanges).toContainEqual({ initial: true, next: false });
      expect(native.persistSettings).not.toHaveBeenCalled();
    } finally {
      for (const cleanup of cleanups) if (typeof cleanup === "function") cleanup();
      finish({ provider: "groq", configured: true, status: "configured", message: null });
    }
  });

  it.each(["configured", "error"] as const)("ignores a late %s cloud result after switching to local", async (status) => {
    vi.stubGlobal("window", { location: { search: "" } });
    vi.mocked(native.loadSettings).mockResolvedValueOnce({
      ...defaultSettings, transcriptionProvider: "groq", cleanupProvider: "gemini", onboardingCompleted: true,
    });
    vi.mocked(native.listInstalledWhisperModels).mockResolvedValueOnce([]);
    let finish!: (value: Awaited<ReturnType<typeof native.getProviderKeyStatus>>) => void;
    vi.mocked(native.getProviderKeyStatus).mockImplementation((provider) => provider === "groq"
      ? new Promise((resolve) => { finish = resolve; })
      : Promise.resolve({ provider, configured: false, status: "unchecked", message: null }));
    renderToStaticMarkup(<App />);
    const cleanups = effects.map((effect) => effect());
    try {
      await vi.waitFor(() => expect(native.getProviderKeyStatus).toHaveBeenCalledWith("groq", false));
      hooks.cursor = 0;
      renderToStaticMarkup(<App />);
      expect(view.update).not.toBeNull();
      expect(view.showScribeSetup).toBe(false);
      view.update!("transcriptionProvider", "local");
      hooks.cursor = 0;
      renderToStaticMarkup(<App />);
      expect(view.speech).toMatchObject({ phase: "missing", modelId: defaultSettings.whisperModel });
      finish({ provider: "groq", configured: status === "configured", status, message: status === "error" ? "Denied" : null });
      await new Promise((resolve) => setTimeout(resolve, 0));
      hooks.cursor = 0;
      renderToStaticMarkup(<App />);
      expect(view.speech).toMatchObject({ phase: "missing", modelId: defaultSettings.whisperModel });
      expect(native.testProviderConnection).not.toHaveBeenCalled();
    } finally {
      for (const cleanup of cleanups) if (typeof cleanup === "function") cleanup();
    }
  });
});
