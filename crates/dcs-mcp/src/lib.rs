//! What the binary is made of, as a library: the parts `main` drives, exposed
//! so they can be tested and read apart from the command line that calls them.
//!
//! The binary's own `main` stays a few lines over these modules. The wire is
//! not here at all — that is `dcs-eval`, linked as a library, and nothing in
//! this crate reimplements a byte of it.
//!
//! The library target is not a convenience. The only honest proof that nothing
//! but protocol frames reaches stdout is to start the real executable and read
//! its two streams apart, and a test can name the executable it is linked
//! beside only when the crate has a library target as well as a binary one. A
//! second effect is worth saying out loud: an item that is `pub` here is not
//! dead code even when nothing calls it yet, so the accessors the tools will
//! reach for do not have to be exercised early just to keep the linter quiet.

// Where every diagnostic goes, decided once. Stdout belongs to the protocol.
pub mod diag;

// Where `Saved Games` is, which `DCS*` variant under it is the target, and
// which trees a target may not be in.
pub mod locate;

// The Win32 calls this crate makes, declared here and nowhere else. The
// client crate declares its own; decision record 0019 is why the two sets
// are not one file.
pub mod sys;
