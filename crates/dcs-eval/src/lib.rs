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
