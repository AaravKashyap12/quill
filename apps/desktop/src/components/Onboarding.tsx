import {
  ArrowLeft,
  ArrowRight,
  Check,
  Gauge,
  Languages,
  Mic2,
  RotateCcw,
  ShieldCheck,
  SlidersHorizontal,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { formatHotkey } from "../defaults";
import {
  recommendSpeechModel,
  type SetupLanguage,
  type SetupPriority,
} from "../onboarding";
import type { AppSettings, SystemProfile } from "../types";
import { BrandMark } from "./BrandMark";
import type { SpeechSetupState } from "./FirstRunSetup";

type SetupStep = "welcome" | "preferences" | "download" | "scribe" | "ready";

interface OnboardingProps {
  settings: AppSettings;
  profile: SystemProfile;
  speech: SpeechSetupState;
  scribeReady: boolean;
  onStartSpeech: (modelId: string, language: string) => Promise<void>;
  onRetrySpeech: () => void;
  onFinish: (openScribe: boolean) => Promise<void>;
}

const steps: SetupStep[] = ["welcome", "preferences", "download", "scribe", "ready"];
const stepLabels: Record<SetupStep, string> = {
  welcome: "Welcome",
  preferences: "Voice model",
  download: "Download",
  scribe: "Scribe",
  ready: "Ready",
};

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  return `${Math.round(bytes / 1_000_000)} MB`;
}

function progressPercent(speech: SpeechSetupState): number {
  if (speech.phase === "ready") return 100;
  if (speech.bytesTotal <= 0) return 0;
  return Math.min(100, Math.round((speech.bytesDownloaded / speech.bytesTotal) * 100));
}

function accelerationLabel(profile: SystemProfile): string {
  if (profile.speechAcceleration === "metal") return "Metal acceleration";
  if (profile.speechAcceleration === "cuda") return "CUDA acceleration";
  return "CPU processing";
}

