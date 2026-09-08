//! The one binary a user downloads: the MCP server over stdio, the installer
//! that places the executor in `Saved Games`, and a command line that speaks
//! the same functions as the tools.
//!
//! Nothing here touches the wire directly. The client is `dcs-eval`, linked
//! as a library, so that the words a reply is given and the bytes it arrived
//! as are decided in two places that cannot drift into each other.

fn main() {}
