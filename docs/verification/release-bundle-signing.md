# Complete ad-hoc signatures for Mac releases

The first v0.4.3 draft built both platform artifacts, but the downloaded universal
Mac app failed `codesign --verify --deep --strict`: the bundle was not signed.
Its main executable carried a linker-generated ad-hoc signature without bound
Info.plist or sealed resources. The Tauri updater signature on the archive was
valid; updater authenticity does not establish a complete OS bundle signature.
The workflow was cancelled before publication and v0.4.3 remained a draft.

Release automation now sets `APPLE_SIGNING_IDENTITY=-` on macOS before
`tauri-action` builds the application, DMG, and updater archive. Tauri signs the
helper, executable and app bundle before creating the signed updater artifact.
A subsequent strict all-architectures bundle check must pass before the final
publish job can run. Windows receives no signing identity and retains its
existing packaging path. No updater key, certificate, permission or app-runtime
policy changed.

The pinned local Tauri CLI built with that environment variable and no custom
post-build signing script. Its complete app passed strict bundle verification.
This local check omitted updater artifacts because it used no private signing
key. CI and the downloaded release archive must establish the universal build
and updater checks separately. No candidate replaced the user's installed app.

This follows [Tauri's ad-hoc signing guidance](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing).
Ad-hoc signing does not supply Apple Developer ID trust or notarization, prevent
OS trust warnings, or preserve approvals across every changed executable.
Ignored build, signature and release evidence is under
`target/macos-evidence/release-0.4.3/`.

A fresh AStack Tier 2 review found no must-fix or should-fix issue in the ten-line
workflow patch. It checked platform isolation, pinned Tauri signing order,
universal bundle path and the publish dependency gate. The first reviewer
launch failed at model capacity before reviewing; the second completed the same
scope. Static review did not substitute for the downloaded-artifact check.
