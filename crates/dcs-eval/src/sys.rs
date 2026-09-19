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

use core::ptr::NonNull;
use std::cell::Cell;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

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
///
/// A process id is not a name, and this is the limit of what the answer is
/// worth. Windows issues the number again once the process holding it has
/// gone, so a probe of an id whose process exited a while ago may open
/// whatever unrelated process now holds it and answer `Running` about that
/// one — confidently, with nothing in the verdict to say it is about a
/// different process than the caller meant. The id is only as good as
/// whatever else bounds how long ago it was issued: held across a restart
/// of the thing that issued it, it is a question this call cannot answer
/// and does not know it cannot.
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

// ---- the reply watch -------------------------------------------------------

/// The `OVERLAPPED` a pending directory read is tracked by. The two
/// offset fields are the union's plain half, unused here because a
/// directory read has no file position; they are written as two `u32`
/// because that is the shape the kernel reads, and zeroing them is part
/// of arming a request.
#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    h_event: Handle,
}

/// The access `ReadDirectoryChangesW` needs on the directory it watches.
const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;

const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const OPEN_EXISTING: u32 = 3;

/// Without this a directory will not open at all: `CreateFileW` opens
/// files, and this is the flag that makes it open the other thing.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

/// Without this the read is synchronous, and the failure is the quiet
/// kind: `ReadDirectoryChangesW` would block inside the call until a
/// change arrived, the deadline would go unread, and a wait that should
/// answer `pending` would hang instead. Everything below about a pending
/// read and a cancellation presupposes it.
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;

const FILE_NOTIFY_CHANGE_FILE_NAME: u32 = 0x0000_0001;
const FILE_NOTIFY_CHANGE_LAST_WRITE: u32 = 0x0000_0010;

/// What `CreateFileW` returns on failure. It is not null, so a null check
/// takes it for a live handle.
const INVALID_HANDLE_VALUE: Handle = usize::MAX as Handle;

/// The overlapped call's ordinary "accepted, not finished".
const ERROR_IO_PENDING: i32 = 997;
/// The completion is not ready yet, which is an answer and not a failure.
const ERROR_IO_INCOMPLETE: i32 = 996;
/// What `CancelIoEx` says when it found nothing to cancel. It is the one
/// failure of that call the drain can carry on from: the completion is
/// already posted, which is what the cancellation was for.
const ERROR_NOT_FOUND: i32 = 1168;

/// The longest the drain waits for a cancelled read to come back. An
/// accepted cancellation completes in microseconds, so this is not a
/// duration anything is expected to spend — it is the bound that keeps a
/// `Drop` from becoming a process that stops.
const DRAIN_MS: u32 = 5_000;

/// What the drain records where the completion simply never arrived. It
/// is negative so that it cannot be read as a Windows error code, because
/// no call failed: the kernel still has the buffer and said nothing about
/// why.
const DRAIN_UNFINISHED: i32 = -1;
/// What a cancelled read completes with, and the thing the drain
/// records. Named for the checks that assert on it, which are the only
/// place the number is compared against anything: the drain itself
/// records whatever it was told without reading it.
#[cfg(test)]
pub(crate) const ERROR_OPERATION_ABORTED: i32 = 995;

#[allow(non_snake_case)]
unsafe extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut core::ffi::c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: Handle,
    ) -> Handle;
    fn CreateEventW(
        lpEventAttributes: *mut core::ffi::c_void,
        bManualReset: i32,
        bInitialState: i32,
        lpName: *const u16,
    ) -> Handle;
    fn ReadDirectoryChangesW(
        hDirectory: Handle,
        lpBuffer: *mut core::ffi::c_void,
        nBufferLength: u32,
        bWatchSubtree: i32,
        dwNotifyFilter: u32,
        lpBytesReturned: *mut u32,
        lpOverlapped: *mut Overlapped,
        lpCompletionRoutine: Option<unsafe extern "system" fn(u32, u32, *mut Overlapped)>,
    ) -> i32;
    fn GetOverlappedResult(
        hFile: Handle,
        lpOverlapped: *mut Overlapped,
        lpNumberOfBytesTransferred: *mut u32,
        bWait: i32,
    ) -> i32;
    fn CancelIoEx(hFile: Handle, lpOverlapped: *mut Overlapped) -> i32;
}

/// An open handle that closes itself, whatever the call that made it was.
/// It exists so that a constructor which has opened one thing and then
/// failed at the next releases the first without a line of its own.
struct Owned(Handle);

