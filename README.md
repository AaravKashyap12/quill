# Quill

**Two hotkeys. Two kinds of voice input. Everything stays local.**

Quill is a free, open-source voice dictation app for Windows. macOS platform
hooks and build scaffolding are included, but speech recognition is still wired
to the packaged Windows runtime, so macOS is not yet functional end to end; see
the [macOS tester brief](docs/macos-testing.md). Quill keeps two independently
configurable global shortcuts active at the same time:

| Hotkey | Behavior |
| --- | --- |
| **Dictation** | Types the stabilized transcript word by word, exactly as spoken. No cleanup. |
| **Scribe** | Holds a short trailing window, resolves spoken self-corrections, removes filler, applies punctuation, and types only the final wording. |

The hotkey is the mode switch. There is no settings menu to open while you are
working.

> Quill is an independent project. It is not affiliated with Wispr Flow,
> Willow Voice, WhimprFlow, Svara, or any other commercial dictation product.

## Project status

Quill is currently an **engineering preview**:

- The trailing-buffer and LocalAgreement contracts are implemented and covered
  by standalone tests.
- The Tauri v2 shell, tray/updater/autostart configuration, settings UI,
  floating overlay, local-provider detection, hotkey polling, text injection,
  recovery checkpoints, packaged CUDA whisper.cpp server, microphone capture,
  and Windows Dictation/Scribe session loop are in source.
- Dictionary entries support spoken-word biasing and literal word/snippet
  replacement. Accepted Scribe edits can produce an optional dictionary
  suggestion; Add and Dismiss are implemented, and dismissed pairs are capped
  and can be cleared from Dictionary settings.
- The desktop React bundle builds successfully.
- A native Windows production binary has been compiled and manually exercised
  with the packaged CUDA runtime. The complete Notepad, VS Code, and Discord
  application matrix and macOS verification are still pending.

This status is intentionally explicit: do not treat `v0.1.0` source as a
finished signed release yet.

## How the two modes differ

Say:

> Write down one two three five — no wait, four and five.

Dictation types:

> Write down one two three five — no wait, four and five.

Scribe types:

> Write down one two three four and five.

Dictation uses rolling re-transcription and LocalAgreement: a word is committed
only when two consecutive whisper.cpp passes agree on the same prefix. Scribe
holds the complete utterance until the user releases or unlocks its shortcut,
then resolves corrections once before any text reaches the cursor.

Scribe detects whether the captured target is email, chat, an AI prompt, notes,
or general text and sends the raw transcript to your local LLM with the matching
"polish, don't invent" instructions. Email may gain a greeting and sign-off;
other registers do not. Every register forbids new facts, commitments, offers,
or constraints, and the detected writing style can be changed in the review
window to regenerate the draft.

Cleanup output is never injected silently. Every Scribe activation opens a
review window with the raw transcript and the cleaned draft side by side;
text reaches the cursor only after you explicitly accept it (or edit it and
accept). Discard is always one click away. This human-in-the-loop step is
what closes the gap left by removing the earlier strict word-provenance
gate, which had blocked legitimate rewrites like `hei` → `Hey`. A
lightweight sanity guard also swaps the LLM output for a safe local draft when
the prompt would exceed its reserved context budget, the provider returns a
missing or malformed text field, generation stops at the output-token limit,
the model returns nothing or balloons past 3× the input word count, or the draft
introduces new promise, availability, proposal, or follow-up language that was
absent from the transcript.

## Requirements

### At runtime

- Windows 10/11 x64
- macOS 12+ is the intended target. Hotkeys, insertion, and build scaffolding
  are implemented, but the ASR runtime packaging is incomplete and the app has
  never been verified on Mac hardware. Do not treat macOS as supported yet; see
  [docs/macos-testing.md](docs/macos-testing.md).
