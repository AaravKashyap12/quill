# macOS verification record

## Scope and build

2026-10-22, Apple M5 Pro, macOS 27.0.1 (26A434), 24 GB RAM. Base commit
`0a63b355598bac4d9aef2970f7211a1092464966` plus the focused macOS repair
commit series. Native debug app, local ad-hoc bundle signature; this is not release-signing or
Intel verification. Goal: onboarding speech reaches its field, transparent
overlays, repeated Dictation and Scribe review/insertion, and local/cloud paths.

AStack guidance used from commit `7360e1e95454b5e561b784b205b741fdf683b91c`.
The code-only Graphify map was generated before implementation (1,573 nodes,
3,145 edges), then refreshed after permission-flow integration (1,852 nodes, 3,985 edges, 98 communities). Generated graph files remain
ignored in `graphify-out/`. No semantic model backend or paid graph inference
was required. The fresh-context Tier2 brief is retained in
[`verification/tier2-review-brief.txt`](verification/tier2-review-brief.txt).

## Reproduced defects and changes

- All three transient routes registered settings initialization effects.
  Separate surface routing prevents their Keychain/provider probes and settings
  writes. Four routing/config regressions failed before the fix.
- Pinned Tauri 2.11.5/runtime-wry 2.11.4 gates macOS transparency on
  `macos-private-api`. Both the macOS Cargo feature and configuration are now
  enabled; transparent CSS was already present.
- Right-side modifier keys failed shortcut matching. Both sides now match.
- Unposted native paste events lacked the Command flag. Regression tests now
  inspect the generated flags; the implementation creates both events before
  posting. This unit test does not establish actual target insertion.
- Onboarding's practice field lacked focus and native status/error feedback.
  It now autofocuses and exposes RuntimeStatus with hold/toggle instructions.
- On source-app launch, a bounded native process sample showed the main thread
  blocked in synchronous Keychain status while the session thread waited for
  Keychain access. The user handled the OS prompt and onboarding reached Ready.
  All credential operations now run on blocking workers; an injected OS-wait
  regression failed inline and passed with the worker boundary. No credentials
  were extracted into test output, and storage permissions were not bypassed.
- Repeated Keychain prompts exposed two further issues. Per-provider concurrent
  reads now share one blocking store read, and successes, missing keys and
  failures remain cached for the process. Only deliberate UI retry can reread
  unavailable credentials. An injected 12-reader regression changed from 12
  store reads to one. Denied access keeps cloud setup selected and exposes Retry
  access instead of falling into local-model setup. Keys remain ordinary
  strings in process memory; no secure-erasure guarantee is added.
- The previously installed local bundle failed strict signature verification:
  its linker signature did not bind Info.plist or resources. The new
  `pnpm macos:build:local` signs the nested helper and complete app with the
  correct identifiers, then verifies both. Strict verification passes. Ad-hoc
  identities still change on rebuild; this is not persistent Developer ID
  signing. Later rebuilds still required renewed approvals; restarting the same
  approved binary restored access without changing its identity.
- Physical testing captured a 2,409 ms microphone-open delay followed by only
  13 ms of recording. A longer hold then produced a nine-word transcription;
  insertion was rejected because the rebuilt app lacked Accessibility access.
  Startup now shows an opening-microphone state and empty recordings retain
  actionable feedback. Queued text is no longer reported as inserted, and
  repeated insertion failures retry at 500 ms with deduplicated diagnostics.
- The initial repaired defaults used Right Option and Right Command. After the
  user reported a Command conflict in T3, defaults changed to Fn for Dictation
  and Control + Fn for Scribe, following documented Wispr Flow shortcuts and
  Willow's Fn default. General offers no-Fn presets and preserves custom pairs.
  Physical Fn uses virtual key 63, not the secondaryFn flag also set by navigation
  keys. The user confirmed both Fn modes on the installed panel build.
  A 250 ms exclusive hold activates; sampled additional keys suppress or cancel
  the hold. An independent 8 ms worker retains cancellation across session
  stalls. The session discards cancelled audio without finalization. Exact old
  Mac default pairs migrate; custom chords and Windows defaults are preserved.
  Polling cannot guarantee detection of transitions wholly between samples or
  media buttons outside virtual keycodes 0–127.
