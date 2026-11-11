//! Keep a managed engine from surviving a Quill crash on Windows.
use std::{io, os::windows::io::AsRawHandle, process::Child};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

pub(super) struct KillOnCloseJob(HANDLE);
// This owned kernel handle can be closed or used from any thread; it is never inherited.
unsafe impl Send for KillOnCloseJob {}
impl KillOnCloseJob {
    pub(super) fn new() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
    pub(super) fn assign(&self, child: &Child) -> io::Result<()> {
        if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
