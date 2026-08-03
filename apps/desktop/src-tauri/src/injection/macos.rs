use anyhow::{anyhow, Result};
use core_foundation::runloop::CFRunLoop;
use std::ffi::c_void;

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;
type CGKeyCode = u16;
const HID_EVENT_TAP: u32 = 0;
const KEY_COMMAND: CGKeyCode = 55;
const KEY_V: CGKeyCode = 9;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CFRelease(value: *const c_void);
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
    post(KEY_COMMAND, true)?;
    post(KEY_V, true)?;
    post(KEY_V, false)?;
    post(KEY_COMMAND, false)?;
    let _ = CFRunLoop::get_current();
    Ok(())
}
