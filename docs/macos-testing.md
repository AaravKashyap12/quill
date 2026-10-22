# Quill on macOS — tester brief

macOS is an unsupported engineering preview. Quill packages a universal Intel +
Apple Silicon whisper.cpp sidecar with Metal support. Repairs have passed
physical cloud Dictation, Scribe, Fn shortcuts and full-screen overlay checks on
an Apple Silicon Mac. The broader matrix remains under verification.
This brief describes expected behavior; the [verification record](macos-verification.md)
distinguishes observed passes from untested paths and published artifacts.

Quill has no telemetry or Quill account requirement. Local speech and local
cleanup keep processing on the machine. Explicitly selected Groq or Gemini
speech uploads the completed recording after release; Gemini cleanup receives
transcript text and any enabled editor context. Provider keys use macOS Keychain.

## 1. Install

For a source build, first build the Whisper sidecar, then use
`pnpm macos:build:local`. This verifies a complete local ad-hoc app signature;
the debug executable's linker signature alone is not a valid app-bundle
signature. Install the resulting `Quill.app` in Applications **after the final
build**, before approving permissions. An ad-hoc signature is tied to that
build, so changing the binary can invalidate saved OS approvals. It is not
Developer ID signing or notarization. Keep one installed copy while testing.

1. Download a preview artifact from the repository's Actions run or Releases,
   or build from source, and move Quill to Applications.
2. Downloaded builds are not Developer ID signed or notarized. If macOS blocks
   opening a trusted Quill artifact, use **System Settings → Privacy & Security
   → Open Anyway**, then confirm Open. Follow [Apple's per-app instructions](https://support.apple.com/en-us/102445);
   do not disable Gatekeeper or strip quarantine as a setup step.
3. Quill needs **Microphone** to record and **Accessibility** to insert into the
   original text field. In the repaired source, General and onboarding offer
   **Enable text insertion**; startup only checks access. After enabling Quill
   in Privacy & Security → Accessibility (**Device Control and Data Access** on
   macOS 27), return to Quill or click **Check access**. Microphone consent is
   requested by macOS on first recording. A missing permission produces an
   actionable error rather than a successful insertion result.

The current Fn path does not explicitly request Input Monitoring. Do not add
Screen Recording, Full Disk Access, or Automation permissions for core dictation.
A clean test account across supported macOS versions is still needed to establish
the minimum CoreGraphics input grants independently of this tester's prior approvals.

Keychain prompts are separate from microphone/Accessibility consent. Only the
selected cloud speech provider is accessed at startup. Gemini cleanup waits until
Scribe or an explicit **Check saved key**; local speech/local cleanup opens neither
cloud item. Passive status checks never open Keychain. Reads and denied results
are cached for the process; **Retry access** is deliberate. These bounds concern
Quill store operations, not a guaranteed number of OS dialogs.

A changed ad-hoc executable can lose its OS identity, even at the same path:
[Apple documents this limitation](https://developer.apple.com/forums/thread/751658).
If this actually happens, refresh only Quill's permission entry for the installed
copy and restart the **same unchanged build**. Do not reset permissions on every
launch. No paid developer membership is required for this preview workflow;
unsigned-distribution warnings and possible renewed consent after rebuilds remain.

## 2. Set up

1. Choose local speech and explicitly start a compatible model download, or
   choose Groq/Gemini and connect your own key. Local speech must show download
   progress; cloud speech must not require a local model. Record the chosen
   path and whether onboarding finishes. Never include the key in a report.
2. Optional, for the Scribe feature: install [Ollama](https://ollama.com/), then
   open **Voice → Cleanup model**. Quill must not select a model automatically.
   Choose TurboSpeak 1.7B (8 GB system RAM minimum) or Qwen 2.5 7B (16 GB
   system RAM minimum). Skip this if you only want to test Dictation.

## 3. What to test

Quill has two hotkeys. Both are configurable in Settings; note which you used.

### Dictation — types exactly what you say

The repaired source build uses **Fn / 🌐** for Dictation and **Control ⌃ + Fn**
for Scribe. Hold the complete shortcut for at least 250 ms. Press Control before
or together with Fn for Scribe; adding it after Dictation activates cancels that
recording before starting Scribe. Other sampled keys suppress or cancel a hold.
General settings offers **Control + Option** and **Command + Control + Option**
presets for keyboards without a physical Apple Fn key, as well as custom chords.
Control + Option can conflict with VoiceOver; choose a custom binding if needed.
If Fn opens emoji or macOS Dictation, set System Settings → Keyboard →
"Press 🌐 key to" to "Do Nothing" if you want to reserve it for Quill.
Quill does not change this system preference. These source defaults do not
change older downloaded release artifacts.

The defaults follow [Wispr Flow's documented Mac shortcuts](https://docs.wisprflow.ai/articles/2612050838-supported-unsupported-keyboard-hotkey-shortcuts).
[Willow also defaults to Fn](https://help.willowvoice.com/en/articles/10876257-hotkey-settings).
No shortcut is conflict-free for every user's keyboard and applications.

Open TextEdit and focus its text field. Hold the Dictation key, wait for the
listening indication (first microphone initialization can take a few seconds),
then say:

> "the quick brown fox jumps over the lazy dog"

Expected: a compact recording pill appears while you speak. After releasing or
unlocking the hotkey, Quill transcribes and inserts the completed text once.
No text should appear in the editor while the hotkey is held. The processing
pill must clear on completion, and no opaque white window should remain.

Report: Did anything appear? Was it correct? Did words duplicate, get dropped,
or arrive out of order?

### Scribe — cleans up what you say, then asks you to approve it

Hold the Scribe hotkey and say:

> "hey so um i wanted to ask if we could move the meeting to tuesday no wait
> thursday would be better"

Expected: a review window opens with your raw words and a cleaned version. Text
only reaches the editor after you click Accept.

Report: Did the review window appear? Did the cleaned version say **Thursday**?
**It must never invent details you did not say** — flag anything added.

### Dictionary

Settings → **Dictionary** → add: when I say `my email` → type your email
address. Then dictate "send it to my email".

Expected: your actual address is typed, not the words "my email".

Also test hesitating mid-phrase — "send it to my… email" — that path was
covered by code changes but still needs macOS runtime verification.

### Target apps

Try Dictation in at least three: TextEdit, a browser text box, Slack or Discord,
and a terminal. Report any app where text goes missing or arrives garbled.

## 4. What to report

For each problem:

- Build version/commit, install path, and whether the build was signed
- Local or cloud speech provider, cleanup provider, and permission state
- What you did, and which mode
- What you expected
- What happened
- Which app you were typing into
- macOS version and Mac model (Apple Silicon or Intel)

Logs are under `~/Library/Application Support/quill/logs/`. They are designed
to omit transcript text, but inspect and redact personal paths or unexpected
content before sharing the relevant error lines. Do not share keys, recordings,
settings files, or recovery checkpoints. Spoken words in logs are a bug.

Please copy any visible error exactly. The useful macOS-specific failures name
the missing sidecar path, microphone settings route, Accessibility settings
route, target-app activation failure, or Metal verification failure.

Crashes, hangs, and "nothing happens at all" are the most useful reports. Don't
polish them — a one-line "hotkey does nothing in Safari" is genuinely helpful.
