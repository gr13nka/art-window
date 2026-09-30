//! One Art Window per login session, and a way to tell it to go.
//!
//! GNOME gets both from `GApplication`; Windows has neither, so each is a named
//! kernel object. The mutex says who is here, and the event is the doorbell `--quit`
//! rings. `Local\` scopes both to the session, so two users on one machine do not
//! find each other.

use anyhow::{bail, Context, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, RegisterWaitForSingleObject, SetEvent,
    UnregisterWaitEx, EVENT_MODIFY_STATE, INFINITE, WT_EXECUTEDEFAULT,
};

const MUTEX: PCWSTR = w!(r"Local\dev.artwindow");
const QUIT_EVENT: PCWSTR = w!(r"Local\dev.artwindow.quit");

type OnQuit = Box<dyn Fn() + Send + Sync>;

/// The claim on this session. Dropping it lets the next launch in.
pub struct Instance {
    mutex: HANDLE,
    quit: HANDLE,
    wait: HANDLE,
    /// Double-boxed so the pointer the thread pool holds is thin, and kept here so
    /// it outlives the wait that calls it.
    on_quit: *mut OnQuit,
}

// SAFETY: the handles are kernel objects usable from any thread, and `on_quit` is
// owned by this value alone and only ever called through a `Send + Sync` closure.
unsafe impl Send for Instance {}

pub fn claim(on_quit: impl Fn() + Send + Sync + 'static) -> Result<Option<Instance>> {
    // SAFETY: the name is a static wide string; the handle is closed on every path.
    let mutex = unsafe { CreateMutexW(None, false, MUTEX) }.context("claiming the session")?;
    // SAFETY: read straight after the call it describes, before anything else runs.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: the handle was just created and is not used again.
        let _ = unsafe { CloseHandle(mutex) };
        return Ok(None);
    }

    // Auto-reset, so each ring wakes the wait once and the next `--quit` is a new one.
    // SAFETY: the name is a static wide string.
    let quit = unsafe { CreateEventW(None, false, false, QUIT_EVENT) }
        .context("creating the quit signal")?;

    let on_quit = Box::into_raw(Box::new(Box::new(on_quit) as OnQuit));
    let mut wait = HANDLE::default();
    // Not `WT_EXECUTEONLYONCE`: the event is auto-reset and the wait stays armed, so
    // a first `--quit` that the loop ignored can be followed by a second.
    // SAFETY: `on_quit` stays allocated until `Drop` has unregistered the wait.
    let registered = unsafe {
        RegisterWaitForSingleObject(
            &mut wait,
            quit,
            Some(rung),
            Some(on_quit.cast_const().cast()),
            INFINITE,
            WT_EXECUTEDEFAULT,
        )
    };
    if let Err(error) = registered {
        // SAFETY: nothing else holds these yet.
        unsafe {
            drop(Box::from_raw(on_quit));
            let _ = CloseHandle(quit);
            let _ = CloseHandle(mutex);
        }
        return Err(error).context("listening for a quit request");
    }

    Ok(Some(Instance {
        mutex,
        quit,
        wait,
        on_quit,
    }))
}

unsafe extern "system" fn rung(context: *mut core::ffi::c_void, _timed_out: bool) {
    // SAFETY: `context` is the pointer `claim` registered, alive until unregistered.
    let on_quit = unsafe { &*context.cast::<OnQuit>() };
    on_quit();
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: waiting on `INVALID_HANDLE_VALUE` blocks until a callback in
        // flight has returned, so freeing the closure afterwards cannot race it.
        unsafe {
            let _ = UnregisterWaitEx(self.wait, Some(INVALID_HANDLE_VALUE));
            drop(Box::from_raw(self.on_quit));
            let _ = CloseHandle(self.quit);
            let _ = CloseHandle(self.mutex);
        }
    }
}

pub fn quit_running() -> Result<()> {
    // SAFETY: the name is a static wide string.
    let Ok(event) = (unsafe { OpenEventW(EVENT_MODIFY_STATE, false, QUIT_EVENT) }) else {
        bail!("Art Window is not running");
    };
    // SAFETY: the handle is ours until closed just below.
    let rung = unsafe { SetEvent(event) };
    // SAFETY: as above, and not used again.
    let _ = unsafe { CloseHandle(event) };
    rung.context("asking the running Art Window to quit")
}
