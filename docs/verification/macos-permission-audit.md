# macOS permission and background-running audit

Base: Quill 0.4.7 (`c8c17871cb09f70f5b7a873ab62bd2e025a5860d`).
AStack: `174b42b8b43c17a807b57870fa6f8f8866dff136`.
Scope: startup, cloud credential access, native permission requests, signing,
Accessibility setup, window close, application menu and tray. This is not a
whole-repository security certification.

## Findings

1. **Confirmed: startup could open Keychain.** Both App initialization and cloud
   runtime constructors read the selected provider's key. The cache prevented
   repeated reads within a process but did not prevent a prompt on each launch.
   Status queries are now passive; constructors only prepare an HTTP client.
   Key access happens for a transcription request or an explicit setup action.
   Missing/denied keys still fail safely, with recovery audio handled by the
   existing session path. This is a separate improvement from the user's
   clarified Accessibility complaint.
2. **Confirmed: premature Accessibility request UI.** The setup card rendered
   while its initial permission result was still unknown. It now waits for the
   result. A failed query still exposes the error and recovery controls. Mount,
   focus and Check access remain passive; only Enable requests OS consent.
3. **Confirmed signing limitation; incident attribution unresolved.** The
   installed `/Applications/Quill.app` is 0.4.7 and passes strict code-signature
   verification. Its designated requirement consists of binary hashes. Release
   configuration uses ad-hoc signing. No valid signing identity was found on
   this Mac. Updates can change the identity used for permission grants.
   However, the user reports repeated Accessibility requests without updating;
   signing changes alone do not establish that cause. Logs contain allowed and
   denied launch states, without enough identity information to prove these
   were launches of an unchanged binary. Do not reset TCC, widen its database
   permissions or loosen code requirements to hide this issue.
4. **Confirmed: Cmd+Q used native Quit.** The Mac application menu now labels
   Cmd+Q “Keep Quill Running in Background” and hides the main window. The tray
   retains an explicit Quit Quill action that exits. Dock reopen shows the main
   window again. Standard editing menus and Windows behaviour are retained.

## Permission inventory

- Microphone: needed to record; capture starts when dictation starts.
- Accessibility: needed to insert into the target app; initial checks do not
  request consent. The explicit setup button requests it.
- Input Monitoring: the shortcut monitor checks status and polls key state;
  no explicit additional consent request was found in this path.
- Keychain: needed only for stored cloud API keys, not local dictation.
- The bundle declares microphone usage and the audio-input entitlement.
  No additional camera, contacts, screen recording, Full Disk Access or Apple
  Events permission request was found in the inspected startup paths.

Apple explains permission continuity through designated requirements in
[TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements).
Stable distribution signing is a separate release prerequisite for reliable
identity across updates; it is not proof of a fix for same-binary relaunches.

## Evidence and remaining checks

- Regression checks for passive cloud startup failed before the change; the
  Accessibility initial-render check also failed before its fix.
- Local checks: 236 Rust tests passed, two explicit model evaluations ignored in the default suite;
  83 frontend tests and 9 prototype tests passed. Typecheck, lint, frontend build,
  Rust formatting and Clippy with warnings denied passed.
- Logs: `target/macos-evidence/permission-audit/` (ignored).
- The first native inspection found PID 18953 with a blank main window. A later
  quit/reopen of that unchanged installed copy produced PID 19881, a rendered
  Ready state and Accessibility/Input Monitoring allowed, with no permission
  prompt observed. The reported same-copy failure was not reproduced.
- With the user's approval, the reviewed test bundle was installed at
  `/Applications/Quill.app`. Its executable SHA-256 is
  `3b98f392154b05b9c2171cbb045316ec2512950d08d27cb1cd1ab09e5b54f05e`.
  The previous app is preserved as `native/installed-before.zip` in the ignored
  evidence directory; it is not a second runnable app with the same identifier.
- First launch of the test build showed Ready without a Keychain prompt, but
  Accessibility was denied. System Settings showed the existing Quill switch
  enabled: the old ad-hoc identity's grant did not match this new binary.
  The user completed Touch ID to refresh the existing entry. Afterward the
  switch was on and Quill's permission card disappeared without another restart.
- Cmd+Q hid the test build's window while PID 41836 remained alive. Reopening
  `/Applications/Quill.app` restored its Ready window. Native menu-bar and Dock
  inspection timed out, so the physical Fn and explicit tray Quit checks were
  handed to the user. The user confirmed background dictation works, then
  reported completing tray Quit. A process check confirmed the installed app
  had exited. The executable hash remained unchanged. Reopening restored Ready
  without an Accessibility card or startup password prompt.
- The user also supplied a screenshot of a Keychain dialog naming
  `com.quill.desktop.cloud-providers`, establishing a second kind of reported
  prompt. Groq and Gemini are separate Keychain items under this service and
  can each require authorization. The process cache serializes and retains
  successful, missing and denied reads per provider; passive status checks
  do not read Keychain. Startup no longer requests either key.
  The user reports this dialog returning even after Always Allow. That report
  is accepted as an unresolved issue, rather than attributing it to the user
  choosing one-time Allow.
  [Apple's Keychain guidance](https://support.apple.com/en-ie/guide/keychain-access/kyca1243/mac)
  distinguishes one-time Allow from persistent Always Allow. A changed ad-hoc
  app identity remains a separate reason to require authorization again.
- After this unchanged-build restart, the explicit Groq Check saved key action
  completed as Connected and ready without a password prompt. Gemini Check
  saved key remained Checking; the user confirmed a Keychain dialog appeared.
  This establishes separate provider authorization states, not a reproducible
  loss of Groq trust on this restart. The native tool explicitly
  blocks SecurityAgent, so that dialog was left untouched; no password was
  requested through chat or extracted.
- After being asked to complete Gemini authorization and check it following a
  full quit/reopen, the user reported: "prompt doesnt return". Gemini persistence
  therefore passed by user report; the agent did not independently operate or
  observe that final authentication/restart. The installed build was left unchanged.
- Same-copy Accessibility persistence passed the observed restart, and both
  cloud providers' access persisted in the checks described above. The earlier
  repeated-prompt incident was not reproduced on this build and its historical
  cause remains unproven. The changes are not released or claimed to fix every
  permission recurrence. No saved keys, model selections or other app settings
  were changed for these checks.

## Independent review

Requested fresh-context review, exact brief:

> Review the uncommitted diff in /Users/aarav/Desktop/quill on fix/macos-permission-prompts read-only. Use /Users/aarav/.local/share/astack/skills/ponytail-review/SKILL.md, pinned AStack 174b42b; do not refresh, edit, run the app, access credentials, or delegate. Acceptance: app/window startup does not read cloud credentials; deliberate transcription and explicit key actions retain secure access and recovery; completed cloud setup is not reset; genuinely missing keys are not silently accepted; Accessibility request UI waits for status and consent remains explicit; on macOS Cmd+Q hides Quill while the dictation runtime remains active, tray Quit exits, Dock reopen works, standard editing menus and Windows behaviour remain intact. Inspect the diff and connected callers. Report concrete must-fix findings and verification gaps, distinguishing code proof from the unresolved reported same-binary Accessibility re-prompt. No publishing.

Review disposition: both findings fixed. Completed cloud setups now use the
shared `speechSetupFor` rule at startup and after settings edits/Discard.
Runtime cloud errors refresh passive cached credential status so a missing-key
reason persists after the runtime returns to Ready. Regression checks cover
both cases. The pinned menu dependency recognizes `Cmd` as the Meta modifier;
the native shortcut results and remaining gaps are recorded above.
