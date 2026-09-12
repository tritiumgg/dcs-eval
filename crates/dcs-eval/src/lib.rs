//! The client: everything that speaks the executor's file-based protocol
//! from outside DCS.
//!
//! This crate is the wire made callable. It owns the handshake and heartbeat
//! readers, path containment, publish by rename, the arm file, wait and
//! collect, and the game-state reads. It knows nothing about MCP or the
//! command line: those live in `dcs-mcp`, which links this crate, and so can
//! any other Rust program that wants to talk to a running DCS.

pub mod protocol;
pub mod publish;

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

// The round-trip control: the client's `send` against the shipped Lua on a
// live tick. A test module and nothing else.
#[cfg(test)]
mod e2e;
