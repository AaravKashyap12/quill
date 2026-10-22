//! Real, nonactivating AppKit panels for the three overlay webviews.
//!
//! tao 0.35.3 creates an NSWindow subclass; its `show()` calls
//! makeKeyAndOrderFront, and `set_focus()` also activates NSApplication.
//! Collection flags alone do not turn that window into a nonactivating panel.
//! Keep Tauri's window/webview/IPC ownership intact, but host its content view
//! in a separately allocated NSPanel. Never change a live object's class.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::mpsc,
};

use anyhow::{anyhow, ensure, Context};
use objc2::{define_class, msg_send, rc::Retained, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSEvent, NSPanel, NSResponder, NSScreen, NSWindow,
    NSWindowCollectionBehavior as Behavior, NSWindowStyleMask as Style,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use tauri::Manager;

const LABELS: [&str; 3] = ["overlay", "scribe-overlay", "scribe-review"];

#[derive(Default)]
struct PanelState {
    review: Cell<bool>,
}

define_class!(
    // SAFETY: This is a newly allocated NSPanel subclass with ordinary owned
    // ivars, not a replacement class for a Tauri allocation. The overrides
    // have NSWindow's documented signatures and only read main-thread state.
    #[unsafe(super = NSPanel)]
    #[name = "QuillOverlayPanel"]
    #[ivars = PanelState]
    struct OverlayPanel;

    impl OverlayPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            self.ivars().review.get()
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

struct HostedPanel {
    panel: Retained<OverlayPanel>,
    backing: Retained<NSWindow>,
    responder: Option<Retained<NSResponder>>,
}

// Retained AppKit objects never cross threads (no unsafe Send/Sync wrappers).
// Workers dispatch labels/booleans to the UI thread, which owns this store.
thread_local! {
    static PANELS: RefCell<HashMap<String, HostedPanel>> = RefCell::new(HashMap::new());
}

impl Drop for HostedPanel {
    fn drop(&mut self) {
        self.panel.orderOut(None);
        // Return the view before Tauri tears down its WKWebView and window.
        // Wry still owns the view and all IPC/protocol handlers throughout.
        let content = self.panel.contentView();
        self.panel.setContentView(None);
        self.backing.setContentView(content.as_deref());
        self.panel.close();
    }
}

/// Called by Tauri setup, after its hidden overlay windows and webviews exist.
pub(crate) fn configure(app: &tauri::App) -> anyhow::Result<()> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| anyhow!("macOS overlay setup requires the main thread"))?;
    let result = (|| {
        for label in LABELS {
            let window = app
                .get_webview_window(label)
                .with_context(|| format!("missing overlay window: {label}"))?;
            let pointer = window.ns_window()?.cast::<NSWindow>();
            // SAFETY: Tauri owns this live NSWindow; `window` stays in scope,
            // and every native operation here runs on the checked UI thread.
            let backing = unsafe { pointer.as_ref() }
                .with_context(|| format!("missing native overlay window: {label}"))?;
            ensure!(
                !backing.isVisible(),
                "overlay must be hidden before hosting: {label}"
            );
            let content = backing
                .contentView()
                .with_context(|| format!("missing overlay content view: {label}"))?;
            let responder = backing.firstResponder();
            let allocated = OverlayPanel::alloc(mtm).set_ivars(PanelState::default());
            // NonactivatingPanel must be present during NSPanel initialization.
            // Setting it later on an NSWindow does not establish this contract.
            let style = Style::NonactivatingPanel | (backing.styleMask() & Style::Resizable);
            // SAFETY: Designated NSPanel initializer, correct argument types,
            // newly allocated subclass, and retained ownership below.
            let panel: Retained<OverlayPanel> = unsafe {
                msg_send![super(allocated),
                    initWithContentRect: backing.frame(),
                    styleMask: style,
                    backing: NSBackingStoreType::Buffered,
                    defer: false,
                ]
            };
            // SAFETY: The retained panel, not AppKit's close operation, owns
            // its lifetime. Drop closes it before releasing that ownership.
            unsafe {
                panel.setReleasedWhenClosed(false);
            }
            panel.setTitle(&backing.title());
            panel.setOpaque(backing.isOpaque());
            panel.setBackgroundColor(Some(&backing.backgroundColor()));
            panel.setHasShadow(backing.hasShadow());
            panel.setLevel(backing.level());
            panel.setContentMinSize(backing.contentMinSize());
            panel.setContentMaxSize(backing.contentMaxSize());
            panel.setFloatingPanel(true);
            panel.setHidesOnDeactivate(false);
            panel.setBecomesKeyOnlyIfNeeded(true);
            panel.setIgnoresMouseEvents(true);
            let behavior = overlay_collection_behavior(
                backing.collectionBehavior(),
                objc2::available!(macos = 13.0),
            );
            panel.setCollectionBehavior(behavior);
            ensure!(
                panel.collectionBehavior() == behavior
                    && !panel.hidesOnDeactivate()
                    && panel.styleMask().contains(Style::NonactivatingPanel),
                "macOS panel configuration readback mismatch: {label}"
            );
            // Reparent the public content view, not Tao's internals or delegate.
            // Wry 0.55 owns this parent view separately and queries the webview's
            // containing window for native operations after it has moved.
            backing.makeFirstResponder(None);
            backing.setContentView(None);
            panel.setContentView(Some(&content));
            let hosted = HostedPanel {
                panel,
                backing: Retained::from(backing),
                responder,
            };
            tracing::info!(
                label,
                collection_behavior = behavior.bits(),
                style_mask = style.bits(),
                level = hosted.panel.level(),
                "created native nonactivating overlay panel; paint not yet verified"
            );
            PANELS.with(|panels| panels.borrow_mut().insert(label.to_owned(), hosted));
        }
        Ok(())
    })();
    if result.is_err() {
        teardown();
    }
    result
}

