use super::EditorTextContext;
use anyhow::{anyhow, Result};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use std::ffi::c_void;
use std::time::{Duration, Instant};

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;
type CGKeyCode = u16;
type AXUIElementRef = *mut c_void;
type AXValueRef = *const c_void;
type CFTypeRef = *const c_void;
type CFTypeID = usize;
type AXError = i32;
const AX_VALUE_CF_RANGE_TYPE: i32 = 4;
const AX_ERROR_SUCCESS: AXError = 0;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeCFRange {
    location: isize,
    length: isize,
}
const HID_EVENT_TAP: u32 = 0;
const COMMAND_FLAG: u64 = 1 << 20;
const KEY_V: CGKeyCode = 9;

#[derive(Clone, Debug)]
pub struct InsertionTarget {
    process_id: i32,
    focus: EditorFocus<AxElement>,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CFRelease(value: *const c_void);
    fn CFRetain(value: *const c_void) -> *const c_void;
    fn CFEqual(left: CFTypeRef, right: CFTypeRef) -> u8;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    fn CFGetTypeID(value: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn AXValueGetType(value: AXValueRef) -> i32;
    fn AXValueGetValue(value: AXValueRef, value_type: i32, value_ptr: *mut c_void) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
}

pub fn accessibility_permission_granted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

pub fn request_accessibility_permission() -> bool {
    let prompt_key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options: CFDictionary<CFString, CFBoolean> =
        CFDictionary::from_CFType_pairs(&[(prompt_key, CFBoolean::true_value())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0 }
}

pub fn capture_target() -> Result<InsertionTarget> {
    ensure_accessibility_permission()?;
    let application = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .ok_or_else(|| {
            anyhow!("macOS did not report a frontmost application for text insertion")
        })?;
    let process_id = application.processIdentifier();
    if process_id <= 0 {
        return Err(anyhow!(
            "macOS reported an invalid process identifier for the frontmost application"
        ));
    }
    let ax = unsafe { AxElement::from_owned(AXUIElementCreateApplication(process_id)) }?;
    let focus = discover_editor(&mut NativeDiscovery {
        application: &application,
        ax,
    })?;
    if !application.isActive() {
        return Err(anyhow!(
            "The focused application changed. Focus your text field and try again."
        ));
    }
    Ok(InsertionTarget { process_id, focus })
}

unsafe fn copy_attribute(element: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    (AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
        == AX_ERROR_SUCCESS
        && !value.is_null())
    .then_some(value)
}

unsafe fn copy_string_attribute(element: AXUIElementRef, name: &str) -> Option<String> {
    let value = copy_attribute(element, name)?;
    if CFGetTypeID(value) != CFStringGetTypeID() {
        CFRelease(value);
        return None;
    }
    Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
}

unsafe fn focused_element(target: &InsertionTarget) -> Option<AXUIElementRef> {
    CFRetain(target.focus.editor.0);
    Some(target.focus.editor.0)
}

unsafe fn selected_range(element: AXUIElementRef) -> Option<NativeCFRange> {
    let value = copy_attribute(element, "AXSelectedTextRange")?;
    let mut range = NativeCFRange::default();
    let valid = AXValueGetType(value as AXValueRef) == AX_VALUE_CF_RANGE_TYPE
        && AXValueGetValue(
            value as AXValueRef,
            AX_VALUE_CF_RANGE_TYPE,
            &mut range as *mut NativeCFRange as *mut c_void,
        );
    CFRelease(value);
    valid.then_some(range)
}

fn utf16_index_to_byte(value: &str, utf16_index: usize) -> usize {
    let mut units = 0usize;
    for (byte_index, character) in value.char_indices() {
        if units >= utf16_index {
            return byte_index;
        }
        units += character.len_utf16();
    }
    value.len()
}

fn bounded_before(value: &str, caret_byte: usize, max_chars: usize) -> Option<String> {
    let prefix = &value[..caret_byte.min(value.len())];
    let start = prefix
        .char_indices()
        .rev()
        .nth(max_chars.saturating_sub(1))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let result = prefix[start..].to_owned();
    (!result.trim().is_empty()).then_some(result)
}

fn bounded_after(value: &str, caret_byte: usize, max_chars: usize) -> Option<String> {
    let suffix = &value[caret_byte.min(value.len())..];
    let end = suffix
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(suffix.len());
    let result = suffix[..end].to_owned();
    (!result.trim().is_empty()).then_some(result)
}

pub fn capture_editor_context(
    target: &InsertionTarget,
    max_context_chars: i32,
) -> Result<Option<EditorTextContext>> {
    if !target_is_foreground(target) {
        return Ok(None);
    }
    let Some(element) = (unsafe { focused_element(target) }) else {
        return Ok(None);
    };
    let result = unsafe {
        let role = copy_string_attribute(element, "AXRole").unwrap_or_default();
        if role.contains("Secure") {
            CFRelease(element);
            return Ok(None);
        }
        let selected_text = copy_string_attribute(element, "AXSelectedText")
            .filter(|value| !value.trim().is_empty());
        let full_value = copy_string_attribute(element, "AXValue");
        let range = selected_range(element);
        CFRelease(element);

        let max_chars = max_context_chars.max(1) as usize;
        let (surrounding_before, surrounding_after) = match (full_value, range) {
            (Some(value), Some(range)) if range.location >= 0 && range.length >= 0 => {
                let start = utf16_index_to_byte(&value, range.location as usize);
                let end = utf16_index_to_byte(
                    &value,
                    range.location.saturating_add(range.length) as usize,
                );
                (
                    bounded_before(&value, start, max_chars),
                    bounded_after(&value, end, max_chars),
                )
            }
            _ => (None, None),
        };
        EditorTextContext {
            selected_text,
            surrounding_before,
            surrounding_after,
        }
    };
    Ok(Some(result))
}

pub fn selected_text_matches(target: &InsertionTarget, expected: &str) -> Result<bool> {
    activate_target(target)?;
    Ok(
        capture_editor_context(target, expected.chars().count().max(1) as i32)?
            .and_then(|context| context.selected_text)
            .is_some_and(|selection| selection == expected),
    )
}

fn running_application(
    target: &InsertionTarget,
) -> Option<objc2::rc::Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(target.process_id)
}

pub fn bundle_identifier(target: &InsertionTarget) -> Option<String> {
    running_application(target)
        .and_then(|application| application.bundleIdentifier())
        .map(|identifier| identifier.to_string())
}

pub fn target_is_foreground(target: &InsertionTarget) -> bool {
    NativeFocus(target).current().ok().flatten().as_ref() == Some(&target.focus)
}

pub fn target_is_available(target: &InsertionTarget) -> bool {
    NativeFocus(target).validate(&target.focus).is_ok()
}

pub fn activate_target(target: &InsertionTarget) -> Result<()> {
    ensure_accessibility_permission()?;
    restore_focus(&target.focus, &mut NativeFocus(target))
}

// AX objects are retained immutable proxies; UI access happens through AX IPC,
// not by dereferencing an AppKit view on the calling thread.
#[derive(Debug)]
struct AxElement(AXUIElementRef);
unsafe impl Send for AxElement {}
unsafe impl Sync for AxElement {}

impl Clone for AxElement {
    fn clone(&self) -> Self {
        unsafe { CFRetain(self.0) };
        Self(self.0)
    }
}

impl Drop for AxElement {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

impl PartialEq for AxElement {
    fn eq(&self, other: &Self) -> bool {
        unsafe { CFEqual(self.0, other.0) != 0 }
    }
}
impl Eq for AxElement {}

impl AxElement {
    unsafe fn from_owned(value: AXUIElementRef) -> Result<Self> {
        if value.is_null() {
            return Err(anyhow!("macOS could not access the original editor."));
        }
        let element = Self(value);
        if AXUIElementSetMessagingTimeout(value, 0.25) != AX_ERROR_SUCCESS {
            return Err(anyhow!("macOS could not access the original editor."));
        }
        Ok(element)
    }