- A microphone
- About 75 MB–1.6 GB for a whisper.cpp model
- Optional Scribe provider:
  - [Ollama](https://ollama.com/)
  - LM Studio, Jan, or llama.cpp server exposing an OpenAI-compatible endpoint

A 7B+ instruct model is the recommended minimum for register-aware Compose and
reliable self-correction resolution. Smaller models remain useful for basic
Polish, such as punctuation and filler removal. Dictation never uses the cleanup
model.

### Speech model requirements

These are conservative minimums shown inside Quill's Voice settings. Actual
whisper.cpp usage varies with backend and quantization; CPU mode requires no
dedicated VRAM.

| Model | Download | Minimum GPU memory | CPU-only memory | Best fit |
| --- | ---: | ---: | ---: | --- |
| `tiny.en` | 75 MB | 1 GB VRAM | 2 GB free RAM | Fastest; basic notes |
| `base.en` | 142 MB | 1 GB VRAM | 2 GB free RAM | Balanced baseline |
| `small.en` | 466 MB | 2 GB VRAM | 4 GB free RAM | Recommended for 4 GB GPUs |
| `medium.en` | 1.5 GB | 5 GB VRAM | 8 GB free RAM | Higher accuracy; slower |
| `distil-large-v3` | 1.5 GB | 5 GB VRAM | 8 GB free RAM | Fast, high-accuracy English long-form transcription |
| `large-v3-turbo` | 1.6 GB | 6 GB VRAM | 10 GB free RAM | Best quality/speed tradeoff |

Quill disables a model in the selector when its verified model file is not
installed, preventing an unavailable choice from breaking speech-engine startup.
English shows English-only models; Auto-detect and other languages show only
compatible multilingual models.

### To build

- Node.js 20+
- pnpm 10+
- Rust stable (`rustup`, `cargo`, and platform Tauri prerequisites)
- CMake 3.20+
- Git
- SDL2 development files for `whisper-stream`
- Windows: Visual Studio Build Tools 2022 and WebView2
- macOS: Xcode Command Line Tools

See the current [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
for operating-system packages.

## Repository layout

```text
apps/
  desktop/              React + Vite UI and Tauri v2 Rust core
    src-tauri/
      src/asr.rs         whisper.cpp sidecar boundary
      src/streaming.rs   LocalAgreement word commit
      src/cleanup.rs     local LLM discovery, prompt, sanity guard
      src/hotkeys/       poll-only Windows/macOS hotkey state
      src/injection/     SendInput / CoreGraphics text insertion
prototypes/
  scribe_buffer.py      dependency-free trailing-buffer prototype
  test_scribe_buffer.py canned correction and safety tests
scripts/
  build-whisper.ps1     Windows whisper.cpp/CUDA build
  build-whisper.sh      macOS whisper.cpp/Metal build
```

## Architecture

| Layer | Choice | Why |
| --- | --- | --- |
| Desktop shell | Tauri v2 + React + TypeScript | One UI and command surface; Windows is verified and macOS remains incomplete |
| Recognition | whisper.cpp | CPU fallback and CUDA on Windows; macOS Metal packaging is not yet wired into the app runtime |
| Live commit | Rolling re-transcription + LocalAgreement | Low perceived latency without flickering unstable words |
| Scribe cleanup | Ollama or OpenAI-compatible localhost server | Model choice stays with the user and speech stays local |
| Windows hotkeys | `GetAsyncKeyState` polling | No system-wide keyboard hook |
| Windows insertion | `SendInput` or clipboard paste | Unicode support and reliable long-text insertion |
| macOS hotkeys | `CGEventSourceKeyState` polling | Global state without an event tap keyboard hook |
| macOS insertion | CoreGraphics events + clipboard | Works through the standard Accessibility permission path |
| Updates | Tauri updater + GitHub Releases | User-confirmed updates from signed release artifacts |

## Development

Install JavaScript dependencies:

```powershell
pnpm install
```

Run the standalone cleanup contract:

```powershell
python prototypes/test_scribe_buffer.py
python prototypes/scribe_buffer.py
```

Run the desktop UI in a browser:

```powershell
pnpm dev:desktop
```

Build the desktop frontend:

```powershell
pnpm build
```

## Building whisper.cpp

The helper scripts clone the upstream project into ignored `third_party/` and
copy the resulting `whisper-stream` binary into Tauri's sidecar directory.

Windows (packaged CUDA runtime with an in-app CPU fallback):

```powershell
./scripts/build-whisper.ps1
```

macOS with Metal:

```bash
./scripts/build-whisper.sh metal
```

Download a model using whisper.cpp's model helper, then place it under the
platform application data directory's `models/` folder. Model downloading and
integrity verification are tracked in the roadmap rather than silently bundled
into the app.

## Native desktop build

After installing the Tauri prerequisites and building the whisper sidecar:

```powershell
pnpm tauri:build
```

Tauri's Windows NSIS configuration uses `currentUser`, so the installer does
not require administrator rights. A macOS `.app`/`.dmg` target is configured,
but its ASR resource layout is incomplete and it has not been verified on Mac
hardware.

## macOS permissions

Quill needs:

1. **Microphone** access to capture speech.
2. **Accessibility** access to paste/type into the active application.

The signed application must include matching usage-description strings and
entitlements. Unsigned local builds may need permissions removed and re-added
after each bundle identity change.

## Signing, notarization, and updater keys

Before creating a public release:

- Generate a Tauri updater signing key and replace
  `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY` in `tauri.conf.json`.
- Add the private updater key/password as GitHub Actions secrets.
- Add the Windows code-signing certificate configuration.
- Add Apple Developer ID, Team ID, certificate, and notarization credentials.

The release workflow is intentionally manual until those secrets are present.
Updates are offered to the user; Quill never installs one silently during a
recording session.

## Privacy

Quill contains no telemetry or analytics client. Local usage statistics, when
implemented, will be stored only on the device. Network requests are limited to:

- localhost cleanup-provider discovery and inference;
- user-triggered model downloads;
- GitHub Releases update checks.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Security
issues should follow [SECURITY.md](SECURITY.md).

## License

[GNU AGPL-3.0-or-later](LICENSE)
