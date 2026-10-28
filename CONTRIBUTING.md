# Contributing to Quill

Thanks for helping build local-first dictation.

## Principles

- Preserve the distinction between raw Dictation and resolved Scribe behavior.
- Do not add cloud telemetry, account requirements, or silent network calls.
- Cleanup must preserve specific facts. New cleanup behavior requires
  correction, action-specific guard, and review-before-insertion tests. The
  strict lexical-provenance gate belongs to the historical prototype.
- Windows global shortcuts must remain poll-based; do not add a system-wide
  keyboard hook.
- Platform-specific code belongs behind a small Rust module boundary.

## Before opening a pull request

```powershell
pnpm test
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

UI changes should include desktop-window and compact-layout evidence and must
work by keyboard. A build is not proof of native hotkeys, microphone access,
window transparency, or insertion; record the platform and actual user path.
Never check models, recordings, signing material, or generated installers into Git.

## Local macOS bundle

After building the sidecar with `bash scripts/build-whisper.sh metal`, build a
local test app with a complete ad-hoc bundle signature:

```bash
pnpm macos:build:local
```

The script signs the bundled helper and app, verifies the complete bundle, and
omits release updater artifacts. It needs no private updater key. Install this
build before granting permissions; rebuilding changes its ad-hoc identity.
Release updates require Tauri updater signing. Trusted platform distribution
also needs publisher signing and, on macOS, notarization; those are separate
from updater signatures. Exercise onboarding, recording, overlays, and insertion on hardware;
the build alone does not prove those paths work.

## Commit and PR scope

Prefer small, reviewable changes. Explain:

- what changed;
- which platform(s) were exercised;
- privacy/network implications;
- how failure falls back safely.

## Maintainer release workflow

For maintainer requests to “fix, commit, and push Quill,” include a new patch
release unless the request explicitly says otherwise. Keep implementation
commits focused, finish the applicable native checks, and require green CI for
the source revision before publishing. A source push alone is not completion
of that request.

Use **Publish app update** (`.github/workflows/app-update.yml`) with the next
unused patch version. It verifies source CI, updates all five version files in
a separate commit, and builds/publishes the Windows and universal macOS assets.
Do not create a competing local version bump. Check both platform jobs, release
notes, installer assets, updater signatures, and `latest.json` after publication.
Synchronize the local branch with the workflow's version commit using a
fast-forward only.

Tauri updater signatures authenticate update artifacts; they do not provide
Windows publisher trust or Apple Developer ID notarization. Describe the actual
signing status and tested platform coverage in release notes. Publishing an
artifact does not establish additional hardware or editor support.

### Homebrew release follow-up

After a Mac release is published, update `Casks/quill.rb` in the public
[`homebrew-quill`](https://github.com/AaravKashyap12/homebrew-quill) tap with
the exact release version and downloaded DMG SHA-256. Compare GitHub's asset
digest and verify the complete app signature plus both executable architectures.
Run the tap CI, which installs into a temporary app directory and uninstalls
without launching Quill or deleting user data. Do not add quarantine-removal or
permission-grant hooks. The website's scoped install command stays unchanged.
