# Mac editor preparation and retained destinations

## Report and evidence

On 2026-10-28, the user reported repeated “Editor is ready” errors after switching
tabs/apps, plus failure to deliver to a previously focused field while browsing
another page. The installed app's content-free log at 14:10:55 UTC records
accessibility initialization followed by that exact startup rejection. Its
14:10:27 permission check was already true; renewed permission was not the cause.

Source inspection found two independent restrictions: cold discovery deliberately
required another shortcut, and ordinary Mac Dictation only pasted while the exact
captured field was foreground. There was no destination memory before recording.

## Intended contract

- Prepare the foreground app's accessibility ahead of recording. If it is still
  cold, finish bounded preparation within the activation before opening audio.
  A click/key press or app/window change during preparation rejects that attempt.
- Remember the most recently observed editable field while browsing non-editable
  content. A newly focused editable field becomes the next destination. Password
  fields clear destination memory; unreadable focus cannot select a fallback.
- Snapshot the destination at recording start. Subsequent navigation cannot
  replace it. Restore and verify the original window and field before pasting.
  Closed, protected or unrestorable targets retain the transcript for recovery.
- The monitor keeps only retained native identities and roles in process memory.
  It does not read values, selected text, clipboard contents, or typed characters.
  Optional Scribe context remains governed by its existing setting.

The monitor samples every 100 ms, outside the session/provider awaits. Very brief
focus changes between samples may not be observed. Browser tabs may discard their
accessibility objects while hidden or after navigation; exact restoration must
succeed, otherwise Quill keeps the transcript rather than selecting another field.
Atomic focus-to-paste delivery is not guaranteed by the macOS APIs.

## Comparable and platform research

FluidVoice was read at `6150956bef129fc1a413684db4c0eac61da078a5` (GPL-3.0).
Its immutable recording-target snapshot and explicit current/original destination
policy informed the comparison. No FluidVoice implementation code was copied.
Its source does not prove native behavior on this Mac or support for all hidden
browser tabs. Quill keeps its existing exact-window/field verification.

