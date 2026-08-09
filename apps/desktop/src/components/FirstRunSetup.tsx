import { AlertCircle, Check, Download, PenLine, RotateCcw } from "lucide-react";

export interface SpeechSetupState {
  phase: "checking" | "missing" | "downloading" | "ready" | "error";
  modelId: string;
  bytesDownloaded: number;
  bytesTotal: number;
  error: string | null;
}

interface FirstRunSetupProps {
  speech: SpeechSetupState;
  showScribeSetup: boolean;
  onOpenVoice: (target: "speech" | "scribe") => void;
  onRetrySpeech: () => void;
  onDismissScribe: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${Math.round(bytes / 1_000_000)} MB`;
  return `${Math.round(bytes / 1000)} KB`;
}

function progressPercent(speech: SpeechSetupState): number {
  if (speech.phase === "ready") return 100;
  if (speech.bytesTotal <= 0) return 0;
  return Math.min(100, Math.round((speech.bytesDownloaded / speech.bytesTotal) * 100));
}

export function FirstRunSetup({
  speech,
  showScribeSetup,
  onOpenVoice,
  onRetrySpeech,
  onDismissScribe,
}: FirstRunSetupProps) {
  const speechNeedsAttention = speech.phase !== "ready";
  if (!speechNeedsAttention && !showScribeSetup) return null;

  const percent = progressPercent(speech);
  const downloading = speech.phase === "downloading";
  const failed = speech.phase === "error" || speech.phase === "missing";

  return (
    <section className="first-run-setup" aria-labelledby="first-run-title">
      <div className="first-run-setup__heading">
        <div>
          <h2 id="first-run-title">Let’s get Quill ready.</h2>
          <p>Dictation sets itself up. Scribe is optional.</p>
        </div>
        <span>On this device</span>
      </div>

      <div className="first-run-setup__steps">
        <div className="setup-step">
          <span className={`setup-step__icon is-${speech.phase}`} aria-hidden="true">
            {failed ? (
              <AlertCircle size={17} />
            ) : speech.phase === "ready" ? (
              <Check size={17} />
            ) : (
              <Download size={17} />
            )}
          </span>
          <div className="setup-step__body">
            <div className="setup-step__title-row">
              <strong>Dictation</strong>
              <span>
                {speech.phase === "checking"
                  ? "Checking…"
                  : downloading
                    ? `${percent}%`
                    : speech.phase === "ready"
                      ? "Ready"
                      : "Needs attention"}
              </span>
            </div>
            <p>
              {downloading
                ? `Downloading ${speech.modelId} automatically. Keep Quill open.`
                : speech.phase === "checking"
                  ? "Checking for a local speech model."
                  : speech.phase === "ready"
                    ? `${speech.modelId} is installed and selected.`
                    : speech.error ?? "The speech model still needs to be downloaded."}
            </p>
            {downloading ? (
              <>
                <div
                  className="setup-progress"
                  role="progressbar"
                  aria-label={`Downloading ${speech.modelId}`}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={percent}
                >
                  <span style={{ width: `${percent}%` }} />
                </div>
                <small>
                  {formatBytes(speech.bytesDownloaded)} of {formatBytes(speech.bytesTotal)}
                </small>
              </>
            ) : null}
            {failed ? (
              <div className="setup-step__actions">
                <button className="primary-button compact" type="button" onClick={onRetrySpeech}>
                  <RotateCcw size={13} /> Retry download
                </button>
                <button className="link-button" type="button" onClick={() => onOpenVoice("speech")}>
                  Voice settings
                </button>
              </div>
            ) : null}
          </div>
        </div>

        {showScribeSetup ? (
          <div className="setup-step">
            <span className="setup-step__icon is-scribe" aria-hidden="true">
              <PenLine size={17} />
            </span>
            <div className="setup-step__body">
              <div className="setup-step__title-row">
                <strong>Scribe</strong>
                <span>Optional</span>
              </div>
              <p>
                Choose TurboSpeak 1.7B for speed (8 GB RAM minimum) or Qwen 2.5
                7B for stronger corrections (16 GB RAM minimum). Nothing is
                selected automatically.
              </p>
              <div className="setup-step__actions">
                <button
                  className="primary-button compact"
                  type="button"
                  onClick={() => onOpenVoice("scribe")}
                >
                  Set up Scribe
                </button>
                <button className="link-button" type="button" onClick={onDismissScribe}>
                  Not now
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  );
}
