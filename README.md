# Quill

**Two hotkeys. Two kinds of voice input. Local by default, cloud only when you choose it.**

Quill is a free, open-source voice dictation app for Windows. macOS has native
platform hooks and a packaged whisper.cpp Metal sidecar. Unreleased repairs
have passed physical Dictation, Scribe, and full-screen overlay checks on Apple
Silicon; the broader application matrix remains under verification. Published
macOS builds remain unsupported; see the [verification record](docs/macos-verification.md)
and [tester brief](docs/macos-testing.md). Quill keeps two independently
configurable global shortcuts active at the same time:

| Hotkey | Behavior |
| --- | --- |
| **Dictation** | Inserts the completed transcript when you stop, exactly as spoken. No cleanup. |
| **Scribe** | Processes the completed utterance, resolves spoken self-corrections, removes filler, and opens a draft for review before insertion. |

The hotkey is the mode switch. There is no settings menu to open while you are
working.

> Quill is an independent project. It is not affiliated with Wispr Flow,
> Willow Voice, WhimprFlow, Svara, or any other commercial dictation product.

## Project status

Quill is currently an **engineering preview**:

- Final insertion is implemented in the desktop session loop. The historical
  trailing-buffer prototype has separate standalone tests.
- The Tauri v2 shell, tray/updater/autostart configuration, settings UI,
  floating overlay, local-provider detection, hotkey polling, text injection,
  recovery checkpoints, bundled CPU whisper.cpp server, optional CUDA runtime,
  microphone capture,
  and Windows Dictation/Scribe session loop are in source.
- Dictionary entries support spoken-word biasing and literal word/snippet
  replacement. Accepted Scribe edits can produce an optional dictionary
  suggestion; Add and Dismiss are implemented, and dismissed pairs are capped
  and can be cleared from Dictionary settings.
- The desktop React bundle builds successfully.
- A native Windows production binary has been compiled and manually exercised
  with CUDA. The complete Notepad, VS Code, and Discord
  application matrix is still pending. Apple Silicon testing confirms cloud
  Dictation insertion and working Scribe in the repaired source build; this
  does not establish Intel or release-installer support.

This engineering preview is not a verified release on every platform.

## How the two modes differ

Say:

> Write down one two three five — no wait, four and five.

Dictation types:

> Write down one two three five — no wait, four and five.

Scribe types:

> Write down one two three four and five.

Dictation keeps rolling transcription warm while the shortcut is active, then
inserts the complete verbatim transcript when the user releases or unlocks it.
Scribe holds the same complete utterance, resolves corrections, and opens a
review draft before any text reaches the cursor.

Scribe detects whether the captured target is email, chat, an AI prompt, notes,
or general text and sends the raw transcript to your selected cleanup provider with the matching
"polish, don't invent" instructions. Email may gain a greeting and sign-off;
other registers do not. Every register forbids new facts, commitments, offers,
or constraints, and the detected writing style can be changed in the review
window to regenerate the draft.

Optional nearby-text context lets Scribe compose replies from the conversation
around the caret or rewrite selected text. It is off by default, never reads
password fields, and always keeps the review step before insertion. With
Gemini selected, included editor text is sent to Gemini; local cleanup keeps it
on the device. Quill learns aggregate per-app presentation preferences from
accepted drafts, but never stores the drafts themselves as examples.

Cleanup output is never injected silently. Every Scribe activation opens a
review window with the raw transcript and the cleaned draft side by side;
text reaches the cursor only after you explicitly accept it (or edit it and
accept). Discard is always one click away. This human-in-the-loop step is
the primary safeguard after removing the earlier strict word-provenance
gate, which had blocked legitimate rewrites like `hei` → `Hey`. A
lightweight sanity guard also swaps the LLM output for a safe local draft when
the prompt would exceed its reserved context budget, the provider returns a
missing or malformed text field, generation stops at the output-token limit,
the model returns nothing or exceeds its action-specific word budget, or the draft
introduces new promise, availability, proposal, or follow-up language that was
absent from the transcript.

## Requirements

### At runtime

- Windows 10/11 x64
- macOS 12+ is the intended target. Native hotkeys, insertion, and universal
  Metal sidecar packaging are implemented. The repaired Apple Silicon source
  build is being verified on hardware. Published macOS builds remain unsupported; see
  [docs/macos-testing.md](docs/macos-testing.md).
