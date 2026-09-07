# The eval bridge, specified for a fresh repository

**Citation convention.** A path written `prior:<path>` is a file in the superseded repository this
document was written from, cited as evidence and readable there on demand. Nothing in the new
repository reads one. An unprefixed path is a file this document expects the new repository to
create. A `D<n>` or `S<n>` is a decision or spike record in `prior:docs/decisions/`.

**What this was written from.** Every file under `prior:pipeline/src/bridge/`, `prior:pipeline/src/
mcp/` and `prior:pipeline/src/census/`; both resident Lua scripts and their controls; the probe
supervisor; the decision records that bind the bridge; the reshape documents; and the committed
census reports under `prior:generated/`. **`prior:docs/memory/**` could not be read**: the session's
permission settings deny it (`prior:.claude/settings.json`, `permissions.deny`), and so does the
`prior:CLAUDE.md` and every `README.md`. Where a memory entry is the only record of a fact, this
document says so in §11 rather than paraphrasing a file it did not open.

**Where a number is asserted**, the command that produced it is beside it or in §12.

---

## 0. What the bridge is, and what this document decides

DCS World embeds seven Lua 5.1.5 states with no shared memory: `hook` (the GameGUI hook state),
`gui`, `scripting` (also answering to the name `server`, S10), `mission`, `missionscripting`,
`config` and `export`. The bridge lets a program outside DCS evaluate Lua inside a chosen state and
read the result back. It is the instrument everything else stands on: a consuming project's walker
runs through it, the probes call into a state through it, and an MCP server (`mcp.md`) exposes
it to coding agents.

Seven decisions, each argued in its own section:

| § | Question | Decision |
|---|---|---|
| 2 | Transport | **File-based, restructured**: one directory per bridge session, pipelined requests, a per-tick CPU budget in the bridge, an event-driven wait in the client. Re-derived under the idle constraint in §2.4 and **it does not move** |
| 3 | Performance | The frame is the floor; the current design pays ~2 frames per reply and one reply per frame; the new one pays one frame per *window* of replies and reports its own CPU cost |
| 3.6–3.8 | Idle cost | **Dormant by default.** A dormant frame is four VM instructions and one `pcall`, no allocation, no syscall; every eighth frame adds one `lfs.attributes` on a fixed path. A client wakes the bridge by creating that file; the bridge returns to dormancy after 3 s without a request |
| 4 | Crash safety | A session is a stamped directory; a request in flight when DCS dies is never seen by the next session; the client learns of death from the stamp and the PID, never from a clock |
| 5 | Seven states | All seven addressed by name from one host; `missionscripting` is a declared two-hop, string-only carrier |
| 6 | Two scripts | **One source file**, `DcsApi.lua`, host-detected at load, installed once and loaded into both hosts |
| 7 | Protocol | Version 2: the same header/blank/body envelope and **two ops, `ping` and `eval`**. `reflect` and `census` were removed on 2026-09-06 (§7.0); the tick budget, the reply ceiling and refusal-never-truncation now bind every evaluation, and an instruction budget bounds a single chunk (§3.4); a request may carry a `chunkname`, and the line numbers it produces must be true (§7.3) |

Constraints that bind every section: the DCS install is read-only, always, including a probe of
whether it is writable (D18, D128); `Saved Games` is writable under park-and-restore and nothing
there that is not this project's is ever lost (D128); Lua is 5.1.5 PUC-Rio, never LuaJIT, never 5.4
(D11, `prior:tools/harness_stubs.lua:25-28`).

**A constraint this document was first written without, added 2026-09-06 in the maintainer's
words:** *"With the bridge installed, it should do as little work as possible unless it is being
used. I'd like to have the bridge installed, even while I'm actually playing the game. When I'm
playing the game and not actively using the bridge, I don't want the bridge to have any noticeable
performance impact."* And, separately: *"The current bridge implementation doesn't seem to have
noticeable performance impact."* The second sentence sets the burden of proof. The current file
transport is a measured baseline that already meets the requirement, so the revision must **hold**
that baseline: any design whose idle path costs more than today's is a regression whatever it buys
the active path. Two things follow and run through every section below. **Permanent installation
is a stated property** (§6.3): the bridge is installed once and left in place across ordinary play
sessions, not parked in for a run. And the bridge has two paths, **dormant** and **armed**, costed
separately: §3 already costed the armed path to the millisecond and never costed the dormant one;
§3.6–3.8 do.

**A second revision, 2026-09-06, later the same day: `census` and `reflect` leave the protocol.**
§7.0 states it in full. In one paragraph: `census` was already an `eval` — the bridge built a chunk
from a parameter prologue and its walker source and shipped it into the target state
(`prior:tools/hooks/DcsApiEval.lua:1399-1408`), and the walk's session state lived in that state's
own globals, never in the bridge (`:801-804`) — so the op was a convenience verb around `eval`, and
the ~2,000 lines behind it belong to the project that parses their output. `reflect` is the same
shape with a smaller chunk. What the two ops had that `eval` lacked — a CPU budget, a refusal in
place of a truncation, a size ceiling — is not census-specific and now binds every evaluation
(§3.4, §7.7), and a request may name its chunk so that errors carry a true file and line (§7.3).
The sections this changes say so where it applies: §1, §3.1, §3.3–3.5, §3.8, §4.2, §4.6, §5.1–5.3,
§6.1–6.3, §7, §8, §10–12.

---

## 1. The seven states, verified

The bridge's host states are `hook` and `export`; the other five are reached from `hook` through
`net.dostring_in`, and `missionscripting` through a second hop (§5). What each state can reach was
read off the committed model rather than assumed. A file `prior:model/<state>/<name>.yaml` exists
only where the census reached that name in that state:

```powershell
foreach ($s in 'hook','gui','scripting','mission','missionscripting','config','export') {
  $row = foreach ($n in 'require','package','io','os','lfs','net','debug','loadstring') {
    if (Test-Path "prior:model/$s/$n.yaml") { $n } else { '-' } }; "$s`: $($row -join ' ')" }
