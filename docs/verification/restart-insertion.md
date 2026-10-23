# Focused editor discovery after a Mac restart

On 2026-10-23, the unchanged installed executable from the previous verification
still reported Accessibility and input access. T3 and Chrome failed before audio
capture, while Hermes worked. Quill's log showed target capture failing and the
speech engine restarting repeatedly during a held shortcut.

A content-free native probe found `AXFocusedWindow` but received
`kAXErrorNoValue` for `AXFocusedUIElement` in T3 and Chrome. Reading the application
role and enabling T3's `AXManualAccessibility` exposed its accessibility tree.
The user then dictated into T3 using the unchanged Quill executable. Chrome's
tree was subsequently available too; the user confirmed insertion in both the
plain textarea and contenteditable fields of a disposable local page. Chrome
inspection also enables accessibility, so that observation alone does not prove
which initialization call enabled Chrome.

Quill now initializes accessibility when a foreground app has a window but no
focused element. It uses Electron's documented `AXManualAccessibility`, with
Chromium's `AXEnhancedUserInterface` fallback. It reads back field availability
with a bounded retry; it does not interpret a setter return as readiness. Chrome
can debounce initialization for two seconds. Already available editors keep the
immediate path.

If no original field could be identified at activation, preparation never starts
recording or chooses a replacement destination. The pill asks for a fresh shortcut
press. Known noneditable/password fields and field-metadata failures terminate
the attempt. App/window changes also stop preparation. Exact original-field
checks on later insertion and review acceptance remain in place.

Startup errors now stay inside the running session engine. They report through
the selected overlay and settings status, release session activity, and suppress
another attempt from the same held shortcut. A new press can try again.

## Independent review

AStack revision `f88d9200f7ee08c9195b0063303442757f1a6f96` guided this Tier 2 repair.
Graphify was unavailable after reboot; source searches traced all callers instead.
The exact fresh-context review brief is [restart-insertion-review-brief.txt](restart-insertion-review-brief.txt).

The reviewer found delayed same-window retargeting and a missing test of startup
error handling. Both were addressed before installation. Discovery no longer
accepts any field found during initialization; a separate press captures it.
Known-field errors do not enter preparation. A regression models field A's failed
metadata followed by field B in the same window. Reintroducing metadata retries
fails that regression. The session-start boundary now has an injected-error test
covering reporting, activity release, held-key suppression and a subsequent
successful start. Removing the handler's suppression makes that test fail.
The lead reviewed these corrections; no second reviewer round was run.

## Evidence and limits

Ignored artifacts are under `target/macos-evidence/restart-insertion/`: native
probe source/results, pre-fix regressions, mutation checks, Rust/frontend checks,
the independent review, and local build/signature verification. No keys, audio,
clipboard contents or arbitrary editor text are included in the probe output.
The native probe did not change macOS permission decisions.

The native app matrix and installed repair results are recorded below as they
are verified. Automated discovery fixtures do not establish every editor's AX
implementation, atomic focus-to-paste delivery, Intel support or Windows desktop
behavior. This does not change the qualified macOS preview support policy.

The installed repair's executable SHA-256 is
`d7d7a113d82b272e8d305083ded543fd661051be4277ee43bea6e828c87b0d97`.
It was installed once after review corrections and passed strict bundle-signature
verification. The changed ad-hoc executable initially reported no Accessibility
access. After the user re-approved this copy, it quit through its own menu and
restarted unchanged as PID 14378 at 05:31:48 UTC. Both Accessibility and input
monitoring reported true. The prior bundle is retained in ignored evidence.

Combined local checks passed: 196 Rust tests with one existing ignored live-model
test, 73 Vitest tests and nine Python tests, typecheck, lint, frontend build,
formatting, Clippy with warnings denied, and a complete signed local app build.
The final discovery-only adjustment was followed by its focused suite, Clippy
and another local build before installation. No bundle-resource override was
needed; the existing speech sidecars were available.

Sources: [Electron accessibility](https://www.electronjs.org/docs/latest/tutorial/accessibility),
[Chromium accessibility design](https://www.chromium.org/developers/design-documents/accessibility/),
and [Chrome 155.0.8059.85 AppKit implementation](https://github.com/chromium/chromium/blob/155.0.8059.85/chrome/browser/chrome_browser_application_mac.mm).

After that unchanged restart, the user quit and reopened Chrome and confirmed
Fn Dictation inserted into its search bar. A cold website input, repaired-build
Scribe acceptance and a post-repair T3 run were not confirmed in that response;
the user then requested Tap support. The earlier plain/rich-page and T3 successes
belong to the prior installed build after accessibility initialization, not to
this repaired executable's cold-start matrix.

## Focused commit checks

The session recovery and editor-discovery slices were checked independently from
exported Git index snapshots, with the existing sidecars supplied as build inputs.
No user settings or vault data were copied into those snapshots.

- `c051701`: session-start recovery; 190 Rust tests passed, one existing test
  ignored, all-target Clippy passed.
- `d3b425e`: cold editor preparation; 196 Rust tests passed, one existing test
  ignored, formatting and all-target Clippy passed. This unpublished commit was
  amended only to correct formatting before its final snapshot was checked.
- `e8441e3`: separate Tap request; 205 Rust tests passed, one existing test ignored,
  75 Vitest and nine Python tests passed, plus typecheck, lint, frontend build,
  formatting and all-target Clippy. Its tracked working tree matched the index.

The tested tree IDs and command logs remain under the ignored evidence directory.
None of these checks establishes Windows desktop behavior or hardware coverage.

The combined Tap build was subsequently tested in a normal Chrome text field:
the user confirmed both Dictation and accepted Scribe inserted automatically
with the pill visible. See [mac-tap.md](mac-tap.md) for the unchanged executable
hash and approval/restart evidence. This confirms the warm-editor path in the
combined repair; it does not turn the earlier search-bar test into cold website
accessibility-discovery proof.
