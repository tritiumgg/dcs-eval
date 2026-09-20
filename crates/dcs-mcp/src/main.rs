//! The one binary a user downloads: the MCP server over stdio, the installer
//! that places the executor in `Saved Games`, and a command line that speaks
//! the same functions as the tools.
//!
//! Nothing here touches the wire directly. The client is `dcs-eval`, linked
//! as a library, so that the words a reply is given and the bytes it arrived
//! as are decided in two places that cannot drift into each other.
//!
//! This file is a shell. The work lives in the crate's own library half, where
//! a test can reach it; here there is only the order things happen in.

// The executor the binary carries, and the hashes that recognise it.
mod embed;

fn main() {
    // First, before anything else can log: a diagnostic on stdout would be
    // read by the client as a protocol frame.
    dcs_mcp::diag::to_stderr();
}