impl Owned {
    /// A handle from a call that says failure with a null — `CreateEventW`
    /// among them.
    fn from_nullable(h: Handle) -> io::Result<Self> {
        if h.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(h))
        }
    }

    /// A handle from a call that says failure with `INVALID_HANDLE_VALUE`,
    /// which is `CreateFileW`'s way and is not null. There are two
    /// constructors because one null check over both would read a refused
    /// `CreateFileW` as a live handle, arm a read against it, and then
    /// close `(HANDLE)-1`.
    fn from_file(h: Handle) -> io::Result<Self> {
        if h == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(h))
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle came from a call that said it succeeded, is
        // closed once, and is not used again.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// How much the kernel may write in one notification. The contents are
/// never read, so the size is only about how much churn a directory may
/// take before the kernel gives up and reports an overflow — which reads
/// here as an ordinary wake anyway.
const BUFFER: usize = 4096;

/// The memory the kernel writes into while a read is pending.
///
/// It is heap-allocated by the `Changes` that owns it, and that is the
/// single reason it is a type of its own: the kernel is handed an
/// address, and a `Changes` moved between the arm and the completion must
/// not take that address with it. A heap allocation stays where it is
/// however often its owner moves.
#[repr(C, align(8))]
struct Pending {
    buffer: [u8; BUFFER],
    overlapped: Overlapped,
}

/// What ended a wait on the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Woke {
    /// The directory said something happened. What it said is never read;
    /// the caller's next act is to list the directory.
    Event,
    /// The nap was spent and the directory said nothing.
    Timeout,
    /// There was nothing to wait on, or the wait itself failed. The nap
    /// was slept either way, so a caller treats this exactly as a poll.
    Deaf,
}

/// A watch on one directory, and the one pending read it may have armed.
///
/// The contents of the buffer are never read. A notification says only
/// "something happened here", and the answer to that is to list the
/// directory, which the caller would do anyway — so there is no
/// `FILE_NOTIFY_INFORMATION` walk to get wrong, and the kernel's own
/// buffer overflow, which completes with zero bytes transferred, reads as
/// an ordinary wake.
///
/// The type is neither `Send` nor `Sync`, by virtue of its `Rc`, and that
/// is wanted: one thread arms a read, waits on it and drains it.
pub(crate) struct Changes {
    dir: Owned,
    event: Owned,
    /// The buffer and the `OVERLAPPED`, held as a bare pointer and never
    /// as a `Box`.
    ///
    /// A heap allocation is what keeps the address stable, but a `Box` is
    /// more than a heap allocation: it asserts that nothing else refers
    /// to what it points at. While a read is outstanding that assertion
    /// is false — the kernel holds a pointer into this very allocation —
    /// and every fresh dereference of the `Box` would be a fresh claim
    /// that it does not. The pointer is derived once, here, and every
    /// address handed to a Win32 call is derived from it rather than
    /// through a reference. `Drop` is what gives the allocation back,
    /// and only once the drain says the kernel has let go of it.
    state: NonNull<Pending>,
    /// Exactly one completion is outstanding, or posted and unconsumed.
    /// Set only by a successful arm; cleared only where a completion has
    /// been consumed, drained or given up on.
    armed: bool,
    /// Whether the drain gave up before it could establish that the
    /// kernel had let go of the buffer. Sticky once set, and the one
    /// thing that stops `Drop` freeing the allocation.
    stranded: bool,
    /// What the drain answered. It is shared rather than returned because
    /// the cancellation runs in `Drop`, which has no caller to return to,
    /// and a drain that ran is the only evidence this design can produce:
    /// the corruption it prevents is invisible to a test.
    drained: Rc<Cell<Option<i32>>>,
}