- A microphone
- About 75 MB–1.6 GB for a whisper.cpp model
- Optional NVIDIA acceleration: about 700 MB to download and 1.25 GB installed
- Optional Scribe provider:
  - [Ollama](https://ollama.com/)
  - LM Studio, Jan, or llama.cpp server exposing an OpenAI-compatible endpoint

Scribe does not choose a cleanup model automatically. After connecting Ollama,
the user explicitly selects one of Quill's evaluated options:

| Scribe model | Download | Clear minimum | Trade-off |
| --- | ---: | --- | --- |
| TurboSpeak 1.7B | ~1.1 GB | 8 GB system RAM, 2 GB free RAM, 1.1 GB disk | Fast on CPU; less reliable on complex corrections |
| Qwen 2.5 7B | ~4.7 GB | 16 GB system RAM, 8 GB free RAM, 4.7 GB disk | Stronger corrections and register-aware rewriting; slower on CPU |

A dedicated GPU is optional for both. Dictation never uses the cleanup model.

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

Quill disables models whose files are absent from the selector. File presence
alone does not prove integrity: the speech engine checks the pinned size and
SHA-256 before loading an installed model and refuses invalid files.
English shows English-only models; Auto-detect and other languages show only
compatible multilingual models.

### To build

- Node.js 20+
- pnpm 10+
- Rust stable (`rustup`, `cargo`, and platform Tauri prerequisites)
- CMake 3.20+
- Git
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
      src/streaming.rs   Timed-word representation shared by speech providers
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
| Desktop shell | Tauri v2 + React + TypeScript | Shared settings and command surface with native platform windows |
| Recognition | whisper.cpp | Bundled Windows CPU runtime and macOS Metal sidecar; optional verified Windows CUDA download |
| Text commit | Final clipboard paste | The target editor changes only after the user stops speaking |
| Speech recognition | Local whisper.cpp, Groq, or Gemini 3.5 Transcribe | Every provider inserts after release |
| Scribe cleanup | Ollama, OpenAI-compatible localhost, Gemini, or disabled | Every draft still passes the same guards and review window |
| Windows hotkeys | `GetAsyncKeyState` polling | No system-wide keyboard hook |
| Windows insertion | Clipboard paste | Reliable Unicode insertion after transcription completes |
| macOS hotkeys | CoreGraphics key-state and modifier-flag polling | Global state without an event tap keyboard hook |
| macOS insertion | CoreGraphics events + clipboard | Works through the standard Accessibility permission path |
| Updates | Tauri updater + GitHub Releases | User-confirmed updates from signed release artifacts |

## Development

Install JavaScript dependencies:

```powershell
pnpm install
```

Run the historical prototype contracts and frontend regression tests:

```powershell
pnpm test
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
build the pinned `whisper-server` runtime. Windows packages CPU runtime
resources and a separate CUDA archive; macOS copies the universal Metal server
into Tauri's sidecar directory.

Windows (bundled CPU runtime plus a separately packaged optional CUDA runtime):

```powershell
./scripts/build-whisper.ps1
```

macOS with Metal:

```bash
./scripts/build-whisper.sh metal
```

Speech models are not bundled with the installer. During onboarding, choose
local speech and start a model download, or explicitly connect and select a
cloud speech provider. Models can be changed or removed from **Voice → Compare
and download models**. Quill stores local speech models in the platform
application-data `models/` folder. Every registry model has a pinned upstream
revision, size, and SHA-256: downloads are checked before installation and
installed files are checked before the speech engine starts. See
[model verification](docs/model-verification.md) for provenance and limits.
NVIDIA users can likewise download the optional CUDA runtime from **Voice →
Compute backend**. Quill verifies the archive before installation, and selecting
CUDA without the pack keeps transcription available on CPU with a visible status.

## Native desktop build

After installing the Tauri prerequisites and building the whisper sidecar:

```powershell
pnpm tauri:build
```

For a local ad-hoc-signed macOS build without updater signing keys, use the
[development bundle command](CONTRIBUTING.md#local-macos-bundle).

Tauri's Windows NSIS configuration uses `currentUser`, so the installer does
not require administrator rights. The release workflow builds a universal
macOS `.app`/`.dmg` with the Metal sidecar. Packaging and compilation do not
establish support. Consult the hardware verification record before treating
a macOS artifact as supported.

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
- user-triggered speech/cleanup model and optional runtime downloads;
- explicitly selected Groq or Gemini processing and connection tests;
- GitHub Releases update checks.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Security
issues should follow [SECURITY.md](SECURITY.md).

## License

[GNU AGPL-3.0-or-later](LICENSE)
Cloud processing is always opt-in. Groq transcription uses
`whisper-large-v3`; Gemini transcription uses `gemini-3.5-transcribe` in
Verbatim mode for Dictation and Smart mode for Scribe. Both upload the
completed recording only after the shortcut is released and do not type live.
Gemini Scribe cleanup uses `gemini-3.1-flash-lite` and receives transcript text
plus optional bounded editor context. API keys are stored in Windows Credential
Manager or macOS Keychain rather than in Quill's settings file. Adding a key
never changes the active provider.
