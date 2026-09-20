//! Where a diagnostic goes, decided once.
//!
//! Serving MCP means stdout *is* the transport: the client on the other end
//! parses every byte of it as a protocol frame. A log line written there is
//! not a cosmetic blemish, it is a corrupt frame, and the failure surfaces at
//! the far end as the client giving up on a server that looks broken — with
//! nothing on the user's screen pointing back at the log line that did it.
//!
//! So the sink is chosen in one place, this module, and the rest of the binary
//! never picks one. The hazard is that the obvious spelling is the wrong one:
//! `tracing_subscriber::fmt()` on its own writes to **stdout**.

/// Send every diagnostic to stderr, for the whole process.
///
/// Called once, first thing in `main`, before anything can log and before the
/// transport is built.
///
/// The level is fixed rather than read from the environment: this is the one
/// thing standing between a diagnostic and the transport, and it should not
/// behave differently on a machine that happens to have a logging variable
/// set. ANSI is off because stderr here is a client's captured pipe far more
/// often than it is a terminal.
pub fn to_stderr() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();
}
