# Security policy

## Reporting a vulnerability

Do not open a public issue for a vulnerability that could expose microphone
audio, recovered transcripts, local replacement snippets, or code-signing
material. Use GitHub's private vulnerability reporting feature for the
repository.

Include the affected commit, platform, reproduction steps, impact, and whether
the issue requires Accessibility permissions.

## Security properties

- Speech audio and transcripts remain local by default. Audio leaves the device
  only when the user explicitly selects Groq transcription; transcript text
  leaves the device only when the user explicitly selects Gemini cleanup.
- Scribe's nearby-text context is disabled by default, including for existing
  installations. When enabled, Quill reads only selected text and a bounded
  region around the caret; it never captures screenshots or password fields.
  Context stays in memory and leaves the device only when Gemini is explicitly
  selected for cleanup.
- Adding a cloud API key never selects or enables that provider automatically.
- Groq and Gemini API keys are stored by Windows Credential Manager or macOS
  Keychain. They are not serialized into `settings.json`, returned to the
  frontend after saving, placed in URLs, logs, tracing fields, or metrics.
- Provider error response bodies are never logged because they can echo audio
  transcripts or prompt content.
- Cleanup runs against a loopback-only LLM endpoint; the client rejects any
  non-loopback host before sending a request.
- Cleanup output can rephrase and restructure the transcript (including
  fixing mishearings and self-corrections) but is prompted to preserve every
  specific fact — names, dates, numbers, URLs — and never invent commitments,
  offers, constraints, or other facts. Email cleanup may add a greeting and
  sign-off; the other detected writing registers may not. A deterministic
  guard replaces the model's output with a safe local draft when the response
  is empty, malformed, truncated at the generation limit, exceeds the
  action-specific output budget, or introduces new promise, availability, proposal, or
  follow-up language absent from the transcript. Inputs that would exceed the
  reserved model-context budget are rejected before any request, preventing
  left-truncation from silently deleting the safety instructions.
- Scribe never inserts cleanup output silently. Every Scribe activation
  opens a review window showing both the raw transcript and the cleaned
  draft; text is only injected into the target editor after the user
  explicitly accepts it (or edits it and then accepts). Discarding the
  draft is always available. This user-in-the-loop step is the primary
  safeguard against model hallucination — the sanity guard above is only a
  best-effort fallback for pathological outputs.
- Transcripts, cleanup source text, dictionary entry contents, and candidate
  dictionary suggestion pairs are never written to the on-disk log or metrics
  files; only counts, lengths, and error classes are recorded.
- Per-app writing preferences store aggregate counts and style choices only.
  Accepted drafts, selected text, and nearby editor content are never retained
  as writing samples.
- Choosing Add persists a suggestion pair as a dictionary entry. Choosing
  Dismiss persists the pair in the local settings file so Quill does not offer
  it again; this dismissal history retains at most the 200 newest pairs and can
  be cleared from Dictionary settings. Neither action sends the pair off-device.
- Update artifacts must be signed before automatic update checks are enabled in
  production.
- Quill does not install global keyboard hooks on Windows.
- Recovery data is stored under the user-local application data directory and
  is deleted after a successful commit.
  If cloud transcription fails before producing text, Quill retains the audio
  checkpoint so the recording is not lost, even when routine audio checkpoints
  were disabled. The recovery banner identifies the failed provider without
  storing its response body.
