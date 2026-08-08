use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProfile {
    total_memory_bytes: u64,
    available_memory_bytes: u64,
    logical_cpu_count: usize,
    platform: &'static str,
    architecture: &'static str,
    speech_acceleration: &'static str,
}

#[tauri::command]
pub fn get_system_profile() -> SystemProfile {
    let (total_memory_bytes, available_memory_bytes) = memory_bytes();

    SystemProfile {
        total_memory_bytes,
        available_memory_bytes,
        logical_cpu_count: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        speech_acceleration: speech_acceleration(),
    }
}

fn speech_acceleration() -> &'static str {
    if cfg!(target_os = "macos") {
        "metal"
    } else {
        // Quill has no DirectML or Intel GPU backend. Windows uses CUDA only
        // after the optional runtime pack is installed, which happens after
        // onboarding. A first-run recommendation must therefore assume CPU.
        "cpu"
    }
}

#[cfg(windows)]
fn memory_bytes() -> (u64, u64) {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
        (status.ullTotalPhys, status.ullAvailPhys)
    } else {
        (0, 0)
    }
}

#[cfg(target_os = "macos")]
fn memory_bytes() -> (u64, u64) {
    use std::ffi::{c_char, c_int, c_void};

    extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            old_value: *mut c_void,
            old_length: *mut usize,
            new_value: *mut c_void,
            new_length: usize,
        ) -> c_int;
    }

    let mut total = 0_u64;
    let mut length = std::mem::size_of::<u64>();
    let name = b"hw.memsize\0";
    let result = unsafe {
        sysctlbyname(
            name.as_ptr().cast(),
            (&mut total as *mut u64).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result == 0 {
        // macOS does not expose an equivalent single-call "available" value.
        // Half of physical RAM is a deliberate conservative setup budget.
        (total, total / 2)
    } else {
        (0, 0)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn memory_bytes() -> (u64, u64) {
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_has_actionable_values() {
        let profile = get_system_profile();
        #[cfg(any(windows, target_os = "macos"))]
        assert!(profile.total_memory_bytes > 0);
        assert!(profile.logical_cpu_count > 0);
        assert!(!profile.platform.is_empty());
        assert!(!profile.architecture.is_empty());
        assert!(matches!(profile.speech_acceleration, "cpu" | "metal"));
    }
}
