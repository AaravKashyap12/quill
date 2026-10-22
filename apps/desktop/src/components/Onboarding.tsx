import {
  ArrowLeft,
  ArrowRight,
  Check,
  Cloud,
  ExternalLink,
  Gauge,
  HardDrive,
  Languages,
  Mic2,
  RotateCcw,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { formatHotkey } from "../defaults";
import {
  describeSetupPath,
  recommendSpeechModel,
  type SetupLanguage,
  type SetupPriority,
  type SetupScribeRoute,
  type SetupSpeechRoute,
} from "../onboarding";
import { openExternal } from "../tauri";
import type {
  AppSettings,
  CloudProvider,
  ProviderKeyStatus,
  RuntimeStatus,
  SystemProfile,
} from "../types";
import { BrandMark } from "./BrandMark";
import { OnboardingPractice } from "./OnboardingPractice";
import type { SpeechSetupState } from "./FirstRunSetup";

type SetupStep = "welcome" | "choices" | "speech" | "scribe" | "ready";

interface OnboardingProps {
  settings: AppSettings;
  profile: SystemProfile;
  speech: SpeechSetupState;
  scribeReady: boolean;
  cloudStatuses: ProviderKeyStatus[];
  runtime: RuntimeStatus;
  onChooseProviders: (
    speech: SetupSpeechRoute,
    scribe: SetupScribeRoute,
  ) => Promise<void>;
  onStartSpeech: (modelId: string, language: string) => Promise<void>;
  onRetrySpeech: () => void;
  onConnectCloud: (
    provider: CloudProvider,
    key?: string,
  ) => Promise<ProviderKeyStatus>;
  onFinish: (openScribe: boolean) => Promise<void>;
}

interface CloudKeySetupProps {
  provider: CloudProvider;
  status: ProviderKeyStatus | undefined;
  onConnect: (
    provider: CloudProvider,
    key?: string,
  ) => Promise<ProviderKeyStatus>;
  onStatus: (status: ProviderKeyStatus) => void;
}

const steps: SetupStep[] = ["welcome", "choices", "speech", "scribe", "ready"];
const stepLabels: Record<SetupStep, string> = {
  welcome: "Welcome",
  choices: "Choose",
  speech: "Speech",
  scribe: "Scribe",
  ready: "Ready",
};

const keyUrls: Record<CloudProvider, string> = {
  groq: "https://console.groq.com/keys",
  gemini: "https://aistudio.google.com/app/apikey",
};

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  return `${Math.round(bytes / 1_000_000)} MB`;
}

function progressPercent(speech: SpeechSetupState): number {
  if (speech.phase === "ready") return 100;
  if (speech.bytesTotal <= 0) return 0;
  return Math.min(
    100,
    Math.round((speech.bytesDownloaded / speech.bytesTotal) * 100),
  );
}

function accelerationLabel(profile: SystemProfile): string {
  if (profile.speechAcceleration === "metal") return "Metal acceleration";
  if (profile.speechAcceleration === "cuda") return "CUDA acceleration";
  return "CPU processing";
}

