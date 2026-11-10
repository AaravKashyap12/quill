# Dictation omissions investigation

Reported setup: Windows, Groq speech recognition. The user reports missing text
in one-to-two-minute recordings, working short recordings, and no recorder
defects. Pauses or hesitations may correlate with omissions; this is not yet
established by a captured failure. App version remains unconfirmed. Source inspected:
`0ed312ee1326ca46cd776564d2b304471e253f3e` (0.4.6). AStack revision:
`f3784322978080349f67527bd60af803e96804eb`.

## Reproduced defect and candidate fix

Groq returns segments, then Quill applies provider-confidence and acoustic
checks before building text. The acoustic check previously required at least
8% voiced frames across the entire provider segment. A segment containing
sustained speech separated by a long pause can fail that ratio. The segment
is removed entirely while other segments survive, producing a partial result.

The existing long-pause test only verified that audio was eligible for upload;
it never checked the later segment filter. Strengthening it to check
`supports_interval` fails on the original implementation. The candidate also
accepts a sustained run of three consecutive voiced frames, using the existing
minimum speech-frame count, while retaining the original ratio path. A Groq
regression verifies that the early passage and final sentence both reach the
text and synthesized insertion words. Additional 60- and 120-second fixtures
check every numbered passage and provider-returned filler across multiple
20-second segments with long pauses, including the final insertion words.
These are synthetic activity/response fixtures, not recorded speech.
Silence/click/sparse-impulse tests remain.

This is a deterministic reproduction using synthetic VAD decisions and a
provider-shaped response, not proof that the detector classified a particular
Windows microphone recording correctly. Three consecutive frames represent
60 ms; allowing this evidence across a long segment may admit noise classified
as speech. Real speech and background-noise trials are still required.

Rejected/accepted segment counts are now logged without text or audio. This
helps distinguish local filtering from incomplete provider output. Confidence
thresholds are unchanged pending actual failure evidence.

## Trace of other possible losses

- Capture appends mono samples to an accumulating buffer. No fixed-duration
  truncation was found there. Stream errors are surfaced by snapshot creation.
- Groq/Gemini get a padded speech window. Internal pauses are retained, but
  boundary trimming can still omit speech a detector misses. No Windows audio
  was available to test that possibility.
- Groq uses `whisper-large-v3`, verbose segment responses, a 90-second request
  timeout and a 25 MiB client upload ceiling. Oversized uploads fail explicitly;
  they do not intentionally upload only a prefix.
- Groq confidence rules can independently reject whole segments, including
  low log probability by itself. That remains a plausible loss source. Changing
  these heuristics without a paired audio/provider-response fixture risks
  restoring silence hallucinations. Count diagnostics make this inspectable.
- Final Dictation inserts the pass's words. The cloud adapter synthesizes words
  when Groq returns only segments. Local Whisper has a separate risk when full
  text and optional word metadata disagree. A proposed local fallback was
  withdrawn after review found it could restore suppressed control tokens;
  it is not part of this Groq candidate.
- No evidence currently establishes Windows hotkey cancellation, stereo
  cancellation, a provider omission, or target-editor truncation as the cause.

## Open-source and provider comparison

[Handy PR #2282](https://github.com/cjpais/Handy/pull/2282), merged at head
`00974ccfc1188f1b7d7cbc565bb06054ed5cf4fa`, adds pause-aware long-form chunking.
Its reviewed patch targets 30-second chunks, allows up to 40 seconds, and keeps
a minimum remainder. It also unifies live/batch finalization. This is useful for
a future long-recording design; it does not justify slicing Groq audio blindly
or prove a few-line recording needs chunking. Boundary-word preservation,
sample coverage and partial-request failures must be tested before adopting it.

[OpenWhispr issue #462](https://github.com/OpenWhispr/openwhispr/issues/462)
records failed validation of RMS/gap compression and discusses retaining
structured provider output before filtering. These are issue reports and design
proposals, not verified guarantees of the current product. The relevant lesson
is to test both hallucinations and missing speech; stronger filtering alone
is not necessarily an improvement.

[Whisper's reference transcriber](https://github.com/openai/whisper/blob/main/whisper/transcribe.py)
uses decoding fallback for poor confidence and lets confident decoding override
a high no-speech probability. Quill's Groq post-filter is more aggressive in
some cases. Groq's server implementation is not established by this comparison.
[Groq's speech documentation](https://console.groq.com/docs/speech-to-text)
describes verbose confidence metadata and chunking for larger files; it does
not establish Quill's thresholds as an accuracy guarantee.

## Verification and next evidence

Local logs are under `target/macos-evidence/transcript-loss/`. The pause regression
failed before the fix. The Groq fixture passes afterward, alongside the full
Rust suite; frontend tests, typecheck, lint and build were also run. These tests
run on macOS and do not establish Windows hardware behavior or live Groq recall.
No credentials, user recordings or new dependencies were used.

Final local checks on branch `fix/preserve-dictation-text`, based on the source
revision above with changes only to `transcription/speech.rs`,
`transcription/groq.rs` and this report:

- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked`:
  229 passed, zero failed; the existing local-model evaluation remains ignored
  because it requires a running cleanup model and 80 requests.
- `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
  --all-targets -- -D warnings`: passed.
- `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check` and
  `git diff --check`: passed.
- `pnpm test`: 77 frontend tests and 9 prototype tests passed.
- `pnpm typecheck`, `pnpm lint`, `pnpm build`: passed. Frontend source was unchanged
  after these checks.

Rust commands used the repository's existing `target/toolchain/cargo` and
`target/toolchain/rustup` environment. Evidence logs are in the directory above.

For the affected Windows installation, record the version and a reproducible
example of the reported one-to-two-minute failure, including pause positions.
Use a disposable numbered sentence sequence for 15 seconds, 60 seconds, and a
short phrase/pause/second phrase. Compare expected text with inserted text.
With the candidate, compare provider/accepted segment counts. A complete
provider response with rejected segments implicates filtering; missing provider
text with intact audio implicates recognition; a short captured duration
implicates capture/shortcut lifecycle. Capture the failing boundary before
choosing a further fix. Audio retention remains opt-in.

This change is being submitted for review and merge. It has not been installed
on Windows or released as an app update, and is not presented as a confirmed
fix for the user's specific incident.

## Review record

An independent read-only reviewer found the control-token regression in the
experimental local Whisper change. That entire change was removed after the
user confirmed Groq. The reviewer found no must-fix in the retained Groq/acoustic
changes and noted the noise-classification limit above. The lead added the Groq
integration fixture afterward and checked the final reduced diff; that last
check is self-review, not a second independent review.

Exact review brief:

> Read-only review of uncommitted diff in /Users/aarav/Desktop/quill, branch fix/preserve-dictation-text. Use AStack ponytail-review skill at /Users/aarav/.local/share/astack/skills/ponytail-review/SKILL.md. AStack pinned f378432; do not refresh, edit, run tests or spawn agents. User reports Windows dictation omits passages; provider unknown. Acceptance: preserve full local Whisper transcript when optional word metadata omits words; preserve measured timestamps when complete; genuine sustained speech separated by long pauses must not be rejected solely by segment-wide activity ratio; silence/click regressions remain; logs contain counts only. Inspect full diff and connected insertion, dictionary, provider callers. Identify concrete bugs or unsafe generalizations, distinguishing controlled reproductions from the unconfirmed user's hardware cause. Report findings and limits. No publishing.
