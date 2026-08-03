import { describe, expect, it } from "vitest";
import {
  modelsForLanguage,
  preferredModelForLanguage,
} from "./VoiceView";

describe("Whisper model filtering", () => {
  it("shows English-only models, including Distil-Whisper, for English", () => {
    const models = modelsForLanguage("en");

    expect(models.every((model) => !model.multilingual)).toBe(true);
    expect(models.map((model) => model.id)).toContain("distil-large-v3");
    expect(models.map((model) => model.id)).not.toContain("base");
    expect(models.map((model) => model.id)).not.toContain("large-v3-turbo");
  });

  it("shows only multilingual models for auto-detect", () => {
    const models = modelsForLanguage("auto");

    expect(models.every((model) => model.multilingual)).toBe(true);
    expect(models.map((model) => model.id)).toContain("base");
    expect(models.map((model) => model.id)).toContain("large-v3-turbo");
    expect(models.map((model) => model.id)).not.toContain("base.en");
    expect(models.map((model) => model.id)).not.toContain("distil-large-v3");
  });

  it("removes multilingual models that are too small for a chosen language", () => {
    const ids = modelsForLanguage("hi").map((model) => model.id);

    expect(ids).not.toContain("tiny");
    expect(ids).not.toContain("base");
    expect(ids).toContain("small");
    expect(ids).toContain("medium");
    expect(ids).toContain("large-v3-turbo");
  });

  it("switches between installed same-size English and multilingual pairs", () => {
    expect(
      preferredModelForLanguage("en", "base", ["base", "base.en"]).id,
    ).toBe("base.en");
    expect(
      preferredModelForLanguage("auto", "base.en", ["base", "base.en"]).id,
    ).toBe("base");
  });

  it("maps Distil-Whisper to turbo when leaving English", () => {
    expect(
      preferredModelForLanguage("auto", "distil-large-v3", [
        "distil-large-v3",
        "large-v3-turbo",
      ]).id,
    ).toBe("large-v3-turbo");
  });
});
