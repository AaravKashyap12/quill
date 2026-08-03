import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronDown,
  ChevronUp,
  Copy,
  LifeBuoy,
  Trash2,
  X,
} from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { discardRecovery, getPendingRecovery } from "../tauri";
import type { RecoveryManifest } from "../types";

function formatWhen(ms: number): string {
  const delta = Date.now() - ms;
  const minutes = Math.round(delta / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  return `${days} d ago`;
}

/** Banner that surfaces a transcript checkpoint left behind by a crashed
 *  session. Persists across launches until the user copies or discards. */
type Status =
  | { kind: "idle" }
  | { kind: "copy-failed"; error: string }
  | { kind: "cleared" }
  | { kind: "discard-failed"; error: string };

export function RecoveryBanner() {
  const [pending, setPending] = useState<RecoveryManifest | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  useEffect(() => {
    let disposed = false;
    let disposeListener: (() => void) | undefined;
    void (async () => {
      if (isTauri()) {
        disposeListener = await listen<RecoveryManifest>(
          "recovery://pending",
          (event) => {
            if (disposed) return;
            setPending(event.payload);
            setExpanded(false);
            setStatus({ kind: "idle" });
          },
        );
      }

      const value = await getPendingRecovery();
      if (!disposed && value) {
        setPending((current) =>
          !current || value.updatedAtUnixMs >= current.updatedAtUnixMs
            ? value
            : current,
        );
      }
    })();
    return () => {
      disposed = true;
      disposeListener?.();
    };
  }, []);

  if (!pending) return null;

  const wordCount = pending.transcript.trim().split(/\s+/).filter(Boolean).length;

  /**
   * Two-step, non-destructive-until-safe: try the clipboard first, and only
   * clear the on-disk checkpoint when that write succeeded. If the discard
   * itself then fails (locked file, permission error), keep the banner
   * visible with an inline notice so the user knows the file survives and
   * can retry manually.
   */
  async function copyAndClear() {
    const recovery = pending;
    if (!recovery) return;
    setStatus({ kind: "idle" });
    try {
      await navigator.clipboard.writeText(recovery.transcript);
    } catch (error) {
      setExpanded(true);
      setStatus({
        kind: "copy-failed",
        error:
          "Clipboard is unavailable — copy the text below manually, then Discard when done.",
      });
      return;
    }
    try {
      await discardRecovery(recovery.id);
      setStatus({ kind: "cleared" });
      window.setTimeout(
        () =>
          setPending((current) =>
            current?.id === recovery.id ? null : current,
          ),
        900,
      );
    } catch (error) {
      const latest = await getPendingRecovery().catch(() => null);
      if (latest && latest.id !== recovery.id) {
        setPending(latest);
        setExpanded(false);
        setStatus({
          kind: "discard-failed",
          error:
            "Copied to clipboard. A newer recording replaced this checkpoint, so Quill kept and displayed the newer recovery instead.",
        });
        return;
      }
      setStatus({
        kind: "discard-failed",
        error: `Copied to clipboard, but the recovery file couldn't be removed: ${String(
          error,
        )}. It will still appear next launch — retry Discard.`,
      });
    }
  }

  async function discard() {
    const recovery = pending;
    if (!recovery) return;
    setStatus({ kind: "idle" });
    try {
      await discardRecovery(recovery.id);
      setPending((current) =>
        current?.id === recovery.id ? null : current,
      );
    } catch (error) {
      const latest = await getPendingRecovery().catch(() => null);
      if (latest && latest.id !== recovery.id) {
        setPending(latest);
        setExpanded(false);
        setStatus({
          kind: "discard-failed",
          error:
            "A newer recording replaced this checkpoint, so Quill kept and displayed the newer recovery instead.",
        });
        return;
      }
      setStatus({
        kind: "discard-failed",
        error: `Couldn't remove the recovery file: ${String(
          error,
        )}. It will still appear next launch.`,
      });
    }
  }

  return (
    <div className="recovery-banner" role="status" aria-live="polite">
      <div className="recovery-banner__head">
        <span className="recovery-banner__icon" aria-hidden="true">
          <LifeBuoy size={16} strokeWidth={1.8} />
        </span>
        <div className="recovery-banner__meta">
          <strong>Recovered {pending.mode} from a previous session</strong>
          <span>
            {wordCount === 1 ? "1 word" : `${wordCount} words`} · saved{" "}
            {formatWhen(pending.updatedAtUnixMs)}
            {pending.audioPath ? " · audio kept" : ""}
          </span>
        </div>
        <div className="recovery-banner__actions">
          <button
            type="button"
            className="ghost-button"
            onClick={() => setExpanded((s) => !s)}
            aria-expanded={expanded}
          >
            {expanded ? (
              <>
                <ChevronUp size={14} strokeWidth={2} />
                Hide
              </>
            ) : (
              <>
                <ChevronDown size={14} strokeWidth={2} />
                Show text
              </>
            )}
          </button>
          <button
            type="button"
            className="ghost-button ghost-button--danger"
            onClick={discard}
            title="Discard recovery"
            aria-label="Discard recovery"
          >
            <Trash2 size={14} strokeWidth={1.8} />
          </button>
          <button
            type="button"
            className="primary-button compact"
            onClick={copyAndClear}
            disabled={status.kind === "cleared"}
            title="Copy the recovered transcript to your clipboard, then delete the checkpoint from disk."
          >
            {status.kind === "cleared" ? (
              <>
                <X size={14} strokeWidth={2} />
                Cleared
              </>
            ) : (
              <>
                <Copy size={14} strokeWidth={2} />
                Copy and clear
              </>
            )}
          </button>
        </div>
      </div>
      {status.kind === "copy-failed" || status.kind === "discard-failed" ? (
        <p className="recovery-banner__status" role="alert">
          {status.error}
        </p>
      ) : null}
      {expanded ? (
        <div className="recovery-banner__body">
          <pre>{pending.transcript.trim() || "(empty)"}</pre>
          {pending.audioPath ? (
            <p className="recovery-banner__audio">
              Raw audio kept at <code>{pending.audioPath}</code>. Delete manually
              if you no longer need it.
            </p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
