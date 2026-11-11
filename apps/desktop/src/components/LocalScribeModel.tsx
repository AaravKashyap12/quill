import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import { deleteLocalScribe, downloadLocalScribe, getLocalScribeStatus, pauseLocalScribe, type LocalScribeStatus } from "../tauri";

export const LOCAL_SCRIBE_MODEL = "qwen3-4b-instruct-2507-q4-k-m";

export function LocalScribeModel({ onChange }: { onChange: () => void }) {
  const [status, setStatus] = useState<LocalScribeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const changed = useRef(onChange);
  changed.current = onChange;
  useEffect(() => {
    let disposed = false;
    const refresh = async () => {
      try {
        const current = await getLocalScribeStatus();
        if (!disposed) setStatus(current);
      } catch (reason) { if (!disposed) setError(String(reason)); }
    };
    void refresh();
    if (!isTauri()) return () => { disposed = true; };
    const progress = listen<{ id: string; bytesDownloaded: number }>("model-download://progress", ({ payload }) => {
      if (payload.id !== LOCAL_SCRIBE_MODEL || disposed) return;
      setStatus((current) => current && { ...current, downloading: true, partialBytes: payload.bytesDownloaded });
    });
    const complete = listen<{ id: string; ok: boolean; error: string | null }>("model-download://complete", ({ payload }) => {
      if (payload.id !== LOCAL_SCRIBE_MODEL || disposed) return;
      setError(payload.ok ? null : payload.error);
      void refresh();
      changed.current();
    });
    return () => {
      disposed = true;
      void progress.then((dispose) => dispose());
      void complete.then((dispose) => dispose());
    };
  }, []);

  async function act(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try { await action(); }
    catch (reason) { setError(String(reason)); }
    finally {
      try { setStatus(await getLocalScribeStatus()); }
      catch (reason) { setError(String(reason)); }
      setBusy(false);
      changed.current();
    }
  }
  const size = status ? `${(status.downloadBytes / 1e9).toFixed(2)} GB` : "2.50 GB";
  const percent = status ? Math.min(100, Math.round(status.partialBytes / status.downloadBytes * 100)) : 0;
  return <div className="local-scribe-model">
    <p><strong>Offline Scribe · Qwen3 4B</strong></p>
    <p>{size} download. Allow at least 4 GB of free memory. Runs inside Quill; your transcript stays on this device. CPU processing can be slow on older computers.</p>
    <p>Models stay installed when Quill updates. You can pause and resume downloads.</p>
    {!status ? <p role="status">Checking local model…</p> : !status.engineAvailable ? <p role="alert">{status.engineError ?? "The Scribe engine is missing. Reinstall Quill to restore it."}</p> : status.installed ? <div className="inline-control">
      <span role="status">Ready for offline Scribe</span>
      <button type="button" className="ghost-action" disabled={busy} onClick={() => void act(deleteLocalScribe)}>Remove model</button>
    </div> : status.downloading || busy ? <div>
      <progress value={percent} max={100} aria-label="Downloading Scribe model" /> <span>{percent}%</span>
      <button type="button" className="ghost-action" onClick={() => void pauseLocalScribe().catch((reason) => setError(String(reason)))}>Pause download</button>
    </div> : <button type="button" className="primary-button" onClick={() => void act(downloadLocalScribe)}>
      {status.partialBytes > 0 ? `Resume download · ${percent}% saved` : `Download Scribe · ${size}`}
    </button>}
    {error ? <p role="status">{error}</p> : null}
  </div>;
}
