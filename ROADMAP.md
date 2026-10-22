# Quill roadmap

Quill is an independent open-source project and is not affiliated with any
commercial dictation product. This roadmap distinguishes implemented source
from platform verification; a checked item is not a claim that macOS works.

## Implemented in the engineering preview

- [x] Historical LocalAgreement and trailing-buffer prototype with canned tests.
- [x] Tauri v2 settings shell, tray, recording/processing overlays, and onboarding.
- [x] Poll-only global hotkeys and clipboard insertion on Windows and macOS.
- [x] Microphone lifecycle, rolling local ASR, and final insertion after release.
- [x] Pinned whisper.cpp `whisper-server`: bundled Windows CPU runtime, optional CUDA pack, universal macOS Metal sidecar.
- [x] Explicit local/Groq/Gemini speech selection and OS credential-store keys.
- [x] Local and Gemini cleanup, action-specific safety guards, and Scribe review before insertion.
- [x] On-device speech-model browser and pinned SHA-256 download/startup verification.
- [x] Recognition-bias dictionary, exact replacements, and spoken snippets.
- [x] Recovery UI for the last audio/transcript checkpoint.
- [x] Optional bounded editor context and aggregate per-app writing preferences.
- [x] User-confirmed updater flow and release artifact configuration.

The production cleanup path uses prompts, fallback guards, and user review.
The strict lexical-provenance gate remains a historical prototype contract.

## Current gate: macOS end-to-end verification

The initial hardware failures prompted repairs to window initialization,
transparency, credential access, physical modifiers, and target insertion.
The user has confirmed cloud Dictation, Scribe, Fn shortcuts and full-screen
overlay visibility on Apple Silicon. The broader acceptance matrix remains open; published
macOS builds remain unsupported. See [the verification record](docs/macos-verification.md).

- [ ] Complete onboarding with local speech and separately with a cloud key.
- [ ] Verify first and repeated Dictation/Scribe activations on Apple Silicon.
- [ ] Verify microphone and Accessibility denial, recovery, and restart behavior.
- [ ] Verify compact transparent overlays and dismissal after success/failure.
- [ ] Verify whisper.cpp Metal acceleration and CPU fallback on hardware.
- [ ] Verify single insertion, target focus, and clipboard restoration in TextEdit, browser fields, chat, and terminals.
- [ ] Record Intel results separately from Apple Silicon results.
- [ ] Produce and verify a Developer ID signed/notarized `.dmg` for external testing.

## Daily-driver hardening

- [ ] Complete the Windows Notepad, VS Code, and Discord application matrix.
- [ ] Validate hotkey conflicts and assistive-technology compatibility.
- [ ] Add disk-space warnings before large model/runtime downloads.
- [ ] Verify multi-monitor overlay placement and full keyboard navigation.
- [ ] Verify signed update installation and recovery on each supported platform.
- [ ] Add local-only word count and time-saved estimates.

## Later

- Additional language-specific cleanup evaluation.
- User-controlled Voice Activity Detection.
- Portable Windows build.
- Signed Linux packages after the Windows/macOS experience is stable.