- [FluidVoice target policy](https://github.com/altic-dev/FluidVoice/blob/6150956bef129fc1a413684db4c0eac61da078a5/Sources/Fluid/Services/DictationTargetPolicy.swift)
- [FluidVoice target capture and restoration](https://github.com/altic-dev/FluidVoice/blob/6150956bef129fc1a413684db4c0eac61da078a5/Sources/Fluid/Services/TypingService.swift)
- [Chromium accessibility initialization](https://www.chromium.org/developers/design-documents/accessibility/)

## Review and local checks

AStack `20ee738d8e08cd1b2d70831230aecc4b1695b26c` guided this Tier 2 change.
Graphify 0.9.96 refreshed a code-only graph; source searches confirmed callers.
The exact [independent review brief](editor-targets-review-brief.txt) is retained.

The cold-editor regression failed with the old second-press error, then passed.
The destination-memory regression failed without browsing fallback, then passed.
Targeted tests also exercise superseding in-flight observations and blocking worker
IPC during an active capture, using the production sampling boundary.
Ignored logs are under `target/macos-evidence/editor-tracking/`.

Native acceptance remains incomplete: the first installed candidate exposed an
application-activation failure during cross-app delivery, described below. Unit
tests do not establish native cross-app/tab delivery or password rejection.

The fresh-context reviewer reported five must-fix findings and one coverage gap.
The lead addressed each within the original review round:

| Finding | Correction and evidence |
| --- | --- |
| Ready and noneditable results bypassed capture guards | All discovery results check app, window and input counters before interpreting the result. First-read input changes and noneditable results have regressions. |
| Unreadable password subrole treated as empty | The shared capture/insertion decoder preserves AX errors. Explicitly unsupported optional subroles are allowed; timeouts and malformed metadata fail closed. A mutation restoring permissive decoding fails the regression. |
| Background self-app exclusion raced with capture | Exclusion now checks the actual captured application identity inside the worker capture path. The preliminary independent app lookup was removed; explicit onboarding capture remains allowed. |
| Focus could change during clipboard writes | Verify the original target again after preparing the clipboard. Injected focus change produces Queued without posting paste. Disabling this check fails the regression. |
| A cold window could require another shortcut | Missing-window readiness joins bounded discovery; input changes while waiting reject the attempt. |
| Worker shutdown lacked boundary coverage | Parked and in-flight sample tests exercise stop, unpark and join through the production worker constructor. |

No second independent review was run. Lead correction checks are distinct from
the original static review. Mutation tests with input/paste guards disabled fail
four regressions; the corrected tree passes them. The native focus-to-paste gap
cannot be made atomic with the current global event API and remains a limitation.

Each implementation commit was tested with its tracked working tree matching the
index. Untracked evidence and documentation were not build inputs:

| Commit | Tested tree | Local checks |
| --- | --- | --- |
| `b4c6403` | `90b2298714a550ab32dd07afc1ab41ba43302150` | 206 Rust tests passed, one existing ignored; format and all-target Clippy passed |
| `83765f7` | `ec959035eaa8d06c3fe9af8668a31b9a56be0e1a` | 211 Rust tests passed, one existing ignored; format and all-target Clippy passed |
| `bde0be7` | `117449ca245801293815a538e615ff907694438f` | 218 Rust tests passed, one existing ignored; format and all-target Clippy passed |

The unchanged frontend passed 75 Vitest tests, nine Python tests, typecheck,
lint and build. The final candidate built with `scripts/build-macos-local.sh`
and passed strict bundle-signature verification. No new dependencies, provider
calls, permission resets or credential-store reads were needed for these checks.
The first installed candidate was executable SHA-256
`468ecd35af874e08f8914f739c92b1fb522d2fa07ef6d39cdfeb8b823f252379`,
built from implementation commit `bde0be7`. After user approval, an unchanged
restart at 14:58:48 UTC confirmed Accessibility and input access.

## Cross-app activation correction

The user reported successful insertion when returning to the editor, but queued
text when releasing Fn while Chrome remained foreground. The 15:00:21 UTC log
records “macOS refused to restore the original application.” At 15:00:24, the
queued text inserted after the original editor regained focus. Destination
retention succeeded; app activation failed before window/control restoration.

The normal AppKit activation request now falls back to setting the retained
destination application's public `AXFrontmost` attribute when AppKit refuses it.
This uses the existing Accessibility grant. AX failure retains the result;
setter success still requires the existing exact application/window/control
readback and the final post-clipboard focus check. No deprecated activation flag,
permission reset, arbitrary destination, or unconditional paste was added.

- [Apple application accessibility attribute](https://developer.apple.com/documentation/applicationservices/kaxfrontmostattribute)
- [Apple app activation API](https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(options:))
- Installed SDK: `NSAccessibilityProtocols.h` exposes writable
  `accessibilityFrontmost`; `AXAttributeConstants.h` defines `AXFrontmost`.

Reinstating the old refusal branch makes the new restoration regression fail.
With the correction, 220 Rust tests pass (one existing ignored), including failed
and ineffective Accessibility activation retaining text without simulated paste.
Formatting, all-target Clippy, the app build and strict bundle-signature verification
pass. The activation-fix candidate was installed with SHA-256
`1174a6c07bec4a0cbd8da3c59e1945d025617ed5491a29b17676361fec6c77cd`.
Its unchanged restart at 15:12:44 UTC confirmed Accessibility and input access.

## Full-screen Spaces delivery follow-up

The user confirmed that the activation fallback returned to T3, but no text
appeared in its chat input. Both apps were in full-screen Spaces. The 15:23:31 UTC
log records successful AX activation, followed 23 ms later by the existing
“text injected” log. That log records posting a paste command, not acknowledgement
from the destination editor; it does not override the user's observed failure.

The remaining hypothesis is that paste preceded completion of the Space/focus
transition. Source inspection found that readiness read the application's own
remembered editor and accepted its first matching response. The correction reads
the system-wide focused application and editor instead, checks the application
PID and exact window/control, and requires 500 ms of stable matching focus after
restoration. A mismatch restarts that interval; a 1.5-second bound retains text
rather than allowing an unverified paste. Already-focused dictation has no added
settling delay. The final post-clipboard target check remains in place.

The settling interval is a bounded mitigation, not a macOS guarantee that every
Space animation has ended. Native full-screen delivery must still prove it on
this machine. No post-command retry was added, which could duplicate a paste
whose result is unknown. The monitor still reads identities/roles only.

An injected transient-focus regression fails when immediate success is restored,
then passes with the settling gate. All 221 Rust tests pass (one existing ignored),
including unchanged fast-path and failed-restoration coverage. Formatting, Clippy,
the complete local app build and strict bundle-signature verification pass.
The installed candidate (commit `5151ff5`, tested tree
`a6d52e726cfe6a40b05dee329304e3f6b561fe34`) has executable SHA-256
`04f86fcbecee395295ddb09a0e71a37e690df718c5541db5c7104591b37f054f`.
After renewed approval, its unchanged restart at 18:07:30 UTC confirmed both
permissions. The user then confirmed that Fn Dictation, released while Chrome
remained in a different full-screen Space, returned to T3 and inserted directly.
The 18:08:04 log records successful activation and 512 ms of stable focus before
posting Paste. The user separately confirmed Ctrl+Fn Scribe, review acceptance,
and insertion into the original T3 field with the same cross-app sequence.

This establishes the reported Chrome-to-T3 workflow on this Apple Silicon Mac.
It does not establish every hidden browser tab, Intel hardware, or a cold-restart
matrix. Existing password rejection remains covered by offline regressions; a new
physical password-field test was not performed in this follow-up. The user’s
installed build remains unchanged; packaging work uses a separate disposable app
folder.
