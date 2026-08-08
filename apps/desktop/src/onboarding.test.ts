import { describe, expect, it } from "vitest";
import { recommendSpeechModel } from "./onboarding";
import type { SystemProfile } from "./types";

const GIB = 1024 ** 3;

function profile(
  totalGiB: number,
  availableGiB: number,
  cpus: number,
  speechAcceleration: SystemProfile["speechAcceleration"] = "cpu",
): SystemProfile {
  return {
    totalMemoryBytes: totalGiB * GIB,
    availableMemoryBytes: availableGiB * GIB,
    logicalCpuCount: cpus,
    platform: "windows",
    architecture: "x86_64",
    speechAcceleration,
  };
}

describe("first-run speech model recommendation", () => {
  it("keeps CPU-only balanced dictation on tiny even with ample system RAM", () => {
    expect(recommendSpeechModel(profile(16, 10, 12), "english", "balanced").id).toBe("tiny.en");
  });

  it("allows base for accuracy on a capable CPU without jumping to small", () => {
    expect(recommendSpeechModel(profile(16, 10, 12), "english", "accurate").id).toBe("base.en");
  });

  it("uses small for a typical Metal-accelerated computer", () => {
    expect(recommendSpeechModel(profile(16, 10, 8, "metal"), "english", "balanced").id).toBe("small.en");
  });

  it("uses medium only when acceleration, accuracy, and headroom are all present", () => {
    expect(recommendSpeechModel(profile(32, 20, 12, "metal"), "english", "accurate").id).toBe("medium.en");
  });

  it("selects multilingual variants when requested", () => {
    expect(recommendSpeechModel(profile(16, 10, 8), "multilingual", "balanced").id).toBe("tiny");
  });

  it("honours the fast preference on accelerated hardware", () => {
    expect(recommendSpeechModel(profile(64, 48, 24, "cuda"), "english", "fast").id).toBe("base.en");
  });
});
