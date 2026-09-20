//! The Win32 calls this crate makes, declared here and nowhere else.
//!
//! Every `unsafe` in this crate is in this file, the shape `dcs-eval`'s own
//! `sys` module takes: each symbol's ABI written out beside a safe wrapper,
//! and the rest of the crate calling only the wrappers. Why the installer's
//! calls are declared here rather than added to the client crate's set is
//! decision record 0019 — the client never makes them, and a library does
//! not declare what it does not call.
//!
//! There is no `#[cfg(windows)]` gate here, for the reason the client's
//! module gives: the toolchain file pins `x86_64-pc-windows-msvc`, Windows
//! is the only supported host and not merely the only target, and a gate
//! would buy a build on a platform where nothing this file serves could be
//! finished anyway.
//!
//! The two `#[link]` attributes are load-bearing. `SHGetKnownFolderPath` is
//! in `shell32` and `CoTaskMemFree` in `ole32`, neither of which the
//! standard library links by default — unlike the `kernel32` symbols the
//! client crate gets for free. Without them this is a link error rather
//! than a compile error, and it surfaces late.

use std::ffi::{OsString, c_void};
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

/// A Win32 `GUID`, laid out as the header lays it out. The layout is what
/// makes the pointer handed across the ABI mean what the shell expects.
#[repr(C)]
#[allow(non_snake_case)]
struct Guid {
    Data1: u32,
    Data2: u16,
    Data3: u16,
    Data4: [u8; 8],
}

/// `FOLDERID_SavedGames`, `{4C5C32FF-BB9D-43B0-B5B4-2D72E54EAAA4}`.
///
/// The known folder is asked for rather than `%USERPROFILE%\Saved Games`
/// being spelled out, because the folder can be relocated and a string
/// built from the profile then names a directory nothing is in.
/// A `static` rather than a `const`, because the shell is handed a pointer
/// to it: a `const` is materialised as a temporary at each use, and the
/// address of a temporary is not a thing to hand across an ABI.
static FOLDERID_SAVED_GAMES: Guid = Guid {
    Data1: 0x4C5C_32FF,
    Data2: 0xBB9D,
    Data3: 0x43B0,
    Data4: [0xB5, 0xB4, 0x2D, 0x72, 0xE5, 0x4E, 0xAA, 0xA4],
};

/// `FOLDERID_LocalAppData`, `{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}`.
///
/// Where this build keeps its own data: the install register and the copies
/// it parks. Asked of the shell rather than read out of `%LOCALAPPDATA%`,
/// for the reason above and for one more — a build that took one folder
/// from the shell and the other from an environment string would owe a
/// written reason for the difference, and there is no reason to owe.
static FOLDERID_LOCAL_APP_DATA: Guid = Guid {
    Data1: 0xF1B3_2785,
    Data2: 0x6FBA,
    Data3: 0x4FCF,
    Data4: [0x9D, 0x55, 0x7B, 0x8E, 0x7F, 0x15, 0x70, 0x91],
};

/// `S_OK`. The shell's answer is an `HRESULT`, and every other value is a
/// failure carrying its own reason.
const S_OK: i32 = 0;

// The names and the parameter spellings are Windows' own, so a reviewer can
// put the declaration beside the documentation and read one line against
// the other.
#[allow(non_snake_case)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetKnownFolderPath(
        rfid: *const Guid,
        dwFlags: u32,
        hToken: *mut c_void,
        ppszPath: *mut *mut u16,
    ) -> i32;
}

#[allow(non_snake_case)]
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoTaskMemFree(pv: *mut c_void);
}

/// Where the shell says this user's `Saved Games` is.
pub fn saved_games() -> io::Result<PathBuf> {
    known_folder(&FOLDERID_SAVED_GAMES, "Saved Games")
}

/// Where the shell says this user's local application data is.
pub fn local_app_data() -> io::Result<PathBuf> {
    known_folder(&FOLDERID_LOCAL_APP_DATA, "Local AppData")
}

/// Whichever known folder `rfid` names. `what` is the folder's name as the
/// refusal should print it, so a failure says which folder went missing
/// rather than only which HRESULT came back.
///
/// The flags are zero — the default behaviour, which is to answer about the
/// folder as it is registered without creating anything — and the token is
/// null, which is this process's own user. The path the shell allocates is
/// freed on both arms, because a failure that still wrote a pointer would
/// otherwise leak it.
fn known_folder(rfid: &'static Guid, what: &str) -> io::Result<PathBuf> {
    let mut wide: *mut u16 = core::ptr::null_mut();
    // SAFETY: `rfid` points at a constant that outlives the call, `hToken`
    // is the documented null for the current user, and `ppszPath` points at
    // a local the shell writes once. The pointer it writes is owned by this
    // function from here on and is freed below on either arm.
    let hr = unsafe { SHGetKnownFolderPath(rfid, 0, core::ptr::null_mut(), &mut wide) };
    let path = if hr == S_OK && !wide.is_null() {
        // SAFETY: on success the shell has written a NUL-terminated wide
        // string it allocated. The length is counted to that NUL and the
        // bytes are copied out before the buffer is freed.
        let len = unsafe { wide_len(wide) };
        // SAFETY: `wide` is valid for `len` `u16`s, counted just above.
        let slice = unsafe { core::slice::from_raw_parts(wide, len) };
        Some(PathBuf::from(OsString::from_wide(slice)))
    } else {
        None
    };
    if !wide.is_null() {
        // SAFETY: the pointer came from the shell's own allocator, which is
        // the one `CoTaskMemFree` releases to, and nothing holds it after
        // this: the path above is a copy.
        unsafe { CoTaskMemFree(wide.cast()) };
    }
    path.ok_or_else(|| {
        io::Error::other(format!(
            "the shell would not say where {what} is (HRESULT {hr:#010x})"
        ))
    })
}

/// How many `u16`s stand before the terminating NUL.
///
/// # Safety
///
/// `p` must be non-null and point at a NUL-terminated wide string.
unsafe fn wide_len(p: *const u16) -> usize {
    let mut n = 0usize;
    // SAFETY: the caller promises a NUL terminator, so the walk stops
    // inside the allocation.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    n
}
