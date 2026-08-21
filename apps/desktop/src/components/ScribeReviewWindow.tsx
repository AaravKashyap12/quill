import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import {
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  FileText,
  PenLine,
  RefreshCw,
  Send,
  X,
} from "lucide-react";
import { FormEvent, useEffect, useRef, useState } from "react";
import {
  acceptScribeReview,
  discardScribeReview,
  getScribeReview,
  regenerateScribeReview,
} from "../tauri";
import type { Register, ScribeReviewDraft } from "../types";

const registerOptions: Array<{ value: Register; label: string }> = [
  { value: "email", label: "Email" },
  { value: "chat", label: "Chat" },
  { value: "prompt", label: "Prompt" },
  { value: "notes", label: "Notes" },
  { value: "generic", label: "General" },
];

interface ScribeReviewWindowProps {
  embedded?: boolean;
  initialReview?: ScribeReviewDraft | null;
  onInserted?: () => void;
  onDiscarded?: () => void;
}

export function ScribeReviewWindow({
  embedded = false,
  initialReview = null,
  onInserted,
  onDiscarded,
}: ScribeReviewWindowProps) {
  const [review, setReview] = useState<ScribeReviewDraft | null>(initialReview);
  const [draft, setDraft] = useState(initialReview?.draft ?? "");
  const [instruction, setInstruction] = useState("");
  const [versions, setVersions] = useState<string[]>(
    initialReview ? [initialReview.draft] : [],
  );
  const [versionIndex, setVersionIndex] = useState(initialReview ? 0 : -1);
  const [working, setWorking] = useState<"regenerate" | "insert" | "discard" | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const reviewId = useRef(initialReview?.id ?? null);

  function applyReview(next: ScribeReviewDraft, focusDraft = true) {
    const isNewSession = reviewId.current !== next.id;
    reviewId.current = next.id;
    setReview(next);
    setDraft(next.draft);
    setError(null);
    setVersions((current) => {
      const base = isNewSession ? [] : current;
      if (base[base.length - 1] === next.draft) {
        setVersionIndex(base.length - 1);
        return base;
      }
      const updated = [...base, next.draft];
      setVersionIndex(updated.length - 1);
      return updated;
    });
    if (focusDraft) window.setTimeout(() => textarea.current?.focus(), 80);
  }

  useEffect(() => {
    if (!isTauri()) return;
    if (!initialReview) {
      void getScribeReview().then((current) => {
        if (current) applyReview(current, true);
      });
    }
    let dispose: (() => void) | undefined;
    let alive = true;
    void listen<ScribeReviewDraft>("scribe-review://updated", (event) => {
      if (!alive) return;
      // Background regeneration updates must never steal focus from the
      // instruction field or the user's current review position.
      applyReview(event.payload, false);
    }).then((unlisten) => {
      if (alive) dispose = unlisten;
      else unlisten();
    });
    return () => {
      alive = false;
      dispose?.();
    };
  }, []);

  function selectVersion(nextIndex: number) {
    const next = versions[nextIndex];
    if (next === undefined) return;
    setVersionIndex(nextIndex);
    setDraft(next);
    setError(null);
    textarea.current?.focus();
  }

  async function regenerate(
    registerOverride?: Register,
    followUp = instruction,
    useContext?: boolean,
  ) {
    if (!review || working) return;
    const previousRegister = review.register;
    if (registerOverride) {
      setReview((current) =>
        current ? { ...current, register: registerOverride } : current,
      );
    }
    setWorking("regenerate");
    setError(null);
    setCopied(false);
    try {
      const next = isTauri()
        ? await regenerateScribeReview(registerOverride, followUp, useContext)
        : {
            ...review,
            register: registerOverride ?? review.register,
            contextUsed: useContext ?? review.contextUsed,
            draft: followUp.trim()
              ? `${draft.trim()}\n\n${followUp.trim().replace(/^./, (letter) => letter.toUpperCase())}.`
              : versions.length % 2 === 0
                ? "Hi Jordan,\n\n5 PM tomorrow works for me. You can use my calendar link to schedule it.\n\nTalk soon."
                : "Hi Jordan,\n\nI’m available at 5 PM tomorrow. Please use my calendar link to book the time.\n\nBest,",
          };
      applyReview(next, false);
      setInstruction("");
    } catch (reason) {
      if (registerOverride) {
        setReview((current) =>
          current ? { ...current, register: previousRegister } : current,
        );
      }
      setError(String(reason));
    } finally {
      setWorking(null);
    }
  }

  async function insert() {
    if (!draft.trim() || working) return;
    setWorking("insert");
    setError(null);
    try {
      if (isTauri()) await acceptScribeReview(draft);
      onInserted?.();
    } catch (reason) {
      setError(String(reason));
      setWorking(null);
    }
  }

  async function discard() {
    if (working) return;
    setWorking("discard");
    setError(null);
    try {
      if (isTauri()) await discardScribeReview();
      onDiscarded?.();
    } catch (reason) {
      setError(String(reason));
      setWorking(null);
    }
  }

  async function copyDraft() {
    try {
      await navigator.clipboard.writeText(draft);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1300);
    } catch (reason) {
      setError(`Couldn't copy the draft: ${String(reason)}`);
    }
  }

  function submitFollowUp(event: FormEvent) {
    event.preventDefault();
    if (instruction.trim()) void regenerate(undefined, instruction);
  }

  const content = !review ? (
    <section className="scribe-composer is-loading" role="status" aria-live="polite">
      <span className="scribe-composer__loading-mark" aria-hidden="true">
        <PenLine size={17} strokeWidth={1.8} />
      </span>
      <span>Preparing your draft</span>
      <i aria-hidden="true" />
    </section>
  ) : (
    <section
      className={`scribe-composer${working === "regenerate" ? " is-regenerating" : ""}`}
      aria-busy={working !== null}
      onKeyDown={(event) => {
        if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
          event.preventDefault();
          void insert();
        } else if (event.key === "Escape") {
          event.preventDefault();
          void discard();
        }
      }}
    >
      <header className="scribe-composer__header" data-tauri-drag-region>
        <h1>Review draft</h1>
        <span className="scribe-composer__versions" aria-label="Draft versions">
          <button
            type="button"
            onClick={() => selectVersion(versionIndex - 1)}
            disabled={versionIndex <= 0 || working !== null}
            aria-label="Previous draft version"
          >
            <ChevronLeft size={13} />
          </button>
          <b>{Math.max(1, versionIndex + 1)} / {Math.max(1, versions.length)}</b>
          <button
            type="button"
            onClick={() => selectVersion(versionIndex + 1)}
            disabled={versionIndex >= versions.length - 1 || working !== null}
            aria-label="Next draft version"
          >
            <ChevronRight size={13} />
          </button>
        </span>

        <label className="scribe-composer__register">
          <span className="sr-only">Writing style</span>
          <select
            value={review.register}
            onChange={(event) => void regenerate(event.target.value as Register, "")}
            disabled={working !== null}
            aria-label="Writing style"
          >
            {registerOptions.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
          <ChevronDown size={12} aria-hidden="true" />
        </label>

        <button
          type="button"
          className="scribe-composer__close"
          onClick={() => void discard()}
          disabled={working !== null}
          aria-label="Close Scribe review"
        >
          <X size={15} />
        </button>
      </header>

      <div className="scribe-composer__context" aria-label="Scribe context">
        <FileText size={12} aria-hidden="true" />
        <span>{review.contextLabel}</span>
        <b>{review.action}</b>
      </div>

      <div className="scribe-composer__editor">
        <label htmlFor="scribe-draft">Generated draft</label>
        <textarea
          ref={textarea}
          id="scribe-draft"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          spellCheck
          disabled={working === "regenerate"}
          aria-describedby={review.warning || error ? "scribe-composer-message" : undefined}
        />
        <div className="scribe-composer__regenerating" aria-hidden="true">
          <span className="voice-pill__dots">
            {Array.from({ length: 11 }, (_, index) => (
              <i key={index} style={{ animationDelay: `${index * 58}ms` }} />
            ))}
          </span>
          <span className="voice-pill__ring" />
        </div>
      </div>

      {review.warning || error ? (
        <p id="scribe-composer-message" className={error ? "is-error" : ""} role={error ? "alert" : undefined}>
          {error ?? review.warning}
        </p>
      ) : null}

      <form className="scribe-composer__follow-up" onSubmit={submitFollowUp}>
        <PenLine size={13} aria-hidden="true" />
        <input
          value={instruction}
          onChange={(event) => setInstruction(event.target.value)}
          placeholder="Refine or add details"
          disabled={working !== null}
          aria-label="Follow-up refinement instruction"
        />
        <button
          type="submit"
          disabled={!instruction.trim() || working !== null}
          aria-label="Apply refinement instruction"
        >
          <Send size={13} />
        </button>
      </form>

      <footer className="scribe-composer__actions">
        <span className="scribe-composer__quick-actions">
          <button type="button" onClick={() => void copyDraft()} disabled={working !== null}>
            {copied ? <Check size={14} /> : <Copy size={14} />}
            <span>{copied ? "Copied" : "Copy"}</span>
          </button>
          <button type="button" onClick={() => void regenerate(undefined, "")} disabled={working !== null}>
            <RefreshCw size={14} />
            <span>{working === "regenerate" ? "Regenerating" : "Regenerate"}</span>
          </button>
          {review.contextAvailable && review.contextUsed ? (
            <button
              type="button"
              onClick={() => void regenerate(undefined, "", false)}
              disabled={working !== null}
              title="Create another draft without nearby editor text"
            >
              <FileText size={14} />
              <span>Without context</span>
            </button>
          ) : null}
        </span>
        <button
          type="button"
          className="scribe-composer__insert"
          onClick={() => void insert()}
          disabled={working !== null || !draft.trim()}
        >
          <span>{working === "insert" ? "Inserting" : "Insert"}</span>
          <Check size={14} />
        </button>
      </footer>

      <span className="sr-only" role="status" aria-live="polite">
        {working === "regenerate" ? "Regenerating draft" : copied ? "Draft copied" : ""}
      </span>
    </section>
  );

  if (embedded) return content;
  return <main className="review-page">{content}</main>;
}
