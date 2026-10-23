# Tap activation for Mac shortcuts

The 2026-10-23 repair enables Tap independently for Dictation and Scribe,
including Fn, Control+Fn, and modifier-only fallback presets. Hold remains the
default. Changing a preset or recording a right-side modifier preserves the
selected activation behavior.

A modifier Tap commits when its trigger is released. The complete sampled chord
must be exclusive; there is no 250 ms hold threshold for Tap. Control+Fn selects
Scribe in either press/release order. Typing while a Tap recording is locked does
not stop it. A contaminated shortcut attempt does not start or stop recording.
The final release sample checks conflicts even after Fn is no longer down.

When either mode uses Tap, the sampler retains ordered state changes while the
session loop awaits microphone or transcription work. Separate mode counters
could select Dictation after a later Scribe tap. The new buffer holds at most
1,024 changes; overflow discards pending events and cancels both modes instead
of treating lost input as a request to finish/upload. It requires physical
trigger release before rearming. Hold-only configurations keep their existing
bounded sampled-state path. No Windows producer or permission flow changed.

Changing a locked Tap binding cancels its session before normal finalization.
The production cancellation helper removes the active session and drops its
activity; it does not call transcription or finish. This shares the existing
cancellation path used by contaminated Hold shortcuts.

## Review and automated evidence

AStack revision `f88d9200f7ee08c9195b0063303442757f1a6f96` guided the Tier 2 review.
The exact brief is [mac-tap-review-brief.txt](mac-tap-review-brief.txt).
The reviewer identified release-frame conflict sampling, cross-mode ordering,
and missing consumer cancellation coverage. All three were addressed in source
and tests; the lead reviewed the corrections without another review round.

The release-frame regression failed before the final scan was added. Real
sampling and KeyRuntime tests cover both idle and locked consumers, with Ctrl
flags or a nonmodifier key first present on the release sample. The ordering
regression reproduced Fn then Control+Fn selecting the wrong mode. Its corrected
version compares immediate and delayed production arbitration for every binary
mode sequence of lengths one through six and all four initial lock states.
Configuration-change tests exercise the production cancellation boundary with
locked consumers for both modes and a real session-activity guard. Disabling
session removal makes that test reach the simulated finish path and fail.
These fixtures perform no microphone capture or provider calls.

Ignored evidence is in `target/macos-evidence/mac-tap/`, including red/green
regressions, the independent report, and check logs. Physical Fn timing, native
microphone lifecycle, actual non-upload behavior and saved-setting round trips
must be verified separately. Polling at 8 ms cannot guarantee detection of a
chord wholly between samples or media buttons outside keycodes 0–127.

## Native verification

The previously installed Hold build remains unchanged while this update is
checked. Tap has not yet been installed or physically verified in this record.
