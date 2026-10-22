import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { defaultSettings, initialRuntimeStatus } from "../defaults";
import { OnboardingPractice } from "./OnboardingPractice";

describe("first dictation practice", () => {
  it("focuses the insertion field and explains when hold dictation is inserted", () => {
    const html = renderToStaticMarkup(
      <OnboardingPractice hotkey={{ ...defaultSettings.dictationHotkey, behavior: "hold" }} runtime={initialRuntimeStatus} />,
    );
    expect(html).toContain('autofocus=""');
    expect(html).toContain("release to insert");
    expect(html).toContain('aria-describedby="onboarding-practice-status"');
    expect(html).toContain('role="status"');
  });

  it("explains tap-to-lock without telling the user to hold the shortcut", () => {
    const html = renderToStaticMarkup(
      <OnboardingPractice hotkey={{ ...defaultSettings.dictationHotkey, behavior: "tap-to-lock" }} runtime={initialRuntimeStatus} />,
    );
    expect(html).toContain("press again to insert");
    expect(html).not.toContain("Hold your");
  });

  it("keeps a native failure visible as an alert without treating it as HTML", () => {
    const html = renderToStaticMarkup(
      <OnboardingPractice hotkey={defaultSettings.dictationHotkey} runtime={{
        state: "error", mode: "dictation", provider: "groq", message: "Microphone <unavailable>",
      }} />,
    );
    expect(html).toContain('role="alert"');
    expect(html).toContain("Microphone &lt;unavailable&gt;");
    expect(html).not.toContain("Microphone <unavailable>");
  });

  it("shows processing state without disabling the insertion field", () => {
    const html = renderToStaticMarkup(
      <OnboardingPractice hotkey={defaultSettings.dictationHotkey} runtime={{
        state: "processing", mode: "dictation", provider: "groq", message: "Transcribing speech",
      }} />,
    );
    expect(html).toContain("Transcribing speech");
    expect(html).not.toContain("disabled");
  });
});
