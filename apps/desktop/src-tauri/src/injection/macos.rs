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

fn running_application(
    target: &InsertionTarget,
) -> Option<objc2::rc::Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(target.process_id)
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
