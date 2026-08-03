# Quill roadmap

Quill is an independent open-source project and is not affiliated with any
commercial dictation product.

## 0.1 — Windows end-to-end preview

- [x] Validate LocalAgreement and trailing-buffer contracts against canned text.
- [x] Build Tauri v2 settings shell, tray configuration, and recording overlay.
- [x] Add poll-only Windows hotkey state using `GetAsyncKeyState`.
- [x] Add Win32 `SendInput` and clipboard insertion primitives.
- [x] Detect Ollama and common OpenAI-compatible localhost endpoints.
- [x] Reject cleanup responses containing unspoken lexical content.
- [ ] Bundle a pinned whisper.cpp `whisper-stream` build and record the upstream commit.
- [ ] Connect microphone lifecycle, rolling ASR passes, word commit, and injection in one session controller.
- [ ] Add hotkey-conflict detection and first-run shortcut validation.
- [ ] Ship an unsigned Windows engineering-preview installer.

## 0.2 — macOS verification

- [x] Add CoreGraphics poll/injection source.
- [ ] Verify hotkey behavior on Apple Silicon.
- [ ] Complete microphone and Accessibility permission onboarding.
- [ ] Verify whisper.cpp Metal acceleration and CPU fallback.
- [ ] Test clipboard restoration across common target applications.
- [ ] Produce a signed/notarized `.dmg` candidate for external testing.

## 0.3 — Daily-driver hardening

- [ ] On-device model browser, verified downloads, and disk-space warnings.
- [ ] Personal recognition bias dictionary.
- [ ] Exact replacements and spoken snippets.
- [ ] Local-only word count and time-saved estimates.
- [ ] Crash recovery UI for the last audio/transcript checkpoint.
- [ ] Multi-monitor overlay placement and full keyboard navigation.
- [ ] Automatic update prompts backed by signed GitHub Release artifacts.

## Later

- Additional languages and per-language cleanup prompts.
- User-controlled Voice Activity Detection.
- Per-application formatting profiles.
- Portable Windows build.
- Signed Linux packages after the Windows/macOS experience is stable.
