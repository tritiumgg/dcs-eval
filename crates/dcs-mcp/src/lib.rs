//! What the binary is made of, as a library: the parts `main` drives, exposed
//! so they can be tested and read apart from the command line that calls them.
//!
//! The binary's own `main` stays a few lines over these modules. The wire is
//! not here at all — that is `dcs-eval`, linked as a library, and nothing in
//! this crate reimplements a byte of it.

// The Win32 calls this crate makes, declared here and nowhere else. The
// client crate declares its own; decision record 0019 is why the two sets
// are not one file.
pub mod sys;