impl Changes {
    /// A watch on `dir`, opened and unarmed. The first arm is left to the
    /// caller so that it happens once the value is where it will live.
    pub(crate) fn open(dir: &Path) -> io::Result<Self> {
        let wide = wide(dir)?;
        // SAFETY: `wide` is NUL-terminated and outlives the call, the
        // security-attributes and template arguments are the documented
        // nulls, and the handle is owned from here on by an `Owned`
        // checked against `CreateFileW`'s own failure value.
        //
        // Delete sharing is deliberately not granted. A held directory
        // refusing its own removal is what the executor's sibling sweep
        // reads as "a client is still watching this session", and it is
        // what every check here that no handle outlives a wait rests on.
        // Granting it was tried, and on this Windows the removal then
        // went through and the refusal stopped happening at all, so the
        // exclusion is load-bearing rather than belt-and-braces. Read
        // and write sharing stay, so the executor's writes into the
        // directory are never blocked.
        let dir_handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                core::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                core::ptr::null_mut(),
            )
        };
        let dir = Owned::from_file(dir_handle)?;
        // SAFETY: a call with three null arguments. The event is
        // auto-reset and starts unsignalled, so the wait that consumes a
        // signal resets it and no `ResetEvent` is wanted anywhere.
        let event = Owned::from_nullable(unsafe {
            CreateEventW(core::ptr::null_mut(), 0, 0, core::ptr::null())
        })?;
        let state = Box::into_raw(Box::new(Pending {
            buffer: [0; BUFFER],
            overlapped: Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                h_event: event.0,
            },
        }));
        let state = NonNull::new(state).expect("a box's address is never null");
        Ok(Self {
            dir,
            event,
            state,
            armed: false,
            stranded: false,
            drained: Rc::new(Cell::new(None)),
        })
    }

    /// Where the drain writes what it saw. Cloned out by a caller that
    /// wants the answer after the `Changes` is gone, which is the only
    /// time there is one.
    pub(crate) fn sink(&self) -> Rc<Cell<Option<i32>>> {
        Rc::clone(&self.drained)
    }

    /// Whether a read is outstanding.
    pub(crate) fn armed(&self) -> bool {
        self.armed
    }

    /// The address of the `OVERLAPPED` the kernel is given, and the only
    /// way anything here names it.
    fn overlapped(&self) -> *mut Overlapped {
        // SAFETY: the allocation is live from `open` until `Drop` gives
        // it back, and the offset is taken without forming a reference to
        // the whole of it, so nothing here claims the kernel is not also
        // holding a pointer into it.
        unsafe { &raw mut (*self.state.as_ptr()).overlapped }
    }

    /// The address of the buffer the kernel writes notifications into.
    fn buffer(&self) -> *mut u8 {
        // SAFETY: as `overlapped`.
        unsafe { (&raw mut (*self.state.as_ptr()).buffer).cast::<u8>() }
    }

    /// Ask the directory to report its next change.
    ///
    /// Not recursive: the reply directory has no subdirectories, and a
    /// subtree walk would be work for notifications nothing here reads.
    pub(crate) fn arm(&mut self) -> io::Result<()> {
        if self.armed {
            return Ok(());
        }
        if self.stranded {
            // A drain that gave up cleared `armed` without establishing
            // that the kernel had finished with the `OVERLAPPED`. Arming
            // again would hand the same structure to a second read while
            // the first may still be writing its status into it, so this
            // watch is over; the caller reads the refusal as a deaf
            // filesystem and goes back to polling.
            return Err(io::Error::other("the previous read was never drained"));
        }
        // A re-arm starts from a zeroed request. The previous one's
        // status is still sitting in these fields, and the kernel is
        // documented to be handed a clean structure with only the event
        // filled in.
        let overlapped = self.overlapped();
        // SAFETY: no read is outstanding here — `armed` was false — so
        // these fields are nobody's but this thread's, and the pointer
        // points at a live allocation.
        unsafe {
            (*overlapped).internal = 0;
            (*overlapped).internal_high = 0;
            (*overlapped).offset = 0;
            (*overlapped).offset_high = 0;
            (*overlapped).h_event = self.event.0;
        }
        // SAFETY: the directory handle is open, the buffer and the
        // `OVERLAPPED` are in one heap allocation this value owns and
        // will not free before the drain has run, and the length passed
        // is the buffer's own.
        let ok = unsafe {
            ReadDirectoryChangesW(
                self.dir.0,
                self.buffer().cast(),
                BUFFER as u32,
                0,
                FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE,
                core::ptr::null_mut(),
                self.overlapped(),
                None,
            )
        };
        if ok != 0 {
            self.armed = true;
            return Ok(());
        }
        let why = io::Error::last_os_error();
        if why.raw_os_error() == Some(ERROR_IO_PENDING) {
            // The ordinary answer on an overlapped handle: accepted, and
            // the completion will arrive on the event.
            self.armed = true;
            return Ok(());
        }
        // Any other failure means the request was refused rather than
        // queued, so the kernel holds no pointer into the allocation and
        // leaving `armed` false is what says so. It is the one premise in
        // this file that rests on Windows' contract alone rather than on
        // something checked here, and it is written out because the
        // alternative is worse in both directions: treating a refusal as
        // possibly-armed would send the drain to wait out its whole bound
        // for a completion nobody will post, on every refused arm, while
        // treating a real queued read as unarmed is the corruption this
        // whole type exists to prevent.
        Err(why)
    }

    /// Wait up to `nap` for the directory to say something.
    ///
    /// `nap` is the caller's already-clamped sleep, and the wait is never
    /// asked to be infinite: a deadline this side computed is always
    /// finite, and an `INFINITE` arrived at by a rounding slip is a hang
    /// rather than a late answer.
    ///
    /// One path returns late. A wait that fails outright drains the read
    /// before it returns, and the drain is bounded rather than instant,
    /// so this call may take up to that bound past `nap`. What the drain
    /// spends comes out of the nap and not on top of it, which is as
    /// close to the deadline as a buffer the kernel still holds allows.
    pub(crate) fn woke(&mut self, nap: Duration) -> Woke {
        if !self.armed {
            std::thread::sleep(nap);
            return Woke::Deaf;
        }
        let ms = nap
            .as_nanos()
            .div_ceil(1_000_000)
            .min(u128::from(u32::MAX - 1)) as u32;
        // SAFETY: the event is open until `Drop` runs, and the wait is
        // bounded.
        let saw = unsafe { WaitForSingleObject(self.event.0, ms) };
        match saw {
            // The read stays armed: nothing was freed and nothing was
            // cancelled, so the next pass has no arming to do.
            WAIT_TIMEOUT => Woke::Timeout,
            WAIT_OBJECT_0 => self.collected(),
            _ => {
                // The drain runs first, because the buffer is the
                // kernel's until it returns, and what it spends is spent
                // out of the nap rather than added to it. A drain that
                // ran to its bound leaves nothing to sleep, which is
                // right: the caller is already late, and this watch is
                // over — a `Deaf` ends it — so there is no loop left here
                // to spin.
                let started = Instant::now();
                self.quiesce();
                std::thread::sleep(nap.saturating_sub(started.elapsed()));
                Woke::Deaf
            }
        }
    }

    /// What the signalled event turned out to mean.
    fn collected(&mut self) -> Woke {
        let mut got = 0u32;
        // SAFETY: the handle is open and the `OVERLAPPED` is the one this
        // value armed and still owns. `bWait` is false on purpose: the
        // auto-reset event's signal has just been consumed, so a call
        // that ever did have to wait would be waiting on something
        // nobody will set again.
        let ok = unsafe { GetOverlappedResult(self.dir.0, self.overlapped(), &mut got, 0) };
        if ok != 0 {
            self.armed = false;
            return Woke::Event;
        }
        if io::Error::last_os_error().raw_os_error() == Some(ERROR_IO_INCOMPLETE) {
            // Signalled, but this read is still outstanding. `armed`
            // stays true, which is the whole reason this arm is written:
            // clearing it would let the drain be skipped and leave the
            // kernel a pointer into memory about to be freed. Reporting a
            // wake anyway is harmless, since all a wake does is send the
            // caller to list the directory.
            return Woke::Event;
        }
        self.quiesce();
        Woke::Deaf
    }

    /// Cancel whatever read is outstanding and wait until the kernel has
    /// finished with the buffer.
    ///
    /// It allocates nothing, formats nothing and asserts nothing, because
    /// it runs from `Drop` and may run while a panic is unwinding. It is
    /// also bounded on every path out, and that is not decoration: a
    /// `Drop` waiting for a completion nobody will post is a process that
    /// stops, with no timeout, no diagnostic and nothing to kill but the
    /// whole of it. Where the drain cannot finish it says so in the sink
    /// and marks the allocation stranded, which costs four kilobytes and
    /// leaves the kernel writing into memory that is still this process's
    /// — the one outcome here that is neither a hang nor a freed buffer.
    fn quiesce(&mut self) {
        if !self.armed {
            return;
        }
        // SAFETY: the handle is open and the `OVERLAPPED` is the one
        // armed against it.
        let asked = unsafe { CancelIoEx(self.dir.0, self.overlapped()) };
        if asked == 0 {
            let why = io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if why != ERROR_NOT_FOUND {
                // Nothing was cancelled and the read is still the
                // kernel's, so there is no completion on its way and
                // waiting for one would never end. `ERROR_NOT_FOUND` is
                // the good failure and the only one: it says there was
                // nothing to cancel because the completion is already
                // posted, and the collection below takes it.
                self.strand(why);
                return;
            }
        }
        // SAFETY: the event is open. The wait is bounded rather than
        // infinite for the reason in the doc comment; a cancellation that
        // has been accepted completes in microseconds, so spending this
        // long at all means something is wrong that waiting will not fix.
        let saw = unsafe { WaitForSingleObject(self.event.0, DRAIN_MS) };
        if saw != WAIT_OBJECT_0 {
            self.strand(DRAIN_UNFINISHED);
            return;
        }
        let mut got = 0u32;
        // SAFETY: as above. `bWait` is false because the completion's own
        // event has just been waited on, so there is nothing left for it
        // to wait for and no second unbounded wait to enter.
        let ok = unsafe { GetOverlappedResult(self.dir.0, self.overlapped(), &mut got, 0) };
        if ok == 0 {
            let why = io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if why == ERROR_IO_INCOMPLETE {
                // The event said the read was done and the read says it
                // is not. Nothing here can tell which to believe, so the
                // buffer stays the kernel's.
                self.strand(why);
                return;
            }
            self.drained.set(Some(why));
        } else {
            self.drained.set(Some(0));
        }
        self.armed = false;
    }

    /// Give up on the read, recording why, and leave the allocation to be
    /// leaked rather than freed under a kernel that may still write to
    /// it. Sticky, because `Drop` may run after a `woke` that already
    /// gave up and must not then free what that call decided not to.
    fn strand(&mut self, why: i32) {
        self.drained.set(Some(why));
        self.armed = false;
        self.stranded = true;
    }
}

