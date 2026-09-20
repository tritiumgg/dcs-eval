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

use dcs_mcp::serve;

// The executor the binary carries, and the hashes that recognise it.
mod embed;

/// Every word this binary answers to today. The installer and the command
/// line that mirrors the tools are not built, so anything else is a usage
/// line rather than a silence.
const USAGE: &str =
    "usage: dcs-mcp serve --saved-games <dir> --variant <name> [--host hook|export]";

fn main() {
    // First, before anything else can log: a diagnostic on stdout would be
    // read by the client as a protocol frame.
    dcs_mcp::diag::to_stderr();

    let mut args = std::env::args().skip(1);
    let verb = args.next();
    let outcome = match verb.as_deref() {
        Some("serve") => serve::Options::parse(args)
            .map_err(|why| format!("{why}\n{USAGE}"))
            .and_then(|opts| serve::run(opts).map_err(|why| why.to_string())),
        Some(other) => Err(format!("dcs-mcp does not take {other}\n{USAGE}")),
        None => Err(USAGE.to_owned()),
    };
    if let Err(why) = outcome {
        // Straight to stderr rather than through the log: a usage line is the
        // program talking to the person who ran it, not a diagnostic.
        eprintln!("{why}");
        std::process::exit(2);
    }
}
