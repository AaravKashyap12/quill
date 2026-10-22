# macOS review responses

Base: `0a63b355598bac4d9aef2970f7211a1092464966`, macOS repair series.
These are source and regression-test resolutions, not native acceptance results.
See [the verification record](../macos-verification.md) for hardware evidence.

## First Tier 2 review

**PID-only Scribe restoration:** retained AX window and editable-control handles
now identify the original target. Restoration verifies both identities before
paste; failures retain review/recovery. The macOS review command runs off the UI
thread so Quill can answer its own AX requests. Fake AX tests cover different
windows, different fields, unavailable targets and ineffective restoration.
Real WebKit restoration remains a native acceptance item.

## Expanded-scope Tier 2 review

**Toggle presses lost during ASR waits:** the Mac producer retains a bounded
rising-edge count, preserving nonzero state and toggle parity on overflow. The
session consumes that count; Windows retains its existing edge fallback. Tests
run the real sampler into `KeyRuntime`, including a complete stop tap during a
consumer delay and 2/3/256/257 taps with stale held state.

**Stale dismissal hides another mode:** native dismissal now uses Tauri's
injected caller window and affects only an overlay surface. Every new surface
state cancels its obsolete timers; Escape's timer also participates in cleanup.
Two-surface timer/event regressions exercise recording and review transitions.
Their native-boundary mock is not proof of desktop dispatch or compositing.

The second review found no further must-fix issue in cancellation ordering or
the AX restoration implementation. Root reviewed these fixes; no additional
independent review of those fixes is claimed. Exact reviewer prompts are retained alongside
this file; full local reports are in ignored `target/macos-evidence/`.

## Credential and signing scope review

New credential caching and local signing received one focused security review.
It found no additional must-fix cache/concurrency or signing defect, but identified
an introduced cloud recovery routing error: General offered Retry download for
Groq/Gemini startup failures. FirstRunSetup now receives the selected provider
and routes cloud recovery to Voice connection controls. Both cloud interaction
regressions failed before the fix and pass afterward; the local retry test still
invokes the download callback. These are React boundary tests, not native UI proof.
Report: `target/macos-evidence/tier2-credentials-review.md`.

## Native panel and Fn integration review

The focused review found one unsupported AppKit call: the new panel code
called `resignKeyWindow` directly even though it is a notification override.
Root replaced both calls with `orderOut`, letting AppKit perform the actual
focus transition. Review acceptance verifies that the panel is hidden and no
longer key before restoring the exact AX editor and pasting. Existing insertion
errors retain the draft and reopen review. Recording transitions also verify
release before reopening a non-key panel. The full 185-test Rust library suite
and Clippy passed after correction; native keyboard routing remains an
acceptance item. No additional concrete Fn or panel finding was reported.

The report and exact review brief are retained in
`target/macos-evidence/tier2-panels-fn-review.md`. Root self-reviewed the fix;
this does not claim a second independent review or full-screen paint proof.

## Passive permission-flow review

The new scope received a fresh security review. It found two P2 UI races while
selected-provider Keychain access was pending: empty provider status appeared
missing, and a late cloud startup result could overwrite newly selected Local
setup. Both are fixed. Both providers begin unchecked; status results publish
independently. Provider/model changes rebind setup to the current selection and
invalidate the old initialization generation. Settings remain editable during
OS consent. Deferred configured/error outcomes are covered by component-boundary
regressions; disabling change invalidation reproduces both failures.

The review found no consent bypass or key exposure in the changed native paths.
Accessibility startup/status checks are passive, the request command rejects
non-main windows, and only Enable requests OS consent. Passive cached credential
status uses a nonblocking lock; selected access cannot retry cached denial.
Native prompt behavior remains a separate acceptance check. Root self-reviewed
the fixes; no second independent review is claimed. Exact brief and findings:
`target/macos-evidence/tier2-passive-permissions-review.md`.