```

| State | `require` | `package` | `io` | `os` | `lfs` | `net` | `debug` | `loadstring` | `socket` global | Reached how | Available when |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `hook` | yes | yes | yes | yes | yes | yes, 98 records incl. `net.dostring_in` | yes | yes | **no** | host | always (poll drives from every callback, D36) |
| `gui` | yes | yes | yes | yes | yes | yes, 98 records incl. `net.dostring_in` | yes | yes | no (`WebSocket` is an ED global here) | `net.dostring_in('gui')` | from the main menu (S4) |
| `scripting` / `server` | yes | yes | yes | yes | yes | no | yes | yes | no | `net.dostring_in('scripting')` | from the main menu (S4) |
| `config` | yes | yes | yes | yes | yes | the `net` table alone, no members reached | yes | yes | no | `net.dostring_in('config')` | from the main menu (S4) |
| `mission` | **no** | **no** | **no** | **no** | **no** | no | **no** (S5) | yes | no | `net.dostring_in('mission')` | from the main menu (S4); `a_do_script` only with a mission loaded |
| `missionscripting` | **no** | **no** | **no** | **no** | **no** | yes, 59 records incl. `net.dostring_in` (unmeasured whether callable) | yes | yes | no | two hops, §5 | only with a mission loaded; mission-scoped lifetime (S17) |
| `export` | yes | yes | yes | yes | yes | **no** (`net` and `DCS` are nil, D94) | yes | yes | no | host | only while a mission runs (`LuaExport*` fire nowhere else) |

Three readings that matter below:

- **No state holds a `socket` global.** `Get-ChildItem prior:model -Recurse -Filter 'socket*.yaml'`
  returns nothing. LuaSocket ships in the install — `LuaSocket/?.lua` is on the container survey's
  search path (`prior:tools/container_survey.lua:250`) and its files are cited as call sites in the
  stdlib records of `config`, `export` and `missionscripting` — so `require('socket')` may load in
  the five states that have `require`. **Whether it does in `hook` is unmeasured**; D36 records the
  external claim that it does as a lead, not a fact. `mission` and `missionscripting` have no
  `require` and could not reach it at all.
- **Every state has `loadstring`**, so a chunk can be compiled in every state without a prologue
  that depends on anything else — and, since §7.3, so that a request's `chunkname` can be honoured
  in every state by compiling the body as its own chunk *there*, which is what keeps line numbers
  true through every carrier.
- **`os.getpid` exists in `hook`, `gui`, `scripting`, `config` and `export`** (`Select-String -Path
  prior:model/*/os.yaml -Pattern 'path: os\.getpid$'`). It is ED's addition to `os` (D126) and §4
  uses it in both host states.

Role changes reach: on a client joined to a real server `net.dostring_in` returns nil for every
state, so `hook` reaches nothing and `export` is reachable only by an instrument resident in it
(S7, D94). That is why the bridge has two hosts and why §6 makes them one file.

---

## 2. Transport

### 2.1 What every transport shares, and it decides the question

The bridge has no thread of its own. Its only execution is inside DCS's Lua callbacks —
`DCS.setUserCallbacks` in `hook` (`prior:tools/hooks/DcsApiEval.lua:2150-2181`), the four
`LuaExport*` slots in `export` (`prior:tools/export/DcsApiExport.lua:1915-1919`) — and every one of
them runs on the simulation thread. Whatever the transport, the bridge can notice a request only
when a callback fires, and it can publish a reply only before that callback returns. So:

- **The latency floor is the tick.** `onSimulationFrame` runs at 57 Hz at the menu and in the
  editor (S4), so the floor is 17.5 ms (`prior:pipeline/src/census/sizing.ts:6`). Measured over the
  file transport from inside Lua: 14 ms median, 30 ms p95, every one of 32 round trips completing in
  exactly one tick (S4). A socket polled from the same callback completes in exactly one tick too.
- **No transport answers during a mission load.** Nothing fires between `onMissionLoadBegin` and
  `onMissionLoadEnd` — 15.5 s and 17.2 s measured on two runs (S4, `prior:pipeline/src/mcp/
  tools.ts:114`). A socket client blocked on `recv` sits on the same stalled thread (D36 §Rejected).
- **The throughput ceiling is CPU on the simulation thread**, not bytes on a channel. A reply's
  cost is the Lua work that produced it, and that work stalls the frame whichever way it is
  delivered.
- **The idle floor is the same for every transport, and it is not zero.** With no thread of its
  own, the bridge cannot be *told* a request has arrived; it can only *look*, from inside a
  callback, on the simulation thread. A socket that blocks costs nothing idle and cannot be used,
  because the thread it would block is the frame. A non-blocking socket check is one syscall per
  look; a filesystem existence check is one syscall per look; a directory listing is three or four
  and two library allocations per look (§3.6). The transport decides the *shape* of the look; the
  bridge alone decides *how often* it looks, and that is where the idle cost is set (§3.7).

Everything that distinguishes the three options is therefore in what happens *beside* the tick:
per-request overhead, idle overhead, failure behaviour, and what the option costs to stand up.

### 2.2 The three options against each other

| | File directory, atomic rename | TCP via LuaSocket, loopback | UDP via LuaSocket, loopback |
|---|---|---|---|
| Latency per round trip | one tick + client poll granularity (§3.1) | one tick | one tick |
| Per-request overhead beside the tick | ~10 filesystem operations (client write+rename; bridge list, open, read, remove, write, rename; client list, read, remove). Listing an empty directory costs 0.098 ms per tick (S4). The rest is unmeasured in the tree and is bounded well under a frame on a local NTFS volume | 2 `send`, 2 `recv`, one `select` per tick | as TCP |
| Throughput per tick | every `.req` present is answered in one tick, in name order (`prior:tools/hooks/DcsApiEval.lua:1974-1979`) — pipelining already exists on the bridge side and is unused by the client, which is strictly serial (`prior:pipeline/src/bridge/client.ts:465-469`) | as many frames as the tick's CPU budget allows, if the bridge reads more than one | as TCP |
| Idle cost per look, no request pending | today: a directory listing — open, read, read, close, five heap allocations, a sort (§3.6; 0.098 ms measured in DCS, S4). Restructured: one `lfs.attributes` on a fixed path, zero allocations, at one look in eight frames (§3.7) | one non-blocking `select`/`recv` per look, zero allocations if the buffer is preallocated, at whatever cadence the bridge chooses; plus a listening socket held open by `DCS.exe` for the whole play session | as TCP, minus the connection: a bound datagram socket held open for the session |
| Idle cost while installed permanently | nothing held open: the transport is a directory that exists whether or not anything reads it | a port held open on `DCS.exe` for every hour the game runs, including online play, whether or not a client will ever connect | as TCP |
| Message size | unbounded; the bridge caps a request at 256 KiB (`prior:tools/hooks/DcsApiEval.lua:253`) | stream; framing needed | one datagram ≤ 64 KiB; a 256 KiB request needs fragmentation and reassembly — that is TCP rebuilt by hand |
| Reply survives the requester dying | yes; swept after 300 s (`:252`, `:1925-1936`) | no; the connection dies with it | no |
| Reply survives DCS dying | yes, on disk — a reply already published is a captured measurement | no | no |
| Request survives DCS dying | yes — **and this is the hazard**: a request written as DCS quit ran on the next launch and killed it (`land.getHeight` in the menu, 2026-09-02; `:1885-1896`, `:2127-2134`). §4 closes it structurally | no — a reset is an unambiguous death signal, which is the one thing TCP does better | no |
| Silence is ambiguous (load vs hang vs dead) | yes; resolved by the heartbeat file and the PID (§4.4) | yes; the connection stays up through a load and a hang alike | yes, worse: no connection to lose |
| Two hosts in one process | two directories (`prior:pipeline/src/bridge/paths.ts:259-272`, D94) | two ports | two ports |
| Availability | `lfs` and `io` present in both hosts, measured (D94) | `require('socket')` in `hook`: **unmeasured**; loads a C module into `DCS.exe` from a hook | as TCP |
| Standing it up | nothing: a directory | a listening port on `DCS.exe`, a firewall grant, a port-collision rule against DCS's own `net`, a non-blocking accept loop that must never block the sim thread | a bound port and a firewall grant |
| The arbitrary-code channel | reachable only by a process that can write into a directory under the user's profile | reachable by any local process, any local account, on loopback | as TCP |
| Crash attribution | the request file is removed before the chunk runs (`:1873-1877`) so a killer does not re-run; the events log marker names it (§4.5) | the killer is in the socket buffer of a dead process | as TCP |

### 2.3 The decision: file-based, restructured

**File-based.** What decides it is §2.1: the tick is the floor for every option, so a socket cannot
buy a frame, and what it could buy — the filesystem operations beside the tick — is a fraction of
one frame that pipelining amortises across a window of replies (§3). Against that, the file
transport has the two properties the crash-safety requirement (§4) wants and a socket cannot
provide: a reply already published survives the process that wrote it, and death presents as a
*different stamp on disk* rather than as silence. The socket's one real advantage, the reset as a
death signal, is recovered by the PID in the handshake (§4.4). Its costs — an unmeasured `require`
in `hook`, a listening port on `DCS.exe`, a network-reachable eval channel — are each individually
sufficient to reject it for a development tool on one machine. UDP is rejected outright: a 48 KiB
reply fits a datagram and a 256 KiB request does not, and reliability over loopback is not zero.

**Reopen condition.** A measurement showing the per-round-trip filesystem cost above 5 ms at p50 on
the target machine, or DCS exposing a Lua execution context off the simulation thread, or (§2.4) a
dormant frame measured above today's 0.098 ms. None is an argument; all three are numbers.

### 2.4 Re-derived under the idle constraint: the decision does not move

The idle requirement (§0) was not in front of §2.3 when it was written, so the question is asked
again from the start rather than answered by an appended paragraph: *of the three transports, which
costs least per frame when no request exists and none is coming, installed permanently, while the
game is being played?*

**What a blocking socket would buy, and why it cannot be had.** A thread blocked in `recv` costs
nothing until a byte arrives; that is the one shape whose idle cost is genuinely zero. DCS's Lua has
no thread to block (§2.1): the only execution the bridge gets is inside a callback on the simulation
thread, and blocking there is a hung frame. So the socket the bridge could actually use is a
non-blocking one, polled from `onSimulationFrame` — and a poll is a look, exactly like a stat.

**A look is a syscall whichever transport it is over.** A non-blocking `select` on a loopback socket
enters the kernel once. `lfs.attributes(path, 'mode')` on a path in a directory the process touched
a frame ago enters the kernel once and is answered from the cache manager without I/O; nothing in
the request path has touched a disk since the directory was created. The two were timed against each
other on this machine outside DCS (§12): 0.80 ms for a missing-file stat, 0.86 ms present, 1.00 ms
to enumerate an empty directory — every one of them inflated by a filter driver in this session's
sandbox to eight times S4's in-DCS figure, so the **absolute** values say nothing about DCS and are
not used anywhere in this document. The **ratio** does say something: an existence stat and an
empty-directory enumeration are within 25% of each other at the kernel boundary. The syscall shape
is not where the saving is. The saving is in what Lua does around the syscall — five allocations
and a sort today, none in the restructured design (§3.6) — and in how often the look is taken
(§3.7).
Both are the bridge's choices, and both are available identically over a socket and over a file.

**What is left is the fixed cost of standing up, and permanence makes it worse for the socket.**
§2.3 rejected a socket for a development tool because of a listening port on `DCS.exe`, a firewall
grant and a network-reachable eval channel. Those were costs of a session; under §6.3 they become
costs of every hour the game runs, including hours online, whether or not a client will ever
connect. The file transport holds nothing open: its idle state is a directory that exists. And the
socket's one advantage in §2.2 — reset as a death signal — is worth nothing to a bridge that is
dormant by design, because a dormant bridge is silent on every transport and §4.7 has to resolve
silence with the PID either way.

**So: file-based, unchanged, for a stronger reason than before.** The idle constraint cannot be met
by any transport's shape; it is met by the bridge doing nothing between looks and by looking
rarely, and the file transport does that with no standing resource. What the constraint *does*
change is inside the bridge and the client — §3.6 to §3.8, §4.7, §6.3, §7.2, §8 — and each says so
where it applies.

**Restructured, in five ways.** Each is specified in full in the section named.

1. **One transport directory per bridge session**, named by the session stamp — §4.2. This is what
   makes a request in flight at a crash structurally invisible to the next session.
2. **Pipelining with a window** — §3.3. The client keeps up to W requests published; the bridge
   answers as many as its per-tick budget allows, in id order.
3. **A per-tick CPU budget in the bridge** — §3.4. Bytes and node counts bound a reply; CPU time
   bounds a tick, and the reply reports what it cost.
4. **An event-driven wait in the client** — §3.2. `fs.watch` on the reply directory with a poll
   fallback, replacing a fixed 25 ms sleep that is a third of today's round trip.
5. **Every request addressed to a session, no exceptions** — §4.3. The "request dropped in by hand
   while DCS was shut down is one the person still means" rule (D36, D37) is withdrawn; it is the
   rule under which the 2026-09-02 kill happened.
6. **Dormant by default, armed by a file** — §3.6–3.8, added under the idle constraint of §0. The
   per-frame listing becomes the *armed* path, entered when a client creates `<session>/arm` and
   left after `QUIET_S` without a request; the dormant path is a counter and one stat in eight
   frames. This is the one restructuring that changes what the bridge does when nobody is using
   it, and the one the maintainer's requirement is about.

What is kept unchanged, because each was bought with a failure: publish by rename, never by writing
the final name (`prior:tools/hooks/DcsApiEval.lua:457-475`; the consumer's half at
`prior:pipeline/src/bridge/client.ts:387-408` and `protocol.ts:260-273`); the `.tmp` written in the
destination directory so the rename never crosses a device; `os.remove` before `os.rename` on a file
rewritten in place, because Windows will not rename onto an existing name; the scanner reading
nothing but an exact `.req` suffix and the collector nothing but an exact `.res`; the request file
removed *before* its chunk runs, so a killer never runs twice (D37); bytes read with `'rb'` and
written with `'wb'`, and a reply read into Node as one codepoint per byte (`prior:pipeline/src/
bridge/fs.ts:18-24`), because ED's strings mix UTF-8 and cp1251 and 599 of 986 descriptions in the
model are Russian (`prior:pipeline/src/bridge/client.ts:508-524`).

---

## 3. Performance

### 3.1 Where the time goes today

The measured shape of a full census, from the committed reports:

```powershell
Get-Content prior:generated/conc1.md -TotalCount 10
# census `hook`, root `_G` — stopped exhausted after 2543 replies in 86.9 s
# identities 382801; round trip mean / p50 / p95 / max  33 / 30 / 37 / 1207 ms
Get-Content prior:generated/census/gui-sp-menu-report.md -TotalCount 6
#   2937 replies, 445959 identities
Get-Content prior:generated/census/gui-sp-mission-loaded-report.md -TotalCount 6
#   3748 replies, 565941 identities
```

| Figure | Value | Source |
|---|---|---|
| Replies per generation, all states | 13,605, 456 MB raw, 32.7 KiB mean per reply | `prior:docs/reshape/captures.md:18` |
| `hook` census | 2,543 replies, 86.9 s, 382,801 identities → 34.2 ms per reply, 150 identities per reply, 4,405 identities/s | `prior:generated/conc1.md:3-10` |
| `gui` census, menu / mission loaded | 2,937 / 3,748 replies; 445,959 / 565,941 identities | `prior:generated/census/*-report.md:3-6` |
| Round trip, driven from Node | mean 33, p50 30, p95 37, max 1,207 ms | `prior:generated/conc1.md:10` |
| Round trip, measured inside Lua | 14 ms median, 30 ms p95, one tick each | S4 |
| Frame | 17.5 ms at 57 Hz | `prior:pipeline/src/census/sizing.ts:6` |
| Client poll interval | 25 ms | `prior:pipeline/src/bridge/client.ts:147` |
| Per-reply budgets | 200 nodes, 2,000 keys, 49,152 bytes, ceiling 61,440 | `prior:tools/hooks/DcsApiEval.lua:273-279` |
| Chunk source compiled per census request | 2,757 + 28,219 = 30,976 bytes | §12 |
| Per-generation round-trip time at 34.2 ms | 13,605 × 34.2 ms = 465 s ≈ 7.8 min | arithmetic |

Reading the 30 ms p50 against a 17.5 ms tick: a reply is published at most one tick after the
request lands, and the client then sleeps in 25 ms steps, so the expected wait is one tick plus half
a poll interval — 17.5 + 12.5 = 30 ms, which is the measured p50. **A third of every round trip is
the client's own sleep.** The next third is the tick. What the walk itself costs inside the tick is
unmeasured: no reply carries it, and the tree holds no per-page CPU figure. The max of 1,207 ms is a
stall of the kind S4 recorded outside loading (8.3 s at the menu, 2.2 s in the sim); it is not the
transport.

The filesystem's share is not measured either. Ten operations per round trip on a local NTFS
volume is bounded by a few milliseconds and is the smallest of the four terms; it is not worth a
transport change (§2.3) but it is worth measuring once, which §3.5 requires.

Serialisation is not a term. The walk builds rows with `table.concat` on a fixed-arity `put`
(`prior:tools/hooks/DcsApiEval.lua:777-787`) and never concatenates strings in a loop, and the
consumer splits on `\t` in one forward pass (`prior:pipeline/src/bridge/protocol.ts:549-684`).
Compiling 31 KB of chunk source per request is 13,605 × 31 KB ≈ 420 MB of `loadstring` per
generation, which at any plausible compile rate is seconds, not minutes. **Since §7.0 that cost is
the consuming project's**, and so is the choice D43 made on the bridge's behalf: D43 rejected
parking a compiled walk in the target state because the *bridge* would then be calling a function
that lives in the state. That stands for the bridge, which now calls nothing but the chunk it was
handed. A consumer's chunk that installs its walker under its own global and calls it on the next
request is the consumer's decision, taken against D43's reasoning and against D37's rule that an
evaluation never calls across a state boundary.

### 3.2 The wait: event-driven, with a poll fallback

The client's `wait` replaces its fixed sleep with a watch on the reply directory
(`fs.watch`, which on Windows is `ReadDirectoryChangesW`), waking on any event and then listing the
directory once. A poll at 25 ms remains as the fallback for a watch that reports nothing, because
`fs.watch` is documented as unreliable on some filesystems and the failure would be silent. Expected
effect: the 12.5 ms poll term disappears, and the round trip's p50 approaches one tick. Measured,
not assumed: §3.5 requires the new client to report its own p50 against S4's 14 ms.

### 3.3 Pipelining: a window of requests, answered in id order

The throughput case is a cursor: a driver that publishes the same chunk again and again to page a
walk forward. The census was that case, and its driver published one request, waited, collected
and published the next (`prior:pipeline/src/census/run.ts:154-163`). Under §7.0 the walk and its
driver belong to the consuming project; what the protocol keeps is the shape that makes such a
driver fast. The bridge already answers every request it finds in a tick, in sorted name order.

- The client library keeps up to **W** requests published (default 8, per call configurable). A
  request id is `<seq>-<tag>` where `<seq>` is a zero-padded 10-digit per-client counter and
  `<tag>` is a short random string, so ids sort in publication order and never collide across two
  clients sharing a session.
- The bridge lists `req/`, sorts, and answers in that order until the tick budget (§3.4) is spent;
  what it did not reach stays for the next tick. Replies therefore arrive in request order.
- A cursor driver publishes its next-page `eval` W-deep and consumes replies in order; when a
  reply's own record says the cursor is closed, the driver drains the replies already in flight.
  Each of those is an evaluation against a closed cursor, and making that harmless is the
  consumer's record grammar, not the protocol's.
- Pipelined requests share a tick, so **a request that kills DCS kills its neighbours' replies
  too**. The events log marker (§4.5) names which request was running; the neighbours are simply
  unanswered and the client reports them `superseded` (§4.4) like any request in a dead session.
- Any caller may pipeline `eval` and order is preserved; the MCP and CLI skins send one at a time,
  because an interactive question is not a throughput case (`mcp.md` §6).

Projected, not measured: at four pages per tick under an 8 ms budget the `hook` census's 2,543 pages
take 636 ticks ≈ 11 s of frame time plus whatever the consumer's walk costs, against 86.9 s today.
The projection is only as good as the unmeasured walk cost, which is why §3.4 makes the bridge
report it on every reply.

### 3.4 The per-tick CPU budget, and the reply reports its cost

**Revised under §7.0: three protections that existed only around `census` now bind every
evaluation.** The tick budget below, the reply ceiling and the refusal-instead-of-truncation rule
of §7.7 were written for the walk because the walk was the first long evaluation; a long `eval`
stalled the same thread with none of them. None is census-specific, so each is stated for `eval`,
which is now the only op that runs anything.

- The bridge measures each tick's work with `os.clock()` (present in both hosts; process CPU time,
  adequate for an interval, D94) and stops taking new requests once the tick has spent
  `TICK_BUDGET_MS` (default 8, of a 17.5 ms frame). A request in progress is never pre-empted by
  this budget; the check is between requests, and the instruction budget below is what bounds one
  chunk.
- Every reply carries `cpu_ms`, the CPU time its own handling took, and `tick`, the bridge's tick
  counter when it was answered. Two replies with the same `tick` shared a frame.
- What bounds a *reply* is the ceiling of §7.7, `max_result_bytes`, and a result over it is
  **refused, never cut** (`stage: oversize`, with `result_bytes`). What bounds a *page* is whatever
  the consumer's own chunk clamps — nodes, keys, bytes, depth, as the walk did (D43) — and a
  consumer that wants its pages under the ceiling reads `max_result_bytes` from the handshake and
  clamps below it, as the walk held `CENSUS_BYTES` under `MAX_RESULT_BYTES − 4096`
  (`prior:tools/hooks/DcsApiEval.lua:278-279`). The bridge clamps nothing on a consumer's behalf.
- A page budget large enough to exceed the tick budget on its own is legal and simply serialises
  one reply per tick.
- The driver may raise its page budget adaptively on `cpu_ms` — climb while a reply costs under
  half the tick budget, back off above it — and this is a driver policy, not a protocol rule.

**One protection that did not exist and now does: an instruction budget inside the chunk.** The
tick budget is checked between requests, so a single chunk that loops forever stalls the frame
until DCS is killed, and nothing above could stop it. A request may carry `max_instructions`
(default `INSTRUCTION_BUDGET`, a constant at the top of the file, clamped to `INSTRUCTION_CEILING`;
`0` disables it), and the wrapper §7.3 compiles the body under installs a count hook,
`debug.sethook(fn, '', n)`, inside the target state before the chunk runs and clears it after.
Exceeding it is `error` with `stage: budget`. Three limits are stated rather than hidden: the hook
counts VM instructions and cannot interrupt one long C call into DCS; a chunk can clear the hook
itself, so this is a guard against accident and not a boundary; and `mission` has no `debug`
(§1, S5), so a reply from there carries `budget: none` and a caller reads it. A count hook is
reported safe in the `scripting` state by an unverified lead, which §11 lists. The trap the
harness must cover is a documented Lua 5.1 behaviour and is checkable without DCS: Lua 5.1 installs
*no hook at all* for a count of `0` or `nil` while `debug.gethook()` still returns the function, so
a constant that is not a positive integer is refused at load and never defaulted silently.

### 3.5 What the first live run must measure

The new design changes three terms it cannot measure offline, so the first live run reports, per
state: round trip p50/p95 with the event-driven wait; `cpu_ms` per reply at the consumer's default
page budget; replies per tick achieved under pipelining at W = 8; and the wall time of a full
seven-state generation — a consuming project's figure, measured through this bridge — against the
465 s the current design costs in round trips alone. A run that does not print these has not
measured the change.

It measures a fourth term, the one the maintainer's requirement is actually about: **the dormant
cost inside DCS**. `cpu_ms` cannot carry it — a dormant frame is below `os.clock`'s resolution — so
it is measured two ways. In the bridge, `ping` reports `dormant_cpu_ms_per_1000_ticks`: the bridge
reads `os.clock()` once at each disarm and once at the next arm, and divides by the ticks between,
which costs two clock reads per transition and nothing per frame. Outside the bridge, DCS's own
frame-time counter over one fixed scene, three ways — hook file absent, hook installed and dormant,
hook installed and armed with an idle client — because "no noticeable performance impact" is a
statement about frame time and only frame time answers it. A dormant figure above today's 0.098 ms
per frame reopens §2 (§2.3).

### 3.6 The dormant path today, and the budget it must meet

**What a frame costs today with no request pending.** Read off `prior:tools/hooks/DcsApiEval.lua`
and produced by the command in §12; the `export` host is the same shape at `prior:tools/export/
DcsApiExport.lua:1683-1699`.

| Step, every frame | Line | Heap allocation | Kernel |
|---|---|---|---|
| `guard` builds the closure it hands to `pcall` — on every invocation, not once at registration | `:2141-2142` | 1 closure | — |
| `B.disabled` read; it is set only at `:2012`, `:2024`, `:2049`, `:2078`, all before `DCS.setUserCallbacks` at `:2181`, so at runtime it guards nothing | `:1971` | — | — |
| `B.ticks + 1` | `:1972` | — | — |
| `B.transport .. '/req'` — Lua 5.1 interns every string, so this is a hash and a string-table lookup of a ~60-byte path per frame rather than an allocation | `:1974` | — | — |
| `list_dir`: `names = {}`; the closure handed to `pcall`; `lfs.dir` returns an iterator function and a directory object (`:485-487`) | `:489-491` | 1 table, 1 closure, 1 iterator, 1 directory object | open, read `.`, read `..`, read end, close |
| `table.sort` on the empty result | `:496` | — | — |
| `wall()` → `pcall(DCS.getRealTime)` and the heartbeat interval test | `:1981-1982` | — | — |
| **Per frame** | | **5** | **4–5 calls** |
| Every 2 s: `heartbeat()` — 10 concatenations, a `table.concat`, a `%.3f` format, an `os.date`, then `write_atomic`'s open, write, close, remove, rename | `:1943-1958`, `:462-475` | ~14 strings, 1 table, 1 file handle | 5 |
| Every 2 s: `sweep_responses` — a second listing, and `lfs.attributes` on every entry | `:1925-1936` | as `list_dir` | 4–5 + one per entry |

At 57 Hz that is 285 heap allocations and roughly 260 kernel entries per second, in perpetuity, in
the menu and in the cockpit alike, while nothing is or will be asked. S4 measured the listing at
0.098 ms per tick, 0.59% of a frame. The maintainer's observation that this is not noticeable is
consistent with that figure; it is also the baseline this section must hold.

**The budget.** "As little work as possible" is not a specification, so here is one. A frame on
which the bridge is dormant and not due to look:

- **Exactly:** one `pcall` of a function that already exists (the callback wrapper, built once at
  registration — the closure at `:2142` moves out of the per-call path); inside it, one table
  read, one integer add, one table write, one integer comparison, one return.
- **Zero** heap allocations, from any source: no closure, no table, no string. Every string the
  dormant path could need — the arm path, the `req/` path — is built at load and held in an
  upvalue.
- **Zero** kernel entries. No `lfs`, no `io`, no `os`, no `DCS.getRealTime`.
- **Zero** concatenations, **no** sort, **no** heartbeat, **no** sweep.

A frame on which the bridge is dormant and due to look (§3.7: one in `PROBE_EVERY`, default 8) adds
**exactly one** C call, `lfs.attributes(ARM_PATH, 'mode')` with `lfs.attributes` cached in a local
at load, and the one kernel entry that call makes. Its result is `nil` or the interned string
`'file'`; neither allocates. Nothing else is added. Averaged over eight frames the dormant path is
therefore one kernel entry per eight frames against four or five per frame today, and zero
allocations against five — the collector, which today absorbs 285 allocations a second on the
bridge's behalf, sees nothing from it at all.

The harness holds the budget as a control (§10): 100,000 dormant ticks with the collector stopped
grow `collectgarbage('count')` by zero, and 12,500 of them call the stubbed `lfs.attributes`
exactly 12,500 times and `lfs.dir` never.

### 3.7 Arming: how the bridge is woken, how cheaply, and how it goes back to sleep

The bridge has two states. **Dormant** is the default at load and the state it returns to.
**Armed** is the state §3.1–3.5 cost: the request directory is listed every frame, the tick budget
applies, the heartbeat is written every 2 s, the sweep runs. The transition is driven by one file.

**The arm file.** `<session>/arm` (§7.2), named absolutely in the handshake as `arm`. Its content
is nothing; its existence is the signal. **The client creates it; the bridge removes it.** Nothing
else touches it.

**The wake check.** Every `PROBE_EVERY`-th tick (default 8) a dormant bridge calls
`lfs.attributes(ARM_PATH, 'mode')`. That is the entire check, and it is the only thing a dormant
bridge ever does beyond counting. It has to be cheaper than the work it avoids or it is no saving:
one stat in eight frames against a listing, a sort and five allocations every frame is a reduction
of roughly thirty kernel entries and forty allocations per look, so it is. Any result other than
`nil` arms the bridge: it writes the heartbeat with `armed: yes` (§4.7), records `os.clock()` for
the dormant figure (§3.5), and handles the `req/` directory on the same tick, so a request published
before the arm file was noticed is answered on the tick the bridge wakes.

**Wake latency, which is what the client pays.** At most `PROBE_EVERY` frames to notice the arm
file plus the one tick every request costs anyway: nine frames, 158 ms at 57 Hz (§12), on the
*first* request after a dormant period and on no other. An interactive tool call with a 15 s wait
(§8) does not notice it; a long run pays it once (§3.8). A client that cannot afford it keeps
the bridge armed by keeping a request in flight, which is what a client that cannot afford it is
doing anyway.

**Disarming.** An armed bridge that has seen no `.req` for `QUIET_S` seconds (default 3, measured
with the `wall()` the armed path already reads every frame) disarms, in this order, because the
order is what makes the race below unwinnable:

1. `os.remove(ARM_PATH)`.
2. List `req/` once more.
3. If the listing holds a `.req`, recreate the arm file, stay armed, and handle the listing as an
   ordinary tick. Otherwise: run the sweep once, write the heartbeat with `armed: no` and `since`
   (§4.7), read `os.clock()`, set the next probe tick, and return to dormancy.

**The client's obligation**, in the library's `send` (§8): after publishing a request by rename,
stat the arm file and create it if it is absent. That is one extra kernel entry per send on the
client's thread, which is not the simulation thread, and it costs the bridge nothing. The client
never removes the arm file.

**Why this cannot strand a request.** Suppose a request `R`, published at `t₂`, sits in `req/`
while the bridge is dormant. The bridge is dormant, so its last disarm listed `req/` at some `b₂`
without seeing `R`, so `b₂ < t₂`; its removal of the arm file was `b₁ < b₂`. The client's stat is
at `t₃ > t₂ > b₁`. Either the arm file is absent at `t₃` — then the client recreates it — or it is
present, because the client or another client created it after `b₁`. In both cases the arm file
exists while the bridge is dormant, the next probe sees it within `PROBE_EVERY` ticks, and the
bridge lists `req/` and finds `R`. If instead the bridge was still armed at `t₂`, it lists `req/`
every frame and finds `R` on the next one. There is no third case. A bridge that finds the arm
file present with nothing to do — a client that died between creating it and publishing — arms,
sees nothing for `QUIET_S`, removes the file and disarms: an abandoned arm file costs one quiet
period and then nothing, which is also what a dead client that left state in a target costs (§3.8).

**What is deliberately not the signal.** The modification time of `req/` — Windows updates a
directory's timestamp on entry creation and removal, and one stat would then need no second file —
is rejected because the guarantee is a property of NTFS's lazy timestamp propagation that nobody
has measured from inside DCS, and `lfs.attributes` returns whole seconds. The existence of a named
file has no such caveat. A `ReadDirectoryChangesW` equivalent is not available to Lua without a C
module (D3, D36 §Rejected, the named pipe). A slower dormant probe than one in eight frames is a
knob, not a design change: `PROBE_EVERY` is a constant at the top of the file, reported in the
handshake so the client computes its own wake deadline from it (§4.7).

### 3.8 Dormancy and a long run: one design, both cases

A long run — the census was one — wants the bridge maximally responsive for hours; the playing
case wants it maximally quiet. They are the two states of §3.7 and nothing else is needed:

- A run arms the bridge with its first request and keeps it armed for its whole duration without
  any further arming step, because a W-deep pipeline (§3.3) never lets `req/` be empty for
  `QUIET_S`. The armed bridge is the bridge §3.1–3.5 cost, unchanged: every reply in one tick, the
  tick budget, `cpu_ms`, the heartbeat every 2 s.
- The transition into a run costs one wake: nine frames, once. The transition out costs
  `QUIET_S` of armed-idle ticks after the last reply — 171 listings at 57 Hz, 16.8 ms of CPU spread
  over 3 s (§12) — and one sweep, then dormancy. A run that ends by saying so pays the same as one
  whose driver died: nobody has to say goodbye.
- An interactive client whose calls are more than `QUIET_S` apart pays a wake per call. That is the
  right trade: a person reading a reply for four seconds is not using the bridge for four seconds,
  and the game they are alt-tabbed from is running.
- State a consumer's chunk leaves in a target state — a walker's frontier and identity map under
  the consumer's own global, which is where the census kept them (§7.0, §7.8) — does not hold the
  bridge armed. It survives dormancy untouched, because dormancy is a property of the bridge and
  the state is not the bridge; the next request after a quiet period wakes the bridge and the chunk
  finds its global where it left it. The alternative — armed while any such state exists — is not
  expressible now that the bridge does not know what a consumer keeps, and it was already the
  requirement's failure case: a dead driver holding the bridge at full cost forever.

Nothing in the armed path changed to make room for the dormant one; the armed path is what the
bridge already is, entered on demand and left on silence.

---

## 4. Crash safety

**Requirement, named.** DCS dies mid-request routinely (`prior:tools/probes/crashermatrix_targets.
lua:53-55` expects five of seven bare cells to end the process). **A request in flight when DCS dies
must not be carried into the next session**, a reply from a dead session must be recognised and
discarded, and the client must learn that its request died rather than wait on it.

### 4.1 What exists today, and what it got wrong once

- A load stamp `tostring(os.time())` identifies the running copy (`prior:tools/hooks/DcsApiEval.
  lua:322`), goes into the handshake and every reply, and — since 2026-09-02 — a request carries
  `for: <stamp>` and is answered `stale-session` without running when the stamp is not the bridge's
  own (`:1885-1896`, commit `c8fc7e2c`). The first build cleared nothing, and a request written as
  DCS quit ran on the next launch (`:2127-2134`).
- A request without `for` still runs on whichever session finds it (`:89-93`): the hand-dropped
  request rule.
- Replies from a previous session are deleted at load (`:2117-2125`); uncollected replies are swept
  after 300 s (`:1925-1936`).
- The probe supervisor names a killer from the probe's own progress file: `B|<label>` before a call,
  `O|<label>|…` after, and the last unbalanced `B|` in file order is the call the process died in
  (`prior:tools/probes/Invoke-ProbeSupervisor.ps1:256-307`, `prior:pipeline/src/probe/capture.ts:
  1-6`). It then waits for the process to exit, archives the crash artefacts, clears the request
  directory by delegation (`:980-987`; `prior:tools/probes/Invoke-LadderWave.ps1:32-34`),
  relaunches, waits for the bridge and for a mission, and resends (`:917-1003`).
- The client never calls a silent bridge dead: a deadline yields `pending` with the bridge's phase
  (`prior:pipeline/src/bridge/client.ts:445-463`), because any timeout short enough to be useful is
  wrong during a load (D37).

Two gaps. The stamp is a header a request may omit, so fencing depends on every writer remembering
it; and a `pending` outcome never resolves to *dead* — the supervisor decides death from the process
list, outside the client. Both are closed below.

### 4.2 A session is a stamped directory

- **Stamp:** `<os.time()>-<os.getpid()>` in both hosts (§1 measured `os.getpid` present in both).
  Two launches in one second cannot collide, and a stamp names a process that can be checked for
  liveness. Where `os.getpid` is absent the bridge refuses to run, which is a visible failure rather
  than a weaker fence; `ping` and the handshake state which.
- **Transport root:** `<lfs.tempdir()>/dcs-api/<host>/`, falling back to `<output>/rpc/` under
  `Logs\` when temp is refused by the containment guard — the same guard and the same fallback order
  as today (`prior:tools/hooks/DcsApiEval.lua:2031-2073`, D37).
- **Session directory:** `<root>/<stamp>/req/` and `<root>/<stamp>/res/`, created at load, and
  `<root>/<stamp>/arm`, created by a client and removed by the bridge (§3.7). The handshake names
  all three absolutely.
- **At load, before anything else is written**, the bridge removes every sibling directory under
  `<root>` whose name is not its own stamp — requests and replies together. Each is a session that
  has ended, and nothing in it is addressed to this one. This replaces "clear responses, keep
  requests" (D36, D37 §Rejected) and replaces the supervisor's `-ClearRequests` step, which is kept
  as a no-op-safe command and is no longer load-bearing. A sibling that cannot be removed — on
  Windows, a directory on which a client still holds a handle, which a client watching `res/`
  through `ReadDirectoryChangesW` does — is logged and left, and the next load tries again. A
  client therefore watches a session directory only while it has a request in flight there, and
  never one it has learned is superseded (`mcp.md` §6).
- **The output directory** (`<lfs.writedir()>/Logs/DcsApi/<host>/`) holds the handshake, the
  heartbeat and the events log and is never swept: it is the durable half and the one place both
  processes can compute without agreeing on anything (D37).

A request written into a dead session's directory is therefore never listed by the next session,
whatever it says inside. That is the structural half of the requirement.

### 4.3 Fencing inside the directory

- Every request **must** carry `for: <stamp>`. One without it is answered `bad-request` and not run.
  The hand-dropped request is not a case any more: a person or a tool reads `bridge.txt`, which
  names the current session's directory and stamp, and writes into that. A request meant for a
  session that has gone is, by construction, in a directory that has gone.
- A `for` that does not match the bridge's own stamp inside its own directory is a client defect;
  it is answered `stale-session` and not run, as today, so the defect is visible.
- Every reply carries `stamp`. The client compares it with the stamp of the session it addressed and
  discards a mismatch as `foreign` — structurally impossible under 4.2 and kept as a cheap assertion
  because the consequence of the structure failing is running a chunk in the wrong session.

### 4.4 How the client learns its request died

`wait` never returns an error on time alone. On every wake it reads two small files and, when they
are stale, checks one process. The heartbeat's `armed` field (§4.7) decides whether its age means
anything: a dormant bridge stops writing it by design, so age is evidence only while `armed: yes`.
"Sent" below is the client's own clock at the moment `send` returned, which is after the arm file
was ensured (§3.7).

| Reading | Outcome | Meaning |
|---|---|---|
| `bridge.txt` stamp ≠ the session the request was sent to | `superseded` | DCS restarted. The request lies in a directory the new session will never list; it did not and will not run. A census session in the state is gone with the process |
| `armed: yes`, heartbeat age ≤ 10 s | `pending` | alive and ticking; the reply is on its way or the request is queued behind the tick budget |
| `armed: no`, sent < 10 s ago, `pid` running | `pending`, flagged `waking` | the bridge was dormant when the request was published and has up to `PROBE_EVERY` + 1 frames to notice the arm file; the client expects a heartbeat with `armed: yes` before the 10 s elapse |
| `armed: no`, sent ≥ 10 s ago, `pid` running, phase `load` | `pending` | a mission load; nothing fires for its duration (S4), including the probe |
| `armed: no`, sent ≥ 10 s ago, `pid` running, any other phase | `pending`, flagged `stalled` | the bridge did not wake in 10 s. `status()` distinguishes the two causes: the arm file is absent from the path the handshake named — a client defect, reported as such — or it is present and the bridge is not ticking, which is the stall case below |
| heartbeat age > 10 s, `pid` from the handshake **not running** | `dead` | DCS is gone and no new session has started. The request did not run |
| `armed: yes`, heartbeat age > 10 s, `pid` running, phase `load` | `pending` | a mission load; nothing fires for its duration (S4) |
| `armed: yes`, heartbeat age > 10 s, `pid` running, any other phase | `pending`, flagged `stalled` | a stall, a hang, or a crash dialog holding the process open (`prior:tools/probes/Invoke-ProbeSupervisor.ps1:700-721`). The client says so and keeps waiting; the supervisor's `Wait-DcsExit` is what resolves it |

`superseded` and `dead` are terminal: the request's id is returned so a caller can log it, and
nothing will ever collect it. `pending` carries the id and the phase, as today, and is collectable
later (`prior:pipeline/src/mcp/tools.ts:227-237`). The PID check on Windows is
`process.kill(pid, 0)` or an equivalent existence probe; it costs nothing in DCS.

### 4.5 Attribution: the bridge writes the supervisor's markers

The bridge appends to `<output>/events.log` before and after every request it handles:

```
B|<id>|<op>|<state>|<stamp>
O|<id>|<status>|<cpu_ms>
```

This is the probe progress file's grammar (`prior:pipeline/src/probe/capture.ts:1-6`), so the
supervisor's reader (`Get-CrasherLabel`) names a *bridge* request that killed DCS the same way it
names a probe call — the last `B|` with no `O|`. Today a request that kills the process leaves no
record of which it was, because the request file is deleted before the chunk runs (correctly) and
the events log records only raises. The `missionscripting` door adds its own markers through
`log.write` inside `mission`, which has no `io` (§5.3).

### 4.6 The supervisor, under the new layout

The loop is unchanged in shape — name the killer, wait for the exit, archive, relaunch, wait for the
bridge, wait for a mission, resend — and gains two things. The killer can now be a bridge request
by id, read from `events.log`. And "clear the transport" is no longer a step it depends on: a
relaunch creates a new session directory and sweeps the old one, so a supervisor that crashes
between the archive and the clear leaves nothing that can run. The step stays in the script as a
belt for a bridge that failed to load at all.

Nothing a consumer's chunk keeps in a state is resumable across a restart, and this document does
not pretend otherwise: a walker's identity map lives in the state (D41, D43) and dies with the
process. What survives is every reply already captured, and a driver that meets `superseded`
starts over and says so.

### 4.7 Dormancy against the heartbeat: which one gives

§4.4 as first written resolved silence with a heartbeat written every 2 s. §3.7 stops writing it.
A dormant bridge that kept writing it would not be dormant — the heartbeat is fourteen string
allocations and five kernel entries (§3.6), which over an evening is the largest single term left —
and a dormant bridge that stops writing it is, to the §4.4 table as written, indistinguishable from
a hung one. The two sections cannot both stand, so this is the resolution, and §4.4's table above is
already the resolved form.

**The heartbeat becomes event-driven while dormant and periodic while armed.** It is written:

- every 2 s while armed, as today, and this is the only periodic write the bridge makes;
- at every arm and every disarm, carrying `armed: yes|no` and `since`, the wall-clock time of that
  transition;
- on every phase change, dormant or not, as today (`prior:tools/hooks/DcsApiEval.lua:1988-1998`):
  a phase change is a callback firing perhaps five times per mission, and the `load` row of §4.4
  depends on it being written while dormant.

**Liveness while dormant is the PID and nothing else.** A heartbeat reading `armed: no` says the
bridge chose silence; its age says when, not whether the process lives. `wait` and `status()` read
`armed` before they read the age, and a stale heartbeat from a dormant bridge is the expected
state, never a flag. What replaces the age as evidence of a *stalled* bridge is the wake deadline:
a client that ensured the arm file at `t` and sees no `armed: yes` heartbeat by `t + 10 s`, with
the PID alive and the phase not `load`, reports `stalled` exactly as it would for an armed bridge
that stopped ticking. The 10 s is the same threshold as before and covers S4's 8.3 s menu stall;
`PROBE_EVERY` frames at any frame rate above one per second is inside it.

**What the arm file adds to crash safety, and what it does not.** It lives in the session
directory, so a dead session's arm file is removed with the session at the next load (§4.2) and
cannot wake the next session; the handshake names it absolutely, so a client that read a stale
handshake creates an arm file in a directory that no longer exists and gets an error rather than a
silent no-op — the library reports that as `superseded` after re-reading `bridge.txt`. It does not
change §4.5: a request that kills DCS is still named by the last `B|` without an `O|`, and a
dormant bridge writes no markers because it runs no requests.

**What a dormant bridge leaves on disk for a client to read:** `bridge.txt`, unchanged since load;
`heartbeat.txt`, last written at the last transition or phase change; nothing else changing. A
`status()` call against a dormant bridge therefore costs the client two file reads and a PID probe
and costs the bridge nothing, which is the property that lets an MCP server's `dcs_status`
be called as often as an agent likes without the game noticing.

---

## 5. Every state handled natively, and the seventh state as a declared special case

### 5.1 Carriers per state

| State | Carrier from the `hook` host | Carrier from the `export` host |
|---|---|---|
| `hook` | `loadstring` + `setfenv` into the host's own `_G` (`prior:tools/hooks/DcsApiEval.lua:1586-1597`) | `unsupported` |
| `gui`, `scripting`/`server`, `mission`, `config`, `export` | `net.dostring_in(state, chunk)` (`:1600-1621`) | `unsupported` |
| `missionscripting` | two hops, §5.2 | `unsupported` |
| `export` | `net.dostring_in('export', …)`, once an aircraft exists (S4) | `loadstring` + `setfenv` in-state (`prior:tools/export/DcsApiExport.lua:1407-1418`) |

`net.dostring_in` is acceptable as a mechanism and its three answers stay apart on the wire, because
a reader that sniffs for one reads a refusal as a success (`prior:tools/hooks/DcsApiEval.lua:
1606-1616`): a Lua string is the chunk's result; `nil` is `refused` (this role may not reach the
state, the name is unknown — S7's client case — or the operator's policy gate refused it: DCS has
gated `net.dostring_in` behind `net.allow_unsafe_api` and `net.allow_dostring_in` in
`Config\autoexec.cfg`, enforced on 2.9.18 and reverted by a hotfix, so a refusal is a live runtime
condition and never a defect to fix by writing that file); the literal `'Invalid state name'` is
`invalid-state` (known, not available in this phase — `export` at the menu). Any other return is
described by type and never stringified with `tostring`, which runs `__tostring` (D37).

**The `export` host answers `export` and nothing else**, as today, because `net` is nil there (D94).
The refusal message names the other host. At `sp` both hosts answer `state: export`; every reply
carries `host`, so a capture read months later says which door it came through (D114).

### 5.2 `missionscripting`: the two hops, precisely

`net.dostring_in` reaches this state under no name (S17: all seven globals tables read in one
process, no two alike). Its door is `a_do_script`, a global of `mission` that runs a chunk in
exactly the environment a `DO SCRIPT` trigger action runs in, measured address for address (D116).
Reaching it is therefore:

1. **Hop one, `hook` → `mission`:** `net.dostring_in('mission', DOOR)`, where `DOOR` is a chunk the
   bridge composes from its own source and from the request's fields as `%q` literals — nothing a
   requester wrote is concatenated into code the far state compiles (`prior:tools/hooks/
   DcsApiEval.lua:1466-1516`). `mission` has `log.write` and no `io` (§1), so the door's markers go
   to `dcs.log` under a fixed subsystem tag (`:296`, `:1484-1488`).
2. **Hop two, `mission` → `missionscripting`:** inside `DOOR`, `pcall(a_do_script, FAR, a1, …, aN)`.
   `FAR` is the far chunk's source; `a1…aN` arrive in the far chunk as `...` with their Lua types
   preserved (`:1453-1461`), which is why the walk's prologue is passed as arguments rather than
   compiled into text. `a_do_script` is nil at the main menu and whenever no mission is loaded; the
   door checks it at the moment of use, never from a previous crossing (`:1489-1495`).
3. **The return list is shifted by one.** A far chunk returning `v1 … vN` arrives as `nil, v1 …
   v(N-1)`, so a lone value is dropped entirely. The far chunk ends `return payload, 0`; the door
   reads slot 2 and names the types of both slots in its failure message, so a build that corrects
   the shift is reported rather than read as an empty walk (`:1497-1512`; `prior:tools/probes/
   tabledepth.lua:102-104` holds the control).

`FAR` is always an `eval` body wrapped as §5.3 and §7.3 require; the reflect and census sources it
used to be are the consumer's chunks now (§7.0). The bridge composes the wrapper and a requester
never supplies `FAR` directly; the wrapper compiles the body as its own chunk under the request's
`chunkname`, so an error inside it is reported against the caller's file and line.

### 5.3 The return-shape limit is a protocol constraint, not a footnote

**A table returned through `a_do_script` corrupts DCS's pool allocator.** A 2,000-deep table
crossed, arrived usable, and DCS began writing a dump five milliseconds later; every fault the
instrument has produced sits in `ed_pool_free`, and the damage surfaces during the call, at mission
teardown, or on the next mission load — a clean run is evidence, never a clearance
(`prior:tools/probes/tabledepth.lua:1-8`, `:77-89`). A flat table of 5,000 keys has survived the
strict test twice and is in the instrument's default-allowed set (`:72-75`); a 100-deep table passed
a weaker test and then killed DCS under the strict one (`:68-70`).

The protocol takes the conservative reading and states it as a rule:

- **Nothing but a string crosses the door.** The far chunk for every op is wrapped so that its
  result is converted *inside `missionscripting`*: a string passes as is; a number, boolean or nil
  becomes its text with `result_type` stated; a table, function, userdata or thread yields an empty
  body and its type name. This is the same `describe_local` rule every state's `eval` already has
  (`prior:tools/hooks/DcsApiEval.lua:1557-1563`) — an `eval` never returns a table from any state —
  applied one hop further out so that the value never reaches the boundary. The flat-table allowance
  the instrument measured is **not taken** by the bridge; it exists for the instrument's own
  bisection and nowhere else.
- **The door refuses a non-string payload it is handed anyway**, without walking it, and reports the
  types of both return slots (`:1508-1512`). This is the backstop for a far chunk that escaped the
  wrapper, and it is not the mechanism.
- **The body size cap is the reply ceiling of §7.7**, as for every state, and a body over it is
  refused with `stage: oversize` rather than cut. D116 names
  `env.info` into `dcs.log` as the channel for a reply too large to return; the protocol does not
  use it, because a reply that arrives through the log is not a reply.

### 5.4 How a caller is told, rather than discovering it by truncation

- The handshake and every `ping` reply carry a `states` header, one entry per state the host
  answers, of the form `name:carrier=<local|dostring_in|a_do_script>,returns=<any|string>,
  needs=<always|menu|mission|slot>`. For the seventh state it reads
  `missionscripting:carrier=a_do_script,returns=string,needs=mission`.
- Every reply from `missionscripting` carries `carrier: a_do_script` and `via: mission`.
- A door that is shut answers a status of its own, `door-shut`, with a body saying no mission is
  loaded. Today this is an `error` with `stage: remote` and a message a consumer has to read
  (`:1491-1495`); a status is what a consumer can branch on.
- The MCP tool description and the CLI help for `state: missionscripting` state the constraint in
  one sentence, and the library's type for a reply from that state is narrowed to string bodies so
  a consumer cannot write code that expects otherwise.
- **The control on the door stays a real trigger.** A reading taken through `a_do_script` is worth
  as much as its agreement with one taken through a `DO SCRIPT` action, which is what
  `prior:tools/probes/s17/` measures with 16-bit user flags because a flag crosses the boundary
  where a Lua value does not (`prior:tools/probes/s17/s17_inject_1.lua`, D116). The new repository
  ports that fixture and runs it once per build.

`net.dostring_in` is itself present in `missionscripting` by the census reading (§1). Whether it is
callable there is unmeasured, and it changes nothing here: a walk never calls, and a call across a
state boundary is barred (D37, D78, D117), so the door is the only carrier.

---

## 6. One script, not two

### 6.1 How DCS loads each half, which is the whole argument

- **The `hook` host** is loaded by DCS at launch: every `.lua` in `Saved Games\DCS\Scripts\Hooks\`
  is executed once in the hook state, and a stamped filename would install beside its predecessor
  and register callbacks twice (`prior:tools/hooks/DcsApiEval.lua:319-321`).
- **The `export` host** is loaded by one `dofile` line in `Saved Games\DCS\Scripts\Export.lua`,
  which DCS runs in the export state when a mission starts; `Export.lua` is a shared file that SRS,
  Tacview and force-feedback drivers also edit, so the collector chains its four callbacks onto the
  previous holders and never replaces them (`prior:tools/export/DcsApiExport.lua:1884-1913`, D94).

Both are "execute this file's top level in my state". Neither needs the file to be different; each
needs the top level to know which state it woke up in. Today the two files share 1,544 of the
export script's 1,925 lines verbatim (80.2%, §12; the carry register's figure by a different method
is 1,551, 80.6%), the three chunk-source blocks are duplicated and held equal by a byte-comparison
control (`prior:tools/export_collector_test.lua:772-794`), and the instrument list is duplicated and
held equal by name (`:796-812`). The duplication was accepted in D94 because a `Scripts\Hooks\`
chunk cannot `require` this repository's code and a build-time emission would make the shipped file
a derived artefact the old repository would not commit. A rewrite is not bound by the second
reason. Roughly 34 KB of the shared lines are the three chunk-source blocks (§12), which leave the
bridge under §7.0, so the ratio for the new file will differ; the argument does not rest on it.

### 6.2 The decision

**One source file, `DcsApi.lua`, installed once at `Saved Games\DCS\Scripts\Hooks\DcsApi.lua`.**
DCS loads it into `hook` from there. The `export` stand-in — the one line that replaces `Export.lua`
for a run (`prior:tools/export/Export.census.lua:29`) — becomes
`dofile(lfs.writedir() .. 'Scripts/Hooks/DcsApi.lua')`, and so does the line a user appends to a
real `Export.lua`. One file on disk, two states, no build step, no byte-equality control: the
protocol code exists once because there is one file, and the walk source exists nowhere in it
(§7.0).

The host is detected at load, from facts measured in each state (§1, D94):

```lua
local HOST
if type(DCS) == 'table' and type(DCS.setUserCallbacks) == 'function' then HOST = 'hook'
elseif type(DCS) ~= 'table' and type(net) ~= 'table' and type(lfs) == 'table' then HOST = 'export'
else return end  -- neither: log to dcs.log and register nothing
```

`DCS` is a table in `hook` and nil in `export`; `net` is nil in `export`; `lfs` is present in both.
A third state that happens to `dofile` the file gets neither host and the file does nothing, loudly.

What is host-specific is a tail, and it is short:

| Concern | `hook` | `export` |
|---|---|---|
| Registration | `DCS.setUserCallbacks` over sixteen guarded callbacks, `try*` variants never (`prior:tools/hooks/DcsApiEval.lua:2150-2181`) | chain onto `LuaExportStart`, `BeforeNextFrame`, `AfterNextFrame`, `Stop`; never `ActivityNextEvent`; a non-function holder is left alone and counted out (`prior:tools/export/DcsApiExport.lua:1884-1925`) |
| Phase vocabulary | `menu`, `load`, `sim`, `paused` | `loaded`, `sim`, `stopped` |
| Clock for intervals | `DCS.getRealTime`, falling back to `os.clock` | `os.clock` only — `LoGetModelTime` is barred (D78, D94) |
| States served | all seven (§5.1) | `export` |
| Output / transport leaf | `Logs/DcsApi/hook/`, `<temp>/dcs-api/hook/` | `Logs/DcsApi/export/`, `<temp>/dcs-api/export/` |
| Install guard root | `lfs.currentdir()` | `lfs.currentdir()`, reported `ABSENT` in the handshake where it does not answer (`:334-345`, `:1860`) |
| Filesystem preflight | as today | `lfs.mkdir`, `lfs.attributes`, `lfs.dir` checked before claiming to run (`:1804-1825`) |

Everything else — containment, `write_atomic`, request parsing, the chunk wrapper of §7.3, the
response envelope, `describe_local`, the sweep, the heartbeat — is one implementation. The
namespace global is `DcsApi`, named for the file, and the handshake publishes it as `namespace`
beside `source`, the leaf of the file DCS loaded. Those two fields are what the census's instrument
exclusion derived inside the walk (D98) and what a consumer's walker now reads to exclude the
bridge's own objects by name and by source leaf, holding no second list to go stale.

**Rejected: a `HOST` constant substituted at build time into two shipped files.** It is the plan
register's 1.4, and it would keep the walk source in one place. Rejected because runtime detection
costs three lines and removes a build step, an install step and a control; the two shipped outputs
would still be two files to install and compare.

**Rejected: keeping two files and the byte-equality control.** It is what exists and it works; it
is also two copies of the most dangerous code in the repository to touch, since revalidating either
needs a live install and crashes (`prior:docs/reshape/11-problems.md`, M3).

### 6.3 Permanent installation

The bridge is installed once and left installed across ordinary play sessions (§0). That is a
different thing from what exists: today the bridge is what `Park-DcsThirdPartyScripts.ps1` leaves
in `Hooks\` while it parks everything else for a census, and the export host is loaded by a
stand-in `Export.lua` that replaces the user's own for the run and is put back afterwards
(`prior:tools/Park-DcsThirdPartyScripts.ps1:8-26`, D128). Four consequences.

**The installer is an install, not a park.** Two idempotent commands in the CLI, `install` and
`uninstall`, and a third, `verify`, that `status()` also runs. `install` copies `DcsApi.lua` to
`Scripts\Hooks\DcsApi.lua`, and appends the one `dofile` line (§6.2) to the user's real
`Scripts\Export.lua`, creating the file if it does not exist and leaving every other line as it
found it, because that file is SRS's and Tacview's as much as ours (§6.1). It is registered under
D128's rules — a row written *before* the copy, in a state named **installed** rather than
**parked**, because a park is a run-scoped condition that must be restored before the session ends
and an install is a standing one that must not be. The Stop hook that refuses to end a session with
a park outstanding does not fire on an install. `uninstall` removes exactly what `install` put
there: the hook file only when its hash is one this repository shipped, the `Export.lua` line only
by exact match, and never the file around it. The census park keeps working unchanged: it already
leaves the bridge in `Hooks\` by name, and its stand-in `Export.lua` — which loads the bridge and
nothing else — is what a measurement wants; permanence is about the file that is there when no
measurement is running.

**What accumulates on disk, and what bounds it.** Over months of a bridge that is installed and
almost never used, what grows is what the bridge writes without being asked, so each such write is
named with its bound:

| Written unasked | Where | Bound |
|---|---|---|
| the session directory | `<temp>/dcs-api/<host>/<stamp>/` | one per launch, and every sibling is removed at the next load (§4.2); a machine on which DCS never launches again holds one empty directory |
| `bridge.txt`, `heartbeat.txt` | `<output>/` | rewritten in place; two files of under 2 KiB |
| `events.log` | `<output>/` | rotated at load: `events.log` becomes `events.prev.log`, one generation kept, so the ceiling is two sessions of activity. A dormant session writes its load banner and its phase changes — some tens of lines per launch, none per frame |
| `dcs.log` door markers | DCS's own log | only when a `missionscripting` request runs (§5.2); a dormant bridge writes none |
| uncollected replies | `<session>/res/` | swept at 300 s while armed and once at disarm (§3.7); at most W of them between; gone with the session directory at the next load |

Nothing is written to the install, ever (D18), and nothing under `Saved Games` outside `Logs\` but
the two installed files (§9).

**A DCS update does not touch `Saved Games`**, so the bridge stays installed across one without the
installer running again; that is the property the maintainer asked for and it costs nothing. What an
update can change is the other side of the boundary: the callback names DCS looks up on the table
the hook supplies (D36, nothing enumerates them), the behaviour of `net.dostring_in`, and the
stdlib surface of each state (§1). Three rules follow. The handshake records the running build —
`_APP_VERSION`, read with `rawget` and never called, the same field the census report carries
(D112) — and `status()` shows it beside the build the committed model was taken from, as a
difference reported and never a refusal. The file's whole top level runs under one `pcall` so a
build on which the load raises writes one line to `dcs.log` and registers nothing: **an installed
bridge must never be the reason DCS fails to start**, and this is a control (§10). And a bridge
that loads but whose driver callback stopped firing on a new build is visible as a `status()` whose
heartbeat never leaves `armed: no` after the arm file is created — the `stalled` row of §4.4 —
rather than as silence a client has to interpret.

**The eval channel is now installed permanently**, and §2.2's row stands as written for that case:
it is reachable only by a process that can write into a directory under the user's profile, which
is a process already running as the user. `ALLOW_EVAL` stays a constant at the top of the file, and
an install with it `false` answers `ping` alone (§7.3). **That is a consequence of §7.0 the removal
reasoning did not name:** the read-only install — a bridge that would reflect but never evaluate —
no longer exists, because reflection is now a chunk the consumer ships and it runs under `eval`.
What an eval-disabled install is still for is liveness and game state, which `ping` and the
heartbeat carry (§7.4, `mcp.md` §3), and an operator who wants that and nothing else has it.
Whether a
multiplayer server's script-integrity check objects to a `dofile` line in `Export.lua` — the `hook`
host is what SRS itself uses and is not the concern — is unmeasured and is listed in §11; the
installer's help says so, and `uninstall` is the remedy if it does.

---

## 7. The protocol, version 2

### 7.0 Revised 2026-09-06: two ops, and what left with the other two

`reflect` and `census` are removed from the protocol. The decision rests on three readings of the
tree, each verifiable:

- **`census` was already an `eval`.** The bridge built its chunk from a parameter prologue plus
  `COMMON_SOURCE .. CENSUS_SOURCE` and shipped it into the target state
  (`prior:tools/hooks/DcsApiEval.lua:1399-1408`); nothing about the carrier differed from `eval`'s.
- **Its session state was not in the bridge either.** The walk did `rawget(_G, __store)` and
  `rawset(_G, __store, sessions)` against `__DCS_API_CENSUS` in the *target state's own globals*
  (`:801-804`). The frontier and the identity map lived in the state being walked; the bridge held a
  session cap and a name check.
- So `census` was a convenience verb around `eval` of a chunk that reads and writes a global, and
  the ~2,000 lines behind it (§12: 2,757 + 28,219 bytes of chunk source, plus the driver, the record
  grammar and its parser) belong to the project that consumes their output, versioned beside the
  collector that parses it. `reflect` is the same shape with a 3,308-byte chunk and no session, and
  no reason was found for it to differ.

**What the protocol keeps, generalised.** Three protections existed only around `census` — the tick
budget, the refusal of an oversized body (`:1697`, whose own comment is the rationale: a consumer
reading a cut body "would be parsing a measurement with a hole it cannot see"), and the reply
ceiling. Census was merely what surfaced them; a long `eval` stalled the same thread with none of
them. §3.4 and §7.7 state each for every evaluation, and §3.4 adds the one that bounds a single
chunk.

**What the protocol gains: `chunkname`.** Today `loadstring(code, 'dcs-api-eval')` is a fixed
literal (`:1590`), so every error reads `[string "dcs-api-eval"]:47`, with no file and a line number
relative to whatever text was shipped. §7.3 lets a caller name the chunk, and requires the line
numbers that result to be true.

**What a consuming project now ships for itself**, because the bridge no longer does:

| Was the bridge's | Is now the consumer's | Where it was |
|---|---|---|
| the walker source (`COMMON_SOURCE`, `REFLECT_SOURCE`, `CENSUS_SOURCE`) | a chunk it evaluates, keeping its state under its own global in the target state | `prior:tools/hooks/DcsApiEval.lua:540-1397` |
| the record grammar: the `dcs-api-census` and `dcs-api-reflect` sentinels, TSV rows, `clean()`, `%.14g`/`%.17g` numbers, `truncated` records, the `end` record | its own body format, doing what §7.6 says a format must do | `:542-546`, `:1013-1057`, `:1092-1112`, `prior:pipeline/src/bridge/protocol.ts:309-312`, `:494-527`, `:549-684` |
| the budget clamping (`census_number`: nodes 1–100,000, keys 1–1,000,000, depth 0–10,000, bytes 4,096 to the ceiling) | clamps inside its chunk, with `max_result_bytes` read from the handshake as the ceiling to stay under | `:273-280`, `:1721-1730` |
| the `end`-record fields lifted into reply headers (`census_status`, `census_stop`, `nodes`, `keys`, `frontier`, `identities`, `capped`, `holes`, `truncations`) | read from its own body; no reply header carries them | `:1707-1718` |
| the session verbs `start`/`continue`/`status`/`end`, the four-session cap, `sessions-full`, `end *` as the recovery for a stranded store | its chunk's own protocol against its own global; a stranded store is its global to clear | `:821-830`, `prior:pipeline/src/census/run.ts:330-379` |
| the instrument exclusion (`CENSUS_INSTRUMENTS`; the store as node 0, excluded by identity) | exclusion by the handshake's `namespace` and `source` fields (§6.2), and by identity of whatever its own chunk installs | `:265-271`, D43, D95, D98 |
| the reflect key cap (400, sorted before cutting) | its own | `:255` |

The bridge's own surface after the change is §7.3: two ops, one wrapper, one ceiling.

### 7.1 Envelope

Unchanged in shape from version 1 (D37), because it needs no quoting rule: header lines, one blank
line, then a body that is everything after it byte for byte.

- A header line is `name: value`; `name` matches `[A-Za-z0-9_-]+` and is read case-insensitively;
  `value` is everything after the first colon with leading whitespace removed. First colon wins.
- Header values are ASCII and **may not contain CR or LF**; a writer refuses one rather than
  escaping it, because a value carrying a newline would inject a header (`prior:pipeline/src/
  bridge/protocol.ts:8-12`, `:125-129`). `\r\n` in the header block is normalised; the body is never
  touched.
- The body is bytes. It is the only place a newline may appear and the only free-form field.
- `protocol: 2` in the handshake, the heartbeat and every reply. A consumer refuses a mismatch
  outright; capability is a separate question answered by `ops` and `states` (D43).

### 7.2 Files and names

| File | Where | Written by | Contents |
|---|---|---|---|
| request | `<session>/req/<id>.req`, published from `<id>.req.tmp` in the same directory | client | request envelope |
| reply | `<session>/res/<id>.res`, published from `<id>.res.tmp` | bridge | reply envelope |
| arm file | `<session>/arm`, created by `io.open(…, 'a')` or its Node equivalent; content ignored | client creates, bridge removes (§3.7) | nothing; existence is the signal |
| handshake | `<output>/bridge.txt`, rewritten in place at load | bridge | `bridge: dcs-api`, `protocol`, `host`, `stamp`, `pid`, `started` (local wall clock), `transport` (the session directory), `req`, `res`, `arm`, `output`, `eval` (`allowed`/`disabled`), `ops` (`ping,eval`), `states` (§5.4), `namespace` and `source` (§6.2), `lfs_tempdir` (raw), `transport_source`, `install_guard`, `tick_budget_ms`, `instruction_budget` and `instruction_ceiling` (§3.4), `probe_every` (frames, §3.7), `quiet_s`, `app_version` (§6.3), `max_request_bytes`, `max_result_bytes` |
| heartbeat | `<output>/heartbeat.txt`, every 2 s **while armed**, and at every arm, disarm and phase change (§4.7) | bridge | `stamp`, `phase`, `armed` (`yes`/`no`), `since` (local wall clock of the last arm or disarm), `ticks`, `answered`, `queued` (requests seen and not yet answered), `last_tick_at` (local wall clock), `busy` (the id being handled, when one is), `last_callback` (`<name>@<tick>` of the last callback other than `onSimulationFrame` to fire, §7.4), `dormant_cpu_ms_per_1000_ticks` (§3.5, from the last dormant period) |
| events log | `<output>/events.log`, append; rotated to `events.prev.log` at load (§6.3) | bridge | load, refusals, raises, phase changes, arm and disarm, and the `B|`/`O|` markers of §4.5 |

An id is `<seq>-<tag>` (§3.3): `[0-9]{10}-[A-Za-z0-9]{4,12}`. The client refuses to collect an id
that is not one before it becomes a path (`prior:pipeline/src/bridge/paths.ts:59-68`); the bridge
answers a request whose name does not parse as an id under its name anyway, since the name is only a
filename to it.

### 7.3 Requests

| Header | Ops | Meaning |
|---|---|---|
| `op` | all | `ping`, `eval` |
| `for` | all, **required** | the session stamp from the handshake (§4.3) |
| `state` | `eval` | `hook`, `gui`, `scripting`, `server`, `mission`, `missionscripting`, `config`, `export`; `[A-Za-z][A-Za-z0-9_]*` |
| `chunkname` | `eval`, optional | the name the chunk is compiled under, passed to `loadstring` verbatim. Lua's own rule applies: `@<path>` reports as `<path>:<line>:`, `=<name>` as `<name>:<line>:`, anything else as `[string "…"]:<line>:`. ASCII, no CR or LF (§7.1), at most 200 bytes. Absent, the bridge uses `=dcs-api-eval` |
| `max_instructions` | `eval`, optional | the count-hook budget of §3.4: a non-negative integer, clamped to `instruction_ceiling`, reported back as `budget` |
| body | `eval` | the chunk, byte for byte; empty is `bad-request` |

A request over 262,144 bytes is answered `bad-request` and never parsed as code. The request file is
removed before the chunk runs (D37). `eval` is refused `unsupported` when the installed copy sets
`ALLOW_EVAL = false`, leaving a bridge that answers `ping` alone (§6.3). The `path` validator of
protocol 1 (`prior:tools/hooks/DcsApiEval.lua:1567-1577`) leaves with `reflect`; a consumer that
takes a dotted path into its own chunk validates it segment by segment for the same reason —
a Lua pattern cannot quantify a group.

**Line numbers must be true.** A `chunkname` is a promise that `<path>:47` names line 47 of the
file the caller has, and a confidently wrong line number is worse than none. The census path broke
that promise by construction: it prepended a generated prologue to the walker source
(`:1399-1408`), so every line in the shipped chunk was offset by the prologue's length. The rule,
and the mechanism that keeps it in every state:

- **The body is never concatenated into code that is compiled.** In `hook` the bridge calls
  `loadstring(body, chunkname)` directly (`:1590`, with the name no longer a literal). For every
  other state the chunk handed to the carrier is a one-line wrapper that carries the body as a `%q`
  literal and compiles it *inside the target state* — `loadstring(<body>, <chunkname>)` — then sets
  the count hook (§3.4), runs it under `pcall`, converts the result as §5.3 requires and returns
  the string. `%q` in Lua 5.1.5 renders a newline as a backslash-newline pair and escapes `\0`,
  `\r`, `"` and `\`, so the literal decodes to the body byte for byte, and the chunk the body
  becomes is its own chunk whose line 1 is the body's line 1. This is the door's existing rule —
  nothing a requester wrote is concatenated into code the far state compiles (§5.2,
  `:1466-1516`) — applied to every carrier.
- A compile error from `loadstring` inside the wrapper is returned as `error`, `stage: compile`,
  with Lua's message verbatim, which begins `<chunkname>:<line>:`. A raise while running is
  `stage: run` with the message, which for an `error()` with a position begins the same way. Lua
  abbreviates a source name over `LUA_IDSIZE` (60) bytes in messages to `...` and its tail, which
  keeps the line true and shortens the path; the reply's `chunkname` header carries the full name.
- Every reply to an `eval` carries `chunkname`, the name that was used, so a consumer that sent
  none can still read what the numbers are relative to.
- A bridge that ever has to prepend anything to a body — none is specified — must state the offset
  in a `line_offset` header, and a consumer subtracts it. The header exists so that the rule has a
  spelling; a reply carrying it under this design is a defect.

The `hook` carrier keeps `setfenv(chunk, ENV)` (`:1593`): the chunk runs in the bridge's own
environment, as today. The wrapper in every other state runs the body in that state's globals, as
`net.dostring_in` did with the bare body.

### 7.4 Replies

| Header | Present | Meaning |
|---|---|---|
| `status` | always | §7.5 |
| `protocol`, `host`, `stamp`, `phase`, `id`, `tick`, `cpu_ms` | always | envelope, session, attribution, cost (§3.4) |
| `stage` | on `error` | `compile`, `run`, `dostring_in`, `remote`, `door`, `oversize`, `budget`, `bridge` — opaque to a consumer, rendered and never branched on (D45). `incomplete` left with `census` (§7.0) |
| `result_type` | `eval` | the Lua type of the value; scalars are printed in the body, anything else has an empty body and its type here (D37) |
| `chunkname` | `eval` | the name the chunk was compiled under (§7.3) |
| `budget` | `eval` | `instructions=<n>` or `none` (§3.4) |
| `result_bytes` | `error`, `stage: oversize` | the length of the result that was refused, so a caller can size its next request; the body is the refusal message and never a prefix of the result (§7.7) |
| `carrier`, `via` | `missionscripting` | `a_do_script`, `mission` (§5.4) |
| `last_callback`, `callbacks` | `ping` | the last callback other than `onSimulationFrame` to fire, as `<name>@<tick>`, and every callback name seen this session, comma-separated. Recorded by the rare callbacks' wrappers with one table write each, so the frame path of §3.6 is untouched. `mcp.md` §3 reads game state from these and from chunks it evaluates; the bridge itself calls no `DCS.*` reader |
| `for` | `stale-session` | the stamp the request named |

The `truncated` header of protocol 1 is gone: nothing truncates a body any more (§7.7). `format`
and the `census_*` headers went with the ops that carried them (§7.0).

### 7.5 Statuses and the error shape

| Status | Body | When |
|---|---|---|
| `ok` | the result | |
| `error` | a message | the chunk raised, the state answered with a message, or the bridge's own dispatch raised — `stage` says which; a raise anywhere inside a callback is caught, because a raise escaping into a DCS callback takes the session with it (`:1898-1905`, `:2138-2148`) |
| `bad-request` | a message | malformed, unknown op, missing `for`, a path that is not a path, an oversize request |
| `unsupported` | a message | eval disabled; no `loadstring`; no `net.dostring_in` on this host; a state this host does not serve |
| `refused` | a message | `net.dostring_in` returned nil: the role, the name, or the operator's policy gate (§5.1, S7) |
| `invalid-state` | a message | `net.dostring_in` returned `'Invalid state name'` |
| `door-shut` | a message | `missionscripting` asked for with no mission loaded (§5.4) |
| `stale-session` | a message | `for` is not this session's stamp (§4.3) |

A status other than `ok` means the body is a message and never a result; a consumer carries an
unknown status through and marks it unknown rather than mapping it to one it knows
(`prior:pipeline/src/bridge/protocol.ts:32-34`, `:77-80`). A reply carrying no `status` is a defect,
not a default: it arrived under its final name, so it was published whole.

### 7.6 Bodies and encoding: what Lua can express and JSON cannot

JSON is rejected again for D37's reason — no state has an encoder, `mission` and `missionscripting`
have no `require` to load one, and a serialiser written here would be a dependency of every reply —
and for a second: the values the bridge must carry are not JSON's. The rules for an `eval` body,
which is now the only body (§7.0):

| Lua value | `eval` body |
|---|---|
| string | the bytes, verbatim; any encoding; may hold NUL |
| number | `%.14g`, widened to `%.17g` where 14 does not read back as the same double; `inf`, `-inf`, `nan` by name, because `%.14g` renders them platform-dependently (`prior:tools/hooks/DcsApiEval.lua:1043-1057`); a consumer reads them back by name (`prior:pipeline/src/bridge/protocol.ts:515-527`) |
| boolean, nil | `true`/`false`; nil is `result_type: nil` and an empty body |
| table, function, userdata, thread | never serialised: `result_type` names the type and the body is empty. A consumer that wants to look inside one ships a chunk that serialises it to a string *in the state*, which is what `reflect` and `census` were |
| a `__tostring`, `__index` function, `__pairs` | never run by the bridge, which never calls `tostring` on a value from a state (`:1557-1563`, D37). What a consumer's chunk does with them is the consumer's, under the same rule if it wants the same safety |

**What a consumer's body format must do, learned from the two that left.** The walk's grammar was
bought with failures, and a consumer writing its own inherits the failures unless it inherits the
rules: escape `\`, `\t`, `\r` and `\n` one pass each way and never in a loop (`:542-546`,
`prior:pipeline/src/bridge/protocol.ts:309-312`); cut a long value *before* its row and say so in a
record of its own, because an appended ellipsis cannot be told from data (`:1013-1031`); give a
table identity by the table itself in a `seen` map, so two paths to one node are one node (D41,
D43), and spell a key that is not an identifier as `[3]`, `["a b"]`, `[true]`, `[<table>]` with its
kind named (`:1092-1112`); record inheritance as a metatable edge and never flatten it (D25); read
with `next` and `rawget` inside `pcall`, queue a userdata for its metatable alone because `next`
raises on it (`:1207-1211`), and use `debug.getmetatable` where `debug` exists, because
`getmetatable` honours a `__metatable` a proxy sets to hide itself (`:548-564`); take a function's
span from `debug.getinfo` and read its two line numbers from the *right*, since a Windows path
holds colons (`prior:pipeline/src/bridge/protocol.ts:494-513`); and put a sentinel at the head and
an end record at the tail, checked on both sides, so that a sanitised state's error message is
never read as an empty result (`:1657-1663`) and a body that arrived short is never read as a
smaller one (`:1673-1687`, D43, `prior:pipeline/src/bridge/protocol.ts:676-682`). None of this is
the protocol's any more; all of it is why the protocol refuses to cut a body (§7.7).

### 7.7 Limits

| Limit | Value | Rule |
|---|---|---|
| request | 262,144 bytes | refused `bad-request`, unread as code |
| reply | 65,536 bytes, `max_result_bytes` in the handshake | **refused, never truncated**, for every evaluation: `error`/`oversize` with `result_bytes`, because a cut lands inside whatever the body is and the loss is silent (`prior:tools/hooks/DcsApiEval.lua:1693-1700`, the census-only form, generalised under §7.0). A consumer with a page budget clamps it below this number itself |
| `chunkname` | 200 bytes, ASCII | §7.3; Lua abbreviates a name over 60 bytes in messages and the header carries it whole |
| instruction budget | `INSTRUCTION_BUDGET` per chunk, overridable per request up to `INSTRUCTION_CEILING`; `none` in `mission` | §3.4 |
| tick budget | 8 ms CPU | §3.4 |
| dormant frame | 0 allocations, 0 kernel entries, 0 concatenations; one `lfs.attributes` on every `PROBE_EVERY`-th frame | §3.6 |
| `PROBE_EVERY` | 8 frames | the wake check's cadence; the client's wake latency bound is `PROBE_EVERY` + 1 frames (§3.7) |
| `QUIET_S` | 3 s without a `.req` | armed → dormant (§3.7); the transition out of a long run costs 171 armed-idle ticks at 57 Hz |
| wake deadline | 10 s from `send` returning | past it, with the PID alive and the phase not `load`, the client reports `stalled` (§4.4, §4.7) |
| uncollected reply | swept after 300 s while armed, and once at disarm | a backstop for a requester that died, not a bin for one that is alive |

Gone with §7.0: the reflect key cap, the census page, node, key and depth budgets, the path, string
and key caps, and the four-session limit. Each is now whatever the consumer's chunk decides, and
the one rule the walk kept that a consumer should keep too is that a page budget is checked between
key iterations and never inside one, so a reference never precedes the row it refers to
(`prior:tools/hooks/DcsApiEval.lua:754-762`).

### 7.8 Sessions: whatever a chunk keeps in a state is the consumer's, and the protocol has none

Protocol 1 carried `start | continue | status | end` with a session id because the thing they named
lived in the target state (§7.0). It still does, and it still cannot be carried on the wire — a
frontier of paths cannot dedupe or name an integer key (D41) — but the bridge no longer needs to
know it exists. A consumer that wants a cursor keeps it under a global of its own in the state, as
`__DCS_API_CENSUS` was kept (`prior:tools/hooks/DcsApiEval.lua:801-804`), and clears it with a chunk
of its own — `end *` was that chunk (`prior:pipeline/src/census/run.ts:330-379`). Excluding that
global from its own walk by identity, and declaring it as node 0 so a reader sees the perturbation
rather than being told about it, is D43's rule and now the consumer's to keep.

What sits above the protocol in a consuming project's library is unchanged in shape and is not this
document's to specify: the run, with its end on every exit path including the failure paths
(`:229-246`); the capture of every reply verbatim before it is parsed, so a later session re-derives
a shard with no DCS; the pipelining window; the adaptive page budget; the restart on `superseded`;
and a run report carrying the walk's own stamps — `_APP_VERSION`, `servermode`, the sandbox
levels — read with `rawget` and never called (D112), so a capture cannot be mislabelled by what an
operator typed.

---

## 8. What the client library is, and what sits on it

One library, two skins, neither the real one (`prior:docs/reshape/21-rewrite.md:151-177`):

- `status()` — reads the handshake and heartbeat, checks the PID, reports phase, `armed` and
  `since`, heartbeat age (qualified as meaningless while `armed: no`, §4.7), ticks, answered,
  queued, `busy`, the transport, whether the arm file exists, the running `app_version` against
  the model's, whether `lfs.tempdir()` agreed with `os.tmpdir()`, and every problem it found (a
  heartbeat from another stamp; a heartbeat naming another transport — two copies installed, or a
  stale file; `prior:pipeline/src/bridge/client.ts:325-367`). No round trip, and no cost to a
  dormant bridge, so it may be called freely.
- `send(spec)` → id. Publishes by rename as today, then stats the arm file and creates it if absent
  (§3.7); the library never removes it. `collect(id)` → reply or nothing; `wait(id)` → `reply |
  pending | superseded | dead` (§4.4), with `pending` flagged `waking` or `stalled` where §4.4 says;
  `request(spec)` = send + wait.
- `ping()`; `eval(state, chunk, {chunkname, max_instructions})`; `evalFile(state, path)`, which
  reads the file outside DCS, hashes it, and sends it with `chunkname: @<path>` (`mcp.md` §4).
  `reflect` and `census` left the library with their ops (§7.0); the cursor a consumer builds on
  `pipeline` is its own.
- `pipeline(specs, W)` — send W-deep and yield replies in id order (§3.3).
- `save(path, reply)` — the reply verbatim, headers and body, through `latin1`, refused inside
  either DCS tree with the path *resolved* first: `normalisePath` cannot expand an 8.3 short name
  or follow a junction, so the nearest existing ancestor is resolved with `realpathSync.native` —
  or the runtime's equivalent, which `mcp.md` §1 decides — and the missing tail reattached
  (`prior:pipeline/src/bridge/paths.ts:117-203`, D38). The console is not a channel a measurement
  may cross (`prior:pipeline/src/bridge/client.ts:508-524`).

The transport path is read out of the handshake and never derived (D37): the client runtime's temp
directory — Node's `os.tmpdir()` today — and the bridge's `lfs.tempdir()` have never been measured
to agree, and the failure would be silent. It
is refused when relative, when inside the install (supplied by `--install` or `DCS_API_DCS_INSTALL`,
since the consumer cannot derive it from a user profile), and when inside `Saved Games` but not
under a `Logs` segment, written as *inside writedir and not inside writedir/Logs* so
`Logs\..\Config` is refused (`prior:pipeline/src/bridge/paths.ts:93-115`, D36). Containment folds
case, resolves `..`, and requires a segment boundary, because a byte-prefix test let all three
through once (D20).

The output directory is discovered as `<Saved Games>/<variant>/Logs/DcsApi/<host>/` over every
`DCS*` variant, and two candidates is an ambiguity for the caller, never a pick (`:307-334`).

The MCP skin is specified as its own project in `mcp.md`, which owns this library so that
several unrelated projects can use it without depending on any one of them. It registers
`dcs_status`, `dcs_ping`, `dcs_game_state`, `dcs_eval`, `dcs_eval_file` and `dcs_collect` — the five
of today (`prior:pipeline/src/mcp/server.ts:92-171`) less `dcs_reflect`, which left with its op
(§7.0), and without the census tools this section once planned, which were never built and now
belong to the consuming project. Each tool is one call into the library; the skin owns no protocol
logic, and a control diffs its output against the CLI's for one run. Its default wait is 15 s and
waiting longer is never a failure (`prior:pipeline/src/mcp/tools.ts:18`, `:104-118`). The CLI skin
is the same functions with `--out`.

---

## 9. Containment, in the bridge

Unchanged from today and restated because it is enforced in code, not in a comment: the bridge
refuses to run at all when `lfs.writedir()` is unreadable, because it then cannot tell where it may
write; refuses any directory inside `lfs.currentdir()` (the install) or `EXTRA_FORBIDDEN`; refuses a
relative path, which resolves against the install; and inside `Saved Games` writes only under
`Logs\`, the one subtree DCS writes and does not read (`prior:tools/hooks/DcsApiEval.lua:400-431`,
D36). A configured transport that is refused stops the bridge; an inferred one — `lfs.tempdir()` is
a guess DCS handed back — falls back beside the output (`:2040-2073`, D37). The bridge deletes
nothing under `Saved Games` that it did not create: its sweep (§4.2) is scoped to its own transport
root, and its output directory is never swept.

---

## 10. Controls the new repository must carry

Every one of these encodes a failure somebody paid for; port the case even where the implementation
changes.

| Control | What it defends | Prior |
|---|---|---|
| A reply still being written (`.res.tmp`) is not collected; the same file renamed is | the consumer's half of publish-by-rename, which a single-process Lua control cannot produce | `prior:pipeline/src/bridge/client.test.ts:111-127` |
| A request is written to `.req.tmp` and renamed, asserted on the operation log and not on the directory, which looks identical either way | the bridge's scanner reads a half-written file | `:84-96`, `prior:pipeline/src/bridge/fs.ts:67-101` |
| A half-written `50-partial.req.tmp` is left untouched by the bridge; no `.tmp` of its own survives a publish | the same, from the bridge side | `prior:tools/bridge_eval_test.lua:479`, `:1415-1424` |
| A request addressed to another session is answered `stale-session` and its chunk does not run, both at load and on a tick | the 2026-09-02 kill | `:487-507`, `:1435-1442` |
| **New:** a request written into a previous session's directory is never listed by the next session; the directory is gone after load | §4.2 | — |
| **New:** a client waiting on a request sees `superseded` when the stamp changes and `dead` when the PID is gone, and `pending` while the PID lives and the heartbeat is stale | §4.4 | — |
| **New:** `B|`/`O|` markers in `events.log` and the supervisor's reader naming a bridge request as the killer from a synthetic unbalanced file | §4.5 | `prior:tools/probes/Invoke-ProbeSupervisor.ps1:402-428` |
| **New:** W-deep pipelined `eval` requests are answered in id order within the tick budget, and every reply's `cpu_ms` is present | §3.3, §3.4 | — |
| **New:** an `eval` whose result exceeds `max_result_bytes` is answered `error`/`oversize` with `result_bytes`, from `hook`, from a `dostring_in` state and through the door; no reply body is ever a prefix of a result | §7.7 | `prior:tools/hooks/DcsApiEval.lua:1693-1700` for the census-only form |
| **New:** a chunk sent with `chunkname: @x.lua` that raises on its own line 47 reports `x.lua:47` from `hook`, from a `dostring_in` state and through the door, and a compile error on that line names the same line; a mutation that prepends one line to the body fails all three | §7.3 — line numbers are true | — |
| **New:** a chunk that loops is stopped by the count hook with `stage: budget` where `debug` exists and reports `budget: none` in `mission`; an `INSTRUCTION_BUDGET` of `0` or a non-integer is refused at load, because Lua installs no hook for it while `debug.gethook()` says otherwise | §3.4 | documented Lua 5.1 behaviour |
| **New:** `ping` reports `last_callback` and `callbacks`, and the frame path's allocation count does not move when a rare callback records itself | §7.4, §3.6 | — |
| **New:** 100,000 dormant ticks with the collector stopped grow `collectgarbage('count')` by zero bytes; the stubbed `lfs.attributes` is called once per `PROBE_EVERY` ticks and `lfs.dir`, `io.open`, `os.date` and `DCS.getRealTime` never; a mutation that reintroduces the per-call closure in the callback wrapper fails the byte count | §3.6 — the idle budget, as a number the harness can refuse | — |
| **New:** a request published while the bridge is dormant is answered within `PROBE_EVERY` + 1 ticks; one published on the tick the bridge disarms (between its `os.remove` of the arm file and its final listing) is still answered, because the harness drives the client's post-publish stat; an arm file with no request costs one quiet period and is then gone | §3.7 — the race proof, exercised rather than believed | — |
| **New:** an armed bridge with a `.req` every frame never disarms across 10,000 ticks; one with none disarms after `QUIET_S` and writes `armed: no` once; a global a chunk set in the target state survives a disarm and is read back by a chunk after re-arming | §3.8 | — |
| **New:** a heartbeat with `armed: no` and any age yields `pending` while the PID lives and the send is under 10 s old; the same at 10 s yields `stalled`; `armed: yes` keeps the previous age rule | §4.4, §4.7 | `prior:pipeline/src/bridge/client.test.ts:169-191` |
| **New:** the file loaded under a harness whose `DCS.setUserCallbacks` raises writes one line and registers nothing, and the harness's own state is untouched | §6.3 — an installed bridge must never stop DCS starting | — |
| **New:** `install` twice leaves one hook file and one `dofile` line; `uninstall` removes the line and not the lines around it, refuses a hook file whose hash it does not know, and leaves an `Export.lua` it did not write alone | §6.3 | `prior:tools/Park-DcsThirdPartyScripts.ps1:16-18` for the hash-before-delete rule |
| Silence yields `pending` with the phase, never a failure; a pending reply is collectable later by id | D37 | `prior:pipeline/src/bridge/client.test.ts:169-191` |
| **Moved to the consuming project under §7.0**, with the cases restated in §7.6 so they are not lost: a body without its sentinel is an error and the sentinel is a prefix; a body with the sentinel and no end record is an error and a short page is refused; a `truncated` record qualifies the row after it, key before value, and a cut string carries no marker | a sanitised state's message read as an empty result; a smaller census read as a complete one; a truncated value filed as complete | `prior:pipeline/src/bridge/protocol.test.ts:142-155`, `prior:tools/bridge_eval_test.lua:754-773`, `:883-903` |
| A cp1251 body and a UTF-8 body both survive request, bridge, reply and capture byte for byte | 599 of 986 descriptions are Russian | `prior:pipeline/src/bridge/client.test.ts:243-257`, `:327-343` |
| `--out` refuses the install, `Saved Games` including `Logs\`, an 8.3 short spelling, a laundering junction, and a path no part of which resolves | D18, D38 | `prior:pipeline/src/bridge/paths.test.ts:99-205` |
| A transport inside `Saved Games` and not under `Logs\` is refused; `Logs\..\Config` is refused | D36 | `:46-56` |
| The `a_do_script` off-by-one is reproduced under reference `lua5.1` so the shift correction is exercised | §5.2 | `prior:tools/probes/tabledepth_test.lua` |
| The `hook` and `export` hosts are both driven by the harness from one file, and the check count is asserted, not the exit code | §6; the harness exits 2 for "nothing ran" | `prior:tools/harness.lua:344-377` |
| The shipped script's own bytes meet the consumer's parser | the two implementations of the protocol drifting | `prior:pipeline/src/bridge/interop.test.ts` |
| `net.dostring_in` under the harness does not evaluate — it returns `''` — and the interop control asserts an empty answer arrives as an empty answer | a stub that models DCS wrongly turns a failing probe into a passing run | `prior:tools/harness_stubs.lua:172-175`, D37 |
| A dispatch raise inside `handle` comes back as a reply staged `bridge` and is logged; the pcall removed, the reply never arrives | a raise escaping into a DCS callback | `prior:tools/bridge_eval_test.lua:1506-1521`, D45 |

The harness stays strict by default: an unmodelled name raises where it is read, naming its path
(D12). Nothing loads into DCS untested, and the interop control is required in CI with the
interpreter installed for it (`prior:.github/workflows/ci.yml:71-77`, `:116`).

---

## 11. What could not be determined from the tree

- **`prior:docs/memory/**` was not readable in this session**, so `eval-bridge-protocol.md`,
  `eval-bridge-consumer.md`, `eval-bridge-live-run.md`, `hook-bridge-transport.md`,
  `bridge-operational-hazards.md`, `missionscripting-state.md`, `census-op-protocol.md`,
  `table-return-corruption.md` and `probe-supervisor.md` are cited here only through the decision
  records and code comments that cite them. The mutation tables those entries hold — twelve on the
  protocol control, fifteen on the census op, seven on the transport — are not reproduced. A reader
  with access should check §10 against them before treating it as complete; the fifteen on the
  census op now bind the consuming project's walker (§7.0), not this document.
- **Whether `lfs.dir` lists the temp transport on this build.** An unverified lead reports ED's
  `lfs.dir` clamping a path that escapes the Saved Games root *to* that root rather than refusing
  it. Unconfirmed here, and `webGUI.lua` is where to confirm it. If that clamp reached
  `<lfs.tempdir()>/dcs-api/`, the bridge would list the wrong directory and look dead. The census
  generation ran through this transport, so either the clamp did not apply or the run used the
  `Logs\` fallback of §4.2, and the handshake's `transport_source` in any live run says which;
  §3.5's first run records it.
- **The operator policy gate.** §5.1. Whether `net.allow_unsafe_api` and `net.allow_dostring_in`
  are enforced on the running build is a live condition, measured absent on 2.9.29.27278 with no
  entry in `autoexec.cfg`. The bridge reports a
  refusal and never edits that file; `mcp.md` §5 says what its `verify` reads there.
- **Which chunks kill DCS is not this document's question.** The bridge evaluates what a caller
  sends and provides the marker discipline that makes a crash name its killer (§4.5). Cataloguing
  the calls that are dangerous belongs to whoever authored the chunk — for the DCS API project, as
  a measured `hazards` fact with a call shape and a fault class on the record it belongs to.
- **What `lfs.tempdir()` returns inside DCS** is recorded in the handshake and reported by `status`
  against `os.tmpdir()`, and the tree holds no committed value for it; the harness models it as a
  directory *inside* the write directory precisely because nobody has measured it
  (`prior:tools/harness_stubs.lua:88-94`). The only hint is a withheld `machineLocal` value shaped
  `C:\Users\<user>\AppData\Local\Temp\DCS\…` (`prior:generated/reports/CONFLICTS.md:10`).
- **Whether `require('socket')` loads in `hook` or `export`.** §1. It does not change §2's decision.
- **The per-page CPU cost of the walk and the filesystem's share of a round trip.** §3.1, §3.5.
- **The cost of one `lfs.attributes` inside DCS.** S4 measured the listing (0.098 ms) and nothing
  else; the stat that replaces it is unmeasured in DCS. The only comparison in the tree is §12's,
  taken outside DCS in a sandbox whose filter driver inflates every filesystem call to ~0.8 ms, so
  only its ratio (stat within 25% of an empty enumeration) is used, and only to say the syscall
  shape is not where the saving is. §3.5's live run measures the real figure, and §2.3's reopen
  condition names the number that would matter.
- **Whether a dormant bridge is below the noise of DCS's frame time.** The maintainer's statement
  that the current bridge is not noticeable is the baseline; §3.5's three-way frame-time comparison
  is what turns "not noticeable" into a number for the new one.
- **Whether a multiplayer server's script-integrity check objects to the `dofile` line in
  `Export.lua`.** §6.3. The `hook` host is not in question — third-party hooks in `Saved Games` are
  the norm — but the install is now permanent and this is the one way it could be visible to a
  server.
- **How NTFS propagates a directory's modification time** when an entry is added, as seen through
  `lfs.attributes` from inside DCS. §3.7 rejected it as the wake signal for that reason; a
  measurement showing it prompt and monotonic would let the arm file go, and would be worth one
  fewer file, not a redesign.
- **Whether `net.dostring_in` is callable from `missionscripting`** (§1). Barred anyway.
- **The `mp-server` role.** Nothing in the tree drives it and this install has no `DCS_server.exe`
  (S7); every figure here is `sp`, with `mp-client` reaching `hook` and `export` alone.
- **The duplication figure.** 1,544 lines by the command in §12 against the register's 1,551
  (`prior:docs/reshape/plan.md:234`); the methods differ and neither number changes §6.

---

## 12. Figures, with the commands that produced them

```powershell
# Line counts
foreach ($f in 'prior:tools/hooks/DcsApiEval.lua','prior:tools/export/DcsApiExport.lua',
  'prior:pipeline/src/bridge/protocol.ts','prior:pipeline/src/bridge/client.ts',
  'prior:pipeline/src/bridge/standin.ts','prior:pipeline/src/bridge/paths.ts',
  'prior:pipeline/src/bridge/fs.ts') { "{0,6} {1}" -f (Get-Content $f).Count, $f }
#   2183 DcsApiEval.lua   1925 DcsApiExport.lua   684 protocol.ts   580 client.ts
#    468 standin.ts        334 paths.ts            101 fs.ts

# Lines of the export script that appear verbatim in the eval script (multiset match)
$a = Get-Content prior:tools/hooks/DcsApiEval.lua
$b = Get-Content prior:tools/export/DcsApiExport.lua
$set = @{}; foreach ($l in $a) { $set[$l] = ($set[$l] + 1) }
$shared = 0; foreach ($l in $b) { if ($set[$l] -gt 0) { $shared++; $set[$l]-- } }
"$shared of $($b.Count) = {0:P1}" -f ($shared / $b.Count)          # 1544 of 1925 = 80.2%

# pcall sites, and which commit introduced each
foreach ($f in 'prior:tools/hooks/DcsApiEval.lua', 'prior:tools/export/DcsApiExport.lua') {
  (Select-String -Path $f -Pattern 'pcall\(' -AllMatches).Matches.Count }   # 36, 28
git blame -l prior:tools/hooks/DcsApiEval.lua | Select-String 'pcall\('
#   99c44c6b (2026-08-11, the first bridge)        23   containment, atomic write, list_dir, clocks,
#                                                       metatable reads, compile/run/dostring_in,
#                                                       request removal, sweep, callback guard
#   91398564 (2026-08-12, the census op)            3   session names, hook-state census run
#   fa7117ac (2026-08-12, the walk's grammar)       3   getinfo 'Su', getfenv, enumeration
#   6523c4f4, df4fbd45 (2026-08-22, D98)            2   installed-slot read, source leaf
#   aa2ffde9 (2026-08-27, D112)                     1   the globals count
#   25f415ac (2026-09-02, the door)                 3   door markers, a_do_script, hop one
#   c8fc7e2c (2026-09-02, stale-session)            1   dispatch under the stamp check

# Chunk source sizes — the three blocks that leave the bridge under §7.0; what a consumer ships
$src = [IO.File]::ReadAllText('prior:tools/hooks/DcsApiEval.lua')
foreach ($n in 'COMMON_SOURCE','REFLECT_SOURCE','CENSUS_SOURCE') {
  $i = $src.IndexOf("local $n = [==["); $o = $src.IndexOf('[==[', $i) + 4
  $c = $src.IndexOf(']==]', $o); "$n`: $($c - $o)" }  # 2757, 3308, 28219 bytes

# Census throughput, from the committed reports
Get-Content prior:generated/conc1.md -TotalCount 10   # 2543 replies, 86.9 s, 382801 identities
"{0:N1} ms/reply; {1:N0} identities/s" -f (86.9e3/2543), (382801/86.9) # 34.2 ms; 4,405
"13605 × 34.2 ms = {0:N0} s" -f (13605*34.2/1000)                     # 465 s per generation
"456 MB / 13605 = {0:N1} KiB" -f (456e6/13605/1024)                   # 32.7 KiB per reply

# The dormant frame today (§3.6): the sites, read off the source
$f = 'prior:tools/hooks/DcsApiEval.lua'; $L = Get-Content $f
foreach ($n in 2141,2142,1971,1972,1974,489,490,491,496,1981,1982) { "{0,5}: {1}" -f $n, $L[$n-1].Trim() }
"heartbeat() '..' count: "  + ([regex]::Matches(($L[1944..1955] -join "`n"), '\.\.')).Count   # 10
"write_atomic() '..' count: " + ([regex]::Matches(($L[461..474] -join "`n"), '\.\.')).Count   # 1
"write_atomic() file calls: " + ([regex]::Matches(($L[461..474] -join "`n"),
  'io\.open|f:write|f:close|os\.remove|os\.rename')).Count                                  # 6
(Select-String -Path $f -Pattern 'B\.disabled = true').LineNumber -join ', '   # 2012, 2024, 2049, 2078
(Select-String -Path $f -Pattern 'DCS\.setUserCallbacks\(callbacks\)').LineNumber   # 2181 — after all four

# Dormant-path arithmetic (§3.6–3.8), at S4's 57 Hz and 0.098 ms per listing
"{0} allocations/s"  -f (57*5)                                # 285
"{0:N0} kernel entries/s" -f (57*4.5 + (5 + 4.5)/2)           # ≈ 261 (4–5 per frame + the 2 s heartbeat and sweep)
"wake bound {0:N0} ms" -f ((8+1)*17.5)                        # 158 ms: PROBE_EVERY + 1 frames
"disarm cost {0} listings, {1:N1} ms CPU" -f (3*57), (3*57*0.098)   # 171 listings, 16.8 ms over 3 s
"probes/s dormant {0:N1}" -f (57/8)                           # 7.1 stats per second against 57 listings

# Stat against empty-directory enumeration, OUTSIDE DCS, this machine, this session (§2.4, §11).
# Absolute values are inflated ~8× over S4's in-DCS figure by a filter driver on this sandbox's
# temp directory and are not used; only the ratio is. N = 3000 per loop.
$root = "$env:TEMP\idle2"; New-Item -ItemType Directory -Force "$root\req" | Out-Null
Set-Content "$root\arm" ''; $N = 3000
foreach ($case in @(
    @{ n = 'stat, missing file '; f = { [IO.File]::Exists("$root\absent") } },
    @{ n = 'stat, present file '; f = { [IO.File]::Exists("$root\arm") } },
    @{ n = 'enumerate empty dir'; f = { [IO.Directory]::GetFileSystemEntries("$root\req") } })) {
  $sw = [Diagnostics.Stopwatch]::StartNew(); for ($i = 0; $i -lt $N; $i++) { $null = & $case.f }
  $sw.Stop(); "{0}: {1:N1} us/op" -f $case.n, ($sw.Elapsed.TotalMilliseconds * 1000 / $N) }
#   stat, missing file : 796.9 us/op
#   stat, present file : 863.6 us/op
#   enumerate empty dir: 1,004.6 us/op      (loop overhead alone: 1.4 us/op)

# Library surface per state (§1)
Get-ChildItem prior:model -Recurse -Filter 'socket*.yaml'             # nothing
Select-String -Path prior:model/*/os.yaml -Pattern '^    path: os\.getpid$' -List |
  ForEach-Object { $_.Path }                          # config export gui hook scripting
Select-String -Path prior:model/*/net.yaml -Pattern '^    path: net\.dostring_in$' -List |
  ForEach-Object { $_.Path }                                          # gui hook missionscripting
```