/// Dispatch only owned Rust data. Execute inline on the UI thread to avoid
/// blocking it while waiting for a closure queued to that very same thread.
fn on_main<T: Send + 'static>(
    window: &tauri::WebviewWindow,
    action: impl FnOnce(MainThreadMarker) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    if let Some(mtm) = MainThreadMarker::new() {
        return action(mtm);
    }
    let (tx, rx) = mpsc::sync_channel(1);
    window.run_on_main_thread(move || {
        let result = MainThreadMarker::new()
            .ok_or_else(|| anyhow!("overlay dispatch did not run on the main thread"))
            .and_then(action);
        let _ = tx.send(result);
    })?;
    rx.recv()
        .context("overlay UI dispatch ended before completing")?
}

pub(crate) fn show(window: &tauri::WebviewWindow, review: bool) -> anyhow::Result<()> {
    let label = window.label().to_owned();
    on_main(window, move |mtm| {
        PANELS.with(|panels| {
            let panels = panels.borrow();
            let hosted = panels
                .get(&label)
                .with_context(|| format!("native overlay panel unavailable: {label}"))?;
            let panel = &hosted.panel;
            position(panel, mtm);
            // A previous review may have been key. End that keyboard ownership
            // before displaying a new recording on the same Scribe surface.
            if !review && panel.isKeyWindow() {
                panel.orderOut(None);
                ensure!(
                    !panel.isKeyWindow(),
                    "review panel still owns keyboard focus"
                );
            }
            panel.ivars().review.set(review);
            panel.setIgnoresMouseEvents(!review);
            panel.orderFrontRegardless();
            if review {
                panel.makeFirstResponder(hosted.responder.as_deref());
                panel.makeKeyWindow();
            }
            // Diagnostic readback is useful for the root's native check, but
            // visibility/Space membership still do not prove pixels painted.
            tracing::info!(
                label,
                review,
                visible = panel.isVisible(),
                key = panel.isKeyWindow(),
                ignores_mouse_events = panel.ignoresMouseEvents(),
                backing_visible = hosted.backing.isVisible(),
                on_active_space = panel.isOnActiveSpace(),
                "ordered native overlay panel"
            );
            Ok(())
        })
    })
}

