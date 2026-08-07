import { describe, expect, it } from "vitest";
import { recommendSpeechModel } from "./onboarding";
import type { SystemProfile } from "./types";

const GIB = 1024 ** 3;

function profile(totalGiB: number, availableGiB: number, cpus: number): SystemProfile {
  return {
    totalMemoryBytes: totalGiB * GIB,
    availableMemoryBytes: availableGiB * GIB,
    logicalCpuCount: cpus,
    platform: "windows",
    architecture: "x86_64",
  };
}

describe("first-run speech model recommendation", () => {
  it("keeps a constrained machine on base", () => {
    expect(recommendSpeechModel(profile(8, 3, 4), "english", "balanced").id).toBe("base.en");
  });

  it("uses small for a typical modern computer", () => {
    expect(recommendSpeechModel(profile(16, 10, 8), "english", "balanced").id).toBe("small.en");
  });

  it("uses medium only when accuracy is requested and headroom is sufficient", () => {
    expect(recommendSpeechModel(profile(32, 20, 12), "english", "accurate").id).toBe("medium.en");
  });

  it("selects multilingual variants when requested", () => {
    expect(recommendSpeechModel(profile(16, 10, 8), "multilingual", "balanced").id).toBe("small");
  });

  it("honours the fast preference even on powerful hardware", () => {
    expect(recommendSpeechModel(profile(64, 48, 24), "english", "fast").id).toBe("base.en");
  });
});
