import { AlertCircle, Download, RefreshCw, RotateCcw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { checkAppUpdate, installAppUpdate } from "../tauri";
import type { AppUpdateEvent, AppUpdateInfo } from "../types";

const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const FIRST_CHECK_DELAY_MS = 6_000;

type UpdateState =
  | { phase: "hidden" }
  | { phase: "available"; update: AppUpdateInfo }
  | {
      phase: "downloading";
      update: AppUpdateInfo;
      downloaded: number;
      total: number | null;
    }
  | { phase: "installing"; update: AppUpdateInfo }
  | { phase: "error"; update: AppUpdateInfo; message: string };

interface AppUpdateBannerProps {
  blockedReason: string | null;
}

function progressPercent(downloaded: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.round((downloaded / total) * 100));
}

export function AppUpdateBanner({ blockedReason }: AppUpdateBannerProps) {
  const [state, setState] = useState<UpdateState>({ phase: "hidden" });
  const dismissedVersion = useRef<string | null>(null);
  const checking = useRef(false);

  const checkForUpdate = useCallback(async () => {
    if (checking.current) return;
    checking.current = true;
    try {
      const update = await checkAppUpdate();
      if (update && update.version !== dismissedVersion.current) {
        setState((current) =>
          current.phase === "downloading" || current.phase === "installing"
            ? current
            : { phase: "available", update },
        );
      }
    } catch {
      // Automatic checks fail quietly. A network outage should never make a
      // local dictation app look broken or demand attention.
    } finally {
      checking.current = false;
    }
  }, []);

  useEffect(() => {
    const preview = new URLSearchParams(window.location.search).has("update");
    const firstCheck = window.setTimeout(
      () => void checkForUpdate(),
      preview ? 0 : FIRST_CHECK_DELAY_MS,
    );
    const interval = window.setInterval(
      () => void checkForUpdate(),
      UPDATE_CHECK_INTERVAL_MS,
    );
    return () => {
      window.clearTimeout(firstCheck);
      window.clearInterval(interval);
    };
  }, [checkForUpdate]);

  if (state.phase === "hidden") return null;

  const update = state.update;
  const percent =
    state.phase === "downloading"
      ? progressPercent(state.downloaded, state.total)
      : null;

  async function install() {
    if (blockedReason) return;
    setState({
      phase: "downloading",
      update,
      downloaded: 0,
      total: null,
    });
    let downloaded = 0;
    let total: number | null = null;
    try {
      await installAppUpdate((event: AppUpdateEvent) => {
        if (event.event === "Started") {
          total = event.data.contentLength;
          setState({ phase: "downloading", update, downloaded, total });
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setState({ phase: "downloading", update, downloaded, total });
        } else {
          setState({ phase: "installing", update });
        }
      });
      setState({ phase: "installing", update });
    } catch (error) {
      setState({
        phase: "error",
        update,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  function dismiss() {
    dismissedVersion.current = update.version;
    setState({ phase: "hidden" });
  }

  const busy = state.phase === "downloading" || state.phase === "installing";
  const status =
    state.phase === "downloading"
      ? percent === null
        ? "Downloading update"
        : `Downloading update · ${percent}%`
      : state.phase === "installing"
        ? "Installing and restarting Quill"
        : state.phase === "error"
          ? "The update couldn't be installed"
          : `Quill ${update.version} is ready`;

  return (
    <section className={`app-update is-${state.phase}`} aria-live="polite">
      <span className="app-update__icon" aria-hidden="true">
        {state.phase === "error" ? (
          <AlertCircle size={16} />
        ) : busy ? (
          <RefreshCw size={16} />
        ) : (
          <Download size={16} />
        )}
      </span>
      <div className="app-update__copy">
        <strong>{status}</strong>
        <span>
          {state.phase === "error"
            ? "Your current version is unchanged. Try again when you're ready."
            : blockedReason ??
              (busy
                ? "Keep Quill open. The app will reopen when it’s finished."
                : "Install when convenient. Your settings and downloaded models stay in place.")}
        </span>
        {state.phase === "downloading" ? (
          <span className="app-update__progress" aria-hidden="true">
            <i className={percent === null ? "is-indeterminate" : ""} style={percent === null ? undefined : { width: `${percent}%` }} />
          </span>
        ) : null}
      </div>
      {!busy ? (
        <div className="app-update__actions">
          <button
            className="primary-button compact"
            type="button"
            disabled={Boolean(blockedReason)}
            onClick={() => void install()}
            title={blockedReason ?? undefined}
          >
            {state.phase === "error" ? <RotateCcw size={13} /> : null}
            {state.phase === "error" ? "Retry" : "Update and restart"}
          </button>
          <button className="link-button" type="button" onClick={dismiss}>
            Later
          </button>
        </div>
      ) : null}
      {state.phase === "error" ? <span className="sr-only">{state.message}</span> : null}
    </section>
  );
}
