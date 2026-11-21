import { expect, it } from "vitest";
import { defaultSettings } from "./defaults";
import { speechSetupFor } from "./speechSetup";

it.each(["groq", "gemini"] as const)("preserves completed %s setup when Discard restores unchecked credentials", (provider) => {
  const settings = { ...defaultSettings, onboardingCompleted: true, transcriptionProvider: provider };
  expect(speechSetupFor(settings, [], [{ provider, configured: false, status: "unchecked", message: null }]))
    .toMatchObject({ phase: "ready", error: null });
  expect(speechSetupFor(settings, [], [{ provider, configured: false, status: "missing", message: null }]))
    .toMatchObject({ phase: "error" });
  expect(speechSetupFor({ ...settings, onboardingCompleted: false }, [], []))
    .toMatchObject({ phase: "checking" });
});
