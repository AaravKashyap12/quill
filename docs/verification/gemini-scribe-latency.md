# Gemini Scribe latency investigation, 2026-10-22

The supplied redacted timings for the macOS Scribe run at 11:00:55 UTC put
32,251 ms of the 32,708 ms release-to-review interval inside the Gemini request
(98.6%). The reported input length was seven words; its content was not accessed.

Observed stages: `wavEncodeMs=2`, `groqRequestMs=441`,
`releaseToTranscriptMs=453`, `geminiRequestMs=32251` (success),
`scribeGuardMs=0`, `releaseToReviewMs=32708`. These are nested intervals, not
durations to sum. Subtracting transcript readiness and the Gemini request from
release-to-review leaves approximately 4 ms of other work at millisecond precision.

## What the code establishes

- `cleanup.rs` starts `geminiRequestMs` after client acquisition and credential
  lookup. It ends after the response body has been read and decoded as JSON.
  It includes connection establishment if needed, upload, provider wait,
  download, JSON decoding, and any scheduling delay during the awaits. It does
  not isolate Google's model execution from network latency.
- The explicit Gemini provider bypasses local provider discovery. HTTP uses
  asynchronous reqwest calls with no application retry loop. The cloud client
  in `credentials.rs` is pooled and shared, with a 5-second connection timeout,
  120-second idle pool timeout, and proxy support. The request has a 45-second
  deadline. These settings were already present and were not changed here.
- The shared checkout's existing credential-cache work performs OS operations
  on a blocking worker and is preserved. Credential lookup and client creation
  are outside the measured 32,251 ms, so neither explains this sample.
- The model is `gemini-3.1-flash-lite` on both platforms. This request code has
  no macOS/Windows branch. There is no matched Windows timing or build/config
  comparison, so a platform-specific regression is not established.

## Narrow correction

The request forced `temperature: 0` and `thinkingBudget: 0`.
Google recommends temperature `1.0` for Gemini 3, warning that lower values can
cause loops or degraded performance. It supports the older thinking budget for
compatibility, but recommends `thinkingLevel` for predictable performance.
The request now uses `temperature: 1.0` and `thinkingLevel: "minimal"`.
See Google's [Gemini 3 guide](https://ai.google.dev/gemini-api/docs/generate-content/gemini-3)
and [thinking guide](https://ai.google.dev/gemini-api/docs/generate-content/thinking).

Flash-Lite already defaults to minimal thinking, and minimal does not guarantee
zero thinking. The old parameter is not proven to have caused the long wait.
Provider load, network conditions, and model behavior remain hypotheses. There
is no measured speedup from this correction. Temperature may change draft
wording; existing prompt boundaries, output guards, and review remain in place.

The model, 512-token output cap, prompts, timeout, retry behavior, and telemetry
are unchanged. Existing timing is sufficient to locate the slow stage.

## Verification and handoff

The offline request regression builds the production reqwest request with a
synthetic key and prompt, without sending it. It checks the model endpoint,
authentication header, absence of a query string, timeout, prompt separation,
and complete generation configuration. The old configuration failed the
generation-config assertion (`/tmp/quill-scribe-latency-red.log`).

After correction: 26 cleanup tests passed, zero failed, and the existing
live-model evaluation remained ignored (`/tmp/quill-scribe-latency-green.log`).
`rustfmt --check --edition 2021` on `cleanup.rs` and scoped `git diff --check`
passed. Verification reached the focused-test rung. The diff and both Scribe
callers were self-reviewed; no independent review was performed under the
no-delegation constraint.

Run from `apps/desktop/src-tauri` with the repository's configured Rust tools:

```sh
cargo test --locked --offline --lib cleanup::tests
```

Only `cleanup.rs` and this evidence note belong to this task. Existing integrated
changes remain in place. No vault, keys, audio, transcripts, recovery records,
or provider calls were accessed. No desktop interaction, full build, install,
delegation, or commit was performed. The installed application remains untouched.

After the owner produces the combined build, repeat comparable short Scribe
runs through the normal controls and compare `geminiRequestMs` and
`releaseToReviewMs`, including outcomes. Use at least five alternating runs per
configuration for a speed claim; keep draft quality review separate from safe
timing evidence. This offline change cannot certify provider response time or
Windows parity.
