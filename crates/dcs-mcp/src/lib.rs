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

// The executor the binary carries, and the hashes that recognise it.
pub mod embed;

// The one line appended to `Export.lua`, the marker that makes its removal
// exact, and the copy parked before it goes in.
pub mod export_line;

// Where `Saved Games` is, which `DCS*` variant under it is the target, and
// which trees a target may not be in.
pub mod locate;

// The server's own data directory: the register of what was moved, written
// before the move, and the store the displaced copies are moved into.
pub mod register;

// Placing the executor in `Scripts\Hooks\`: what was already there, whether
// it may be displaced, and the rename that puts the bytes at their name.
pub mod install;

// Taking it back out: the hook only where its hash is one this project
// shipped, the line only by whole-line match, and every file that was parked
// put back where it was found.
pub mod uninstall;

// What the installation looks like from outside: the hook's hash, the one
// line in `Export.lua`, the policy gate, the session — and no write at all.
pub mod verify;

// The MCP server itself: the options it is started with, the client it builds
// per call, and the stdio transport it speaks over.
pub mod serve;

// The six tools the server offers, decided in one block so that the set that
// is registered is the set that is listed.
pub mod tools;

// One line per evaluation, carrying where the bytes came from and the digest
// the reader took of them. The only writer of that record.
pub mod runs;

// The read-and-eval verbs from a terminal. Each is a line over the same
// function its tool is a line over, so the two cannot word a reply apart.
pub mod cli;

// The Win32 calls this crate makes, declared here and nowhere else. The
// client crate declares its own; decision record 0019 is why the two sets
// are not one file.
pub mod sys;

// How an answer is worded: the one renderer every tool body goes out through,
// so a refusal reads as a refusal in one place rather than in six.
pub mod wording;

// Scaffolding the tests in this crate share. Never in the binary.
#[cfg(test)]
mod testing;

// What a call leaves behind on the executor's session directories, watched
// from outside through the sweep the next executor session does. A unit
// module rather than a test binary, because it drives `testing`, and there
// is to be only one of those.
#[cfg(test)]
mod watching;

// What the server costs while nobody is asking it anything. A unit module for
// the same reason as the one above — it drives `testing` — and what it
// watches is the server doing nothing: no byte on the wire and no arm file on
// the executor, over sixty seconds of the runtime's own clock.
#[cfg(test)]
mod idle;
