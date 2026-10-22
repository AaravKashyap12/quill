import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { defaultSettings } from "./defaults";
import * as native from "./tauri";

const { effects } = vi.hoisted(() => ({ effects: [] as Array<() => unknown> }));

// Collect mount effects without a DOM; child surfaces have separate lifecycles.
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useEffect: (effect: () => unknown) => { effects.push(effect); },
}));
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
  vi.clearAllMocks();
  vi.mocked(native.loadSettings).mockResolvedValue({ ...defaultSettings });
});
afterEach(() => vi.unstubAllGlobals());

describe("window startup ownership", () => {
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
        expect(native.getProviderKeyStatus).toHaveBeenCalledWith("groq");
        expect(native.getProviderKeyStatus).toHaveBeenCalledWith("gemini");
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
});