pub(crate) fn hide(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    let label = window.label().to_owned();
    on_main(window, move |_| {
        PANELS.with(|panels| {
            let panels = panels.borrow();
            let hosted = panels
                .get(&label)
                .with_context(|| format!("native overlay panel unavailable: {label}"))?;
            hosted.panel.orderOut(None);
            hosted.panel.ivars().review.set(false);
            hosted.panel.setIgnoresMouseEvents(true);
            Ok(())
        })
    })
}

/// Nonactivating panels can own keyboard focus while the original application
/// remains frontmost. Relinquish that ownership before restoring/inserting into
/// its editor; the existing insertion error path can explicitly refocus review.
pub(crate) fn release_focus(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    let label = window.label().to_owned();
    on_main(window, move |_| {
        PANELS.with(|panels| {
            let panels = panels.borrow();
            let hosted = panels
                .get(&label)
                .with_context(|| format!("native overlay panel unavailable: {label}"))?;
            hosted.panel.ivars().review.set(false);
            // Removing the panel from the window list asks AppKit to perform
            // the real focus transition. resignKeyWindow is a notification
            // override point, not an operation clients may invoke directly.
            // Insertion failure reopens the retained review through show().
            hosted.panel.orderOut(None);
            ensure!(
                !hosted.panel.isVisible() && !hosted.panel.isKeyWindow(),
                "review panel did not release visibility and keyboard focus"
            );
            Ok(())
        })
    })
}

fn position(panel: &OverlayPanel, mtm: MainThreadMarker) {
    let mouse = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    let screen = screens
        .iter()
        .find(|screen| contains_point(screen.frame(), mouse))
        .or_else(|| screens.iter().next());
    if let Some(screen) = screen {
        panel.setFrameOrigin(panel_origin(screen.visibleFrame(), panel.frame().size));
    }
}

fn contains_point(frame: NSRect, point: NSPoint) -> bool {
    point.x >= frame.origin.x
        && point.y >= frame.origin.y
        && point.x < frame.origin.x + frame.size.width
        && point.y < frame.origin.y + frame.size.height
}

fn panel_origin(area: NSRect, size: NSSize) -> NSPoint {
    // AppKit screen rectangles are in points with a bottom-left origin. No
    // physical-pixel conversion, main-screen height, or fixed Retina scale.
    NSPoint::new(
        area.origin.x + ((area.size.width - size.width) / 2.0).max(0.0),
        area.origin.y + 16.0_f64.min((area.size.height - size.height).max(0.0)),
    )
}

/// Called on the UI thread before Tauri releases a backing window or exits.
pub(crate) fn remove(label: &str) {
    if MainThreadMarker::new().is_some() {
        PANELS.with(|panels| {
            panels.borrow_mut().remove(label);
        });
    }
}

pub(crate) fn teardown() {
    if MainThreadMarker::new().is_some() {
        PANELS.with(|panels| panels.borrow_mut().clear());
    }
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
    fn panel_geometry_uses_appkit_points_and_visible_frame_insets() {
        // A screen to the left of and below the primary display, with a left
        // Dock and bottom inset. Retina scaling does not alter point geometry.
        let area = NSRect::new(NSPoint::new(-1380.0, -850.0), NSSize::new(1380.0, 830.0));
        assert_eq!(
            panel_origin(area, NSSize::new(480.0, 390.0)),
            NSPoint::new(-930.0, -834.0)
        );
        // Oversized windows remain anchored to the usable origin.
        assert_eq!(panel_origin(area, NSSize::new(1600.0, 900.0)), area.origin);
    }

    #[test]
    fn adjacent_screen_boundary_selects_exactly_one_screen() {
        let left = NSRect::new(NSPoint::new(-1440.0, 0.0), NSSize::new(1440.0, 900.0));
        let right = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0));
        let boundary = NSPoint::new(0.0, 450.0);
        assert!(!contains_point(left, boundary));
        assert!(contains_point(right, boundary));
        assert!(contains_point(left, NSPoint::new(-0.5, 450.0)));
    }

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
