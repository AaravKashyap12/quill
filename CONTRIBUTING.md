# Contributing to Quill

Thanks for helping build local-first dictation.

## Principles

- Preserve the distinction between raw Dictation and resolved Scribe behavior.
- Do not add cloud telemetry, account requirements, or silent network calls.
- Cleanup must never add meaning. New cleanup behavior requires provenance and
  correction tests.
- Windows global shortcuts must remain poll-based; do not add a system-wide
  keyboard hook.
- Platform-specific code belongs behind a small Rust module boundary.

## Before opening a pull request

```powershell
python prototypes/test_scribe_buffer.py
pnpm typecheck
pnpm build
```

If Rust is installed:

```powershell
cd apps/desktop/src-tauri
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

UI changes should include desktop and mobile screenshots and must work by
keyboard. Never check models, recordings, signing material, or generated
installers into Git.

## Commit and PR scope

Prefer small, reviewable changes. Explain:

- what changed;
- which platform(s) were exercised;
- privacy/network implications;
- how failure falls back safely.
