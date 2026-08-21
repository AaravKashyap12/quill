use super::EditorTextContext;
use anyhow::{anyhow, Result};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::runloop::CFRunLoop;
use core_foundation::string::{CFString, CFStringRef};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use std::ffi::c_void;
use std::time::Duration;

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
const KEY_COMMAND: CGKeyCode = 55;
const KEY_V: CGKeyCode = 9;

#[derive(Clone, Debug)]
pub struct InsertionTarget {
    process_id: i32,
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
    fn CFRelease(value: *const c_void);
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

pub fn request_accessibility_permission() -> bool {
    let prompt_key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options: CFDictionary<CFString, CFBoolean> =
        CFDictionary::from_CFType_pairs(&[(prompt_key, CFBoolean::true_value())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0 }
}

pub fn capture_target() -> Result<InsertionTarget> {
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
    Ok(InsertionTarget { process_id })
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
    let application = AXUIElementCreateApplication(target.process_id);
    if application.is_null() {
        return None;
    }
    let focused =
        copy_attribute(application, "AXFocusedUIElement").map(|value| value as AXUIElementRef);
    CFRelease(application);
    focused
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
    running_application(target).is_some_and(|application| application.isActive())
}

pub fn target_is_available(target: &InsertionTarget) -> bool {
    running_application(target).is_some_and(|application| !application.isTerminated())
}

pub fn activate_target(target: &InsertionTarget) -> Result<()> {
    ensure_accessibility_permission()?;
    let application = running_application(target)
        .ok_or_else(|| anyhow!("The application where dictation started is no longer running"))?;
    if !application.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows) {
        return Err(anyhow!(
            "macOS refused to restore the application where dictation started. Return to that app and try again."
        ));
    }

    // AppKit activation is asynchronous. Bound the wait and verify instead of
    // blindly pasting into whichever application still has focus.
    for _ in 0..20 {
        if application.isActive() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(anyhow!(
        "macOS did not restore the application where dictation started within 500 ms. No text was pasted."
    ))
}

fn ensure_accessibility_permission() -> Result<()> {
    if unsafe { AXIsProcessTrusted() != 0 } {
        return Ok(());
    }
    Err(anyhow!(
        "macOS Accessibility permission is required for Quill to type into other apps. Open System Settings → Privacy & Security → Accessibility, enable Quill, then try again."
    ))
}

fn post(key: CGKeyCode, key_down: bool) -> Result<()> {
    let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null_mut(), key, key_down) };
    if event.is_null() {
        return Err(anyhow!(
            "macOS Accessibility API could not create a keyboard event"
        ));
    }
    unsafe {
        CGEventPost(HID_EVENT_TAP, event);
        CFRelease(event);
    }
    Ok(())
}

pub fn paste() -> Result<()> {
    ensure_accessibility_permission()?;
    post(KEY_COMMAND, true)?;
    post(KEY_V, true)?;
    post(KEY_V, false)?;
    post(KEY_COMMAND, false)?;
    let _ = CFRunLoop::get_current();
    Ok(())
}