- Physical diagnostic holds on this Mac reported right Option in the
  device-side modifier flags while both combined/HID key-state samples reported
  the left Option code instead. The right trigger never became down. Modifier
  sampling now uses one device-side flags snapshot per poll for all eight
  left/right modifiers; nonmodifier keys still use key-state sampling. This
  prevents both the missed right trigger and phantom left-key contamination.
  Three reproducing fixtures failed before the fix; side isolation, both-side
  conflicts, all other-key cancellations and custom chords pass afterward.
  The user subsequently confirmed actual Right Option and Right Command activation.
- The user reported that the pill was below the Dock. Positioning now uses
  the usable monitor area with a 16-point margin. Native Mac panels use AppKit
  visible-frame coordinates in points; Windows retains Tauri's scaled work area.
  Geometry regressions cover signed origins, screen boundaries and Dock layouts.
- Collection flags on Tao's ordinary NSWindow did not fix full-screen visibility;
  the user reproduced this on the installed flags build. The replacement hosts
  the existing webviews in actual nonactivating NSPanel instances without class
  replacement or private methods. Recording does not request keyboard focus;
  review does. A fresh review caught direct use of a notification-only AppKit
  method; root replaced it with `orderOut` and verifies release before paste.
  Allocation, initial readback and clean quit pass on hardware. The user then
  confirmed full-screen visibility and working modes; separate keyboard-edited
  review-to-target acceptance remains to be explicitly exercised.
- Tier2 found a pre-existing Scribe target bug: only the PID was retained.
  The fix retains and verifies the original AX window and editable field, and
  dispatches review acceptance off the macOS UI thread. Fake AX failure paths
  pass; actual WebKit focus restoration still needs the native test.