impl Drop for Changes {
    fn drop(&mut self) {
        // Cancel, drain, and only then release. The order is the whole of
        // it: the drain returns before the allocation the kernel was
        // handed a pointer into is given back, and putting it in `Drop`
        // is what makes it impossible to skip on any path out of a wait,
        // a `?` or a panic. The two handles the compiler closes after
        // this body returns.
        self.quiesce();
        if self.stranded {
            // The drain could not establish that the kernel has let go,
            // so the allocation is leaked on purpose. Closing the handle
            // below is still right — it is what cancels the read the
            // kernel is holding — but the memory it may write into stays
            // this process's and is never handed to anything else.
            return;
        }
        // SAFETY: the pointer came from `Box::into_raw` in `open`, no
        // other owner of it was ever made, the drain above has returned
        // and said the kernel is finished, so no read is outstanding
        // against the allocation, and nothing reads `self.state` after
        // this.
        drop(unsafe { Box::from_raw(self.state.as_ptr()) });
    }
}

/// A path as a NUL-terminated wide string.
///
/// A path carrying an interior NUL is refused rather than passed, because
/// the call would read it as a shorter path and open something the caller
/// never named.
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut out: Vec<u16> = path.as_os_str().encode_wide().collect();
    if out.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the path carries a NUL",
        ));
    }
    out.push(0);
    Ok(out)
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

    // ---- the reply watch ---------------------------------------------
    //
    // None of these names carries the word the higher-level suite is
    // selected by, so the count that suite prints stays that suite's
    // alone.

    use crate::testing::Sandbox;
    use std::fs;

    /// A directory to observe, inside a sandbox that goes away with the
    /// test.
    fn a_directory(b: &Sandbox) -> std::path::PathBuf {
        let dir = b.join("res");
        fs::create_dir_all(&dir).expect("the directory is made");
        dir
    }

    /// A file renamed into `dir`, which is how a reply appears: written
    /// under a temporary name and moved into place.
    fn land(dir: &std::path::Path, name: &str) {
        let tmp = dir.join(format!("{name}.tmp"));
        fs::write(&tmp, b"something").expect("the bytes land");
        fs::rename(&tmp, dir.join(name)).expect("the rename lands it");
    }

    #[test]
    fn createfilew_opens_a_directory_and_the_close_lets_it_go() {
        let b = Sandbox::new();
        let dir = a_directory(&b);
        let observer = Changes::open(&dir).expect("the directory opens");
        assert!(!observer.armed(), "it is opened unarmed");
        drop(observer);
        fs::remove_dir(&dir).expect("the close let the directory go");
    }

    #[test]
    fn createfilew_says_no_with_a_handle_that_is_not_null() {
        // The failure a null check would read as success. What matters is
        // that the refusal is recognised as one at all: a bogus handle
        // taken for a live one would be armed against and then closed.
        let b = Sandbox::new();
        let missing = b.join("no-such-directory");
        let why = match Changes::open(&missing) {
            Ok(_) => panic!("there is nothing there to open"),
            Err(why) => why,
        };
        assert_eq!(
            why.kind(),
            io::ErrorKind::NotFound,
            "the OS said why, and it was not a null handle that said it: {why}"
        );
    }

    #[test]
    fn createeventw_gives_an_event_that_starts_unsignalled() {
        let b = Sandbox::new();
        let dir = a_directory(&b);
        let observer = Changes::open(&dir).expect("the directory opens");
        // SAFETY: the event is open for as long as the observer is.
        let saw = unsafe { WaitForSingleObject(observer.event.0, 0) };
        assert_eq!(
            saw, WAIT_TIMEOUT,
            "a fresh event is not standing signalled, so the first wait is a real one"
        );
    }

    #[test]
    fn a_rename_into_the_directory_signals_the_event() {
        let b = Sandbox::new();
        let dir = a_directory(&b);
        let mut observer = Changes::open(&dir).expect("the directory opens");
        observer.arm().expect("the read arms");
        land(&dir, "0000000001-abcd.res");
        assert_eq!(
            observer.woke(Duration::from_secs(5)),
            Woke::Event,
            "the directory reported the rename"
        );
        assert!(
            !observer.armed(),
            "and the completion was consumed, so the next look needs a new read"
        );
    }

    #[test]
    fn a_pending_read_is_cancelled_and_drained_before_the_buffer_is_freed() {
        // Nothing happens in the directory, so the read is still pending
        // when the observer goes. The drain is the only evidence this
        // design can produce that the kernel let go of the buffer before
        // it was freed.
        let b = Sandbox::new();
        let dir = a_directory(&b);
        let sink = {
            let mut observer = Changes::open(&dir).expect("the directory opens");
            observer.arm().expect("the read arms");
            let sink = observer.sink();
            assert_eq!(sink.get(), None, "nothing has drained yet");
            sink
        };
        assert_eq!(
            sink.get(),
            Some(ERROR_OPERATION_ABORTED),
            "the pending read was cancelled and waited for, not merely abandoned"
        );
    }

    #[test]
    fn dropping_an_unarmed_observer_closes_both_handles() {
        let b = Sandbox::new();
        let dir = a_directory(&b);
        let sink = {
            let observer = Changes::open(&dir).expect("the directory opens");
            observer.sink()
        };
        assert_eq!(
            sink.get(),
            None,
            "there was no read to cancel, so no drain ran"
        );
        fs::remove_dir(&dir).expect("and both handles are closed");
    }

    #[test]
    fn a_second_event_needs_a_second_arm() {
        let b = Sandbox::new();
        let dir = a_directory(&b);
        let mut observer = Changes::open(&dir).expect("the directory opens");
        observer.arm().expect("the first read arms");
        land(&dir, "one.res");
        assert_eq!(observer.woke(Duration::from_secs(5)), Woke::Event);

        land(&dir, "two.res");
        assert_eq!(
            observer.woke(Duration::from_millis(50)),
            Woke::Deaf,
            "an observer with no read outstanding hears nothing, however loud the directory is"
        );
        observer.arm().expect("the second read arms");
        land(&dir, "three.res");
        assert_eq!(
            observer.woke(Duration::from_secs(5)),
            Woke::Event,
            "and the second read hears the next change"
        );
    }

    #[test]
    fn an_open_directory_refuses_its_own_removal() {
        // The OS fact everything about holding no handle rests on, pinned
        // beside the code that assumes it. A Windows that stopped
        // refusing would make every such check vacuous, and this is where
        // it would say so.
        let b = Sandbox::new();
        let dir = a_directory(&b);
        let observer = Changes::open(&dir).expect("the directory opens");
        let why = fs::remove_dir(&dir).expect_err("a held directory does not go");
        assert_eq!(
            why.raw_os_error(),
            Some(32),
            "the removal was refused because the directory is in use: {why}"
        );
        drop(observer);
        fs::remove_dir(&dir).expect("and once the handle is gone it goes");
    }
}
