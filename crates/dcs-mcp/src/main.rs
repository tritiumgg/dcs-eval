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

use dcs_mcp::{cli, installer, serve};

/// Every word this binary answers to. Anything else is a usage line rather
/// than a silence.
fn usage() -> String {
    format!(
        "usage: dcs-mcp serve --saved-games <dir> --variant <name> \
         [--host hook|export]\n       [--data-dir <dir>]\n{}\n{}",
        cli::USAGE,
        installer::USAGE
    )
}

fn main() {
    // First, before anything else can log: a diagnostic on stdout would be
    // read by the client as a protocol frame.
    dcs_mcp::diag::to_stderr();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("serve") => serve::Options::parse(args.into_iter().skip(1))
            .map_err(|why| format!("{why}\n{}", usage()))
            .and_then(|opts| serve::run(opts).map_err(|why| why.to_string()))
            .map(|()| 0),
        // The verbs print to stdout and answer with an exit code of their
        // own, so this branch and the one below are the only ones that leave
        // with anything but nought or the usage code.
        Some(word) if cli::takes(word) => {
            cli::run(args, &mut std::io::stdout()).map_err(|why| format!("{why}\n{}", usage()))
        }
        Some(word) if installer::takes(word) => {
            // The snippet `install` prints names the executable that ran it,
            // so a client configured from it starts this same file.
            let exe = std::env::current_exe().unwrap_or_else(|_| "dcs-mcp.exe".into());
            installer::run(args, &mut std::io::stdout(), &exe)
                .map_err(|why| format!("{why}\n{}", usage()))
        }
        Some(other) => Err(format!("dcs-mcp does not take {other}\n{}", usage())),
        None => Err(usage()),
    };
    match outcome {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(why) => {
            // Straight to stderr rather than through the log: a usage line is
            // the program talking to the person who ran it, not a diagnostic.
            eprintln!("{why}");
            std::process::exit(2);
        }
    }
}
