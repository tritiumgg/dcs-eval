# ADR 0019: The installer's Win32 calls are the binary's own

## Status

Accepted

## Context

The installer has to find `Saved Games`, and `mcp.md` §5.1 says how:

> 1. **Find `Saved Games`** through the shell's known folder
>    (`FOLDERID_SavedGames`), never a string built from `%USERPROFILE%` — the
>    folder can be relocated — and `--saved-games` overrides it. List the
>    `DCS*` variants there. One is the target; two is an ambiguity, asked (or
>    `--variant`), never picked (`prior:pipeline/src/bridge/paths.ts:307-334`).
> 2. **Refuse the wrong tree.** Resolve the target's real path; refuse it if it
>    lies under the DCS install when the install is known, and refuse anything
>    that is not under `Saved Games`. The install is read-only, always,
>    including a probe of whether it is writable (D18, D128).

A known folder is `SHGetKnownFolderPath`, which allocates a wide string the
caller releases with `CoTaskMemFree`. Neither symbol is declared anywhere in
this tree. [ADR 0011](0011-the-client-crate-declares-its-win32-calls-and-takes-no-dependency.md)
already settled how a Win32 call is made here, and it names the set:

> - The Win32 symbols it calls are declared in one module, `sys.rs`, each with
>   its ABI written out and wrapped in a safe function, so that every `unsafe`
>   in the crate is in one file a reviewer can audit whole. The set is small and
>   known: `OpenProcess`, `WaitForSingleObject` and `CloseHandle` for liveness,
>   and `CreateFileW`, `CreateEventW`, `ReadDirectoryChangesW`,
>   `GetOverlappedResult`, `WaitForSingleObject` and `CancelIoEx` for the watch.

It also says what it does not cover:

> `dcs-mcp` is not covered by this. It is the binary a user downloads, it takes
> `rmcp` at T37 as §1.2 admits, and nothing here argues against a dependency
> that ends at the executable.

So the question the earlier record leaves open is not whether a dependency is
allowed in the binary, but where a declaration goes when the call belongs to
the binary and the file that holds every other declaration belongs to the
library.

## Decision

`SHGetKnownFolderPath` and `CoTaskMemFree` are declared in
`crates/dcs-mcp/src/sys.rs`, in the shape ADR 0011 gives the client's own
module: the ABI written out, one safe wrapper, no `#[cfg(windows)]` gate, and
a module header saying why.

The library declares only what the library calls. Its enumerated set is the
protocol's — liveness and the reply watch — and finding `Saved Games` is
neither; a consumer that links this crate for the wire gets an installer's
shell call it will never make, and the "one file a reviewer can audit whole"
property is bought by keeping each crate's `unsafe` to its own reason rather
than to one address.

Rejected:

- **`windows-sys` in the binary.** Admissible under ADR 0011, which stops at
  the library — but two declarations are less code than the feature list that
  would select them, and the binary gains a build-time tree for it.
- **Declaring the two symbols in `dcs-eval::sys`.** It would keep the workspace
  to one `unsafe` file, at the cost of a library declaring a call it never
  makes, which is the thing ADR 0011's enumeration is for.
- **Reading `%USERPROFILE%` and appending `Saved Games`.** §5.1 forbids it in
  the same breath as it names the known folder, and the reason is the relocated
  folder.

## Consequences

"Every `unsafe` in one file" becomes a per-crate property rather than a
workspace one. A reviewer auditing the unsafe in this build now reads two
files, not one, and each module header says so and names the other.

The two `#[link]` attributes are the part most likely to be got wrong later:
`shell32` and `ole32` are not linked by the standard library by default, so an
omission is a link error at `cargo build` rather than a compile error where the
declaration is.

What would reopen this: a third crate wanting a Win32 call, or the binary's set
growing past what one file usefully holds. At two symbols neither is near.
