# Quill on macOS — tester brief

This is Quill's first end-to-end macOS build. It compiles a universal Intel +
Apple Silicon whisper.cpp sidecar with Metal support, but it has not been run
on physical Mac hardware yet. Treat every result—especially a clear failure
message—as useful test data.

**Everything is local.** Quill has no accounts, no telemetry, and no servers.
Audio and transcripts stay on your machine.

## 1. Install

1. Download the `.dmg` from the repo's Actions run (or Releases) and drag Quill
   to Applications.
2. **The build is unsigned, so macOS will block it.** Right-click the app →
   **Open** → **Open** again in the dialog. Do not double-click — that gives a
   dead end with no "open anyway" option.

   If macOS still refuses:

   ```bash
   xattr -dr com.apple.quarantine /Applications/Quill.app
   ```

3. Grant two permissions when prompted, or in **System Settings → Privacy &
   Security**:
   - **Microphone** — to hear you
   - **Accessibility** — to type into other apps. Without this, Quill records
     but nothing appears.

   If you reinstall a new build, **remove Quill from Accessibility and add it
   back**. macOS keys the permission to the app's identity and unsigned rebuilds
   change it. Stale permission looks exactly like a broken app.

   If you deny Microphone, Quill should explicitly direct you back to **Privacy
   & Security → Microphone**. If Accessibility is absent, it must refuse to
   paste and name **Privacy & Security → Accessibility** instead of silently
   typing into the wrong app.

## 2. Set up

1. Keep Quill open while the first-run setup downloads `medium.en` (about
   1.5 GB). Confirm progress appears in both the setup sheet and the sidebar,
   then disappears when the model is ready.
2. Optional, for the Scribe feature: install [Ollama](https://ollama.com/), then
   open **Voice → Cleanup model**. Quill must not select a model automatically.
   Choose TurboSpeak 1.7B (8 GB system RAM minimum) or Qwen 2.5 7B (16 GB
   system RAM minimum). Skip this if you only want to test Dictation.

## 3. What to test

Quill has two hotkeys. Both are configurable in Settings; note which you used.

### Dictation — types exactly what you say

Open TextEdit. Hold the Dictation hotkey and say:

> "the quick brown fox jumps over the lazy dog"

Expected: words appear progressively as you speak, then settle.

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
recently fixed and has never run on macOS.

### Target apps

Try Dictation in at least three: TextEdit, a browser text box, Slack or Discord,
and a terminal. Report any app where text goes missing or arrives garbled.

## 4. What to report

For each problem:

- What you did, and which mode
- What you expected
- What happened
- Which app you were typing into
- macOS version and Mac model (Apple Silicon or Intel)

Logs are under `~/Library/Application Support/quill/logs/` — attach
the newest `quill*.log` file. **It contains no transcript text by design**, so
it's safe to share. If you find any spoken words in it, that is itself a bug
worth reporting.

Please copy any visible error exactly. The useful macOS-specific failures name
the missing sidecar path, microphone settings route, Accessibility settings
route, target-app activation failure, or Metal verification failure.

Crashes, hangs, and "nothing happens at all" are the most useful reports. Don't
polish them — a one-line "hotkey does nothing in Safari" is genuinely helpful.
