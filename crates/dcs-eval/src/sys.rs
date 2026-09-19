//! The Win32 calls this crate makes, declared here and nowhere else.
//!
//! Every `unsafe` in the crate is in this file, so a reviewer can audit the
//! lot in one reading: each symbol's ABI is written out beside a safe
//! wrapper, and the rest of the crate calls only the wrappers. Why the
//! crate declares these rather than linking a binding crate is decision
//! record 0011.
//!
//! There is no `#[cfg(windows)]` gate anywhere in here, and that is
//! deliberate rather than an oversight: the toolchain file pins
//! `x86_64-pc-windows-msvc`, Windows is the only supported host and not
//! merely the only target, and a gate would buy a build on a platform
//! where every done-condition past the wire needs a running DCS. A
//! contributor on another host reads this paragraph rather than finding
//! out at a link error.
//!
//! Liveness is the one call whose obvious form is wrong.
//! `GetExitCodeProcess` reports `STILL_ACTIVE` — 259 — for a process that
//! exited with code 259, and so calls a dead executor alive. A process
//! object is signalled when the process ends, so a zero-millisecond wait
//! on it answers the question without an exit code being involved at all:
//! it times out while the process runs and returns at once once it has
//! gone.

use std::io;

/// A Win32 `HANDLE`.
type Handle = *mut core::ffi::c_void;

/// The one right a zero-millisecond wait on a process object needs.
/// Asking for more is asking to be refused by a process this one does not
/// own.
const SYNCHRONIZE: u32 = 0x0010_0000;

const WAIT_OBJECT_0: u32 = 0x0000_0000;
const WAIT_TIMEOUT: u32 = 0x0000_0102;
const WAIT_FAILED: u32 = 0xFFFF_FFFF;

/// What Windows says when no process has the id asked for, as against the
/// access refusals, which mean the id is somebody's.
const ERROR_INVALID_PARAMETER: i32 = 87;

// The names and the parameter spellings are Windows' own, kept as the
// header writes them so a reviewer can put the declaration beside the
// documentation and read one line against the other; renaming them to
// Rust's casing would make that comparison a translation exercise.
#[allow(non_snake_case)]
unsafe extern "system" {
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> Handle;
    fn WaitForSingleObject(hHandle: Handle, dwMilliseconds: u32) -> u32;
    fn CloseHandle(hObject: Handle) -> i32;
}

/// An open handle that closes itself. The one owning wrapper, so no path
/// out of a probe can forget the close.
struct Process(Handle);

impl Process {
    /// A process opened for the wait alone, or what the OS said about why
    /// it would not open.
    fn open(pid: u32) -> Result<Self, io::Error> {
        // SAFETY: a call with no pointer arguments. The handle returned is
        // null on failure, which is what is checked, and is owned from
        // here on by the `Process` that closes it.
        let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    /// Whether the process object is still unsignalled, which is to say
    /// whether the process is still running.
    fn waited(&self) -> u32 {
        // SAFETY: `self.0` is non-null and open until `Drop` runs, and a
        // zero-millisecond wait returns without blocking.
        unsafe { WaitForSingleObject(self.0, 0) }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from `OpenProcess`, is closed once, and
        // is not used again.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// What a probe of one process id could establish.
#[derive(Debug)]
pub enum Liveness {
    /// The wait timed out: the process object is not signalled, so it
    /// runs.
    Running,
    /// Signalled, or no process with that id at all.
    Exited,
    /// The probe could not decide — an access refusal among the reasons.
    /// Never read as `Exited`: only a probe that positively says the
    /// process is gone may produce a verdict that says a request will
    /// never run.
    Unknown { why: io::Error },
}

/// Whether the process with `pid` is still running.
///
/// A handle that will not open is not an answer by itself. Windows
/// answers `ERROR_INVALID_PARAMETER` where no process has the id, which is
/// the one refusal that means gone; every other refusal, access denied
/// among them, means the id is somebody's and this probe may not ask.
pub fn liveness(pid: u32) -> Liveness {
    let process = match Process::open(pid) {
        Ok(process) => process,
        Err(why) if why.raw_os_error() == Some(ERROR_INVALID_PARAMETER) => return Liveness::Exited,
        Err(why) => return Liveness::Unknown { why },
    };
    match process.waited() {
        WAIT_TIMEOUT => Liveness::Running,
        WAIT_OBJECT_0 => Liveness::Exited,
        WAIT_FAILED => Liveness::Unknown {
            why: io::Error::last_os_error(),
        },
        saw => Liveness::Unknown {
            why: io::Error::other(format!("the wait returned {saw}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::a_pid_that_has_exited;

    #[test]
    fn waitforsingleobject_times_out_on_this_process_so_it_is_running() {
        let verdict = liveness(std::process::id());
        assert!(
            matches!(verdict, Liveness::Running),
            "this process is running, and the probe saw {verdict:?}"
        );
    }

    #[test]
    fn waitforsingleobject_signals_a_child_that_exited_with_code_259() {
        // The case the obvious call gets wrong: 259 is `STILL_ACTIVE`, so
        // a `GetExitCodeProcess` probe would call this child alive. The
        // `Child` is held across the probe, which keeps the id from being
        // recycled under the test and so keeps a failure a failure rather
        // than a flake.
        let (_child, pid) = a_pid_that_has_exited();
        let verdict = liveness(pid);
        assert!(
            matches!(verdict, Liveness::Exited),
            "a reaped child is gone, and the probe saw {verdict:?}"
        );
    }

    #[test]
    fn waitforsingleobject_is_not_reached_when_the_process_will_not_open() {
        // Pid 4 is the System process. Unelevated the open is refused and
        // the verdict is `Unknown`; elevated it may open and read
        // `Running`. Either is right; what must never happen is `Exited`,
        // which would say a live session's request will never run.
        let verdict = liveness(4);
        assert!(
            !matches!(verdict, Liveness::Exited),
            "a handle that would not open is not evidence of an exit: {verdict:?}"
        );
    }
}