    fn element_attribute(&self, name: &str) -> Result<Self> {
        self.optional_element_attribute(name)?.ok_or_else(|| anyhow!("The application did not expose {name} (AX error -25212). Focus an editable field and try again."))
    }

    fn optional_element_attribute(&self, name: &str) -> Result<Option<Self>> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let status = unsafe {
            AXUIElementCopyAttributeValue(self.0, attribute.as_concrete_TypeRef(), &mut value)
        };
        if status != AX_ERROR_SUCCESS || value.is_null() {
            if !value.is_null() {
                unsafe { CFRelease(value) };
            }
            if status == -25212 {
                return Ok(None);
            }
            return Err(anyhow!("The application did not expose the focused window or text field ({name}, AX error {status}). Focus an editable field and try again."));
        }
        unsafe {
            if CFGetTypeID(value) != AXUIElementGetTypeID() {
                CFRelease(value);
                return Err(anyhow!(
                    "The application did not expose an accessible text field."
                ));
            }
            Self::from_owned(value as AXUIElementRef).map(Some)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EditorFocus<E> {
    window: E,
    editor: E,
}

trait EditorDiscovery<E> {
    fn active(&self) -> bool;
    fn window(&mut self) -> Result<E>;
    fn editor(&mut self, window: &E) -> Result<Option<E>>;
    fn enable(&mut self);
    fn wait(&mut self);
}

fn discover_editor<E: Eq>(operations: &mut impl EditorDiscovery<E>) -> Result<EditorFocus<E>> {
    let changed = || {
        anyhow!("The focused application or window changed. Focus your text field and try again.")
    };
    if !operations.active() {
        return Err(changed());
    }
    let window = operations.window()?;
    if let Some(editor) = operations.editor(&window)? {
        return Ok(EditorFocus { window, editor });
    }
    operations.enable();
    // Chrome debounces EnhancedUserInterface by two seconds. Never re-enable
    // inside the poll: that would keep postponing its initialization.
    let deadline = Instant::now() + Duration::from_secs(3);
    for _ in 0..60 {
        if !operations.active() || operations.window()? != window {
            return Err(changed());
        }
        // No trustworthy field was captured at activation. Preparation
        // may expose a different field, so only a fresh press can bind it.
        if matches!(operations.editor(&window), Ok(Some(_))) {
            return Err(anyhow!("Editor is ready — press your shortcut again."));
        }
        if Instant::now() >= deadline {
            break;
        }
        operations.wait();
    }
    Err(anyhow!("The application has not exposed its focused text field. Focus an editable field and press your shortcut again."))
}

struct NativeDiscovery<'a> {
    application: &'a NSRunningApplication,
    ax: AxElement,
}

impl EditorDiscovery<AxElement> for NativeDiscovery<'_> {
    fn active(&self) -> bool {
        self.application.isActive()
    }