- The universal sidecar build failed Xcode 27's multi-architecture `lipo
  -verify_arch` invocation. Each architecture is now verified separately;
  the rebuilt sidecar contains arm64 and x86_64.
- Model downloads previously did not enforce the promised hashes. Downloads
  and installed-file startup now validate pinned artifacts; see
  [model-verification.md](model-verification.md). CI now runs frontend tests.

## Checks and evidence

Useful local artifacts are retained in ignored `target/macos-evidence/`.
Model/voice fixtures are generated test data, not user speech.

| Check | Result | Evidence / limit |
| --- | --- | --- |
| `pnpm test` | Pass: 9 Python, 73 Vitest | `permissions-frontend-tests.log`; SSR/mocked-effect tests do not run native UI |
| Typecheck/lint and Vite build | Pass | `permissions-lint.log`, `permissions-build.log`; later hint wrapping: `hint-wrap-build.log` |
| `cargo test --locked` | Pass: 187, 1 existing ignored | `permissions-rust-tests.log`; final archived credential/permission commits also pass; no bundle-resource override in root checkout |
| `cargo fmt --check` | Pass | `integrated-format.log` |
| Clippy all targets, warnings denied | Pass | `permissions-clippy.log`; host macOS targets only |
| Complete local bundle signature | Pass | `final-permissions-app-build.log`; `codesign --verify --deep --strict`, local ad-hoc only |
| Universal whisper.cpp build | Pass | `whisper-build.log`; pinned upstream revision |
| Metal recognition | Pass | `metal-server.log`, `metal-transcript.json`; generated WAV through bundled sidecar |
| CPU recognition (`--no-gpu`) | Pass | `cpu-server.log`, `cpu-transcript.json`; same generated WAV |
| Native app launch / BYOK onboarding | Pass for startup | Rebuilt async-credential app remains responsive and reaches Ready with saved provider setup |
| Microphone → cloud → target insertion | Pass for reported Notes/T3 targets; onboarding field not rerun | User confirmed repeated automatic insertion on the repaired flags build; matching microphone/transcription/injection diagnostics. Earlier onboarding attempt was permission-blocked |
| Overlay transparency on active recording | User-confirmed pass in windowed apps and full-screen T3 | Ordinary-window flags failed; actual native panel build resolved full-screen visibility. Native readback shows visible, non-key, clickthrough recording panel on the active Space |
| Scribe approval/discard/original-target insertion | User-confirmed accept/insertion pass; broader matrix pending | Final full-screen T3 test explicitly requested Scribe acceptance; user replied both modes work well. Separate edited/discarded reviews and the full target matrix are not independently observed |
| Physical modifiers, hold/toggle/repeated use | Right Option/Command and subsequent Fn / Ctrl+Fn pass | User confirmed both Fn modes with visible full-screen pill; toggle retention has injected producer/consumer coverage only |
| Permission denial/recovery, clipboard restoration | Not run | Do not reset user permissions or overwrite private clipboard to manufacture a test |
| Intel/Windows execution, signed installer/update | Not run | Host evidence does not establish these platforms or distribution paths |

The earlier 14 ms session logs exclude microphone initialization. The new
physical test demonstrates a 2.4-second cold-open delay with release before
capture was ready; later warm opens were about 62–67 ms. This explains that
observed empty recording, but does not prove every historical short recording
had the same cause. Configured-key diagnostics showed matching session/HID
states. The user's first shortcut follow-up requested standalone right-side holds;
the integrated sampler regressions cover cancellation, including a simulated
2,409 ms consumer stall. Later Right Option/Command tests passed; the subsequent
Fn defaults and native panels later passed the physical test described below.

The earlier integrated app was built and installed at `/Applications/Quill.app` for a
stable permission target. The user reported enabling Accessibility and handling
Keychain prompts. Groq connection testing passed in the installed UI and Gemini showed connected.
The first physical Right Option test produced no pill or recording, despite the
correct migrated settings and live worker/session threads. No insertion success
was claimed for that attempt; later diagnostics identified the modifier mismatch.

The second fresh-context review found retained toggle-edge loss during local
ASR waits and cross-surface dismissal timers. Both are fixed in source with sampler-to-session and timer/event regressions; see
`verification/tier2-final-review-brief.txt` for the exact review request.

After the user reported repeated Keychain prompts, a focused security review
of the credential cache, recovery UI and local signing found one cloud recovery
routing defect. It is fixed with failing-then-passing interaction regressions;
see `verification/review-responses.md`. The corrected bundle was installed at
`/Applications/Quill.app` and verified again in place. Executable SHA-256:
`b148ece14d15c98583b76869f3afb4ab00621f4fee6ced1ed284c0d45613e733`.
PID 45660 launched at 10:44 UTC. Startup reports Accessibility false while
System Settings still shows the earlier Quill entry enabled. A bounded process
sample places credential waits on blocking workers, with the UI event loop
responsive. At that checkpoint, OS approval and microphone-to-target verification were pending;
that build was kept unchanged while collecting the physical modifier evidence.
Both Groq and Gemini connection tests then completed through Voice settings
without reopening Keychain. The UI previously lost Gemini's connected state
after a passive vault refresh; tested status now survives that refresh and is
invalidated by missing/inaccessible keys or a subsequent failed test.

The observed modifier mismatch required a native repair and replacement build.
The replacement passed strict verification in `/Applications/Quill.app` and
launched as PID 49256 at 10:52 UTC. Executable SHA-256:
`f320140ba7971a1fc2cbc4b47a7a9952fde7ec5cf6cc591e5cb46744edf658cb`.
This was the first successful physical-test target. Physical Right Option reached
activation at the 250 ms threshold; the first attempts then failed the native
Accessibility guard. After the user's approval and a restart of the unchanged
binary, PID 49628 reported both Accessibility and input-monitoring access true
at 10:55:45 UTC. The app reached provider readiness without another Keychain
prompt. Automated modifier presses still do not reach the monitored physical
state. The user then confirmed repeated automatic Dictation insertion in Notes
and this T3 chat. Logs show right-only activation, 60–64 ms warm microphone
startup and successful release transcription/injection. The pill is visible in
windowed apps but absent above full-screen T3, confirmed by toggling T3 between
windowed/full-screen. Full-screen Spaces behavior required a further repair; the user
also confirmed Scribe works in a windowed editor, but reported slow Gemini
refinement. Content-free metrics for that short Scribe session isolate 441 ms
in Groq transcription and 32,251 ms in the Gemini request, with 32,708 ms from
release to review. This single observation establishes the delayed stage, not
a general macOS-versus-Windows performance difference.

## Latest installed panel/Fn build

The flags-only follow-up (`901fdb9616c0ea0a5921581c821cbe97c0752fbf5a9d2a3146ab5f555c4e79a8`)
still failed full-screen visibility by user report. Its two later Gemini requests
took 1,500 ms and 4,608 ms; see the [latency investigation](verification/gemini-scribe-latency.md)
for the limits of that comparison.

The typed-panel/Fn build is installed at `/Applications/Quill.app`, executable
SHA-256 `ab1da320f2ad440661d6c14de08ead6eff8c8d16ce814887a7021b08361764ed`.
It first launched as PID 63247 at 11:37 UTC, created all three native panels,
and showed Fn / Control + Fn in General. After the user refreshed OS approvals,
the same binary cleanly quit and restarted as PID 64425 at 11:38:54 UTC.
Accessibility and input-monitoring readback both report true. Groq readiness
and the rendered shortcut selectors were observed through native accessibility.
At 11:40 UTC the user tested Fn Dictation and Ctrl+Fn Scribe in full-screen T3,
then reported that everything worked and the pill was properly visible. Logs
corroborate a non-key, clickthrough recording panel on the active Space, successful
Dictation insertion, and a key, interactive Scribe review panel. An initial short
hold ended before the 2.3-second cold microphone startup completed; the next hold
transcribed and inserted normally. This is separate from the later Gemini delay.

The same Scribe run took 588 ms for Groq, 597 ms release-to-transcript, 22,748 ms
inside Gemini, and 23,354 ms release-to-review. A subsequent retry was fast by user report, and the user closed the latency
investigation without further changes. No causal Mac-versus-Windows explanation
has been established.
The user requests retaining review only after refinement finishes.

Follow-up work replaces the colored Fn emoji with monochrome key symbols and
removes app-controlled redundant permission requests. The user has no paid
developer-program membership and explicitly excludes paid signing from scope.

## Final permission-flow candidate

Source through `84be358` was built once and installed at `/Applications/Quill.app`.
Executable SHA-256: `af5d5506b3cd8f938dd1cc4199c2a60744399c03521d9a9b90abd7056eeddfb6`.
The previous bundle is retained outside the repository. The old background test
instance had no window accessible to computer use; after its Quit action returned
`noWindowsAvailable`, only that owned PID was terminated before replacement.
The new candidate passed strict signature verification in place.

PID 75802 launched at 12:16:49 UTC. Settings rendered and remained navigable
while selected Groq Keychain access was pending. Startup only checked Accessibility
and reported false; the explicit Enable action changed to Check access. After
the user's approval, Check access removed the notice and Groq reached Ready.
The actual native screenshot shows monochrome Fn/globe and Control keycaps.
Voice shows Gemini access unchecked with an enabled saved-key check, preserving
the selected Gemini cleanup without opening it just to display status.

The candidate quit normally through its own Quit shortcut. The same executable
restarted as PID 76122 at 12:18:31 UTC with Accessibility true and subsequently
input-monitoring true. Groq returned to Ready. The unchanged-build final physical
check requested Fn Dictation and Ctrl+Fn Scribe in full-screen T3, including Scribe
acceptance; the user replied **“both work well.”** No further permission reset or
rebuild was performed. This establishes the reported core path, not a clean-state
permission matrix or a guaranteed OS dialog count. The user did not separately
quantify Keychain prompts in that reply.

Passive credential status now performs no OS reads and never waits behind an
existing prompt. Two fake-store regressions failed before the fix; cached denial
and missing results remain cached even with initial-access requests. Only selected
speech uses initial access; cleanup waits until Scribe or explicit Test. The new
permission-flow review found two pending-startup UI races; both are fixed with
explicit unchecked initialization, independently published statuses, generation
invalidation and current-provider setup. See the review response record.

One later source-only cosmetic commit (`be4242e`) keeps the Fn help paragraph in
one text flow instead of flex columns. Browser computed layout, typecheck and
build pass; it was intentionally not reinstalled, preserving the user's working
permissions. Keycap icons themselves are present in the installed candidate.

All native-affected commits through the permission changes and all substantive
frontend slices were checked from archived snapshots, including their own tests,
format/type/build/lint checks. Logs and exact commit IDs are in
`target/macos-evidence/commit-checks/`. Archive native checks exclude missing bundle
resources through `TAURI_CONFIG`; they do not substitute for the complete root
app build and native runs. Source publication does not publish a release installer
or change the qualified macOS preview support policy.

The generated local verifier is `.agents/skills/verify-quill/SKILL.md` (ignored
by repository convention). Its complete native feature-drive/cleanup validation
remains pending; it is not itself E2E proof. macOS support must remain qualified
until the native acceptance paths above pass.

## Remote CI follow-up

The first push passed frontend checks and macOS Rust tests, but Windows stopped
in Tauri's build-script feature validation. Cargo declared `macos-private-api`
globally while its matching config flag was only in the Mac override. The flag
now lives in shared config and the redundant target-only declaration is removed.
The pinned build checker validates these lists on every target; effective merged
Mac config is unchanged. The config regression failed before the move and passes
afterward. The installed candidate is unchanged. Remote rerun validates Windows;
this is separate from physical Windows desktop verification.
