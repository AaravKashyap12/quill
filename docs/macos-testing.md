# Quill on macOS — tester brief

Quill has never been run on a Mac. Its hotkey and text-insertion hooks are in
source, but speech recognition still starts the Windows CUDA server and the
macOS release job does not yet provision the resource layout expected by the
app. There is no testable end-to-end macOS build yet. Keep this brief for the
first build after that packaging work is completed.

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

## 2. Set up

1. Open Quill → **Voice** → download a speech model. Start with `small.en`
   (466 MB).
2. Optional, for the Scribe feature: install [Ollama](https://ollama.com/) and
   run `ollama pull qwen2.5:7b`. Skip this if you only want to test Dictation.

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

Logs are at `~/Library/Application Support/com.quill.app/` — attach the log
file. **It contains no transcript text by design**, so it's safe to share. If
you find any spoken words in it, that is itself a bug worth reporting.

Crashes, hangs, and "nothing happens at all" are the most useful reports. Don't
polish them — a one-line "hotkey does nothing in Safari" is genuinely helpful.
