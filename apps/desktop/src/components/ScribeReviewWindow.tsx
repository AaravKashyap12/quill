import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Check,
  ChevronDown,
  LoaderCircle,
  PenLine,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { capDismissedSuggestions, createDictionaryEntryId } from "../dictionary";
import {
  acceptScribeReview,
  discardScribeReview,
  getScribeReview,
  loadSettings,
  persistSettings,
  regenerateScribeReview,
} from "../tauri";
import type { DictionarySuggestion, Register, ScribeReviewDraft } from "../types";

const registerOptions: Array<{ value: Register; label: string }> = [
  { value: "email", label: "Email" },
  { value: "chat", label: "Chat" },
  { value: "prompt", label: "AI prompt" },
  { value: "notes", label: "Notes" },
  { value: "generic", label: "General" },
];

export function ScribeReviewWindow() {
  const [review, setReview] = useState<ScribeReviewDraft | null>(null);
  const [suggestion, setSuggestion] = useState<DictionarySuggestion | null>(null);
  const [draft, setDraft] = useState("");
  const [processingMessage, setProcessingMessage] = useState("Transcribing on your device");
  const [working, setWorking] = useState<
    "regenerate" | "done" | "discard" | "add-suggestion" | "dismiss-suggestion" | null
  >(null);
  const [error, setError] = useState<string | null>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);

  function applyReview(next: ScribeReviewDraft, focusDraft = true) {
    setSuggestion(null);
    setReview(next);
    setDraft(next.draft);
    setError(null);
    if (!focusDraft) return;
    window.setTimeout(() => {
      textarea.current?.focus();
      textarea.current?.setSelectionRange(next.draft.length, next.draft.length);
    }, 40);
  }

  useEffect(() => {
    void getScribeReview().then((current) => {
      if (current) applyReview(current);
    });
    const unlisten = listen<ScribeReviewDraft>("scribe-review://updated", (event) => {
      applyReview(event.payload);
    });
    const unlistenProcessing = listen<{ message: string }>(
      "scribe-review://processing",
      (event) => {
        setReview(null);
        setSuggestion(null);
        setDraft("");
        setError(null);
        setProcessingMessage(event.payload.message);
      },
    );
    return () => {
      void unlisten.then((dispose) => dispose());
      void unlistenProcessing.then((dispose) => dispose());
    };
  }, []);

  async function regenerate(registerOverride?: Register) {
    const previousRegister = review?.register;
    if (registerOverride) {
      setReview((current) =>
        current ? { ...current, register: registerOverride } : current,
      );
    }
    setWorking("regenerate");
    setError(null);
    try {
      applyReview(
        await regenerateScribeReview(registerOverride),
        registerOverride === undefined,
      );
    } catch (reason) {
      if (registerOverride && previousRegister) {
        setReview((current) =>
          current ? { ...current, register: previousRegister } : current,
        );
      }
      setError(String(reason));
    } finally {
      setWorking(null);
    }
  }

  async function done() {
    setWorking("done");
    setError(null);
    try {
      const nextSuggestion = await acceptScribeReview(draft);
      if (nextSuggestion) {
        setSuggestion(nextSuggestion);
        setReview(null);
        setDraft("");
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(null);
    }
  }

  async function resolveSuggestion(action: "add" | "dismiss") {
    if (!suggestion) return;
    setWorking(action === "add" ? "add-suggestion" : "dismiss-suggestion");
    setError(null);
    try {
      const settings = await loadSettings();
      if (action === "add") {
        const knownWords = settings.dictionary.flatMap((entry) => [
          entry.spoken.trim().toLowerCase(),
          entry.replacement.trim().toLowerCase(),
        ]);
        const spokenKey = suggestion.spoken.trim().toLowerCase();
        const replacementKey = suggestion.replacement.trim().toLowerCase();
        if (!knownWords.includes(spokenKey) && !knownWords.includes(replacementKey)) {
          await persistSettings({
            ...settings,
            dictionary: [
              ...settings.dictionary,
              {
                id: createDictionaryEntryId(),
                spoken: suggestion.spoken,
                replacement: suggestion.replacement,
                kind: "word",
              },
            ],
          });
        }
      } else {
        const alreadyDismissed = settings.dismissedSuggestions.some(
          (dismissed) =>
            dismissed.spoken.toLowerCase() === suggestion.spoken.toLowerCase() &&
            dismissed.replacement.toLowerCase() === suggestion.replacement.toLowerCase(),
        );
        if (!alreadyDismissed) {
          await persistSettings({
            ...settings,
            dismissedSuggestions: capDismissedSuggestions([
              ...settings.dismissedSuggestions,
              suggestion,
            ]),
          });
        }
      }
      await getCurrentWindow().hide();
      setSuggestion(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(null);
    }
  }

  async function discard() {
    setWorking("discard");
    setError(null);
    try {
      await discardScribeReview();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(null);
    }
  }

  if (suggestion) {
    return (
      <main className="review-page review-page--suggestion">
        <section className="review-suggestion" aria-busy={working !== null}>
          <p role="status" aria-live="polite">
            <span>Teach Quill:</span>
            <strong>{suggestion.spoken}</strong>
            <span aria-hidden="true">→</span>
            <strong>{suggestion.replacement}</strong>
          </p>
          <span className="review-suggestion__actions">
            <button
              type="button"
              className="review-button review-button--primary review-button--compact"
              onClick={() => void resolveSuggestion("add")}
              disabled={working !== null}
            >
              {working === "add-suggestion" ? "Adding" : "Add"}
            </button>
            <button
              type="button"
              className="review-button review-button--quiet review-button--compact"
              onClick={() => void resolveSuggestion("dismiss")}
              disabled={working !== null}
            >
              {working === "dismiss-suggestion" ? "Dismissing" : "Dismiss"}
            </button>
          </span>
          {error ? (
            <small className="review-suggestion__error" role="alert">
              {error}
            </small>
          ) : null}
        </section>
      </main>
    );
  }

  if (!review) {
    return (
      <main className="review-page">
        <section className="review-card review-card--loading" aria-live="polite">
          <span className="review-loading-mark" aria-hidden="true">
            <PenLine size={17} strokeWidth={1.8} />
          </span>
          <span className="review-loading-copy">
            <b>Preparing your draft</b>
            <span>{processingMessage}</span>
          </span>
          <LoaderCircle className="review-loading-spinner" size={17} aria-hidden="true" />
          <small>
            <ShieldCheck size={12} aria-hidden="true" />
            Stays on this device
          </small>
        </section>
      </main>
    );
  }

  return (
    <main
      className="review-page"
      onKeyDown={(event) => {
        if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
          event.preventDefault();
          void done();
        }
      }}
    >
      <section className="review-card" aria-busy={working !== null}>
        <header className="review-header" data-tauri-drag-region>
          <span className="review-mark" aria-hidden="true">
            <PenLine size={15} strokeWidth={1.9} />
          </span>
          <span className="review-heading" data-tauri-drag-region>
            <b>Scribe draft</b>
            <small>Review before Quill types</small>
          </span>
          <label className="review-register">
            <span>Writing style</span>
            <select
              value={review.register}
              onChange={(event) =>
                void regenerate(event.target.value as Register)
              }
              disabled={working !== null}
              aria-describedby="review-register-help"
            >
              {registerOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <span id="review-register-help" className="sr-only">
            Changing the writing style regenerates the draft from what Quill heard.
          </span>
          <span className="sr-only" role="status" aria-live="polite">
            {working === "regenerate" ? "Regenerating draft" : ""}
          </span>
          <span className="review-local">
            <ShieldCheck size={13} aria-hidden="true" />
            Local
          </span>
        </header>

        <div className="review-editor">
          <label htmlFor="scribe-draft">Your final text</label>
          <textarea
            ref={textarea}
            id="scribe-draft"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            spellCheck
            aria-describedby={review.warning || error ? "review-message" : undefined}
          />
        </div>

        {review.warning || error ? (
          <p id="review-message" className={`review-message ${error ? "is-error" : ""}`}>
            {error ?? review.warning}
          </p>
        ) : null}

        <details className="review-source">
          <summary>
            What Quill heard
            <ChevronDown size={14} aria-hidden="true" />
          </summary>
          <p>{review.source}</p>
        </details>

        <footer className="review-actions">
          <button
            type="button"
            className="review-button review-button--quiet"
            onClick={() => void discard()}
            disabled={working !== null}
          >
            <Trash2 size={14} />
            Discard
          </button>
          <span className="review-action-group">
            <button
              type="button"
              className="review-button"
              onClick={() => void regenerate()}
              disabled={working !== null}
            >
              <RefreshCw size={14} className={working === "regenerate" ? "is-spinning" : ""} />
              {working === "regenerate" ? "Regenerating" : "Regenerate"}
            </button>
            <button
              type="button"
              className="review-button review-button--primary"
              onClick={() => void done()}
              disabled={working !== null || !draft.trim()}
            >
              <Check size={15} strokeWidth={2.2} />
              {working === "done" ? "Inserting" : "Done"}
            </button>
          </span>
        </footer>
      </section>
    </main>
  );
}