    fn window(&mut self) -> Result<AxElement> {
        self.ax.element_attribute("AXFocusedWindow")
    }

    fn editor(&mut self, window: &AxElement) -> Result<Option<AxElement>> {
        let Some(editor) = self.ax.optional_element_attribute("AXFocusedUIElement")? else {
            return Ok(None);
        };
        if editor.element_attribute("AXWindow")? != *window {
            return Err(anyhow!(
                "The focused editor changed. Focus your text field and try again."
            ));
        }
        let role = unsafe { copy_string_attribute(editor.0, "AXRole") }.unwrap_or_default();
        let subrole = unsafe { copy_string_attribute(editor.0, "AXSubrole") }.unwrap_or_default();
        if !editable_role(&role, &subrole) {
            return Err(anyhow!(
                "Focus an accessible, non-password text field before starting Quill."
            ));
        }
        Ok(Some(editor))
    }

    fn enable(&mut self) {
        // Reading the application role initializes Chromium's native AX support.
        let _ = unsafe { copy_string_attribute(self.ax.0, "AXRole") };
        let manual = CFString::new("AXManualAccessibility");
        let enabled = CFBoolean::true_value();
        let manual_result = unsafe {
            AXUIElementSetAttributeValue(
                self.ax.0,
                manual.as_concrete_TypeRef(),
                enabled.as_CFTypeRef(),
            )
        };
        // Electron exposes ManualAccessibility; Chrome uses EnhancedUserInterface.
        let enhanced_result = if manual_result != AX_ERROR_SUCCESS {
            let enhanced = CFString::new("AXEnhancedUserInterface");
            Some(unsafe {
                AXUIElementSetAttributeValue(
                    self.ax.0,
                    enhanced.as_concrete_TypeRef(),
                    enabled.as_CFTypeRef(),
                )
            })
        } else {
            None
        };
        // Chrome can return NotImplemented after handling EnhancedUserInterface;
        // only successful focused-field readback establishes readiness.
        tracing::info!(
            process_id = self.application.processIdentifier(),
            manual_result,
            ?enhanced_result,
            "requested editor accessibility initialization"
        );
    }

