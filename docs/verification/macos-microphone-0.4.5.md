# macOS microphone and permission recovery, 2026-11-08

## Findings

The reproduced zero-audio failure is an OS authorization failure. TCC logs for
the official 0.4.5 executable report a mismatch with the stored microphone code
requirement, followed by denial of a new prompt because the hardened app lacks
`com.apple.security.device.audio-input`. The official 0.4.4 comparison reproduces
the same missing-entitlement denial on this Mac. This establishes a packaging
defect in both builds, not a regression caused by 0.4.5's downsampling filter.

The 0.4.5 baseline includes a 6.3-second empty recording. Fn remained active
until release, so early release does not explain that attempt. The version diff
does not change microphone startup or Fn handling; the new filter operates on
completed snapshots, after the sound meter reads input samples.

Both official builds use ad-hoc signatures. Their designated requirements bind
to executable hashes, so approval for an older executable may not authorize an
update. The Settings toggle can remain enabled while the new process is denied.
Apple describes this version-specific identity in
[TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements).
Consistent certificate-backed signing is the path to identity continuity across
updates; this change does not promise persistent approval for ad-hoc releases.
Paid signing remains outside this task.

## Change

- Add the microphone entitlement to macOS packaging and local bundle signing.
  Retain hardened runtime and the existing audio and shortcut implementation.
- Verify the complete signature, bundle identifier, microphone usage description,
  runtime flags and entitlement on every executable architecture. Run signed
  negative fixtures in macOS CI and verify release bundles before publication.
- Give denied `Check access` results visible recovery instructions. Provide a
  separate `Open System Settings` action without repeating the consent prompt.
  The native action accepts no URL or command input, opens a fixed Accessibility
  destination, and rejects calls outside the main settings window.

## Hardware evidence

Source baseline: `fd583aaeb05aae7ac18528ba10c0137e207e39c2`.
AStack: `f3784322978080349f67527bd60af803e96804eb`.

The controlled candidate is the official 0.4.5 app with its bundle re-signed to
include the entitlement. Both architecture `__TEXT,__text` sections compare
equal before/after, as do Info.plist, the whisper sidecar and icon. The installed
executable at `/Applications/Quill.app/Contents/MacOS/quill` has SHA-256
`d9b954bd743ad5ee72d56356ebdf9735e8579f0671a5708cd0167ccca3a51370`.
Strict signature, hardened-runtime and entitlement verification pass for arm64
and x86_64. Hardware execution was on Apple Silicon only.

The initial candidate process (PID 8896) could not start a session because its
Accessibility permission was denied. System Settings initially showed the
renamed backup, then showed Quill enabled while the app still reported denied.
The user removed/re-added Quill and restarted the unchanged candidate (PID 12436).
At 13:15 UTC, recording and insertion succeeded repeatedly. The user confirmed
that the follow-up message itself was dictated through Quill.

The first cold attempt released Fn before the 2,015 ms microphone startup
completed and captured only 13 ms. Subsequent opens took 46–47 ms. Three sessions
inserted 4, 25 and 50 words, with release-to-insertion times of 546, 900 and
1,484 ms. The latter sessions lasted roughly 10 and 22 seconds including
processing; no premature Fn stop was observed. These are individual trials,
not a performance guarantee.

Settings, models and credentials were preserved. No blanket permission reset
or security-protection change was used. The verified installed candidate remains
unchanged; the new recovery UI is source-only and has not been installed over it.

## Checks and limits

Evidence is retained locally under `target/macos-evidence/microphone-0.4.5/`.
Raw TCC logs and app bundles are not committed. `hardware-metrics.log` contains
timings and word counts only.

- Signing regression: configuration fails before the entitlement is added;
  the original release is rejected by the new verifier. Six tests pass after
  the fix, including real universal signed fixtures rejecting absent/false
  entitlements, disabled hardened runtime and tampering.
- Recovery regressions fail before the UI change and pass afterward: denied
  readback has actionable feedback, settings can reopen without another consent
  request, errors are visible, and granted access dismisses the notice.
- Frontend: 9 Python tests and 77 Vitest tests pass; typecheck, lint and build pass.
- Rust: 227 tests pass, 1 existing ignored; format and Clippy checks pass.
- Independent read-only review covers signing and recovery changes. Review briefs
  and results are retained with the local evidence.

The new Settings-opening command is compiled and its frontend interactions are
tested with mocks. The updated native UI and exact Settings destination still
need a run from a newly built app; native computer control failed during this
session. Replacing the installed app would change its newly approved ad-hoc
identity, so the working hardware candidate was preserved. Windows/Intel
execution, a packaged release built in CI, and update-to-update permission
retention are not established by this hardware test. No release was published.
