import type { HotkeyConfig, RuntimeStatus } from "../types";
import { MacAccessibilityNotice } from "./MacAccessibilityNotice";

export function OnboardingPractice({ hotkey, runtime }: {
  hotkey: HotkeyConfig;
  runtime: RuntimeStatus;
}) {
  return (
    <div className="onboarding-practice">
      <MacAccessibilityNotice />
      <label htmlFor="onboarding-practice">
        Try it here <span>Optional</span>
      </label>
      <textarea
        id="onboarding-practice"
        autoFocus
        aria-describedby="onboarding-practice-status"
        placeholder={hotkey.behavior === "hold"
          ? "Hold your Dictation shortcut, speak, then release to insert…"
          : "Press your Dictation shortcut, speak, then press again to insert…"}
      />
      <p
        id="onboarding-practice-status"
        className={runtime.state === "error" ? "onboarding-error" : "onboarding-practice-status"}
        role={runtime.state === "error" ? "alert" : "status"}
        aria-atomic="true"
      >
        {runtime.message}
      </p>
    </div>
  );
}
