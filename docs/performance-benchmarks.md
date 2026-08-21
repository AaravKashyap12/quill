# Performance benchmarks

This file records release-relevant latency baselines for Quill. Future changes
to provider prompts, context capture, per-app style learning, safety guards, or
review preparation should be compared against the same measurements.

## Scribe cloud baseline

Measured on 2026-08-21 using a Windows development build with an under-five-
second utterance and the following explicit provider path:

`Groq whisper-large-v3 -> Gemini 3.1 Flash-Lite -> safety guards -> review`

| Stage | Metric | Result |
| --- | --- | ---: |
| Audio encoding | `wavEncodeMs` | 4 ms |
| Groq transcription request | `groqRequestMs` | 931 ms |
| Gemini cleanup request | `geminiRequestMs` | 1,289 ms |
| Scribe output guards | `scribeGuardMs` | <1 ms |
| Release to transcript | `releaseToTranscriptMs` | 945 ms |
| **Release to review** | **`releaseToReviewMs`** | **2,241 ms** |

### Regression ceiling

**2,241 ms release-to-review is the current regression ceiling for this path.**
More complex per-app context or style learning should not make this comparable
short-utterance flow slower without an explicit product decision and a measured
user benefit.

This is a single real-world sample, not a percentile distribution. Preserve it
as the initial ceiling, then supplement it with p50 and p95 results once at
least 30 comparable successful activations have been collected. Do not combine
different audio-duration buckets, provider paths, failures, or cold-start runs
into the same comparison.

### Comparison protocol

- Use the same provider models recorded above.
- Compare the `under5s` audio-duration bucket.
- Measure from shortcut release until the review draft is ready.
- Record successful requests separately from timeouts, quota errors, and other
  provider failures.
- Keep warm and cold provider/network runs distinguishable.
- Record only content-free timing metrics; never include audio, transcripts,
  prompts, editor context, API keys, filenames, or provider response bodies.
