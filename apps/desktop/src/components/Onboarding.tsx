import {
  ArrowLeft,
  ArrowRight,
  Check,
  Download,
  Gauge,
  Languages,
  Mic2,
  PenLine,
  RotateCcw,
  ShieldCheck,
  Sparkles,
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

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  return `${Math.round(bytes / 1_000_000)} MB`;
}

function progressPercent(speech: SpeechSetupState): number {
  if (speech.phase === "ready") return 100;
  if (speech.bytesTotal <= 0) return 0;
  return Math.min(100, Math.round((speech.bytesDownloaded / speech.bytesTotal) * 100));
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
    headingRef.current?.focus();
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
      <div className="onboarding-chrome" aria-hidden="true">
        <span className="onboarding-wordmark">Quill</span>
        <span>Private by design</span>
      </div>

      <section className="onboarding-card" aria-labelledby="onboarding-title">
        <ol className="onboarding-progress" aria-label={`Setup step ${currentIndex + 1} of 5`}>
          {steps.map((item, index) => (
            <li
              key={item}
              className={index <= currentIndex ? "is-complete" : ""}
              aria-current={item === step ? "step" : undefined}
            />
          ))}
        </ol>

        {step === "welcome" ? (
          <div className="onboarding-panel is-welcome">
            <span className="onboarding-emblem" aria-hidden="true">
              <Mic2 size={24} />
            </span>
            <p className="eyebrow">Welcome to Quill</p>
            <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
              Let’s make Quill yours.
            </h1>
            <p className="onboarding-lede">
              Answer two quick questions. Quill will choose the right speech model and set
              everything up on this device.
            </p>
            <div className="onboarding-promise">
              <ShieldCheck size={17} aria-hidden="true" />
              Your microphone audio and words stay on your computer.
            </div>
            <button className="primary-button onboarding-primary" type="button" onClick={() => setStep("preferences")}>
              Get started <ArrowRight size={15} aria-hidden="true" />
            </button>
          </div>
        ) : null}

        {step === "preferences" ? (
          <div className="onboarding-panel">
            <p className="eyebrow">Your voice engine</p>
            <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
              How will you use Quill?
            </h1>
            <p className="onboarding-lede compact">
              Quill detected {Math.max(1, Math.round(profile.totalMemoryBytes / 1024 ** 3))} GB of
              memory and {profile.logicalCpuCount} processor threads.
            </p>

            <fieldset className="onboarding-fieldset">
              <legend>What will you speak?</legend>
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
                  <small>Automatically detects what you speak</small>
                </label>
              </div>
            </fieldset>

            <fieldset className="onboarding-fieldset">
              <legend>What matters most?</legend>
              <div className="onboarding-options is-three">
                {([
                  ["fast", "Faster", "Lightest on your computer", Zap],
                  ["balanced", "Balanced", "Recommended", Gauge],
                  ["accurate", "Accuracy", "More detail, more memory", Sparkles],
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
              <span><Check size={14} aria-hidden="true" /> Recommended for this computer</span>
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
            <span className={`onboarding-emblem is-${speech.phase}`} aria-hidden="true">
              {speech.phase === "ready" ? <Check size={24} /> : <Download size={22} />}
            </span>
            <p className="eyebrow">Local speech model</p>
            <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
              {speech.phase === "ready" ? "Dictation is ready." : speech.phase === "error" ? "Download paused." : "Preparing your voice engine."}
            </h1>
            <p className="onboarding-lede compact">
              {speech.phase === "ready"
                ? `${speech.modelId} is installed and selected.`
                : speech.phase === "error"
                  ? speech.error ?? "Quill could not finish the download."
                  : `Downloading ${speech.modelId}. You can leave Quill open in the background.`}
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
            <span className="onboarding-emblem is-scribe" aria-hidden="true"><PenLine size={22} /></span>
            <p className="eyebrow">Optional</p>
            <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
              {scribeReady ? "Scribe is ready too." : "Want Quill to refine your words?"}
            </h1>
            <p className="onboarding-lede compact">
              {scribeReady
                ? "Your local writing model is connected. Scribe can clean up wording before you approve it."
                : "Scribe uses a separate local writing model. Dictation already works without it."}
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
            <span className="onboarding-emblem is-ready" aria-hidden="true"><Check size={24} /></span>
            <p className="eyebrow">Setup complete</p>
            <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>You’re ready to speak.</h1>
            <p className="onboarding-lede compact">Hold a shortcut while you talk. Release it when you’re done.</p>
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
      </section>
    </main>
  );
}
