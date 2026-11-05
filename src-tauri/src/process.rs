//! Safe child-process helpers: no shell, no console window, enforced timeouts, and on Windows a
//! job object so children die with the app even if it crashes.

use crate::error::{AppError, AppResult};
use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub fn hide_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Run to completion, killing the process if it outlives `timeout` (a wedged USB link can hang adb).
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> AppResult<Output> {
    hide_window(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AppError::AdbUnavailable(e.to_string()))?;

    // Drain on separate threads: a full pipe would otherwise deadlock the child.
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Adb("adb did not respond in time.".into()));
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    Ok(Output {
        status,
        stdout: stdout.and_then(|h| h.join().ok()).unwrap_or_default(),
        stderr: stderr.and_then(|h| h.join().ok()).unwrap_or_default(),
    })
}

fn drain<R: Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        buf
    })
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    struct Job(HANDLE);
    // A job handle is a process-wide kernel object and is safe to use from any thread.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    fn job() -> Option<&'static Job> {
        JOB.get_or_init(|| unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            (ok != 0).then_some(Job(handle))
        })
        .as_ref()
    }

    pub fn adopt(child: &Child) {
        if let Some(job) = job() {
            unsafe {
                AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE);
            }
        }
    }
}

/// Tie a long-running child's lifetime to this process. The job handle is held until exit, so the
/// OS kills the child if the app dies. On other platforms the child exits when its stdout pipe and
/// the device sockets close.
pub fn adopt(child: &Child) {
    #[cfg(windows)]
    job::adopt(child);
    #[cfg(not(windows))]
    let _ = child;
}
