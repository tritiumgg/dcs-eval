# ADR 0011: The client crate declares its Win32 calls and takes no dependency

## Status

Accepted

## Context

Stage 6 is the first work that wants something the standard library does not
give: a probe for whether a PID is still running, a watch on the reply
directory, a short random tag for an id, and SHA-256 over a file's bytes. The
workspace holds no third-party crate — `Cargo.lock` names `dcs-eval` and
`dcs-mcp` and nothing else — so each of these is the first of its kind, and
the choice made for one is the precedent for the rest.

The specifications decide the language and check the SDK, and say nothing
about what the library may link. §1.2 examines `rmcp` at length and admits it
for the binary; §1.6 weighs the installer first:

> 1. **The installer.** `dcs-mcp.exe` with `DcsApi.lua` inside it
>    (`include_bytes!`), one download, no runtime, no package manager, on a
>    Windows machine whose owner wants to fly.

For the library, the nearest thing to a rule is §8's last paragraph, which says
why the skin is specified apart from it:

> The MCP skin is specified as its own project in `mcp.md`, which owns this
> library so that several unrelated projects can use it without depending on
> any one of them.

That sentence is about not making a consumer take the MCP server to get the
wire. A dependency tree is the same argument one level down: a consumer that
links this crate for the protocol takes everything the crate links with it.

§3.2 also names an implementation by name, which is the one place a
specification reaches for an OS call directly:

> The client's `wait` replaces its fixed sleep with a watch on the reply
> directory (`fs.watch`, which on Windows is `ReadDirectoryChangesW`), waking
> on any event and then listing the directory once. A poll at 25 ms remains as
> the fallback for a watch that reports nothing, because `fs.watch` is
> documented as unreliable on some filesystems and the failure would be
> silent.

## Decision

`dcs-eval` links no crates.io dependency. What it needs from outside the
standard library it declares or writes:

- The Win32 symbols it calls are declared in one module, `sys.rs`, each with
  its ABI written out and wrapped in a safe function, so that every `unsafe`
  in the crate is in one file a reviewer can audit whole. The set is small and
  known: `OpenProcess`, `GetExitCodeProcess` and `CloseHandle` for liveness,
  and `CreateFileW`, `CreateEventW`, `ReadDirectoryChangesW`,
  `GetOverlappedResult` and `WaitForSingleObject` for the watch.
- SHA-256 is written in the crate and proved against the published vectors and
  against `sha256sum` on a fixture. It is a fixed algorithm with published
  test vectors, used here for provenance rather than for security.
- The id tag is a seeded generator in the crate. Its job is collision-freedom
  between two clients sharing one session, which a counter seeded from the
  PID, the clock and an address satisfies; it guards nothing.

`dcs-mcp` is not covered by this. It is the binary a user downloads, it takes
`rmcp` at T37 as §1.2 admits, and nothing here argues against a dependency
that ends at the executable.

Rejected:

- **`sha2`, `rand`, `windows-sys` and `notify`.** Four crates and their trees
  enter every project that links this library for the wire, to save code that
  is either provable against published vectors or a page of declarations.
- **`windows-sys` alone, for the OS calls.** The smallest of the four and the
  most defensible, but the line it draws is "no dependencies except the
  convenient one", which does not survive the next argument. Declaring the
  eight symbols the crate calls is less code than the feature list that would
  select them, and the standard library declares its own for the same reason.
- **Shelling out to `tasklist` for liveness.** A process spawn inside a call
  §8 says "may be called freely" and that must cost a dormant executor
  nothing.

## Consequences

The crate carries `unsafe`, which it did not before. It is confined to
`sys.rs`; a safe wrapper is what the rest of the crate calls, and a test drives
each wrapper against a real handle — a PID that is running, a PID that has
exited, a handle that cannot be opened.

The SHA-256 is this project's to be wrong about. It is proved against the
published vectors for the empty string, `abc` and the 896-bit case, and
against `sha256sum` on a fixture, in the commit that adds it; a hash that
disagrees with `sha256sum` is a defect in this crate and not in the tool.

The watch is the most code this decision costs, which is why it is its own
task (T56) rather than a paragraph inside `wait`. T54 ships the 25 ms poll
§3.2 requires as the fallback, and the watch lands on top of a `wait` that
already works; T41, which proves no watch is held between tool calls, needs
both.

What would reopen this: something Stage 7 or 8 needs that cannot be declared
or written in a few hundred lines with a test that proves it. `rmcp` in the
binary is not that, because it stops at the binary.
