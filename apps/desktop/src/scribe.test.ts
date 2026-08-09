import { describe, expect, it } from "vitest";
import { isScribeReady } from "./scribe";
import type { ProviderStatus } from "./types";

const providers: ProviderStatus[] = [
  {
    kind: "ollama",
    baseUrl: "http://127.0.0.1:11434",
    available: true,
    models: ["qwen2.5:7b"],
  },
];

describe("Scribe model readiness", () => {
  it("does not infer a default from installed models", () => {
    expect(isScribeReady({ cleanupModel: "" }, providers)).toBe(false);
  });

  it("requires the explicitly chosen model to be installed", () => {
    expect(isScribeReady({ cleanupModel: "turbospeak" }, providers)).toBe(false);
    expect(isScribeReady({ cleanupModel: "qwen2.5:7b" }, providers)).toBe(true);
  });

  it("does not treat a model on an unavailable provider as ready", () => {
    expect(
      isScribeReady(
        { cleanupModel: "qwen2.5:7b" },
        [{ ...providers[0], available: false }],
      ),
    ).toBe(false);
  });
});
