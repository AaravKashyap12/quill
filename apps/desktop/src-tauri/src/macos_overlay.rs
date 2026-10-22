use anyhow::{anyhow, ensure, Context};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior as Behavior};
use tauri::Manager;

/// Called from Tauri setup, after its configured windows are created on the UI thread.
pub(crate) fn configure(app: &tauri::App) -> anyhow::Result<()> {
    let _main_thread = MainThreadMarker::new()
        .ok_or_else(|| anyhow!("macOS overlay setup requires the main thread"))?;
    for label in ["overlay", "scribe-overlay", "scribe-review"] {
        let window = app
            .get_webview_window(label)
            .with_context(|| format!("missing overlay window: {label}"))?;
        let pointer = window.ns_window()?.cast::<NSWindow>();
        // SAFETY: Tauri owns this live NSWindow; the handle stays in scope and
        // the main-thread check above precedes every AppKit call.
        let native = unsafe { pointer.as_ref() }
            .with_context(|| format!("missing native overlay window: {label}"))?;
        let behavior = overlay_collection_behavior(
            native.collectionBehavior(),
            objc2::available!(macos = 13.0),
        );
        native.setCollectionBehavior(behavior);
        native.setHidesOnDeactivate(false);

        // Readback proves configuration only; fullscreen painting needs a user-path check.
        let actual = native.collectionBehavior();
        let hides_on_deactivate = native.hidesOnDeactivate();
        tracing::info!(
            label,
            collection_behavior = actual.bits(),
            hides_on_deactivate,
            "configured macOS overlay Spaces behavior"
        );
        ensure!(
            actual == behavior && !hides_on_deactivate,
            "macOS overlay configuration readback mismatch: {label}"
        );
    }
    Ok(())
}

fn overlay_collection_behavior(mut current: Behavior, macos_13_or_newer: bool) -> Behavior {
    // AppKit permits only one role in each group. Clear modern roles even on
    // the legacy path so macOS 12 never receives unsupported role bits.
    current.remove(
        Behavior::MoveToActiveSpace
            | Behavior::FullScreenPrimary
            | Behavior::FullScreenNone
            | Behavior::Primary
            | Behavior::Auxiliary
            | Behavior::CanJoinAllApplications,
    );
    current.insert(Behavior::CanJoinAllSpaces | Behavior::FullScreenAuxiliary);
    if macos_13_or_newer {
        // Apple documents this role for floating overlays joining other apps' fullscreen Spaces.
        current.insert(Behavior::CanJoinAllApplications);
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_can_join_fullscreen_spaces_on_modern_macos() {
        assert_eq!(
            overlay_collection_behavior(Behavior::Default, true),
            Behavior::CanJoinAllSpaces
                | Behavior::FullScreenAuxiliary
                | Behavior::CanJoinAllApplications
        );
    }

    #[test]
    fn replaces_conflicting_space_and_fullscreen_roles() {
        for app_role in [Behavior::Primary, Behavior::Auxiliary] {
            for fullscreen_role in [Behavior::FullScreenPrimary, Behavior::FullScreenNone] {
                let current = app_role | fullscreen_role | Behavior::MoveToActiveSpace;
                let result = overlay_collection_behavior(current, true);
                assert!(!result.intersects(current));
                assert!(result.contains(
                    Behavior::CanJoinAllSpaces
                        | Behavior::FullScreenAuxiliary
                        | Behavior::CanJoinAllApplications
                ));
            }
        }
    }

    #[test]
    fn macos_12_uses_only_legacy_space_roles() {
        let expected = Behavior::CanJoinAllSpaces | Behavior::FullScreenAuxiliary;
        assert_eq!(
            overlay_collection_behavior(Behavior::Default, false),
            expected
        );
        let modern_roles =
            Behavior::Primary | Behavior::Auxiliary | Behavior::CanJoinAllApplications;
        assert_eq!(overlay_collection_behavior(modern_roles, false), expected);
    }

    #[test]
    fn preserves_unrelated_flags_and_is_idempotent() {
        let unrelated = Behavior::Transient
            | Behavior::IgnoresCycle
            | Behavior::FullScreenDisallowsTiling
            | Behavior::from_bits_retain(1 << 30);
        for modern in [false, true] {
            let result = overlay_collection_behavior(unrelated, modern);
            assert!(result.contains(unrelated));
            assert_eq!(overlay_collection_behavior(result, modern), result);
        }
    }
}
