//! The live-run instrument: the measurements only a running game can give,
//! taken one phase at a time and kept in a ledger of their own.
//!
//! It is a verb of this binary rather than a script or a second executable.
//! A script driving the command line would start a process per request, so
//! every round trip it timed would include a process start-up; and a second
//! executable is a second thing to package for no gain over a verb that
//! already parses where the install is. The figures have to be taken again
//! after every DCS update, so the instrument ships to users with the rest.

// The append-only record of what each phase measured.
pub mod ledger;
