import { useMemo, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { createDictionaryEntryId } from "../dictionary";
import type { AppSettings, DictionaryEntry } from "../types";

interface DictionaryViewProps {
  settings: AppSettings;
  update: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void;
}

export function DictionaryView({ settings, update }: DictionaryViewProps) {
  const entries = settings.dictionary;
  const [query, setQuery] = useState("");
  const [spoken, setSpoken] = useState("");
  const [replacement, setReplacement] = useState("");
  const [kind, setKind] = useState<DictionaryEntry["kind"]>("word");
  const [formError, setFormError] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  const filtered = useMemo(
    () =>
      entries.filter((entry) =>
        `${entry.spoken} ${entry.replacement}`.toLowerCase().includes(normalizedQuery),
      ),
    [entries, normalizedQuery],
  );

  function addEntry() {
    const trimmedSpoken = spoken.trim();
    const trimmedReplacement = replacement.trim();

    if (!trimmedSpoken || !trimmedReplacement) {
      setFormError("Enter both what you say and what Quill should type.");
      return;
    }

    const entry: DictionaryEntry = {
      id: createDictionaryEntryId(),
      spoken: trimmedSpoken,
      replacement: trimmedReplacement,
      kind,
    };

    update("dictionary", [...entries, entry]);
    setSpoken("");
    setReplacement("");
    setFormError(null);
    setStatus(`Added ${trimmedSpoken}. Save changes to keep it.`);
  }

  function removeEntry(entry: DictionaryEntry) {
    update(
      "dictionary",
      entries.filter((item) => item.id !== entry.id),
    );
    setStatus(`Removed ${entry.spoken}. Save changes to confirm.`);
  }

  function clearDismissedSuggestions() {
    const count = settings.dismissedSuggestions.length;
    if (count === 0) return;
    update("dismissedSuggestions", []);
    setStatus(
      `Cleared ${count} dismissed ${count === 1 ? "suggestion" : "suggestions"}. Save changes to confirm.`,
    );
  }

  return (
    <div className="view-stack">
      <header className="view-heading">
        <h1>Teach Quill your words.</h1>
      </header>

      <section className="dictionary-compose" aria-labelledby="new-entry-title">
        <div className="section-heading">
          <h2 id="new-entry-title">Add a replacement</h2>
          <span>Stored only on this device.</span>
        </div>
        <form
          className="compose-row"
          onSubmit={(event) => {
            event.preventDefault();
            addEntry();
          }}
          aria-describedby={formError ? "dictionary-form-error" : undefined}
        >
          <label>
            <span>When I say</span>
            <input
              value={spoken}
              onChange={(event) => {
                setSpoken(event.target.value);
                setFormError(null);
              }}
              placeholder="whisper dot cpp"
              aria-invalid={formError ? true : undefined}
              aria-describedby={formError ? "dictionary-form-error" : undefined}
            />
          </label>
          <span className="compose-arrow" aria-hidden="true">→</span>
          <label>
            <span>Type</span>
            <input
              value={replacement}
              onChange={(event) => {
                setReplacement(event.target.value);
                setFormError(null);
              }}
              placeholder="whisper.cpp"
              aria-invalid={formError ? true : undefined}
              aria-describedby={formError ? "dictionary-form-error" : undefined}
            />
          </label>
          <label>
            <span>Kind</span>
            <select
              className="dictionary-kind-select"
              value={kind}
              onChange={(event) => setKind(event.target.value as DictionaryEntry["kind"])}
            >
              <option value="word">Word</option>
              <option value="snippet">Snippet</option>
            </select>
          </label>
          <button className="primary-button compact" type="submit">
            <Plus size={16} aria-hidden="true" />
            Add
          </button>
        </form>
        {formError ? (
          <p className="dictionary-form-error" id="dictionary-form-error" role="alert">
            {formError}
          </p>
        ) : null}
        <span className="sr-only" role="status" aria-live="polite">
          {status}
        </span>
      </section>

      <section aria-labelledby="entries-title">
        <div className="section-heading with-search">
          <div>
            <h2 id="entries-title">Saved terms</h2>
            <span>{entries.length} local replacements</span>
          </div>
          <input
            className="search-input"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter terms"
            aria-label="Filter dictionary terms"
          />
        </div>
        <div className="dictionary-list">
          {filtered.length ? (
            filtered.map((entry) => (
              <div className="dictionary-row" key={entry.id}>
                <span className="dictionary-kind">{entry.kind === "word" ? "Word" : "Snippet"}</span>
                <span>{entry.spoken}</span>
                <span className="dictionary-arrow" aria-hidden="true">→</span>
                <strong>{entry.replacement}</strong>
                <button
                  className="icon-button subtle"
                  type="button"
                  onClick={() => removeEntry(entry)}
                  aria-label={`Remove ${entry.spoken}`}
                >
                  <Trash2 size={15} aria-hidden="true" />
                </button>
              </div>
            ))
          ) : (
            <div className="empty-state">
              <strong>{entries.length ? "No matching terms" : "No saved terms yet"}</strong>
              <span>
                {entries.length
                  ? "Try a different filter or add the phrase above."
                  : "Add a word or snippet above, then save your changes."}
              </span>
            </div>
          )}
        </div>
      </section>

      <section aria-labelledby="dismissed-suggestions-title">
        <div className="dictionary-suggestions-setting">
          <div>
            <h2 id="dismissed-suggestions-title">Dismissed suggestions</h2>
            <p id="dismissed-suggestions-description">
              {settings.dismissedSuggestions.length
                ? `${settings.dismissedSuggestions.length} correction ${settings.dismissedSuggestions.length === 1 ? "pair is" : "pairs are"} hidden on this device.`
                : "No correction suggestions are currently hidden."}
            </p>
          </div>
          <button
            type="button"
            className="dictionary-clear-button"
            onClick={clearDismissedSuggestions}
            disabled={settings.dismissedSuggestions.length === 0}
            aria-label="Clear dismissed suggestions"
            aria-describedby="dismissed-suggestions-description"
          >
            Clear
          </button>
        </div>
      </section>
    </div>
  );
}