function CloudKeySetup({
  provider,
  status,
  onConnect,
  onStatus,
}: CloudKeySetupProps) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const connected = status?.status === "connected";
  const providerName = provider === "groq" ? "Groq" : "Gemini";

  async function connect() {
    setBusy(true);
    setError(null);
    try {
      const next = await onConnect(provider, key.trim() || undefined);
      onStatus(next);
      if (next.status === "connected") setKey("");
      if (next.status !== "connected") {
        setError(next.message ?? `${providerName} could not be reached.`);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="onboarding-key-setup">
      {connected ? (
        <div className="onboarding-connected" role="status">
          <span>
            <Check size={14} aria-hidden="true" />
          </span>
          <div>
            <strong>{providerName} connected</strong>
            <small>The key is stored in your system credential vault.</small>
          </div>
          <button
            className="ghost-action"
            type="button"
            disabled={busy}
            onClick={() => void connect()}
          >
            Test again
          </button>
        </div>
      ) : (
        <>
          <label htmlFor={`onboarding-${provider}-key`}>
            {providerName} API key
          </label>
          <div className="onboarding-key-row">
            <input
              id={`onboarding-${provider}-key`}
              type="password"
              value={key}
              autoComplete="off"
              placeholder={`Paste your ${providerName} key`}
              onChange={(event) => setKey(event.target.value)}
            />
            <button
              className="primary-button"
              type="button"
              disabled={busy || (!key.trim() && !status?.configured && status?.status !== "error")}
              onClick={() => void connect()}
            >
              {busy ? "Testing…" : !key.trim() && status?.status === "error" ? "Retry access" : "Save and test"}
            </button>
          </div>
          <p className="onboarding-key-help">
            <ShieldCheck size={13} aria-hidden="true" /> Saved to your system
            vault, never Quill settings.
            <button
              className="link-button"
              type="button"
              onClick={() => void openExternal(keyUrls[provider])}
            >
              Get a free key <ExternalLink size={11} aria-hidden="true" />
            </button>
          </p>
        </>
      )}
      {error || status?.status === "error" ? (
        <p className="onboarding-error" role="alert">
          {error ?? status?.message}
        </p>
      ) : null}
    </div>
  );
}

export function Onboarding({
  settings,
  profile,
  speech,
  scribeReady,
  cloudStatuses,
  runtime,
  onChooseProviders,
  onStartSpeech,
  onRetrySpeech,
  onConnectCloud,
  onFinish,
}: OnboardingProps) {
  const [step, setStep] = useState<SetupStep>("welcome");
  const [speechRoute, setSpeechRoute] = useState<SetupSpeechRoute>(
    settings.transcriptionProvider,
  );
  const [scribeRoute, setScribeRoute] = useState<SetupScribeRoute>(
    settings.cleanupProvider === "gemini"
      ? "gemini"
      : settings.cleanupProvider === "disabled"
        ? "disabled"
        : "local",
  );
  const [language, setLanguage] = useState<SetupLanguage>(
    settings.language === "en" ? "english" : "multilingual",
  );
  const [priority, setPriority] = useState<SetupPriority>("balanced");
  const [statuses, setStatuses] = useState(cloudStatuses);
  const [actionError, setActionError] = useState<string | null>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const recommendation = useMemo(
    () => recommendSpeechModel(profile, language, priority),
    [language, priority, profile],
  );
  const dataPath = describeSetupPath(speechRoute, scribeRoute);
  const percent = progressPercent(speech);
  const currentIndex = steps.indexOf(step);
  const groqConnected = statuses.some(
    (status) => status.provider === "groq" && status.status === "connected",
  );
  const geminiConnected = statuses.some(
    (status) => status.provider === "gemini" && status.status === "connected",
  );

  useEffect(() => setStatuses(cloudStatuses), [cloudStatuses]);
  useEffect(() => {
    // The final step focuses its practice field so dictation has an insertion target.
    if (step !== "ready") headingRef.current?.focus({ preventScroll: true });
  }, [step]);

  function updateStatus(next: ProviderKeyStatus) {
    setStatuses((current) => [
      ...current.filter((status) => status.provider !== next.provider),
      next,
    ]);
  }

  async function continueFromChoices() {
    setActionError(null);
    try {
      await onChooseProviders(speechRoute, scribeRoute);
      setStep("speech");
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function beginDownload() {
    setActionError(null);
    try {
      await onStartSpeech(
        recommendation.id,
        language === "english" ? "en" : "auto",
      );
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function finish(openScribe: boolean) {
    setActionError(null);
    try {
      await onFinish(openScribe);
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  function completeSpeechStep() {
    setStep(scribeRoute === "disabled" ? "ready" : "scribe");
  }

  async function skipLocalScribe() {
    setActionError(null);
    try {
      await onChooseProviders(speechRoute, "disabled");
      setScribeRoute("disabled");
      setStep("ready");
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : String(cause));
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
          <ol
            className="onboarding-progress"
            aria-label={`Setup step ${currentIndex + 1} of 5`}
          >
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
            Local by default. Cloud only when you choose it.
          </p>
        </aside>

        <div className="onboarding-card">
          {step === "welcome" ? (
            <div className="onboarding-panel is-welcome">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                Set up Quill for your voice.
              </h1>
              <p className="onboarding-lede">
                Choose where speech and Scribe run, connect anything you need,
                then try your shortcuts before setup ends.
              </p>
              <ul className="onboarding-promise" aria-label="Setup details">
                <li>About 2 minutes</li>
                <li>No Quill account</li>
                <li>You control the data path</li>
              </ul>
              <button
                className="primary-button onboarding-primary"
                type="button"
                onClick={() => setStep("choices")}
              >
                Begin setup <ArrowRight size={15} aria-hidden="true" />
              </button>
            </div>
          ) : null}

          {step === "choices" ? (
            <div className="onboarding-panel">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                Choose how Quill should work.
              </h1>
              <p className="onboarding-lede compact">
                Local stays private. Cloud can feel lighter on
                slower computers, but sends only the named data.
              </p>
              <fieldset className="onboarding-fieldset">
                <legend>Speech recognition</legend>
                <div className="onboarding-options is-three">
                  <label
                    className={speechRoute === "local" ? "is-selected" : ""}
                  >
                    <input
                      type="radio"
                      name="speech-route"
                      checked={speechRoute === "local"}
                      onChange={() => setSpeechRoute("local")}
                    />
                    <span className="option-icon">
                      <HardDrive size={17} aria-hidden="true" />
                    </span>
                    <strong>On this device</strong>
                    <small>Private, with text inserted when you stop</small>
                  </label>
                  <label
                    className={speechRoute === "groq" ? "is-selected" : ""}
                  >
                    <input
                      type="radio"
                      name="speech-route"
                      checked={speechRoute === "groq"}
                      onChange={() => setSpeechRoute("groq")}
                    />
                    <span className="option-icon">
                      <Cloud size={17} aria-hidden="true" />
                    </span>
                    <strong>Groq Cloud</strong>
                    <small>Audio is sent after you finish</small>
                  </label>
                  <label
                    className={speechRoute === "gemini" ? "is-selected" : ""}
                  >
                    <input
                      type="radio"
                      name="speech-route"
                      checked={speechRoute === "gemini"}
                      onChange={() => setSpeechRoute("gemini")}
                    />
                    <span className="option-icon">
                      <Sparkles size={17} aria-hidden="true" />
                    </span>
                    <strong>Gemini quality</strong>
                    <small>25 free transcriptions a day</small>
                  </label>
                </div>
              </fieldset>
              <fieldset className="onboarding-fieldset">
                <legend>Scribe cleanup</legend>
                <div className="onboarding-options is-three">
                  <label
                    className={scribeRoute === "disabled" ? "is-selected" : ""}
                  >
                    <input
                      type="radio"
                      name="scribe-route"
                      checked={scribeRoute === "disabled"}
                      onChange={() => setScribeRoute("disabled")}
                    />
                    <span className="option-icon">
                      <Check size={17} aria-hidden="true" />
                    </span>
                    <strong>Not now</strong>
                    <small>Dictation only</small>
                  </label>
                  <label
                    className={scribeRoute === "local" ? "is-selected" : ""}
                  >
                    <input
                      type="radio"
                      name="scribe-route"
                      checked={scribeRoute === "local"}
                      onChange={() => setScribeRoute("local")}
                    />
                    <span className="option-icon">
                      <HardDrive size={17} aria-hidden="true" />
                    </span>
                    <strong>Local model</strong>
                    <small>Transcript stays here</small>
                  </label>
                  <label
                    className={scribeRoute === "gemini" ? "is-selected" : ""}
                  >
                    <input
                      type="radio"
                      name="scribe-route"
                      checked={scribeRoute === "gemini"}
                      onChange={() => setScribeRoute("gemini")}
                    />
                    <span className="option-icon">
                      <Sparkles size={17} aria-hidden="true" />
                    </span>
                    <strong>Gemini</strong>
                    <small>Transcript is sent to Google</small>
                  </label>
                </div>
              </fieldset>
              <dl
                className="onboarding-data-path"
                aria-label="Chosen data path"
              >
                <div>
                  <dt>Audio</dt>
                  <dd>{dataPath.audio}</dd>
                </div>
                <span aria-hidden="true">→</span>
                <div>
                  <dt>Transcript</dt>
                  <dd>{dataPath.transcript}</dd>
                </div>
              </dl>
              <div className="onboarding-actions">
                <button
                  className="ghost-action"
                  type="button"
                  onClick={() => setStep("welcome")}
                >
                  <ArrowLeft size={14} aria-hidden="true" /> Back
                </button>
                <button
                  className="primary-button"
                  type="button"
                  onClick={() => void continueFromChoices()}
                >
                  Continue <ArrowRight size={14} aria-hidden="true" />
                </button>
              </div>
              {actionError ? (
                <p className="onboarding-error" role="alert">
                  {actionError}
                </p>
              ) : null}
            </div>
          ) : null}

          {step === "speech" && speechRoute === "groq" ? (
            <div className="onboarding-panel">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                Connect Groq for speech.
              </h1>
              <p className="onboarding-lede compact">
                Quill sends one recording after you release the shortcut. It
                does not upload while you speak.
              </p>
              <CloudKeySetup
                provider="groq"
                status={statuses.find((status) => status.provider === "groq")}
                onConnect={onConnectCloud}
                onStatus={updateStatus}
              />
              <div className="onboarding-actions">
                <button
                  className="ghost-action"
                  type="button"
                  onClick={() => setStep("choices")}
                >
                  <ArrowLeft size={14} aria-hidden="true" /> Back
                </button>
                <button
                  className="primary-button"
                  type="button"
                  disabled={!groqConnected}
                  onClick={completeSpeechStep}
                >
                  Continue <ArrowRight size={14} aria-hidden="true" />
                </button>
              </div>
            </div>
          ) : null}

          {step === "speech" && speechRoute === "gemini" ? (
            <div className="onboarding-panel">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                Connect Gemini for speech.
              </h1>
              <p className="onboarding-lede compact">
                Gemini 3.5 Transcribe prioritizes accuracy and understands
                corrections. Google currently allows 25 free transcriptions a
                day; audio is uploaded only after you finish.
              </p>
              <CloudKeySetup
                provider="gemini"
                status={statuses.find((status) => status.provider === "gemini")}
                onConnect={onConnectCloud}
                onStatus={updateStatus}
              />
              <p className="onboarding-cloud-disclosure">
                Quill requests deletion immediately after transcription. If
                deletion fails, Google may retain the upload for up to 48
                hours. Free-tier content may be used to improve Google products.
              </p>
              <div className="onboarding-actions">
                <button
                  className="ghost-action"
                  type="button"
                  onClick={() => setStep("choices")}
                >
                  <ArrowLeft size={14} aria-hidden="true" /> Back
                </button>
                <button
                  className="primary-button"
                  type="button"
                  disabled={!geminiConnected}
                  onClick={completeSpeechStep}
                >
                  Continue <ArrowRight size={14} aria-hidden="true" />
                </button>
              </div>
            </div>
          ) : null}

          {step === "speech" && speechRoute === "local" ? (
            <div className="onboarding-panel">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                {speech.phase === "ready"
                  ? "Dictation is ready."
                  : speech.phase === "error"
                    ? "Download paused."
                    : speech.phase === "downloading"
                      ? "Downloading your voice model."
                      : "Fit speech to this computer."}
              </h1>
              {speech.phase === "missing" || speech.phase === "checking" ? (
                <>
                  <p className="onboarding-lede compact">
                    Detected{" "}
                    {Math.max(
                      1,
                      Math.round(profile.totalMemoryBytes / 1024 ** 3),
                    )}{" "}
                    GB memory, {profile.logicalCpuCount} processor threads, and{" "}
                    {accelerationLabel(profile)}.
                  </p>
                  <fieldset className="onboarding-fieldset">
                    <legend>Spoken language</legend>
                    <div className="onboarding-options is-two">
                      <label
                        className={language === "english" ? "is-selected" : ""}
                      >
                        <input
                          type="radio"
                          name="language"
                          checked={language === "english"}
                          onChange={() => setLanguage("english")}
                        />
                        <span className="option-icon">
                          <Mic2 size={17} aria-hidden="true" />
                        </span>
                        <strong>English only</strong>
                        <small>Sharper English recognition</small>
                      </label>
                      <label
                        className={
                          language === "multilingual" ? "is-selected" : ""
                        }
                      >
                        <input
                          type="radio"
                          name="language"
                          checked={language === "multilingual"}
                          onChange={() => setLanguage("multilingual")}
                        />
                        <span className="option-icon">
                          <Languages size={17} aria-hidden="true" />
                        </span>
                        <strong>Multiple languages</strong>
                        <small>Detects the language you speak</small>
                      </label>
                    </div>
                  </fieldset>
                  <fieldset className="onboarding-fieldset">
                    <legend>Performance preference</legend>
                    <div className="onboarding-options is-three">
                      {(
                        [
                          ["fast", "Faster", "Lightest load", Zap],
                          ["balanced", "Balanced", "Recommended", Gauge],
                          [
                            "accurate",
                            "Accuracy",
                            "More detail",
                            SlidersHorizontal,
                          ],
                        ] as const
                      ).map(([value, title, caption, Icon]) => (
                        <label
                          key={value}
                          className={priority === value ? "is-selected" : ""}
                        >
                          <input
                            type="radio"
                            name="priority"
                            checked={priority === value}
                            onChange={() => setPriority(value)}
                          />
                          <span className="option-icon">
                            <Icon size={17} aria-hidden="true" />
                          </span>
                          <strong>{title}</strong>
                          <small>{caption}</small>
                        </label>
                      ))}
                    </div>
                  </fieldset>
                  <div className="onboarding-recommendation" aria-live="polite">
                    <span>
                      <Check size={14} aria-hidden="true" /> Best fit for this
                      computer
                    </span>
                    <strong>
                      {recommendation.id} · {recommendation.sizeLabel}
                    </strong>
                    <p>{recommendation.reason}</p>
                  </div>
                </>
              ) : (
                <p className="onboarding-lede compact">
                  {speech.phase === "ready"
                    ? `${speech.modelId} is installed and selected.`
                    : speech.phase === "error"
                      ? (speech.error ?? "Quill could not finish the download.")
                      : `Downloading ${speech.modelId}. Keep Quill open until it finishes.`}
                </p>
              )}
              {speech.phase === "downloading" ? (
                <div className="onboarding-download" aria-live="polite">
                  <div className="onboarding-download__meta">
                    <strong>{percent}%</strong>
                    <span>
                      {formatBytes(speech.bytesDownloaded)} of{" "}
                      {formatBytes(speech.bytesTotal)}
                    </span>
                  </div>
                  <div
                    className="setup-progress"
                    role="progressbar"
                    aria-label={`Downloading ${speech.modelId}`}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={percent}
                    aria-busy="true"
                  >
                    <span style={{ width: `${percent}%` }} />
                  </div>
                </div>
              ) : null}
              <div className="onboarding-actions">
                <button
                  className="ghost-action"
                  type="button"
                  onClick={() => setStep("choices")}
                >
                  <ArrowLeft size={14} aria-hidden="true" /> Back
                </button>
                {speech.phase === "ready" ? (
                  <button
                    className="primary-button"
                    type="button"
                    onClick={completeSpeechStep}
                  >
                    Continue <ArrowRight size={14} aria-hidden="true" />
                  </button>
                ) : speech.phase === "error" ? (
                  <button
                    className="primary-button"
                    type="button"
                    onClick={onRetrySpeech}
                  >
                    <RotateCcw size={14} aria-hidden="true" /> Retry
                  </button>
                ) : speech.phase === "missing" ||
                  speech.phase === "checking" ? (
                  <button
                    className="primary-button"
                    type="button"
                    onClick={() => void beginDownload()}
                  >
                    Download {recommendation.sizeLabel}{" "}
                    <ArrowRight size={14} aria-hidden="true" />
                  </button>
                ) : null}
              </div>
              {actionError ? (
                <p className="onboarding-error" role="alert">
                  {actionError}
                </p>
              ) : null}
            </div>
          ) : null}

          {step === "scribe" && scribeRoute === "gemini" ? (
            <div className="onboarding-panel">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                Connect Gemini for Scribe.
              </h1>
              <p className="onboarding-lede compact">
                Only transcript text and relevant selected context are sent to
                Gemini when you use Scribe.
              </p>
              <CloudKeySetup
                provider="gemini"
                status={statuses.find((status) => status.provider === "gemini")}
                onConnect={onConnectCloud}
                onStatus={updateStatus}
              />
              <div className="onboarding-actions">
                <button
                  className="ghost-action"
                  type="button"
                  onClick={() => setStep("speech")}
                >
                  <ArrowLeft size={14} aria-hidden="true" /> Back
                </button>
                <button
                  className="primary-button"
                  type="button"
                  disabled={!geminiConnected}
                  onClick={() => setStep("ready")}
                >
                  Continue <ArrowRight size={14} aria-hidden="true" />
                </button>
              </div>
            </div>
          ) : null}

          {step === "scribe" && scribeRoute === "local" ? (
            <div className="onboarding-panel is-centered">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                {scribeReady
                  ? "Local Scribe is ready."
                  : "Set up a local Scribe model."}
              </h1>
              <p className="onboarding-lede compact">
                {scribeReady
                  ? "Scribe can refine wording without sending transcript text away."
                  : "Choose TurboSpeak 1.7B for lighter hardware or Qwen 2.5 7B for stronger corrections. Quill will not choose for you."}
              </p>
              <div className="onboarding-choice-actions">
                {scribeReady ? (
                  <button
                    className="primary-button"
                    type="button"
                    onClick={() => setStep("ready")}
                  >
                    Continue <ArrowRight size={14} aria-hidden="true" />
                  </button>
                ) : (
                  <>
                    <button
                      className="primary-button"
                      type="button"
                      onClick={() => void finish(true)}
                    >
                      Open local Scribe setup
                    </button>
                    <button
                      className="ghost-action"
                      type="button"
                      onClick={() => void skipLocalScribe()}
                    >
                      Not now
                    </button>
                  </>
                )}
              </div>
              {actionError ? (
                <p className="onboarding-error" role="alert">
                  {actionError}
                </p>
              ) : null}
            </div>
          ) : null}

          {step === "ready" ? (
            <div className="onboarding-panel">
              <h1 id="onboarding-title" ref={headingRef} tabIndex={-1}>
                You’re ready to speak.
              </h1>
              <p className="onboarding-lede compact">
                Try a shortcut below. Quill will type into this field, just as
                it will in your other apps.
              </p>
              <dl className="onboarding-shortcuts">
                <div>
                  <dt>
                    <strong>Dictation</strong>
                    <span>Exactly what you say</span>
                  </dt>
                  <dd>{formatHotkey(settings, "dictation")}</dd>
                </div>
                <div>
                  <dt>
                    <strong>Scribe</strong>
                    <span>
                      {scribeRoute === "disabled"
                        ? "Set up later in Voice"
                        : "Refine, review, then insert"}
                    </span>
                  </dt>
                  <dd>{formatHotkey(settings, "scribe")}</dd>
                </div>
              </dl>
              <OnboardingPractice hotkey={settings.dictationHotkey} runtime={runtime} />
              <button
                className="primary-button onboarding-primary"
                type="button"
                onClick={() => void finish(false)}
              >
                Open Quill <ArrowRight size={14} aria-hidden="true" />
              </button>
              {actionError ? (
                <p className="onboarding-error" role="alert">
                  {actionError}
                </p>
              ) : null}
            </div>
          ) : null}
        </div>
      </section>
    </main>
  );
}