export function Onboarding({
  settings,
  profile,
  speech,
  scribeReady,
  onStartSpeech,
  onRetrySpeech,
  onFinish,
}: OnboardingProps) {
  const [step, setStep] = useState<SetupStep>("welcome");
  const [language, setLanguage] = useState<SetupLanguage>(
    settings.language === "en" ? "english" : "multilingual",
  );
  const [priority, setPriority] = useState<SetupPriority>("balanced");
  const [actionError, setActionError] = useState<string | null>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const recommendation = useMemo(
    () => recommendSpeechModel(profile, language, priority),
    [language, priority, profile],
  );
  const percent = progressPercent(speech);
  const currentIndex = steps.indexOf(step);

  useEffect(() => {
    headingRef.current?.focus({ preventScroll: true });
  }, [step]);

  async function beginDownload() {
    setActionError(null);
    setStep("download");
    try {
      await onStartSpeech(
        recommendation.id,
        language === "english" ? "en" : "auto",
      );
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    }
  }

  async function finish(openScribe: boolean) {
    setActionError(null);
    try {
      await onFinish(openScribe);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <main className="onboarding-page">
      <section className="onboarding-shell" aria-labelledby="onboarding-title">
        <aside className="onboarding-rail">
          <div className="onboarding-identity">
            <BrandMark />
            <p>Setup on this device</p>
          </div>

          <ol className="onboarding-progress" aria-label={`Setup step ${currentIndex + 1} of 5`}>
            {steps.map((item, index) => (
              <li
                key={item}
                className={`${index < currentIndex ? "is-complete" : ""} ${item === step ? "is-current" : ""}`}
                aria-current={item === step ? "step" : undefined}
              >
                <span aria-hidden="true">
                  {index < currentIndex ? <Check size={12} /> : index + 1}
                </span>
                <strong>{stepLabels[item]}</strong>
              </li>
            ))}
          </ol>

          <p className="onboarding-privacy">
            <ShieldCheck size={15} aria-hidden="true" />
            Speech stays on this computer.
          </p>
        </aside>

        <div className="onboarding-card">
          {step === "welcome" ? (
            <div className="onboarding-panel is-welcome">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                Set up Quill for your voice.
              </h1>
              <p className="onboarding-lede">
                Two quick choices help Quill select a speech model that fits this computer.
                The model downloads here and stays here.
              </p>
              <ul className="onboarding-promise" aria-label="Setup details">
                <li>About 2 minutes</li>
                <li>No account</li>
                <li>No cloud processing</li>
              </ul>
              <button className="primary-button onboarding-primary" type="button" onClick={() => setStep("preferences")}>
                Choose a model <ArrowRight size={15} aria-hidden="true" />
              </button>
            </div>
          ) : null}

          {step === "preferences" ? (
            <div className="onboarding-panel">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                What should Quill optimise for?
              </h1>
              <p className="onboarding-lede compact">
                Detected {Math.max(1, Math.round(profile.totalMemoryBytes / 1024 ** 3))} GB memory,
                {" "}{profile.logicalCpuCount} processor threads, and {accelerationLabel(profile)}.
              </p>

              <fieldset className="onboarding-fieldset">
                <legend>Spoken language</legend>
                <div className="onboarding-options is-two">
                  <label className={language === "english" ? "is-selected" : ""}>
                    <input type="radio" name="language" value="english" checked={language === "english"} onChange={() => setLanguage("english")} />
                    <span className="option-icon"><Mic2 size={17} aria-hidden="true" /></span>
                    <strong>English only</strong>
                    <small>Sharper English recognition</small>
                  </label>
                  <label className={language === "multilingual" ? "is-selected" : ""}>
                    <input type="radio" name="language" value="multilingual" checked={language === "multilingual"} onChange={() => setLanguage("multilingual")} />
                    <span className="option-icon"><Languages size={17} aria-hidden="true" /></span>
                    <strong>Multiple languages</strong>
                    <small>Detects the language you speak</small>
                  </label>
                </div>
              </fieldset>

              <fieldset className="onboarding-fieldset">
                <legend>Performance preference</legend>
                <div className="onboarding-options is-three">
                  {([
                    ["fast", "Faster", "Lightest on this computer", Zap],
                    ["balanced", "Balanced", "Recommended", Gauge],
                    ["accurate", "Accuracy", "More detail and memory", SlidersHorizontal],
                  ] as const).map(([value, title, caption, Icon]) => (
                    <label key={value} className={priority === value ? "is-selected" : ""}>
                      <input type="radio" name="priority" value={value} checked={priority === value} onChange={() => setPriority(value)} />
                      <span className="option-icon"><Icon size={17} aria-hidden="true" /></span>
                      <strong>{title}</strong>
                      <small>{caption}</small>
                    </label>
                  ))}
                </div>
              </fieldset>

              <div className="onboarding-recommendation" aria-live="polite">
                <span><Check size={14} aria-hidden="true" /> Best fit for this computer</span>
                <strong>{recommendation.id} · {recommendation.sizeLabel}</strong>
                <p>{recommendation.reason}</p>
              </div>

              <div className="onboarding-actions">
                <button className="ghost-action" type="button" onClick={() => setStep("welcome")}>
                  <ArrowLeft size={14} aria-hidden="true" /> Back
                </button>
                <button className="primary-button" type="button" onClick={() => void beginDownload()}>
                  Download and continue <ArrowRight size={14} aria-hidden="true" />
                </button>
              </div>
            </div>
          ) : null}

          {step === "download" ? (
            <div className="onboarding-panel is-centered">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                {speech.phase === "ready" ? "Dictation is ready." : speech.phase === "error" ? "Download paused." : "Downloading your voice model."}
              </h1>
              <p className="onboarding-lede compact">
                {speech.phase === "ready"
                  ? `${speech.modelId} is installed and selected.`
                  : speech.phase === "error"
                    ? speech.error ?? "Quill could not finish the download."
                    : `Downloading ${speech.modelId}. Keep Quill open until it finishes.`}
              </p>

              {speech.phase === "downloading" ? (
                <div className="onboarding-download" aria-live="polite">
                  <div className="onboarding-download__meta">
                    <strong>{percent}%</strong>
                    <span>{formatBytes(speech.bytesDownloaded)} of {formatBytes(speech.bytesTotal)}</span>
                  </div>
                  <div className="setup-progress" role="progressbar" aria-label={`Downloading ${speech.modelId}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} aria-busy="true">
                    <span style={{ width: `${percent}%` }} />
                  </div>
                </div>
              ) : null}

              {speech.phase === "error" ? (
                <button className="primary-button onboarding-primary" type="button" onClick={onRetrySpeech}>
                  <RotateCcw size={14} aria-hidden="true" /> Retry download
                </button>
              ) : null}
              {speech.phase === "ready" ? (
                <button className="primary-button onboarding-primary" type="button" onClick={() => setStep("scribe")}>
                  Continue <ArrowRight size={14} aria-hidden="true" />
                </button>
              ) : null}
              {actionError ? <p className="onboarding-error" role="alert">{actionError}</p> : null}
            </div>
          ) : null}

          {step === "scribe" ? (
            <div className="onboarding-panel is-centered">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                {scribeReady ? "Scribe is ready too." : "Want Quill to refine your words?"}
              </h1>
              <p className="onboarding-lede compact">
                {scribeReady
                  ? "Your local writing model is connected. Scribe can clean up wording before you approve it."
                  : "Scribe uses a separate local writing model. Choose TurboSpeak 1.7B for speed (8 GB RAM minimum) or Qwen 2.5 7B for stronger corrections (16 GB RAM minimum). Quill will not choose for you."}
              </p>
              <div className="onboarding-choice-actions">
                {scribeReady ? (
                  <button className="primary-button" type="button" onClick={() => setStep("ready")}>
                    Continue <ArrowRight size={14} aria-hidden="true" />
                  </button>
                ) : (
                  <>
                    <button className="primary-button" type="button" onClick={() => void finish(true)}>
                      Set up Scribe
                    </button>
                    <button className="ghost-action" type="button" onClick={() => setStep("ready")}>
                      Not now
                    </button>
                  </>
                )}
              </div>
              {actionError ? <p className="onboarding-error" role="alert">{actionError}</p> : null}
            </div>
          ) : null}

          {step === "ready" ? (
            <div className="onboarding-panel is-centered">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>You’re ready to speak.</h1>
              <p className="onboarding-lede compact">Hold a shortcut, speak, then release it. Quill types into the app you were using.</p>
              <dl className="onboarding-shortcuts">
                <div><dt>Dictation</dt><dd>{formatHotkey(settings, "dictation")}</dd></div>
                <div><dt>Scribe</dt><dd>{formatHotkey(settings, "scribe")}</dd></div>
              </dl>
              <button className="primary-button onboarding-primary" type="button" onClick={() => void finish(false)}>
                Open Quill <ArrowRight size={14} aria-hidden="true" />
              </button>
              {actionError ? <p className="onboarding-error" role="alert">{actionError}</p> : null}
            </div>
          ) : null}
        </div>
      </section>
    </main>
  );
}
