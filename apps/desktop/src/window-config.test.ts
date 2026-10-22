import { describe, expect, it } from "vitest";
import config from "../src-tauri/tauri.conf.json";
import macConfig from "../src-tauri/tauri.macos.conf.json";

describe("transparent transient windows on pinned Tauri 2.11", () => {
  it("enables the macOS transparency API required by this runtime", () => {
    expect((macConfig as { app?: { macOSPrivateApi?: boolean } }).app?.macOSPrivateApi).toBe(true);
  });

  it("starts each transient surface hidden with a transparent native canvas", () => {
    const transient = config.app.windows.filter((window) => window.label !== "main");
    expect(transient.map((window) => window.label)).toEqual([
      "overlay", "scribe-overlay", "scribe-review",
    ]);
    for (const window of transient) {
      expect(window).toMatchObject({ visible: false, transparent: true, decorations: false });
    }
  });
});