    fn wait(&mut self) {
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn editable_role(role: &str, subrole: &str) -> bool {
    matches!(role, "AXTextField" | "AXTextArea" | "AXComboBox") && !subrole.contains("Secure")
}

fn focused_editor(process_id: i32) -> Result<EditorFocus<AxElement>> {
    let application = unsafe { AxElement::from_owned(AXUIElementCreateApplication(process_id)) }?;
    let window = application.element_attribute("AXFocusedWindow")?;
    let editor = application.element_attribute("AXFocusedUIElement")?;
    if editor.element_attribute("AXWindow")? != window {
        return Err(anyhow!(
            "The focused editor changed. Focus the original text field and try again."
        ));
    }
    Ok(EditorFocus { window, editor })
}

trait FocusOperations<E> {
    fn validate(&mut self, target: &EditorFocus<E>) -> Result<()>;
    fn current(&mut self) -> Result<Option<EditorFocus<E>>>;
    fn activate(&mut self) -> Result<()>;
    fn raise(&mut self, window: &E) -> Result<()>;
    fn focus(&mut self, editor: &E) -> Result<()>;
    fn wait(&mut self);
}

fn restore_focus<E: Eq>(
    target: &EditorFocus<E>,
    operations: &mut impl FocusOperations<E>,
) -> Result<()> {
    operations.validate(target)?;
    if operations.current().ok().flatten().as_ref() == Some(target) {
        return Ok(());
    }
    operations.activate()?;
    operations.raise(&target.window)?;
    operations.focus(&target.editor)?;
    // AX setters and app activation can complete before keyboard focus moves.
    let deadline = Instant::now() + Duration::from_millis(500);
    for _ in 0..20 {
        if operations.current()?.as_ref() == Some(target) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        operations.wait();
    }
    Err(anyhow!("macOS could not verify focus in the original window and text field. No text was pasted. Focus that field and try again."))
}

struct NativeFocus<'a>(&'a InsertionTarget);

impl FocusOperations<AxElement> for NativeFocus<'_> {
    fn validate(&mut self, target: &EditorFocus<AxElement>) -> Result<()> {
        if !running_application(self.0).is_some_and(|app| !app.isTerminated())
            || target.editor.element_attribute("AXWindow")? != target.window
            || unsafe { copy_string_attribute(target.window.0, "AXRole") }.as_deref()
                != Some("AXWindow")
        {
            return Err(anyhow!(
                "The original window or text field is no longer available. Quill kept your text."
            ));
        }
        Ok(())
    }

    fn current(&mut self) -> Result<Option<EditorFocus<AxElement>>> {
        if !running_application(self.0).is_some_and(|app| app.isActive()) {
            return Ok(None);
        }
        focused_editor(self.0.process_id).map(Some)
    }

    fn activate(&mut self) -> Result<()> {
        let application = running_application(self.0).ok_or_else(|| {
            anyhow!("The application where dictation started is no longer running")
        })?;
        if !application.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows) {
            return Err(anyhow!("macOS refused to restore the original application. Return to that app and try again."));
        }
        Ok(())
    }

    fn raise(&mut self, window: &AxElement) -> Result<()> {
        let action = CFString::new("AXRaise");
        if unsafe { AXUIElementPerformAction(window.0, action.as_concrete_TypeRef()) }
            != AX_ERROR_SUCCESS
        {
            return Err(anyhow!("macOS could not restore the original window. Quill kept your text; open that window and try again."));
        }
        Ok(())
    }

    fn focus(&mut self, editor: &AxElement) -> Result<()> {
        let attribute = CFString::new("AXFocused");
        let focused = CFBoolean::true_value();
        if unsafe {
            AXUIElementSetAttributeValue(
                editor.0,
                attribute.as_concrete_TypeRef(),
                focused.as_CFTypeRef(),
            )
        } != AX_ERROR_SUCCESS
        {
            return Err(anyhow!("macOS could not restore the original text field. Quill kept your text; focus that field and try again."));
        }
        Ok(())
    }

