use anyhow::{Context, Result};

/// Sets the process to the lowest schedulable priority on Windows.
#[cfg(windows)]
pub fn set_lowest_priority(pid: u32) -> Result<()> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        IDLE_PRIORITY_CLASS, OpenProcess, PROCESS_SET_INFORMATION, SetPriorityClass,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            anyhow::bail!("OpenProcess failed for engine pid {pid}");
        }

        let ok = SetPriorityClass(handle, IDLE_PRIORITY_CLASS);
        CloseHandle(handle);

        if ok == 0 {
            anyhow::bail!("SetPriorityClass(IDLE) failed for engine pid {pid}");
        }
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn set_lowest_priority(_pid: u32) -> Result<()> {
    Ok(())
}

pub fn set_lowest_priority_for_child(pid: Option<u32>) -> Result<()> {
    let pid = pid.context("engine child has no process id")?;
    set_lowest_priority(pid)
}
