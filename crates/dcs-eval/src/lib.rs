//! The client: everything that speaks the executor's file-based protocol
//! from outside DCS.
//!
//! This crate is the wire made callable. It owns the handshake and heartbeat
//! readers, path containment, publish by rename, the arm file, wait and
//! collect, and the game-state reads. It knows nothing about MCP or the
//! command line: those live in `dcs-mcp`, which links this crate, and so can
//! any other Rust program that wants to talk to a running DCS.

pub mod file;

// The axes: what the game is doing, derived from the handshake, the
// heartbeat, a process probe and the reads. It owns the vocabulary and
// the reason every unknown carries, and it decides nothing it was not
// given evidence for.
pub mod game;
pub mod id;
pub mod paths;
pub mod pipeline;
pub mod protocol;
pub mod publish;
pub mod readers;

// Which `DCS.*` calls this client will ever send, and how each one is
// wrapped. The list is a constant because the client authors these chunks
// and the executor runs whatever it is given.
pub mod reads;

// The hash the provenance record carries. Written here because the crate
// takes no crates.io dependency; decision record 0011.
pub mod sha256;

// The other half of evaluating a file: the bytes, once `file` has judged
// the path they come from.
pub mod source;
pub mod status;

// The crate carries `unsafe`, which it did not before, and all of it is in
// this one module: the Win32 calls it makes, each declared beside a safe
// wrapper. Decision record 0011 is why they are declared here rather than
// taken from a crate.
pub mod sys;
pub mod wait;

// How a wait sleeps: the event-driven watch over the reply directory, and
// the poll that runs beside it whatever the watch reports. Private,
// because it is `wait`'s own pacing and nothing outside the crate names
// it.
mod watch;

// The stand-in is a test double: the crate's own tests always have it, and
// another crate's tests get it through the `standin` feature. A user's
// binary never links it.
#[cfg(any(test, feature = "standin"))]
pub mod standin;

#[cfg(test)]
pub(crate) mod testing;

// The interop control: the shipped Lua's bytes under this crate's parser.
// A test module and nothing else, so it lives beside `testing`.
#[cfg(test)]
mod interop;

// The round-trip control: the client's `send` and `wait` against the shipped
// Lua, answered on a live tick or superseded by a restart. A test module and
// nothing else.
#[cfg(test)]
mod e2e;
