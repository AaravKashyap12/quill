import { describe, expect, it } from "vitest";
import config from "../src-tauri/tauri.conf.json";
import macConfig from "../src-tauri/tauri.macos.conf.json";

describe("transparent transient windows on pinned Tauri 2.11", () => {
  it("keeps the Cargo feature allowlist consistent even without the Mac override", () => {
    expect((config.app as { macOSPrivateApi?: boolean }).macOSPrivateApi).toBe(true);
    const merged = { ...config.app, ...(macConfig as { app?: { macOSPrivateApi?: boolean } }).app };
    expect(merged.macOSPrivateApi).toBe(true);
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