    fn wait(&mut self) {
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn ensure_accessibility_permission() -> Result<()> {
    if unsafe { AXIsProcessTrusted() != 0 } {
        return Ok(());
    }
    Err(anyhow!(
        "macOS Accessibility permission is required for Quill to type into other apps. Open System Settings → Privacy & Security → Accessibility, enable Quill, then try again."
    ))
}

fn paste_event(key_down: bool) -> Result<CGEventRef> {
    let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null_mut(), KEY_V, key_down) };
    if event.is_null() {
        return Err(anyhow!(
            "macOS Accessibility API could not create a keyboard event"
        ));
    }
    // Each event carries its own modifiers; posting a Command key first does
    // not reliably update a subsequently created event's flags.
    unsafe { CGEventSetFlags(event, COMMAND_FLAG) };
    Ok(event)
}

pub fn paste() -> Result<()> {
    ensure_accessibility_permission()?;
    let down = paste_event(true)?;
    let up = match paste_event(false) {
        Ok(event) => event,
        Err(error) => {
            unsafe { CFRelease(down) };
            return Err(error);
        }
    };
    unsafe {
        CGEventPost(HID_EVENT_TAP, down);
        CGEventPost(HID_EVENT_TAP, up);
        CFRelease(down);
        CFRelease(up);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ColdEditor {
        enabled: usize,
        waits: usize,
        ready_after: usize,
        active: bool,
        changed_window: bool,
        acceptable: bool,
    }

    impl ColdEditor {
        fn new() -> Self {
            Self {
                enabled: 0,
                waits: 0,
                ready_after: 2,
                active: true,
                changed_window: false,
                acceptable: true,
            }
        }
    }

    impl EditorDiscovery<u8> for ColdEditor {
        fn active(&self) -> bool {
            self.active
        }
        fn window(&mut self) -> Result<u8> {
            Ok(if self.enabled > 0 && self.changed_window {
                2
            } else {
                1
            })
        }
        fn editor(&mut self, _: &u8) -> Result<Option<u8>> {
            if !self.acceptable {
                return if self.enabled == 0 {
                    Err(anyhow!("field A metadata invalid or noneditable"))
                } else {
                    Ok(Some(20)) // Field B in the same window after initialization.
                };
            }
            if self.ready_after == 0 || self.enabled > 0 && self.waits >= self.ready_after {
                Ok(Some(10))
            } else {
                Ok(None)
            }
        }
        fn enable(&mut self) {
            self.enabled += 1;
        }
        fn wait(&mut self) {
            self.waits += 1;
        }
    }

    #[test]
    fn cold_editor_prepares_once_but_requires_a_fresh_capture() {
        let mut app = ColdEditor::new();
        assert!(discover_editor(&mut app)
            .unwrap_err()
            .to_string()
            .contains("Editor is ready"));
        assert_eq!(app.enabled, 1);
        assert_eq!(app.waits, 2);
        assert_eq!(
            discover_editor(&mut app).unwrap(),
            EditorFocus {
                window: 1,
                editor: 10
            }
        );
        assert_eq!(app.enabled, 1);
        assert_eq!(app.waits, 2);
    }

    #[test]
    fn warm_editor_needs_no_initialization_or_wait() {
        let mut app = ColdEditor::new();
        app.ready_after = 0;
        assert!(discover_editor(&mut app).is_ok());
        assert_eq!((app.enabled, app.waits), (0, 0));
    }

    #[test]
    fn discovery_never_captures_a_changed_window_or_background_app() {
        for changed_window in [false, true] {
            let mut app = ColdEditor::new();
            app.changed_window = changed_window;
            app.active = changed_window;
            assert!(discover_editor(&mut app).is_err());
            assert_eq!(app.waits, 0);
        }
    }

    #[test]
    fn unavailable_editor_has_bounded_discovery() {
        let mut app = ColdEditor::new();
        app.ready_after = usize::MAX;
        assert!(discover_editor(&mut app).is_err());
        assert_eq!(app.enabled, 1);
        assert!(app.waits <= 60);
    }

    #[test]
    fn known_invalid_field_never_waits_to_accept_a_different_field() {
        let mut app = ColdEditor::new();
        app.acceptable = false;
        assert!(discover_editor(&mut app).is_err());
        assert_eq!((app.enabled, app.waits), (0, 0));
    }

    #[test]
    fn discovery_accepts_editors_but_never_passwords_or_web_areas() {
        for role in ["AXTextField", "AXTextArea", "AXComboBox"] {
            assert!(editable_role(role, ""));
            assert!(!editable_role(role, "AXSecureTextField"));
        }
        for role in ["AXWebArea", "AXGroup", "AXButton", ""] {
            assert!(!editable_role(role, ""));
        }
    }

    struct FakeFocus {
        current: EditorFocus<u32>,
        active: bool,
        closed: bool,
        unreadable: bool,
        restore_fails: bool,
        focus_fails: bool,
        restore_has_effect: bool,
        restored: bool,
        pasted: Vec<EditorFocus<u32>>,
        recovery_retained: bool,
    }

    impl FakeFocus {
        fn different_window() -> Self {
            Self {
                current: EditorFocus {
                    window: 2,
                    editor: 20,
                },
                active: true,
                closed: false,
                unreadable: false,
                restore_fails: false,
                focus_fails: false,
                restore_has_effect: true,
                restored: false,
                pasted: Vec::new(),
                recovery_retained: true,
            }
        }

        fn accept(&mut self, target: &EditorFocus<u32>) -> Result<()> {
            // Same success gate as review acceptance: an error must not paste
            // or permit the caller to discard its recovery record.
            restore_focus(target, self)?;
            self.pasted.push(self.current.clone());
            self.recovery_retained = false;
            Ok(())
        }
    }

    impl FocusOperations<u32> for FakeFocus {
        fn validate(&mut self, _target: &EditorFocus<u32>) -> Result<()> {
            if self.closed {
                Err(anyhow!("target closed"))
            } else {
                Ok(())
            }
        }
        fn current(&mut self) -> Result<Option<EditorFocus<u32>>> {
            if self.unreadable {
                return Err(anyhow!("AX inaccessible"));
            }
            Ok(self.active.then(|| self.current.clone()))
        }
        fn activate(&mut self) -> Result<()> {
            // Activating a process does not change its frontmost window/control.
            self.active = true;
            Ok(())
        }
        fn raise(&mut self, window: &u32) -> Result<()> {
            if self.restore_fails {
                return Err(anyhow!("AX raise failed"));
            }
            if self.restore_has_effect {
                self.current.window = *window;
            }
            self.restored = true;
            Ok(())
        }
        fn focus(&mut self, editor: &u32) -> Result<()> {
            if self.focus_fails {
                return Err(anyhow!("AX focus failed"));
            }
            if self.restore_has_effect {
                self.current.editor = *editor;
            }
            Ok(())
        }
        fn wait(&mut self) {}
    }

    #[test]
    fn acceptance_restores_the_exact_window_and_field_even_in_the_same_process() {
        let target = EditorFocus {
            window: 1,
            editor: 10,
        };
        for (active, window) in [(true, 2), (false, 2), (true, 1)] {
            let mut ops = FakeFocus::different_window();
            ops.active = active;
            ops.current.window = window;
            ops.accept(&target).unwrap();
            assert!(ops.restored);
            assert_eq!(ops.pasted, vec![target.clone()]);
            assert!(!ops.recovery_retained);
        }
    }

    #[test]
    fn closed_inaccessible_or_unrestored_target_never_pastes_or_reports_success() {
        let target = EditorFocus {
            window: 1,
            editor: 10,
        };
        for failure in 0..5 {
            let mut ops = FakeFocus::different_window();
            match failure {
                0 => ops.closed = true,
                1 => ops.unreadable = true,
                2 => ops.restore_fails = true,
                3 => ops.focus_fails = true,
                _ => ops.restore_has_effect = false,
            }
            assert!(ops.accept(&target).is_err(), "failure case {failure}");
            assert!(ops.pasted.is_empty());
            assert!(ops.recovery_retained);
        }
    }

    #[test]
    fn already_focused_field_does_not_require_a_restore_action() {
        let mut ops = FakeFocus::different_window();
        let target = ops.current.clone();
        ops.restore_fails = true;
        ops.accept(&target).unwrap();
        assert!(!ops.restored);
        assert_eq!(ops.pasted, vec![target]);
    }

    extern "C" {
        fn CGEventGetFlags(event: CGEventRef) -> u64;
    }

    #[test]
    fn paste_key_events_include_command_without_posting_input() {
        for down in [true, false] {
            let event = paste_event(down).unwrap();
            let flags = unsafe { CGEventGetFlags(event) };
            unsafe { CFRelease(event) };
            assert_eq!(
                flags,
                1 << 20,
                "paste requires Command, without held shortcut modifiers"
            );
        }
    }
}
