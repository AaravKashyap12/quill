# Optional offline Dictation and Scribe

Working branch: `fix/macos-permission-prompts`, based on `c8c1787` (0.4.7).
AStack pinned at `174b42b8b43c17a807b57870fa6f8f8866dff136`.

The installer contains the speech and text inference engines. Model weights
are optional downloads in setup or Voice settings. Existing cloud and external
server choices are preserved. Selecting local Scribe now means Quill's own
runtime, rather than asking the user to install Ollama.

Whisper speech models retain the existing hardware/language recommendations.
Base is about 148 MB (142 MiB); smaller and larger choices remain available.
Offline Scribe uses Qwen3 4B Instruct 2507 Q4_K_M, a 2.50 GB download. It is an
optional starting point, not a claim of best accuracy across languages or PCs.
Allow at least 4 GB free memory; CPU processing can be slow. Model files live
in per-user application data, outside the update/replacement app bundle.

## Competitor source review

- [Handy setup](https://handy.computer/docs) downloads the chosen speech model
  on first launch, asks for Microphone and Accessibility on Mac, and runs from
  the tray/menu bar. Its [FAQ](https://handy.computer/docs/faq) explains that
  Mac releases use the author's Apple developer identity. Signing continuity
  matters across updates; that does not diagnose Quill's same-binary symptom.
- [Handy's downloader](https://github.com/cjpais/Handy/blob/f2483011d60fccab639d3c33f40b3456e92f2940/src-tauri/src/managers/model/download.rs)
  retains partial downloads, uses Range, validates checksums, handles ignored
  ranges and bounds stalled transfers. Quill now follows those same principles
  using its existing downloader. Its fixed model catalog remains allowlisted.
- [Handy's lifecycle](https://github.com/cjpais/Handy/blob/f2483011d60fccab639d3c33f40b3456e92f2940/src-tauri/src/lib.rs)
  hides the window on close and keeps a tray entry. On Windows it brings setup
  forward if microphone privacy access is denied. Mac-specific consent is not
  presented as a Windows permission step. Quill's requested Cmd+Q behavior is
  implemented specifically in its Mac menu; tray Quit remains an actual exit.
- [OpenWhispr's runtime](https://github.com/OpenWhispr/openwhispr/blob/9208f248f992e644ffa7e24c42c36263d36bccc4/src/helpers/llamaServer.js)
  uses an app-managed llama.cpp server, loopback binding, bounded context and
  idle unloading. Quill uses a smaller lifecycle: one serialized request at a
  time, then immediate engine shutdown. This frees memory between uses but
  incurs a model load on each Scribe request. Keeping a warm engine is deferred
  until latency and memory measurements justify it.

## Implementation and boundaries

- Pinned llama.cpp v0.9.0 source; universal Mac Metal binary, Windows x64 CPU
  build. Web UI, HTTPS/OpenSSL and OpenMP are disabled. Windows and Intel Mac builds use AVX2, FMA, F16C and BMI2. Quill checks
  those CPU features before offering the download or starting the engine.
  The MSVC runtime is static. Mac architectures are built separately with
  native ARM/x86 kernels, then combined with lipo. No runtime installer
  or network model discovery is needed. New engine build is in CI and releases.
- Loopback-only endpoint, a fresh OS-random API key per process, no proxy or
  redirects, no inherited model/server configuration, bounded readiness and
  request timeouts. Prompts and responses are not written to engine logs.
  Quill's existing prompt budgets, output guards and review-before-insertion
  remain in use. Only deliberate Scribe processing loads the model.
- The model download uses an immutable URL, exact size limit and SHA-256.
  Pause and network interruptions retain `.part` files. Resume hashes the
  prefix; ignored Range requests restart safely. A bad checksum removes the
  partial. Removal is rejected during an active download or Scribe request.
- `getrandom` 0.2.17 was already in Cargo.lock and is now a direct dependency
  for per-process authentication. Verified against its official Rust crate
  documentation: macOS uses getentropy, Windows uses BCryptGenRandom; the
  crate has no build script. No additional package version was resolved.

## Verification

Evidence: `target/macos-evidence/permission-audit/` and
`target/macos-evidence/offline-scribe/` (ignored, no personal dictation).

The first real Qwen3 test failed a spoken date correction. Generic correction
instructions alone still failed that case. The local-only prompt now includes
explicit correction examples, counted inside the existing input budget. Eight
synthetic cases then passed: held-out day/time and quantity corrections, the
original failure, budget and name corrections, negation, technical numbers,
and an AI-prompt target that must preserve rather than answer the request.
Tests inspect raw model output and require no safety fallback. The prompt case
uses the actual Prompt register; Generic Compose was observed to answer that
request and is not a proof of verbatim prompt preservation. This is a smoke
check, not a multilingual evaluation or a broad quality guarantee.

Command for the real-model check (sha2 is optimized to avoid measuring debug
hashing overhead):

```sh
QUILL_SCRIBE_TEST_MODEL=/absolute/path/to/pinned-model.gguf cargo test --locked \
  --config 'profile.dev.package.sha2.opt-level=3' \
  --manifest-path apps/desktop/src-tauri/Cargo.toml \
  local_scribe::tests::managed_engine_synthetic_draft -- --ignored --nocapture
```

The normal CI suite does not fetch 2.50 GB: this explicit hardware check is
ignored there. Separate deterministic HTTP fixtures cover resume, ignored
Range, pause, complete retained files and checksum rejection. UI checks cover
explicit download, resume and reopening settings during a download.
Windows native execution and performance are not established by Mac checks.

## Independent review brief

> Review the uncommitted optional offline-model implementation in /Users/aarav/Desktop/quill on fix/macos-permission-prompts read-only. Use /Users/aarav/.local/share/astack/skills/ponytail-review/SKILL.md at pinned AStack 174b42b; do not refresh, edit, run the app, access credentials, or delegate. Inspect tracked diffs AND new files (git status). Scope: local_scribe.rs, managed cleanup dispatch, downloads.rs resumability/removal, setup/settings UI, provider readiness, build-scribe.py, Tauri packaging and CI/release integration, and connected callers. Acceptance: small installer with optional persistent speech/Scribe weights; no separate Ollama required for the new local route; existing providers preserved; model download explicit, size shown, pause/resume/corruption safe; pinned engine/model supply chain; local engine bound to loopback with per-process authentication, no prompt logging, bounded startup/generation and reliable shutdown on success/error/quit; only explicit Scribe use loads the model; Windows and universal Mac packaging consistent. Check concurrency, untrusted paths/HTTP, compatibility and realistic UX. Report concrete must-fix issues, limitations and lean simplifications. Distinguish automated/source evidence from unrun Windows/native app flows and limited model-quality smoke checks. The earlier permission/Cmd+Q change already received its own review; do not restart that review except for regressions from this offline feature. No publishing.

## Review disposition

The fresh-context Opus 5.5 review found Windows scalar CPU flags and the
Python executable selection as must-fix issues. Both are corrected; setup-python
now supplies `python` on both runners. The Intel Mac build also now compiles
architectures separately before lipo. A Windows run is still required before release.

Other fixes: verified model metadata caching; a remove/re-download instruction
for a damaged model; a Windows kill-on-close Job Object; deterministic tests
for a child that exits immediately or never becomes healthy; prefix hashing
before the Range request; and excluding the managed provider from external
server warm-up probes. The local setup hides the irrelevant external-server
refresh control. Model size labels use the returned size.

Deferred: Mac force-quit/crash during processing can leave the engine alive.
Normal quit, request failure and timeout cleanup are covered. Quill maintainers
own this remaining crash-lifecycle work. Cold starts are intentional; source
build caching and consolidating two small deletion loops are optional follow-ups.
The brief port reservation/bind race remains; an active local process could
claim that port. A downgrade to the old 0.4.7 binary cannot parse the new `local`
provider value: choose a legacy provider before downgrading.

Fresh local results after review fixes: 236 Rust checks, 83 frontend checks,
9 prototype checks and 6 real Mac signing-policy checks passed. Typecheck,
Clippy with warnings denied and formatting passed; build/native checks are
recorded alongside the final artifact. The real-model eight-case test passed
with the revised engine. An additional real-engine probe returned 401 for
missing/wrong keys and 200 for the correct key; lsof confirmed loopback-only
listening, and the process exited after the check. No Windows execution or
performance claim follows from these Mac results.
