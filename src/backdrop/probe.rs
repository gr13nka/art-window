//! The two ways a meeting is told to be live, one per kind of app.
//!
//! A desktop client has a process that exists for exactly as long as a call does;
//! a browser has no such thing, and the only trace a call leaves outside it is the
//! camera being in use. Both are cheap enough to ask twice a second.

/// The process that exists while, and only while, a zoom.us meeting is live.
#[cfg(target_os = "macos")]
const MEETING_PROCESS: &str = "CptHost";

/// Whether a meeting is live: a process named exactly `CptHost` exists.
#[cfg(target_os = "macos")]
pub fn meeting_process() -> bool {
    // SAFETY: both calls write only within the buffers handed to them, whose sizes
    // are passed alongside, and the count returned is clamped to what was allocated.
    unsafe {
        let wanted = libc::proc_listallpids(std::ptr::null_mut(), 0);
        if wanted <= 0 {
            return false;
        }
        // Room to spare, because processes start between the two calls.
        let mut pids = vec![0 as libc::pid_t; wanted as usize + 256];
        let bytes = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
        let found = libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes);
        let found = (found.max(0) as usize).min(pids.len());
        let mut name = [0u8; 64];
        pids[..found].iter().any(|&pid| {
            let len = libc::proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32);
            len > 0 && &name[..len as usize] == MEETING_PROCESS.as_bytes()
        })
    }
}

#[cfg(not(target_os = "macos"))]
pub fn meeting_process() -> bool {
    false
}

#[cfg(target_os = "macos")]
mod cmio {
    //! The few CoreMediaIO declarations needed to ask whether a camera is running,
    //! written out by hand because nothing else in the tree uses the framework. The
    //! four-character constants are the framework's own, as numbers.

    use std::ffi::c_void;

    #[repr(C)]
    pub struct PropertyAddress {
        pub selector: u32,
        pub scope: u32,
        pub element: u32,
    }

    /// `kCMIOObjectSystemObject`.
    pub const SYSTEM_OBJECT: u32 = 1;
    /// `kCMIOHardwarePropertyDevices`, `'dev#'`.
    pub const DEVICES: u32 = 0x6465_7623;
    /// `kCMIODevicePropertyDeviceIsRunningSomewhere`, `'gone'`.
    pub const RUNNING_SOMEWHERE: u32 = 0x676F_6E65;
    /// `kCMIOObjectPropertyScopeGlobal`, `'glob'`.
    pub const SCOPE_GLOBAL: u32 = 0x676C_6F62;
    /// `kCMIOObjectPropertyElementMain`.
    pub const ELEMENT_MAIN: u32 = 0;

    #[link(name = "CoreMediaIO", kind = "framework")]
    extern "C" {
        pub fn CMIOObjectGetPropertyDataSize(
            object: u32,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        pub fn CMIOObjectGetPropertyData(
            object: u32,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: u32,
            used: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }
}

/// Whether any camera, system-wide, is being used by anything at all.
///
/// Reading these properties is not capturing, so it needs no camera permission and
/// does not light the camera. Every failure reads as no.
#[cfg(target_os = "macos")]
pub fn camera_in_use() -> bool {
    use cmio::*;
    use std::ffi::c_void;
    let address = |selector| PropertyAddress {
        selector,
        scope: SCOPE_GLOBAL,
        element: ELEMENT_MAIN,
    };
    // SAFETY: every call is handed an address that lives through it and a buffer
    // whose byte size is passed alongside; the device list is sized by the
    // framework's own answer and only the whole `u32`s it filled are read back.
    unsafe {
        let devices = address(DEVICES);
        let mut bytes = 0u32;
        if CMIOObjectGetPropertyDataSize(SYSTEM_OBJECT, &devices, 0, std::ptr::null(), &mut bytes)
            != 0
            || bytes == 0
        {
            return false;
        }
        let mut ids = vec![0u32; bytes as usize / std::mem::size_of::<u32>()];
        let mut used = 0u32;
        let capacity = (ids.len() * std::mem::size_of::<u32>()) as u32;
        if CMIOObjectGetPropertyData(
            SYSTEM_OBJECT,
            &devices,
            0,
            std::ptr::null(),
            capacity,
            &mut used,
            ids.as_mut_ptr().cast::<c_void>(),
        ) != 0
        {
            return false;
        }
        let filled = (used as usize / std::mem::size_of::<u32>()).min(ids.len());
        let running = address(RUNNING_SOMEWHERE);
        ids[..filled].iter().any(|&device| {
            let mut value = 0u32;
            let mut got = 0u32;
            CMIOObjectGetPropertyData(
                device,
                &running,
                0,
                std::ptr::null(),
                std::mem::size_of::<u32>() as u32,
                &mut got,
                (&mut value as *mut u32).cast::<c_void>(),
            ) == 0
                && value != 0
        })
    }
}

#[cfg(not(target_os = "macos"))]
pub fn camera_in_use() -> bool {
    false
}
