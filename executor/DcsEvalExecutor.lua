-- The executor: the one Lua file DCS loads, and the whole of the in-game
-- half. ADR 0002 gives it its name.
--
-- DCS runs this file in two Lua states. At launch it runs every file in
-- `Saved Games\DCS\Scripts\Hooks\` in the hook state, where `DCS` is a table
-- and `DCS.setUserCallbacks` is the way in. At each mission start it runs
-- `Saved Games\DCS\Scripts\Export.lua` in the export state, where one
-- `dofile` line points here; that state has no `DCS` and no `net`, and the
-- way in is the four `LuaExport*` globals, which other exporters also hold.
-- The top level tells the two apart, decides where the file may write, and
-- registers the tail the state wants. Everything past registration is one
-- implementation.
--
-- The whole top level runs under one `pcall`. An installed executor must
-- never be the reason DCS fails to start, so a build on which the load
-- raises writes one line to `dcs.log` and registers nothing.
--
-- Every read of a host global goes through `rawget(_G, name)`. Off DCS this
-- file runs under a harness whose state models raise on a name they do not
-- carry, so `type(DCS)` in the export state would raise where DCS itself
-- answers nil. `rawget` reads the same answer in both.

local NAME = "DcsEvalExecutor"

-- The figures a client reads from the handshake and the executor holds
-- itself to. Each is a constant here rather than anything configured,
-- because nothing configures this file: the installer places it and
-- appends one line, and no environment reaches a Lua state DCS starts. All
-- of them are published from the first load, before the paths that spend
-- them exist, so the client is written once against the whole shape and a
-- later path reads its figure from here and nowhere else. The most a
-- request may be is enforced: past it a request is answered `bad-request`
-- and never read, so no client can have the executor hold a chunk of that
-- size in a frame. The two instruction figures are provisional: the
-- specification names neither, and no live chunk has been measured
-- against them.
local PROTOCOL = 2
local ALLOW_EVAL = true
local TICK_BUDGET_MS = 8
local INSTRUCTION_BUDGET = 1000000
local INSTRUCTION_CEILING = 50000000
local PROBE_EVERY = 8
local QUIET_S = 3
local MAX_REQUEST_BYTES = 262144
local MAX_RESULT_BYTES = 65536

-- What an `eval` is compiled under when the request names nothing, and the
-- most a request may name. The default is this project's own name and not
-- the one the specification inherited from the project this one replaces,
-- for the reason ADR 0002 gives. The `=` form makes Lua report a raise as
-- `dcs-eval:<line>:`, with no `[string "..."]` around it. The limit exists
-- because Lua abbreviates a source name over 60 bytes in its messages, so a
-- name of any length is honoured in the header and only the tail of a long
-- one appears in the message; past 200 bytes a name is refused, so no reply
-- carries an unbounded echo.
local DEFAULT_CHUNKNAME = "=dcs-eval"
local MAX_CHUNKNAME_BYTES = 200

-- What each host answers for each state, as the handshake and every ping
-- reply declare it: the carrier that reaches the state, whether a result
-- comes back as any Lua value or as a string, and what has to be true of
-- the simulator first. The hook host reaches its own state directly, five
-- more through `net.dostring_in`, which answers a string, and
-- `missionscripting` through `a_do_script` in the mission state, which
-- answers a string too. The export host has no `net` and answers its own
-- state alone. `server` is a second name for `scripting` and is not listed
-- twice. Declared before the carriers are built, for the reason the
-- figures above are.
local STATES = {
  hook = {
    { "hook", "local", "any", "always" },
    { "gui", "dostring_in", "string", "menu" },
    { "scripting", "dostring_in", "string", "menu" },
    { "mission", "dostring_in", "string", "menu" },
    { "config", "dostring_in", "string", "menu" },
    { "export", "dostring_in", "string", "slot" },
    { "missionscripting", "a_do_script", "string", "mission" },
  },
  export = {
    { "export", "local", "any", "always" },
  },
}

-- The hook callbacks registered: each row is the name, the phase it moves
-- the executor to where it moves one, and its kind. `tick` marks the one
-- callback that is the frame: it advances the tick and serves the session,
-- and records nothing, because the frame path is where the dormant cost is
-- counted. `frame` marks another per-frame callback that does neither.
-- Every other callback is rare and records itself, so that `ping` can say
-- which fired last, at what tick, and every name seen this session, which
-- is how a client learns what the simulator has been doing without the
-- executor calling a `DCS.*` reader, which it never does. Every callback is
-- guarded, so a raise inside it never reaches DCS, and none returns a
-- value. The `try*` variants are not here on purpose: their return value
-- overrides DCS's own handling and every other hook checking the same
-- thing, so registering one would change behaviour for every tool on the
-- machine. The names past the simulation set were offered in a measured
-- session and not all seen to fire, which is "did not fire" rather than
-- "does not exist"; each costs one table entry.
local HOOK_CALLBACKS = {
  { "onMissionLoadBegin", "load" },
  { "onMissionLoadProgress" },
  { "onMissionLoadEnd" },
  { "onSimulationStart", "sim" },
  { "onSimulationFrame", nil, "tick" },
  { "onSimulationPause", "paused" },
  { "onSimulationResume", "sim" },
  { "onSimulationStop", "menu" },
  { "onPlayerChangeSlot" },
  { "onGameEvent" },
  { "onChatMessage" },
  { "onNetConnect" },
  { "onNetMissionChanged" },
  { "onShowGameMenu" },
  { "onShowBriefing" },
  { "onShowMissionEditor" },
  { "onShowMainInterface" },
  { "onShowMultiplayer" },
}

-- The export globals chained onto, in the same shape. The frame here is the
-- callback after the frame rather than the one before it, so that a chunk
-- evaluated in this state reads a frame DCS has finished stepping.
-- `LuaExportActivityNextEvent` is not here: it returns the time DCS should
-- call it next, and DCS acts on that return.
local EXPORT_CALLBACKS = {
  { "LuaExportStart", "sim" },
  { "LuaExportBeforeNextFrame", nil, "frame" },
  { "LuaExportAfterNextFrame", nil, "tick" },
  { "LuaExportStop", "stopped" },
}

-- The namespace: what this file publishes as the global `DcsEvalExecutor`,
-- and only once registration has succeeded, so a failed load leaves no
-- trace of itself. It carries the host, the phase, the raise count, the
-- tick, which counts frames from 0 at load and stamps every reply, the
-- two write roots with how each was chosen, and the session: its stamp
-- with the time and pid it was built from, its directory with the `req`,
-- `res` and `arm` paths under it, and how many earlier sessions the load
-- swept. `sweep_left` appears when one could not be removed, and
-- `last_raise` on the first raise a guard catches. Once the session exists
-- it also carries the operations on it, `frame`, `publish`, `reply`,
-- `take`, `parse` and `admit`, with `max_request_bytes` beside them, so that
-- a driver off DCS can take a request, read it and publish a reply as the
-- tick does, and `ops`, the table the tick dispatches through, `ping` and
-- `eval` in it. The
-- callback record is `callbacks`, every name seen in the order first seen,
-- with `last_callback_name` and `last_callback_tick` once one has fired;
-- `unpublished` counts the replies the tick could not publish, with
-- `last_unpublished` the reason for the latest. `handshake` is the path of
-- the file a client reads first, and `events` of the one a supervisor
-- reads after a crash; `unrecorded` counts the lines that did not reach
-- it, with `last_unrecorded` the reason for the latest, and
-- `events_left` appears where the load could not rotate it.
local E

local function nothing() end

-- The frame, defined once the session operations it drives exist.
local tick

-- One line appended to the events log, defined further down beside the
-- session's other writers. It is declared here because the frame is not
-- the only path that writes one: a rare callback, whose body is built
-- above, writes a line of its own when the phase moves. Were the name
-- resolved where that body sits, it would be a read of a global that is
-- never set, and the guard's own `pcall` would swallow the raise.
local record

-- The heartbeat writer, defined beside the handshake it is shaped like, and
-- declared here for the same reason `record` is: the two transitions and the
-- rare callback that call it are all written above it.
local heartbeat

-- `lfs.attributes`, read once at load and kept here. The dormant frame's
-- one filesystem call must not walk a global table chain to find it: that
-- path exists so a frame handling nothing costs nothing measurable, and a
-- lookup per frame is a cost paid for nothing.
local attributes

-- The `os.clock` reading the tick took just before the request it is
-- handling, which every reply to that request is charged from; nil while
-- no request is being handled.
local began

-- The status and the cost of the last reply framed, kept so the marker
-- that closes a request carries the figures the reply carried rather than
-- a second reading of a clock that has moved since (ADR 0007). One
-- request is handled at a time, so these are that request's.
local answered, charged

-- The callback names seen this session, as a set beside the list the
-- namespace publishes, so a rare callback firing again is one lookup.
local seen = {}

-- One wrapper per callback, built here once. DCS calls the wrapper and the
-- wrapper pcalls a body that already exists. Nothing is allocated per call,
-- because one of these is the frame and the dormant budget is counted in
-- VM instructions. A rare callback's body moves the phase where the row
-- says, then records itself: its name and the tick it fired at, which is
-- what `ping` reports as the last callback, and its name in the list of
-- those seen, once. A raise in the body is counted and kept, not rethrown:
-- a raise escaping into a DCS callback takes the session with it.
local function guard(name, phase, kind)
  local body
  if kind == "tick" then
    body = function()
      tick()
    end
  elseif kind == "frame" then
    body = nothing
  else
    body = function()
      if phase then
        E.phase = phase
      end
      E.last_callback_name, E.last_callback_tick = name, E.tick
      if not seen[name] then
        seen[name] = true
        E.callbacks[#E.callbacks + 1] = name
      end
    end
  end
  return function(...)
    local ok, err = pcall(body, ...)
    if not ok then
      E.raised = E.raised + 1
      E.last_raise = name .. ": " .. tostring(err)
    end
  end
end

local function register_hook(DCS)
  local callbacks = {}
  for _, row in ipairs(HOOK_CALLBACKS) do
    callbacks[row[1]] = guard(row[1], row[2], row[3])
  end
  DCS.setUserCallbacks(callbacks)
end

-- Chain onto one export global. `Export.lua` is a shared file that SRS,
-- Tacview and force-feedback drivers also edit, so the previous holder is
-- kept and called after ours, unprotected and as a tail call: a raise in
-- somebody else's exporter reaches DCS exactly as it did before this file
-- was installed, under their traceback rather than this file's name. Ours
-- runs first, so it has run whether or not theirs raises. A holder that is
-- not a function is left alone, because calling it would raise every frame
-- and replacing it would discard whatever was meant by it. The number of
-- slots chained is published, so three of four is visible rather than
-- claimed as four.
local function chain(name, phase, kind)
  local previous = rawget(_G, name)
  if previous ~= nil and type(previous) ~= "function" then
    return false
  end
  local ours = guard(name, phase, kind)
  if previous then
    rawset(_G, name, function(...)
      ours(...)
      return previous(...)
    end)
  else
    rawset(_G, name, ours)
  end
  return true
end

local function register_export()
  local chained = 0
  for _, row in ipairs(EXPORT_CALLBACKS) do
    if chain(row[1], row[2], row[3]) then
      chained = chained + 1
    end
  end
  E.chained = chained
end

-- Which state this file woke in, from facts that hold in each: `DCS` is a
-- table in the hook state and nil in export, `net` is nil in export, and
-- `lfs` is present in both. Any other state gets neither, the load stops
-- here, and the raise says what it saw.
local function detect()
  local DCS = rawget(_G, "DCS")
  local net = rawget(_G, "net")
  local lfs = rawget(_G, "lfs")
  if type(DCS) == "table" and type(DCS.setUserCallbacks) == "function" then
    return "hook", DCS
  elseif DCS == nil and net == nil and type(lfs) == "table" then
    return "export"
  end
  error("no host: DCS is " .. type(DCS) .. ", net is " .. type(net) .. ", lfs is " .. type(lfs), 0)
end

-- Where this file may write, decided before anything is registered. Two
-- directories come out. The output, `<lfs.writedir()>\Logs\DcsEval\<host>`,
-- is where the durable files go: under `Logs\`, the one subtree of
-- `Saved Games` that DCS writes and does not read, so nothing left there
-- can change what the next launch loads. The transport root is where a
-- session's requests and replies will go: `<lfs.tempdir()>\dcs-eval\<host>`
-- when that passes the same test as the output, and `<output>\rpc` when it
-- does not. Both are inferred, because nothing configures this file: the
-- installer places it under one name and appends one line, and no
-- environment reaches a Lua state DCS starts.
--
-- Containment here is textual. Case is folded, `.` and `..` are collapsed,
-- and a root matches only at a segment boundary, so `LogsX` is not under
-- `Logs`. An 8.3 short name or a junction is not seen through, because
-- nothing in a DCS Lua state can resolve one; the client resolves a real
-- path before it trusts what the handshake names.

local SEP = "\\"

-- Without trailing separators. `lfs.writedir()` answers with one.
local function tidy(p)
  return (p:gsub("[/\\]+$", ""))
end

-- A drive with a separator after it, or a leading separator. `C:foo` is
-- relative to the drive's current directory and is not absolute.
local function absolute(p)
  return p:find("^%a:[/\\]") ~= nil or p:find("^[/\\]") ~= nil
end

-- One spelling for a path: forward slashes, lower case, `.` dropped and
-- `..` resolved against the segment before it. The anchor, a drive or a
-- leading separator, is never popped: Windows reads `C:\..\x` as `C:\x`,
-- and a normaliser that let `..` eat the drive would let
-- `C:\..\Program Files\...` past the install check. A drive on its own is
-- an anchor too: `tidy` leaves `C:\` as `C:`, and a root spelt that way
-- still holds everything on the drive. A relative path is refused before
-- it gets here, and spells itself under `.` if it does.
local function normalise(p)
  local drive = p:match("^(%a:)[/\\]") or p:match("^(%a:)$")
  local anchor = drive and drive:lower() or (p:find("^[/\\]") and "" or ".")
  local rest = drive and p:sub(3) or p
  local out = {}
  for segment in rest:gmatch("[^/\\]+") do
    if segment == ".." then
      if #out > 0 then
        table.remove(out)
      end
    elseif segment ~= "." then
      out[#out + 1] = segment:lower()
    end
  end
  return anchor .. "/" .. table.concat(out, "/")
end

-- `path` is `root` or lies under it, at a segment boundary. A root with no
-- segment, a drive or the bare separator, already ends in the boundary;
-- appending another would spell a prefix no path has.
local function inside(path, root)
  local p, r = normalise(path), normalise(root)
  local boundary = r:sub(-1) == "/" and r or r .. "/"
  return p == r or p:sub(1, #boundary) == boundary
end

-- Whether `dir` may be written into, given the write directory and the
-- install, which is nil where `lfs.currentdir()` does not answer. `true`,
-- or `false` and one line naming the rule that refused it and the path.
-- Inside `Saved Games` the rule is written as "inside the write directory
-- and not inside its `Logs`", not as a spelling test on the path, so
-- `Logs\..\Config` is refused.
local function may_write(dir, wd, install)
  if not absolute(dir) then
    return false, dir .. " is relative, and would resolve against the install"
  end
  if install and inside(dir, install) then
    return false, dir .. " is inside the install, " .. install
  end
  if inside(dir, wd) and not inside(dir, wd .. SEP .. "Logs") then
    return false, dir .. " is inside Saved Games and not under Logs"
  end
  return true
end

-- One directory read from `lfs`, as DCS spelt it, or nil where the read
-- raises or answers something that is not a path. Each read is guarded on
-- its own: the export state's `lfs.currentdir()` is not known to answer,
-- and a read that raises must not take the others with it.
local function read_dir(lfs, name)
  local ok, value = pcall(function()
    return lfs[name]()
  end)
  if ok and type(value) == "string" and value ~= "" then
    return value
  end
  return nil
end

-- The two roots for `host`, or a raise saying why there are none. The
-- output is refused outright: it is where the handshake goes, and a file
-- that quietly worked somewhere else would be reporting from a place
-- nobody reads. The temp candidate is refused quietly: `lfs.tempdir()` is
-- a guess DCS handed back, not what anybody meant, and it can land inside
-- the install or beside `Config\`. Where it does, the transport goes beside
-- the output, which has already passed. Nothing is created here; that waits
-- for the stamp.
local function roots(host)
  local lfs = rawget(_G, "lfs")
  local wd = read_dir(lfs, "writedir")
  if not wd then
    error("lfs.writedir() is unreadable, so nothing says where this file may write", 0)
  end
  wd = tidy(wd)
  local install = read_dir(lfs, "currentdir")
  install = install and tidy(install)
  local output = wd .. SEP .. "Logs" .. SEP .. "DcsEval" .. SEP .. host
  local allowed, why = may_write(output, wd, install)
  if not allowed then
    error("the output directory " .. why, 0)
  end
  local r = { output = output, install_guard = install or "ABSENT" }
  local temp = read_dir(lfs, "tempdir")
  r.lfs_tempdir = temp or "ABSENT"
  if temp then
    local candidate = tidy(temp) .. SEP .. "dcs-eval" .. SEP .. host
    allowed, why = may_write(candidate, wd, install)
    if allowed then
      r.transport_root, r.transport_source = candidate, "lfs.tempdir"
    else
      r.transport_refusal = why
    end
  else
    r.transport_refusal = "lfs.tempdir() is unreadable"
  end
  if not r.transport_root then
    r.transport_root, r.transport_source = output .. SEP .. "rpc", "fallback: beside the output"
  end
  return r
end

-- A session is a directory under the transport root named by a stamp,
-- `<os.time()>-<os.getpid()>`. The clock keeps two launches apart and the
-- pid names a process that can be checked for liveness, so a request
-- addressed to a stamp is addressed to one launch and no other. Without
-- `os.getpid`, an addition of DCS's that stock Lua lacks, two launches in
-- one second could share a name and nothing could tell a live session from
-- a dead one; the file refuses to run rather than fence with the clock
-- alone, and the refusal is the one `dcs.log` line a stopped load writes.
--
-- The time, the pid and the stamp, or a raise saying what is missing.
local function stamp()
  local os = rawget(_G, "os")
  local getpid = type(os) == "table" and rawget(os, "getpid")
  if type(getpid) ~= "function" then
    error("os.getpid is absent, so there is no stamp to fence a session with", 0)
  end
  local pid = getpid()
  if type(pid) ~= "number" then
    error("os.getpid() answered a " .. type(pid) .. ", so there is no stamp to fence a session with", 0)
  end
  local started = os.time()
  return started, pid, string.format("%d-%d", started, pid)
end

-- The clock the tick is held to its budget by and every reply's `cpu_ms`
-- is read from, required at load for the same reason the pid is: the
-- handshake promises `tick_budget_ms`, and an executor with no clock could
-- keep neither that nor the cost every reply carries, so it refuses to
-- run rather than publish a promise. Read through `rawget` to the field,
-- as the pid is, so a state without one answers nil here rather than
-- raising somewhere else.
local function clocked()
  local os = rawget(_G, "os")
  local clock = type(os) == "table" and rawget(os, "clock")
  if type(clock) ~= "function" then
    error("os.clock is absent, so no tick can be held to its budget", 0)
  end
end

-- The two instruction figures, required at load to be counts Lua will
-- hook. Lua 5.1 installs no hook at all for a count of `0`, while
-- `debug.gethook` still answers the function it was handed, so a budget
-- that is not a positive integer would read as set and bound nothing; a
-- count past what a C `int` holds is not a count `debug.sethook` takes.
-- Neither is defaulted: the handshake publishes both, and a figure quietly
-- replaced is a promise the file does not keep. A default past the
-- ceiling would be clamped by nothing, so that is refused too.
local MAX_HOOK_COUNT = 2147483647

local function budgeted()
  for _, figure in ipairs({
    { "INSTRUCTION_BUDGET", INSTRUCTION_BUDGET },
    { "INSTRUCTION_CEILING", INSTRUCTION_CEILING },
  }) do
    local name, n = figure[1], figure[2]
    if type(n) ~= "number" or not (n >= 1 and n <= MAX_HOOK_COUNT) or n % 1 ~= 0 then
      error(name .. " is " .. (type(n) == "number" and string.format("%.17g", n) or "a " .. type(n))
        .. ", where Lua hooks a positive integer count up to " .. MAX_HOOK_COUNT
        .. " and installs no hook for 0 while debug.gethook says one is set", 0)
    end
  end
  if INSTRUCTION_BUDGET > INSTRUCTION_CEILING then
    error("INSTRUCTION_BUDGET is " .. INSTRUCTION_BUDGET .. ", over INSTRUCTION_CEILING, " .. INSTRUCTION_CEILING, 0)
  end
end

-- One directory, with its missing parents. `lfs.mkdir` makes one level and
-- fails under a parent that is not there, so the walk goes up to the first
-- directory that exists, or to the drive, and makes each on the way back.
-- `true`, or nil, the path that refused and the library's reason.
local function ensure(lfs, path)
  if lfs.attributes(path, "mode") == "directory" then
    return true
  end
  local parent = path:match("^(.+)[/\\][^/\\]+$")
  if parent and not parent:find("^%a:$") then
    local ok, at, why = ensure(lfs, parent)
    if not ok then
      return nil, at, why
    end
  end
  local ok, why = lfs.mkdir(path)
  if not ok then
    return nil, path, tostring(why)
  end
  return true
end

-- The events log, rotated at load: `events.log` becomes `events.prev.log`
-- and this session starts an empty one, so what a supervisor reads after
-- a crash is this launch's and the launch before it, and the file has a
-- ceiling of two sessions rather than growing for as long as the install
-- lives. The remove of the older generation comes first, because Windows
-- will not rename onto an existing name, and its answer is discarded: on
-- a first launch there is nothing to remove. The rename is attempted only
-- where there is a file to move, so a first launch is silent, and a
-- failure — on Windows, a reader holding the file open — is recorded and
-- not raised: the session then appends to the generation already there,
-- which costs a reader one launch of extra history and is no reason to
-- refuse to run.
local function rotate(lfs, os, E)
  if lfs.attributes(E.events, "mode") ~= "file" then
    return
  end
  local prev = E.output .. SEP .. "events.prev.log"
  os.remove(prev)
  local ok, why = os.rename(E.events, prev)
  if not ok then
    E.events_left = E.events .. ": " .. tostring(why)
  end
end

-- A session is two directories deep, `<stamp>\req` and `<stamp>\res`, with
-- files in those. The sweep goes no deeper: a directory further down is
-- not something a session made, and is left with the sibling it is in.
local SESSION_DEPTH = 2

-- One directory and everything in it: files by `os.remove`, directories by
-- `lfs.rmdir` once they are empty, stopping at the first refusal. The names
-- are read before anything is removed, so the listing is not walked while
-- it changes. `true`, or nil, the path that refused and why.
local function remove_tree(lfs, os, path, depth)
  if depth > SESSION_DEPTH then
    return nil, path, "is deeper than a session goes"
  end
  local names = {}
  for name in lfs.dir(path) do
    if name ~= "." and name ~= ".." then
      names[#names + 1] = name
    end
  end
  for _, name in ipairs(names) do
    local entry = path .. SEP .. name
    local ok, at, why
    if lfs.attributes(entry, "mode") == "directory" then
      ok, at, why = remove_tree(lfs, os, entry, depth + 1)
    else
      ok, why = os.remove(entry)
      at = entry
    end
    if not ok then
      return nil, at, tostring(why)
    end
  end
  local ok, why = lfs.rmdir(path)
  if not ok then
    return nil, path, tostring(why)
  end
  return true
end

-- Every sibling of the session under the transport root, removed before
-- anything is written. Each is a session that has ended, and nothing in it
-- is addressed to this one, so its requests and replies go together, and a
-- request left in one is never listed, whatever it says inside. A sibling
-- that cannot be removed, which on Windows is one a client still holds a
-- handle on, is left for the next load to try again, named on the
-- namespace with what refused, and logged where there is a log; the export
-- state has none, which is why the namespace carries it. A file directly
-- under the root is not a session and is left alone. Nothing under the
-- output is ever swept.
--
-- The listing of the root itself is not guarded: the root was made or
-- found a moment ago, so a listing that raises says the filesystem is not
-- what it just was, and that stops the load with its reason rather than
-- going on to write there. Every name listed is checked as a path under
-- the root before anything is done to it, so a listing that answered for
-- some other directory would remove nothing.
local function sweep(E, lfs, os, log)
  local root = E.transport_root
  local names = {}
  for name in lfs.dir(root) do
    if name ~= "." and name ~= ".." and name ~= E.stamp then
      names[#names + 1] = name
    end
  end
  E.swept = 0
  for _, name in ipairs(names) do
    local path = root .. SEP .. name
    if lfs.attributes(path, "mode") == "directory" then
      local called, ok, at, why = pcall(remove_tree, lfs, os, path, 1)
      if called and ok then
        E.swept = E.swept + 1
      else
        if not called then
          at, why = path, tostring(ok)
        end
        local left = name .. ": " .. at .. " " .. why
        E.sweep_left = E.sweep_left or {}
        E.sweep_left[#E.sweep_left + 1] = left
        if type(log) == "table" and type(log.write) == "function" then
          log.write(NAME, log.WARNING, "left a session that could not be removed, " .. left)
        end
      end
    end
  end
end

-- The directories, made once the stamp exists: the output, the transport
-- root, and under the root `<stamp>\req` and `<stamp>\res`, which is the
-- session, with the root swept between. The output has already passed
-- containment, so one that cannot be made stops the load, as one that
-- failed containment did. A transport root from `lfs.tempdir()` gets the
-- quiet handling its containment refusal gets: it was a guess DCS handed
-- back, and a guess that cannot be made goes beside the output too; the
-- fallback itself failing stops the load. The arm file is named and not
-- made: a client creates it, and its presence is what wakes the executor,
-- so the executor making it would wake itself.
local function open_session(E, lfs, os, log)
  local ok, at, why = ensure(lfs, E.output)
  if not ok then
    error("the output directory " .. at .. " could not be created: " .. why, 0)
  end
  E.events = E.output .. SEP .. "events.log"
  rotate(lfs, os, E)
  ok, at, why = ensure(lfs, E.transport_root)
  if not ok and E.transport_source == "lfs.tempdir" then
    E.transport_refusal = at .. " could not be created: " .. why
    E.transport_root, E.transport_source = E.output .. SEP .. "rpc", "fallback: beside the output"
    ok, at, why = ensure(lfs, E.transport_root)
  end
  if not ok then
    error("the transport root " .. at .. " could not be created: " .. why, 0)
  end
  sweep(E, lfs, os, log)
  E.session = E.transport_root .. SEP .. E.stamp
  E.req = E.session .. SEP .. "req"
  E.res = E.session .. SEP .. "res"
  E.arm = E.session .. SEP .. "arm"
  -- The field the frame branches on, and the call the dormant branch makes
  -- to leave itself, taken from the same `lfs` the load already holds. A
  -- load is asleep because nothing has asked for anything yet: the arm file
  -- a client writes is the only thing that says otherwise, and a session
  -- nobody is using should cost a frame the counter and nothing else.
  E.armed = false
  E.quiet_since = nil
  attributes = lfs.attributes
  for _, dir in ipairs({ E.req, E.res }) do
    ok, at, why = ensure(lfs, dir)
    if not ok then
      error("the session directory " .. at .. " could not be created: " .. why, 0)
    end
  end
end

-- The envelope. A reply, the handshake and the heartbeat are one shape:
-- header lines, one blank line, then a body that is everything after it,
-- byte for byte. The shape needs no quoting rule, because the body is the
-- only place a newline may appear, and that holds only while every header
-- is checked here: a value carrying one would end its own line early and
-- the rest would be read as a header nobody wrote. So a bad header is
-- refused, never escaped, and nothing is written.
--
-- `headers` is a list of `{ name, value }` pairs in the order they are
-- written, never a map: `pairs` orders a table however it likes, and the
-- client's parser, when it comes, is checked against the bytes this file
-- produces. A name is
-- `[A-Za-z0-9_-]+`, spelt explicitly rather than with `%w`, which follows
-- the process locale, and appears once per envelope, read without regard
-- to case. A value is a string or a number, ASCII, without CR or LF, and
-- does not begin with whitespace, which a reader strips and would lose.
-- That is the reader's own rule and no stricter: a reply echoes the
-- request's `chunkname`, so a writer that refused what a request may carry
-- would fail a legal request at reply time. The body is a string, or nil
-- for none, and is never inspected.
--
-- The bytes, or nil and a reason naming the header.
local function frame(headers, body)
  local lines, seen = {}, {}
  for i, header in ipairs(headers) do
    local name, value = header[1], header[2]
    if type(name) ~= "string" or not name:find("^[A-Za-z0-9_%-]+$") then
      return nil, "header " .. i .. ": the name " .. tostring(name) .. " is not [A-Za-z0-9_-]+"
    end
    local key = name:lower()
    if seen[key] then
      return nil, name .. ": repeated"
    end
    seen[key] = true
    if type(value) == "number" then
      value = tostring(value)
    elseif type(value) ~= "string" then
      return nil, name .. ": the value is a " .. type(value) .. ", not a string"
    end
    if value:find("[\r\n]") then
      return nil, name .. ": the value carries a CR or LF"
    end
    if value:find("[\128-\255]") then
      return nil, name .. ": the value is not ASCII"
    end
    if value:find("^[ \t\v\f]") then
      return nil, name .. ": the value begins with whitespace, which a reader strips"
    end
    lines[#lines + 1] = name .. ": " .. value .. "\n"
  end
  if body == nil then
    body = ""
  elseif type(body) ~= "string" then
    return nil, "the body is a " .. type(body) .. ", not a string"
  end
  return table.concat(lines) .. "\n" .. body
end

-- One file, published by rename. The bytes go to `<path>.tmp`, in the
-- directory the file will live in so the rename never crosses a device,
-- and the final name appears whole or not at all: a reader that lists
-- nothing but an exact suffix never meets a half-written file. `os.remove`
-- of the final name comes first, because Windows will not rename onto an
-- existing name, and its answer is discarded: a fresh reply has no final to
-- remove, and a hold on the final that makes the remove fail makes the
-- rename fail after it, which is where the verdict is read. From the moment
-- the open succeeds a `.tmp` exists, and no failure past that point leaves
-- it behind. `write` and `close` are checked apart, because in Lua 5.1 a
-- buffered write can fail only at the close.
--
-- `true`, or nil and what refused.
local function publish(path, bytes)
  local io, os = rawget(_G, "io"), rawget(_G, "os")
  local tmp = path .. ".tmp"
  local fh, why = io.open(tmp, "wb")
  if not fh then
    return nil, tmp .. ": " .. tostring(why)
  end
  local ok
  ok, why = fh:write(bytes)
  if ok then
    ok, why = fh:close()
  else
    fh:close()
  end
  if ok then
    os.remove(path)
    ok, why = os.rename(tmp, path)
  end
  if not ok then
    os.remove(tmp)
    return nil, path .. ": " .. tostring(why)
  end
  return true
end

-- The reply to `id`: the session's headers, then the caller's, then the
-- body, published as `<res>\<id>.res`. `status` comes first so a reader
-- with one line has the verdict; `protocol` is the envelope's version;
-- `host`, `stamp` and `phase` say which session answered and what it was
-- doing; `id` echoes the name the request came under, which is only a
-- filename here, so the reply takes it whatever it spells; `tick` is the
-- frame counter as the reply is framed, so two replies carrying the same
-- one shared a frame, and one made before any frame carries 0. `cpu_ms`
-- is what handling the request has cost so far, in milliseconds to three
-- places: from just before the tick took it off the disk to the moment the
-- reply is framed, so the take and the chunk count and the publish of the
-- reply itself does not, because it has not happened. A reply framed
-- outside a tick, by a driver off DCS, handled nothing and reads `0.000`,
-- the clock read once. The name is the wire's; what it counts is
-- `os.clock`, which under the Microsoft C runtime is time elapsed and not
-- CPU time, stepping in whole milliseconds, so a cheap request reads
-- `0.000` and a request that blocks, on the disk or in a call into the
-- host, is charged the time it blocked. The session's headers are read off
-- the namespace as the reply is framed, so a figure the session learns to
-- keep later lands here without the caller changing.
--
-- `true`, or nil and what refused, in which case nothing was written.
local function reply(id, status, headers, body)
  local clock = rawget(rawget(_G, "os"), "clock")
  local now = clock()
  answered, charged = status, string.format("%.3f", (now - (began or now)) * 1000)
  local all = {
    { "status", status },
    { "protocol", PROTOCOL },
    { "host", E.host },
    { "stamp", E.stamp },
    { "phase", E.phase },
    { "id", id },
    { "tick", E.tick },
    { "cpu_ms", charged },
  }
  for _, header in ipairs(headers or {}) do
    all[#all + 1] = header
  end
  local bytes, why = frame(all, body)
  if not bytes then
    return nil, why
  end
  return publish(E.res .. SEP .. id .. ".res", bytes)
end

-- One request off the disk: its bytes, with the file gone before they are
-- returned, so that a chunk which kills the process cannot run again at
-- the next listing. The size comes from a stat, and a request over the
-- limit is removed without ever being opened. A file that is not there, or
-- that will not open, is `gone`: it went between the listing and this, or
-- is not a file, and there is nothing to answer and nothing to answer to.
-- A directory stats with a size and is `gone` because `io.open` refuses
-- it, which it does on this host; were one to open, the remove after the
-- read would refuse instead, and the bytes would be withheld either way.
-- A file read whole that cannot then be removed is `error`, and its bytes
-- are withheld, because a chunk that runs now and again at the next
-- listing is the case the remove exists to prevent.
--
-- The bytes, or nil, a status and a message.
local function take(path)
  local lfs, io, os = rawget(_G, "lfs"), rawget(_G, "io"), rawget(_G, "os")
  local size, why = lfs.attributes(path, "size")
  if type(size) ~= "number" then
    return nil, "gone", path .. ": " .. tostring(why)
  end
  if size > MAX_REQUEST_BYTES then
    os.remove(path)
    return nil, "bad-request",
      "the request is " .. size .. " bytes, over the " .. MAX_REQUEST_BYTES .. "-byte limit, and was not read"
  end
  local fh
  fh, why = io.open(path, "rb")
  if not fh then
    return nil, "gone", path .. ": " .. tostring(why)
  end
  local bytes = fh:read("*a")
  fh:close()
  local ok
  ok, why = os.remove(path)
  if not ok then
    return nil, "error", path .. " was read and could not be removed: " .. tostring(why)
  end
  return bytes
end

-- The start of a line for a message, so a refusal names what it saw
-- without carrying a whole line of a request into the reply.
local function excerpt(line)
  if #line > 80 then
    return line:sub(1, 80) .. "..."
  end
  return line
end

-- A request's envelope read back: header lines, one blank line, then the
-- body, which is everything after it byte for byte. The header block is
-- read one line at a time and the body is never scanned: a request is
-- mostly body, and a chunk may hold anything, including a line shaped like
-- a header. A line ends at LF, and one CR before the LF is dropped, which is
-- the whole of the CRLF normalisation and reads a block that mixes the two
-- endings. The empty line ends the headers, and bytes without one, or none
-- at all, are not an envelope and are refused.
--
-- A header line is `name: value`. The name is `[A-Za-z0-9_-]+`, spelt as
-- the framer spells it, and is read without regard to case, so the map holds
-- it lowered; one that repeats is refused, as the framer refuses to write
-- one. The value is everything after the first colon, so a value may carry
-- colons of its own, with leading blanks dropped, the ones the framer
-- refuses to write, and trailing ones kept; it may be empty. A value with a
-- byte past ASCII or a CR inside it is refused here rather than at reply
-- time, because a reply echoes what the request carried, and the framer
-- would refuse it then, after the request had run.
--
-- The headers as a map of lowered names, and the body; or nil and a reason
-- naming the line.
local function parse(bytes)
  local headers, pos, n = {}, 1, 0
  while true do
    local nl = bytes:find("\n", pos, true)
    if not nl then
      return nil, "the headers never end: no blank line before the bytes ran out, after " .. n .. " header lines"
    end
    local last = nl - 1
    if last >= pos and bytes:byte(last) == 13 then
      last = last - 1
    end
    if last < pos then
      return headers, bytes:sub(nl + 1)
    end
    n = n + 1
    local line = bytes:sub(pos, last)
    local name, value = line:match("^([A-Za-z0-9_%-]+):(.*)$")
    if not name then
      return nil, "line " .. n .. " is not a header: " .. excerpt(line)
    end
    value = value:gsub("^[ \t\v\f]+", "")
    if value:find("\r", 1, true) then
      return nil, "line " .. n .. ": " .. name .. ": the value carries a CR"
    end
    if value:find("[\128-\255]") then
      return nil, "line " .. n .. ": " .. name .. ": the value is not ASCII"
    end
    local key = name:lower()
    if headers[key] ~= nil then
      return nil, "line " .. n .. ": " .. name .. ": repeated"
    end
    headers[key] = value
    pos = nl + 1
  end
end

-- The ops whose body is the thing they run, so an empty one is a request
-- to run nothing and is refused before it gets that far. A `ping` carries
-- no body and whatever it carries is ignored. An install with `eval`
-- disabled requires nothing of one, so that every `eval` it sees is
-- answered `unsupported` by the op, an empty one included, rather than
-- `bad-request` here for a body that would never have run.
local BODY_REQUIRED = ALLOW_EVAL and { eval = true } or {}

-- One request, from its path to the point of running it or to the reply
-- that refuses it. The id is the filename with `.req` taken off and is
-- never checked against the shape of an id: the name is only a filename
-- here, and whatever it spells, the reply is published under it.
--
-- `take` has the bytes with the file gone. A request that went before it
-- could be taken is nothing to answer, and nothing is written. One read
-- that cannot be removed is answered `error` with `stage: bridge`, the
-- wire's word for the executor's own failure, and its bytes are withheld,
-- so the client learns rather than waits; what the tick loop does with a
-- file that stays is its own. An oversize request is answered with the
-- refusal `take` made without opening it.
--
-- Past the envelope, four things are checked here and no more. A request
-- must name the session it is for: one without `for`, or with an empty
-- one, carries no stamp for the fence to judge, and is refused before any
-- comparison. The stamp it names must be this session's: one that names
-- another is a client defect and is answered `stale-session`, echoing the
-- stamp it asked for so the client can see which session it addressed,
-- and it is refused here rather than by the op so that nothing it carries
-- can run. The echo is whole and uncapped while the message excerpts it,
-- which ADR 0006 weighs against the reply limit. That is the fence, and it
-- stands alone: a request for another session is judged on its stamp and
-- on nothing else, so a foreign one that would also be refused for a
-- second reason — no op, an unknown one, an empty body an op runs, a state
-- no host serves — still reads as foreign rather than as the refusal the
-- session it was written for would have sent. Only the envelope comes
-- first: bytes the parser cannot read carry no `for` to judge, and are
-- `bad-request` however foreign the stamp they spell. A request for this
-- session must then name an op, though which ops exist is the dispatcher's
-- to know, so an unknown one passes through to be refused there. And an op
-- that runs its body must have one. What an op makes of its other headers
-- is that op's.
--
-- The request as its id, headers and body, for the caller to run; or nil,
-- a status and a message, the reply already on the disk where there was
-- one to write, and a reply that could not be published reported as
-- `error` with the reason and a fourth value, true, because the request
-- is gone from the disk by then and a client waiting on it must not be
-- left to wait; the fourth value is for a caller that counts such losses,
-- so it need not read the message to know one happened.
local function admit(path)
  local id = path:match("[^/\\]+$") or path
  id = id:match("^(.*)%.req$") or id
  local bytes, status, why = take(path)
  local extra
  if bytes then
    local headers, body = parse(bytes)
    status = "bad-request"
    if not headers then
      why = body
    elseif headers["for"] == nil or headers["for"] == "" then
      why = "no for: the request does not name the session stamp it is for"
    elseif headers["for"] ~= E.stamp then
      status = "stale-session"
      extra = { { "for", headers["for"] } }
      why = "for: " .. excerpt(headers["for"]) .. " is not this session's stamp, "
        .. E.stamp .. ": the request was written for another session and was not run"
    elseif headers.op == nil or headers.op == "" then
      why = "no op"
    elseif BODY_REQUIRED[headers.op] and body == "" then
      why = "the body is empty, and " .. headers.op .. " runs it"
    else
      return { id = id, headers = headers, body = body }
    end
  elseif status == "gone" then
    return nil, status, why
  end
  if status == "error" then
    extra = { { "stage", "bridge" } }
  end
  local ok, failed = reply(id, status, extra, why)
  if not ok then
    return nil, "error", "the " .. status .. " reply to " .. id .. " was not published: " .. tostring(failed), true
  end
  return nil, status, why
end

-- One line appended to the events log, with its newline. Append and not
-- publish-by-rename: this file is a record a supervisor reads after the
-- process died, so a line must be on the disk the moment it is written
-- and the last line of a killed session is the point of the file. It is
-- opened and closed per line for the same reason — a handle held open
-- across the kill is a buffer nobody flushes — and that cost is paid per
-- request handled and never per frame. A line that cannot be written is
-- counted and swallowed: the events log is how a crash is read afterwards
-- and never how a request is answered, so a session that cannot write it
-- goes on answering.
--
-- `true`, or nil.
record = function(line)
  local io = rawget(_G, "io")
  local fh, why = io.open(E.events, "ab")
  if fh then
    local ok
    ok, why = fh:write(line .. "\n")
    if ok then
      ok, why = fh:close()
    else
      fh:close()
    end
    if ok then
      return true
    end
  end
  E.unrecorded = E.unrecorded + 1
  E.last_unrecorded = E.events .. ": " .. tostring(why)
  return nil
end

-- One field of a marker, out of bytes a client wrote. The reader splits a
-- record on `|` and on the line, so a value that carries either byte would
-- be a record of the client's own writing, and a request may be a quarter
-- of a megabyte where the file it lands in is bounded by two launches. So
-- a field is cut to the excerpt every refusal message keeps and every byte
-- in it that is not printable ASCII, the separator included, is written
-- `?`. ADR 0007 holds the argument, and a field the executor spells — the
-- stamp, a status, a cost — goes in as it is.
local function field(value)
  return (excerpt(tostring(value)):gsub("[^\32-\126]", "?"):gsub("|", "?"))
end

-- The four fields that open a marker for one request: which request, what
-- it asked for, where, and the session that took it. An op that names no
-- state, which is `ping`, leaves that field empty.
local function marked(req)
  return field(req.id) .. "|" .. field(req.headers.op) .. "|"
    .. field(req.headers.state or "") .. "|" .. E.stamp
end

-- The `states` header for `host`: one entry per state it answers, in the
-- order declared, each `name:carrier=…,returns=…,needs=…`. The entries
-- are separated by one space, because the comma is taken inside them.
local function states(host)
  local entries = {}
  for i, row in ipairs(STATES[host]) do
    entries[i] = row[1] .. ":carrier=" .. row[2] .. ",returns=" .. row[3] .. ",needs=" .. row[4]
  end
  return table.concat(entries, " ")
end

-- The last callback other than the frame to fire, as `<name>@<tick>`, or
-- the empty string while none has. Two files-worth of readers want the same
-- string — a reply a client asked for, and the heartbeat a client reads
-- without asking — and two spellings of one field would drift.
local function last_callback()
  if E.last_callback_name then
    return E.last_callback_name .. "@" .. E.last_callback_tick
  end
  return ""
end

-- The ops, by the name a request spells, each taking an admitted request
-- and answering what `reply` answers. The table is published on the
-- namespace, so a driver off DCS can hang an op on it and see the tick
-- carry it.
--
-- `ping` answers with what the session is doing: the phase and tick every
-- reply carries, `states` as the handshake declares them, and the callback
-- record. `last_callback` is `<name>@<tick>`, the last callback other than
-- the frame to fire; `callbacks` is every name seen this session in the
-- order first seen, comma separated. Both are empty until one has fired,
-- an empty value being the wire's spelling for none, where `ABSENT` is its
-- spelling for a read that did not answer. The body is `pong`, whatever
-- the request carried.
local OPS = {}

function OPS.ping(req)
  return reply(req.id, "ok", {
    { "states", states(E.host) },
    { "last_callback", last_callback() },
    { "callbacks", table.concat(E.callbacks, ",") },
  }, "pong")
end

-- The row of `STATES[host]` for a state name, or nil. `server` is a second
-- name for `scripting` on the wire and is looked up as that.
local function state_row(host, name)
  if name == "server" then
    name = "scripting"
  end
  for _, row in ipairs(STATES[host]) do
    if row[1] == name then
      return row
    end
  end
  return nil
end

-- What a chunk finished with, converted to the three fields a reply is
-- built from, as Lua source. It is source and not functions because the
-- same conversion runs in two places: here, for the host's own state, and
-- inside every other state, where it travels in as part of the wrapper
-- `net.dostring_in` carries, so that a value is converted where it lives
-- and only a string ever crosses back. One copy of the source is what
-- keeps the two from drifting apart; the price is that the language
-- server does not check inside a string, so a slip in here is found by
-- the harness and not by the lint. The host's own carrier compiles it
-- the first time it answers, and never at load, because a load and a
-- `ping` parse nothing as code; the wrapper embeds it as it is. Nothing
-- in it reads a global the sanitised states lack: `string`, `tonumber`
-- and `type` are in every state, and infinity is computed rather than
-- read from `math`.
--
-- What the source says. A number is printed so that a reader gets the
-- same double back: `%.14g` is what `tostring` prints and reads well;
-- where it does not read back as the number it was printed from, `%.17g`
-- does, because seventeen significant digits identify every double, so
-- `0.1 + 0.2` reads `0.30000000000000004` and never `0.3`. `inf`, `-inf`
-- and `nan` are named, because what `%g` prints for them is the C
-- runtime's, `-nan(ind)` on this host, and a consumer reads the three
-- names back. What a chunk raised with is carried verbatim where it is a
-- string, which for `error("x")` begins `<chunkname>:<line>:`, printed
-- where it is a number, and otherwise named by its type. What a chunk
-- returned is the bytes verbatim for a string, its name for a boolean, a
-- number as printed, and for everything else, nil included, an empty body
-- with the type beside it: a table is never serialised, and a consumer
-- that wants inside one ships a chunk that does it in the state.
-- `tostring` is never called on a value from a state, because it would
-- run a `__tostring` the chunk installed, and the executor runs nothing
-- of a chunk's but the chunk.
--
-- The function the source returns takes what `pcall` answered about the
-- chunk, what the chunk returned or raised with, the ceiling, and whether
-- the chunk spent its instruction budget, and answers a status, a detail
-- and a body: `ok` with the type and the printed value; `run` with the
-- message; `budget` with the message, for a chunk that spent its budget,
-- whatever it went on to return; or `oversize`, with what was
-- too big and its length, when the body would be over the ceiling. A
-- body over the ceiling is refused whole and never cut, because a cut
-- lands somewhere inside whatever the body is and the reader cannot tell
-- the loss from data. The ceiling holds for a raise as for a return,
-- because a chunk builds a message as cheaply as a value, and the reply
-- is what the ceiling in the handshake bounds. Only the first value a
-- chunk returns is answered.
local CONVERT = [==[
local INF = 1 / 0
local function number(n)
  if n ~= n then
    return "nan"
  elseif n == INF then
    return "inf"
  elseif n == -INF then
    return "-inf"
  end
  local printed = string.format("%.14g", n)
  if tonumber(printed) ~= n then
    printed = string.format("%.17g", n)
  end
  return printed
end
local function raised(value)
  local kind = type(value)
  if kind == "string" then
    return value
  elseif kind == "number" then
    return number(value)
  end
  return "(error object is a " .. kind .. " value)"
end
local function described(value)
  local kind = type(value)
  if kind == "string" then
    return kind, value
  elseif kind == "number" then
    return kind, number(value)
  elseif kind == "boolean" then
    return kind, value and "true" or "false"
  end
  return kind, ""
end
return function(ok, value, ceiling, exceeded)
  local status, detail, body
  if exceeded then
    ok, status, detail, body = false, "budget", "", raised(value)
  elseif ok then
    status, detail, body = "ok", described(value)
  else
    status, detail, body = "run", "", raised(value)
  end
  if #body > ceiling then
    return "oversize", ok and "result" or "error message", string.format("%d", #body)
  end
  return status, detail, body
end
]==]

-- What bounds a chunk, as Lua source for the reason the conversion is: it
-- runs in the host's own state and inside every other, where it travels in
-- the wrapper. The function it returns takes the instruction count the
-- request settled on and answers the budget a reply reports, and the
-- function that runs a chunk under it.
--
-- The count hook is set in the state the chunk runs in, just before the
-- chunk, and cleared just after. The budget is `none`, and the chunk runs
-- under a bare `pcall`, where the count is `0`, where the state has no
-- `debug` to set a hook with, as `mission` has none, and where a hook is
-- already set: that hook is DCS's or a debugger's, and this executor never
-- displaces something another tool installed. `debug` is read when the
-- chunk is bound, never at load, as every other read of a state is.
--
-- Once the count is spent the hook raises in the chunk, with the position
-- it stopped at, so the message reads `<chunkname>:<line>:` where the loop
-- was. It raises once more on every instruction after that, because a
-- chunk that caught the first raise with `pcall` would otherwise go on
-- looping, caught once a count, forever; a budget once spent stays spent.
-- What the chunk finished with is then `budget` whatever `pcall` answered.
-- The hook fires by count and not by place, so it can fire on the few
-- instructions of `run` itself between the chunk returning and the hook
-- being cleared; a firing there is the executor's own and passes, and
-- `run` is the local the hook compares against for that reason.
--
-- Three limits, stated rather than hidden. The hook counts VM instructions
-- and cannot interrupt one long C call. A chunk can clear the hook itself,
-- so this is a guard against accident and not a boundary. And a coroutine
-- the chunk runs escapes it: Lua 5.1's `debug` keeps a hook's function per
-- thread, so a loop inside `coroutine.wrap` counts against nothing.
local RUN = [==[
return function(count)
  local debug = rawget(_G, "debug")
  local sethook, gethook, getinfo
  if type(debug) == "table" then
    sethook, gethook, getinfo = rawget(debug, "sethook"), rawget(debug, "gethook"), rawget(debug, "getinfo")
  end
  if count == 0 or type(sethook) ~= "function" or type(gethook) ~= "function"
    or type(getinfo) ~= "function" or gethook() ~= nil then
    return "none", function(chunk)
      local ok, value = pcall(chunk)
      return ok, value, false
    end
  end
  local exceeded, run = false, nil
  local function hook()
    if getinfo(2, "f").func == run then
      return
    end
    if not exceeded then
      exceeded = true
      sethook(hook, "", 1)
    end
    error("the chunk ran past its budget of " .. count .. " instructions and was stopped", 2)
  end
  run = function(chunk)
    sethook(hook, "", count)
    local ok, value = pcall(chunk)
    sethook()
    return ok, value, exceeded
  end
  return "instructions=" .. count, run
end
]==]

-- The functions `CONVERT` and `RUN` return, compiled by the host's own
-- carrier when it first answers.
local finish, bind

-- `headers` with `tail` after it, either of which may be nil.
local function joined(headers, tail)
  if not tail then
    return headers
  end
  local all = {}
  for _, header in ipairs(headers or {}) do
    all[#all + 1] = header
  end
  for _, header in ipairs(tail) do
    all[#all + 1] = header
  end
  return all
end

-- The reply to a chunk from the four fields the carrier answered with,
-- which for the host's own state are `finish`'s with the budget it bound
-- the chunk under, and for every other state are what the wrapper sent
-- back. `ok` carries the type and the
-- value; `oversize` is `error` with `result_bytes`, so the caller can
-- size its next request, and a body that is the refusal and not one byte
-- of the result; `unsupported` is a state that could not compile at all;
-- and any other status is `error` under that stage with the message as
-- the body. `no-mission` is `a_do_script` finding no mission, a status
-- of its own so a caller can branch on it. `chunkname` rides every reply
-- to a chunk that was compiled, so a reader knows what its line numbers
-- are relative to. `budget` follows it wherever the carrier said what bound
-- the chunk, and is empty, and absent from the reply, where nothing did:
-- a state that answered outside the wrapper's shape said nothing about
-- it. `tail` is the headers a carrier adds to every reply it answers,
-- after the rest.
local function answer(req, chunkname, status, detail, budget, body, tail)
  local named = { { "chunkname", chunkname } }
  if budget ~= "" then
    named[2] = { "budget", budget }
  end
  if status == "ok" then
    return reply(req.id, "ok", joined(joined({ { "result_type", detail } }, named), tail), body)
  elseif status == "oversize" then
    return reply(req.id, "error", joined(joined(joined({ { "stage", "oversize" } }, named), {
      { "result_bytes", body },
    }), tail), "the " .. detail .. " is " .. body .. " bytes, over the " .. MAX_RESULT_BYTES
      .. "-byte ceiling, and was refused whole rather than cut")
  elseif status == "unsupported" or status == "no-mission" then
    return reply(req.id, status, joined(nil, tail), body)
  end
  return reply(req.id, "error", joined(joined({ { "stage", status } }, named), tail), body)
end

-- The carrier for the host's own state: the body compiled with the
-- request's `chunkname` and nothing else, run with the host's `_G` as its
-- globals. Nothing is ever put in front of the body, because a `chunkname`
-- is a promise that `<name>:47` is the caller's line 47, and one line of
-- prologue would make every number in every message a lie. Compiling with
-- the host's `_G` rather than this file's environment makes no difference
-- on DCS, where the two are one table, and is what lets a chunk read the
-- namespace this file published and never a local of this file. A compile
-- failure is `stage: compile` with Lua's message verbatim, a raise while
-- running `stage: run`, and both carry `chunkname` beside the message, so a
-- reader knows what its line numbers are relative to. The chunk runs under
-- the `count` the request settled on, and every reply to it says what
-- bound it, a compile failure included, which would have been bound by
-- the same. What the chunk returned or raised with is `answer`'s to reply
-- to.
local function eval_local(req, chunkname, count)
  local loadstring, setfenv = rawget(_G, "loadstring"), rawget(_G, "setfenv")
  if type(loadstring) ~= "function" then
    return reply(req.id, "unsupported", nil, "no loadstring in this state")
  elseif type(setfenv) ~= "function" then
    return reply(req.id, "unsupported", nil, "no setfenv in this state")
  end
  if not finish then
    local compiled = {}
    for i, source in ipairs({ { "convert", CONVERT }, { "run", RUN } }) do
      local built, reason = loadstring(source[2], "=" .. NAME .. "." .. source[1])
      if not built then
        error("the " .. source[1] .. " source did not compile: " .. reason, 0)
      end
      setfenv(built, _G)
      compiled[i] = built()
    end
    finish, bind = compiled[1], compiled[2]
  end
  local budget, run = bind(count)
  local chunk, why = loadstring(req.body, chunkname)
  if not chunk then
    return answer(req, chunkname, "compile", "", budget, why)
  end
  setfenv(chunk, _G)
  local ok, value, exceeded = run(chunk)
  local status, detail, body = finish(ok, value, MAX_RESULT_BYTES, exceeded)
  return answer(req, chunkname, status, detail, budget, body)
end

-- The wrapper `net.dostring_in` carries into another state, in two parts
-- around the two literals a request supplies. The body is never
-- concatenated into code that is compiled: it travels as a `%q` literal,
-- which Lua 5.1.5 renders so that it decodes byte for byte (a newline as
-- a backslash and a newline, `\0` as `\000`, `\r`, `"` and `\` escaped),
-- and the wrapper compiles it in the target state with `loadstring`
-- under the request's `chunkname`, so the chunk the body becomes is its
-- own chunk whose line 1 is the body's line 1 and `<name>:47` is the
-- caller's line 47 there as it is here. The chunk is run in the target
-- state's globals, which is what the bare body would have had, and what
-- it finished with is converted in that state by the conversion source
-- above, so nothing but a string crosses back.
--
-- The string that crosses is four fields: a status, a detail, the budget
-- and the body, the first three on a line each and the body the rest, so
-- a body of any bytes and any length is carried whole. The status is the
-- conversion's `ok`, `run`, `budget` or `oversize`; `compile` where
-- `loadstring` refused the body, with Lua's message; `unsupported` where
-- the state has no `loadstring` or no `setfenv`, the refusal the local
-- carrier makes for the same lack; and `bridge` where the wrapper itself
-- raised, which is the executor's own failure and named as such. The
-- budget is what bound the chunk, read in the state because only the state
-- knows whether it has a `debug` to hook with, and settled before the body
-- is compiled, so `compile` carries it as every status past that point
-- does; `unsupported` and `bridge` leave it empty. The count the request
-- settled on travels in as a number the executor printed, never as
-- anything a requester wrote. The whole wrapper runs under `pcall`,
-- because what `net.dostring_in` does with a chunk that raises is not
-- measured, and a raise that reached it would be one this executor could
-- not answer. ADR 0005 holds the argument for the four fields.
local WRAP_HEAD = "return (function() local wrapped, answered = pcall(function() "
  .. "local finish = (function() " .. CONVERT .. " end)() "
  .. "local bind = (function() " .. RUN .. " end)() "
  .. 'local loadstring, setfenv = rawget(_G, "loadstring"), rawget(_G, "setfenv") '
  .. 'if type(loadstring) ~= "function" then return "unsupported\\n\\n\\nno loadstring in this state" end '
  .. 'if type(setfenv) ~= "function" then return "unsupported\\n\\n\\nno setfenv in this state" end '
  .. "local budget, run = bind("
local WRAP_MID = ") "
  .. "local chunk, why = loadstring("
local WRAP_TAIL = ") "
  .. 'if not chunk then return "compile\\n\\n" .. budget .. "\\n" .. why end '
  .. "setfenv(chunk, _G) "
  .. "local ok, value, exceeded = run(chunk) "
  .. "local status, detail, body = finish(ok, value, " .. MAX_RESULT_BYTES .. ", exceeded) "
  .. 'return status .. "\\n" .. detail .. "\\n" .. budget .. "\\n" .. body end) '
  .. "if wrapped then return answered end "
  .. 'return "bridge\\n\\n\\n" .. (type(answered) == "string" and answered or type(answered)) end)()'

local function wrapper(body, chunkname, count)
  return WRAP_HEAD .. string.format("%d", count) .. WRAP_MID .. string.format("%q", body) .. ", "
    .. string.format("%q", chunkname) .. WRAP_TAIL
end

-- The `a_do_script` carrier: the one way into `missionscripting`, which
-- `net.dostring_in` reaches under no name. It takes two hops. The first
-- is `net.dostring_in` into `mission`, carrying `NEAR`; the second is
-- `NEAR` calling `a_do_script`, a global of `mission` that runs a chunk
-- in exactly the environment a mission's `DO SCRIPT` trigger runs in.
--
-- `FAR` is what `a_do_script` runs: the same wrapper the other states
-- get, with the body, `chunkname` and the instruction count read from its
-- arguments rather than from literals, because `a_do_script` hands a chunk
-- what it was passed after the source as `...`. So the far chunk's source
-- is the same bytes for every request, and what a requester wrote crosses
-- into `mission` once, as a `%q` literal in `NEAR`, and into
-- `missionscripting` as a string argument, never as code. The count
-- crosses as the string the executor printed, because strings are what
-- `a_do_script` was measured passing on. A far chunk that receives no
-- string body, name or count says so, rather than compiling nothing,
-- because whether `a_do_script` passes its arguments on is measured on
-- one build.
--
-- `a_do_script` shifts what the chunk returns by one: `v1 … vN` arrives
-- as `nil, v1 … v(N-1)`, so a lone value is dropped entirely. `FAR` ends
-- with a second, sacrificial value, `0`, and `NEAR` reads the payload out
-- of slot 2. Where the payload in slot 2 is not a string, `NEAR` refuses
-- it without reading it and names the types of both slots: were a DCS
-- build to correct the shift, the payload would be in slot 1 and the
-- message says so, rather than the reply reading as an empty result.
--
-- Nothing but a string crosses `a_do_script`. A table returned through it
-- has corrupted DCS's allocator, the damage surfacing during the call, at
-- mission teardown or on the next load, so a clean crossing clears
-- nothing. The wrapper converts the result inside `missionscripting`,
-- which is the mechanism; the slot-2 refusal is the backstop for a far
-- chunk that escaped it, and not the mechanism.
--
-- `NEAR` marks the crossing where the events log cannot reach. The
-- session's own markers bracket the dispatch, in a file this state has no
-- `io` to write; these are the same grammar through `log.write`, into
-- `dcs.log`, so a kill inside `missionscripting` leaves an opening marker
-- with no closing one in each file and a reader of either names the same
-- request. The opening one is written before `a_do_script` is looked for,
-- the closing one once the answer is in hand, with the four fields the
-- executor built and its `cpu_ms` empty: `mission` is sanitised and has
-- no clock to charge one from, and the tick has already charged the
-- crossing. ADR 0007 holds the shape. A state with no `log` is marked in
-- neither direction rather than refused, as every other read of a host
-- global here is.
--
-- `a_do_script` is nil at the main menu and whenever no mission is
-- loaded, and `NEAR` reads it at the moment of use, never from a
-- previous crossing, because this executor outlives any one mission.
-- Finding none is `no-mission`. `NEAR` answers in the wrapper's four
-- fields, adding two statuses of its own: `no-mission`, and `a_do_script`
-- for a call that raised or did not hand back a string. Its own raise is
-- `bridge`, as the wrapper's is. None of its own answers ran a chunk, so
-- each leaves the budget empty.
local FAR = 'local body, chunkname, count = ... '
  .. 'if type(body) ~= "string" or type(chunkname) ~= "string" or type(count) ~= "string" then '
  .. 'return "a_do_script\\n\\n\\nthe far chunk was handed a " .. type(body) .. " body, a " .. type(chunkname) '
  .. '.. " chunkname and a " .. type(count) .. " count, where a_do_script passes its arguments on as strings", 0 end '
  .. WRAP_HEAD .. "tonumber(count)" .. WRAP_MID .. "body, chunkname" .. WRAP_TAIL .. ", 0"

local NEAR_HEAD = "return (function(far, body, chunkname, count, mark) "
  .. "local called, answered = pcall(function() "
  .. 'local log = rawget(_G, "log") '
  .. 'local marks = type(log) == "table" and type(log.write) == "function" '
  .. 'if marks then log.write("' .. NAME .. '", log.INFO, "B|" .. mark) end '
  .. "local crossing = (function() "
  .. 'local a_do_script = rawget(_G, "a_do_script") '
  .. 'if type(a_do_script) ~= "function" then return "no-mission\\n\\n\\nno mission is loaded: a_do_script is " '
  .. '.. type(a_do_script) .. " in the mission state, and it is defined only with a mission loaded" end '
  .. "local crossed, lead, payload = pcall(a_do_script, far, body, chunkname, count) "
  .. 'if not crossed then return "a_do_script\\n\\n\\na_do_script raised: " '
  .. '.. (type(lead) == "string" and lead or "(error object is a " .. type(lead) .. " value)") end '
  .. 'if type(payload) ~= "string" then return "a_do_script\\n\\n\\nslot 1 is " .. type(lead) .. " and slot 2 is " '
  .. '.. type(payload) .. ", where a_do_script\'s shift puts a nil in slot 1 and the string payload in slot 2" end '
  .. "return payload end)() "
  .. 'if marks then log.write("' .. NAME .. '", log.INFO, "O|" .. mark:match("^[^|]*") '
  .. '.. "|" .. crossing:match("^[^\\n]*") .. "|") end '
  .. "return crossing end) "
  .. "if called then return answered end "
  .. 'return "bridge\\n\\n\\n" .. (type(answered) == "string" and answered or type(answered)) end)('

local function near(body, chunkname, count, mark)
  return NEAR_HEAD .. string.format("%q", FAR) .. ", " .. string.format("%q", body) .. ", "
    .. string.format("%q", chunkname) .. ", " .. string.format('"%d"', count) .. ", "
    .. string.format("%q", mark) .. ")"
end

-- The headers every reply through `a_do_script` carries, so a reader
-- knows the result crossed two hops and could only have been a string.
local A_DO_SCRIPT_TAIL = { { "carrier", "a_do_script" }, { "via", "mission" } }

-- The statuses a wrapper sends back, so that a string in the four-field
-- shape by accident is not read as one, and `NEAR`'s, which adds two.
local WRAPPER_STATUS = {
  ok = true,
  run = true,
  budget = true,
  oversize = true,
  compile = true,
  unsupported = true,
  bridge = true,
}

local A_DO_SCRIPT_STATUS = { ["no-mission"] = true, a_do_script = true }
for status in pairs(WRAPPER_STATUS) do
  A_DO_SCRIPT_STATUS[status] = true
end

-- The four fields out of what a wrapper sent back, or nil where the
-- string is not in the shape of one of `statuses`.
local function decode(answered, statuses)
  local first = answered:find("\n", 1, true)
  if not first or not statuses[answered:sub(1, first - 1)] then
    return nil
  end
  local second = answered:find("\n", first + 1, true)
  local third = second and answered:find("\n", second + 1, true)
  if not third then
    return nil
  end
  return answered:sub(1, first - 1), answered:sub(first + 1, second - 1), answered:sub(second + 1, third - 1),
    answered:sub(third + 1)
end

-- The carrier for the states `net.dostring_in` reaches from the hook
-- host. Its three answers are kept apart on the wire, because a reader
-- that sniffs for one reads a refusal as a success. A string is what the
-- wrapper sent back, decoded and answered as the local carrier's is. `nil`
-- is `refused`: this role may not reach the state, the name is not one
-- this DCS knows, or the operator's policy gate refused it, and the
-- executor cannot tell which; the gate is a live runtime condition and
-- never a defect to fix by writing a configuration file. The literal
-- `Invalid state name` is `invalid-state`: a state DCS knows and cannot
-- reach in this phase, as `export` at the menu. A string not in the
-- wrapper's shape is the state answering with something other than the
-- wrapper's reply, and any other value is a carrier answering with
-- something other than a string; both are `error` under `stage:
-- dostring_in`, the string carried as the body under the ceiling, the
-- value named by its type and never stringified. `net` is read at the
-- call, not at load, because the executor holds nothing of the host's
-- but what it read for this request. The state is passed as the request
-- spelt it: `server` is a name DCS answers to, and a caller who said it
-- reaches what it named.
--
-- The `a_do_script` carrier is this carrier too, into `mission` with
-- `NEAR` in place of the wrapper. Every reply it answers carries
-- `A_DO_SCRIPT_TAIL`, `NEAR`'s own statuses are read, and a string in
-- neither shape is `stage: a_do_script`, because `NEAR` hands back only
-- its own answers or the payload.
local function eval_dostring(req, chunkname, count, state, through_mission)
  local chunk, statuses, unshaped, tail = wrapper(req.body, chunkname, count), WRAPPER_STATUS, "dostring_in", nil
  if through_mission then
    chunk, statuses, unshaped, tail = near(req.body, chunkname, count, marked(req)), A_DO_SCRIPT_STATUS, "a_do_script",
      A_DO_SCRIPT_TAIL
  end
  local net = rawget(_G, "net")
  local dostring_in = type(net) == "table" and rawget(net, "dostring_in")
  if type(dostring_in) ~= "function" then
    return reply(req.id, "unsupported", joined(nil, tail), "no net.dostring_in on this host")
  end
  local answered = dostring_in(state, chunk)
  if answered == nil then
    return reply(req.id, "refused", joined(nil, tail), "net.dostring_in returned nil for " .. state
      .. ": this role may not reach it, the name is not one this DCS knows, or the operator's policy gate refused it")
  elseif answered == "Invalid state name" then
    return reply(req.id, "invalid-state", joined(nil, tail), "net.dostring_in answered 'Invalid state name' for "
      .. state .. ": DCS knows the state and cannot reach it in this phase")
  elseif type(answered) ~= "string" then
    return reply(req.id, "error", joined({ { "stage", "dostring_in" }, { "chunkname", chunkname } }, tail),
      "net.dostring_in answered a " .. type(answered) .. " value for " .. state .. ", and the executor reads a string alone")
  end
  local status, detail, budget, body = decode(answered, statuses)
  if not status then
    status, detail, budget, body = unshaped, "", "", answered
    if #body > MAX_RESULT_BYTES then
      status, detail, body = "oversize", "answer", string.format("%d", #body)
    end
  end
  return answer(req, chunkname, status, detail, budget, body, tail)
end

-- `eval` runs the body in the state the request names. An install with
-- `ALLOW_EVAL` off answers `unsupported` to every one, before anything is
-- read. A request must name its state: the specification names no
-- default, and a chunk that runs somewhere the caller did not say is the
-- one thing this op must never do, so one without is refused as one
-- without `for` is. A state this host does not declare is `unsupported`,
-- the wire's word for a thing this install does not do. `missionscripting`
-- is reached through `a_do_script`, one hop past `mission`.
-- `chunkname` is echoed only where a chunk was compiled under it; a
-- refusal compiled nothing and carries none. `max_instructions` is the
-- count the chunk may spend: absent or empty it is `INSTRUCTION_BUDGET`,
-- `0` runs the chunk unbounded, a count over `INSTRUCTION_CEILING` is
-- held to the ceiling rather than refused, and anything but digits is
-- `bad-request`, because a count read loosely is the one Lua would not
-- hook.
function OPS.eval(req)
  if not ALLOW_EVAL then
    return reply(req.id, "unsupported", nil, "eval is disabled in this install")
  end
  local state = req.headers.state
  if state == nil or state == "" then
    return reply(req.id, "bad-request", nil, "no state: the request does not name the state to run in")
  elseif not state:find("^[A-Za-z][A-Za-z0-9_]*$") then
    return reply(req.id, "bad-request", nil, "state: " .. excerpt(state) .. " is not [A-Za-z][A-Za-z0-9_]*")
  end
  local chunkname = req.headers.chunkname
  if chunkname == nil or chunkname == "" then
    chunkname = DEFAULT_CHUNKNAME
  elseif #chunkname > MAX_CHUNKNAME_BYTES then
    return reply(req.id, "bad-request", nil,
      "chunkname: " .. #chunkname .. " bytes, over the " .. MAX_CHUNKNAME_BYTES .. "-byte limit")
  end
  local count = req.headers.max_instructions
  if count == nil or count == "" then
    count = INSTRUCTION_BUDGET
  elseif not count:find("^%d+$") then
    return reply(req.id, "bad-request", nil,
      "max_instructions: " .. excerpt(count) .. " is not a non-negative integer")
  else
    count = tonumber(count)
    if count > INSTRUCTION_CEILING then
      count = INSTRUCTION_CEILING
    end
  end
  local row = state_row(E.host, state)
  if not row then
    return reply(req.id, "unsupported", nil, state .. " is not a state this host serves")
  elseif row[2] == "local" then
    return eval_local(req, chunkname, count)
  elseif row[2] == "dostring_in" then
    return eval_dostring(req, chunkname, count, state)
  elseif row[2] == "a_do_script" then
    return eval_dostring(req, chunkname, count, "mission", true)
  end
  return reply(req.id, "unsupported", nil, state .. " is declared with a carrier this executor does not build")
end

-- One admitted request to its reply. A name the table lacks is
-- `bad-request`. An op that raises is answered `error` under
-- `stage: bridge`, the wire's word for the executor's own failure, with
-- the message as the body, and the raise goes no further: the request is
-- already off the disk, so one that escaped would leave its client waiting
-- on a reply that never comes, and would end the tick for every request
-- listed after it.
--
-- `true`, or nil and what refused, as `reply` answers.
local function dispatch(req)
  local op = OPS[req.headers.op]
  if op then
    local called, ok, why = pcall(op, req)
    if called then
      return ok, why
    end
    return reply(req.id, "error", { { "stage", "bridge" } }, tostring(ok))
  end
  return reply(req.id, "bad-request", nil, "unknown op: " .. req.headers.op)
end

-- The names of requests answered `error` because they were read and could
-- not be removed. Each stays on the disk, and is skipped while it is
-- listed, so it is neither answered again nor, worse, run again; it is
-- forgotten once it is gone, so the same name can come back as a new
-- request.
local held = {}

-- The way back to sleep, in the order that makes the race unwinnable, which
-- is the only reason the order is worth stating. A client publishes its
-- request by rename and then, finding no arm file, creates one. So the arm
-- file goes first and the last listing second: a request that lands after
-- this listing lands beside an arm file the client had to create, because
-- this executor had already let go of it, and the next probe finds both.
-- Were the listing first, a client could publish between the listing and
-- the removal, see the file still on the disk and create nothing, and the
-- removal would then leave its request in front of an executor asleep with
-- nothing left to wake it.
--
-- A listing that does hold a request is the other end of the same race: the
-- arm file goes back, the executor stays awake, and the ordinary frame
-- after this one answers what it found. A failure to put the file back is
-- swallowed the way a failure to record a line is, because staying awake is
-- the safe end of it and a client that finds the file gone creates it
-- again.
--
-- The sweep that belongs with a disarm is not built. The heartbeat is: it is
-- written off the frame's own clock reading, which is why `now` comes in
-- rather than being read here — this is only ever called from the foot of an
-- armed frame, which already holds one (ADR 0009).
local function disarm(now)
  local os = rawget(_G, "os")
  os.remove(E.arm)
  local lfs = rawget(_G, "lfs")
  for name in lfs.dir(E.req) do
    if name:sub(-4) == ".req" then
      local fh = rawget(_G, "io").open(E.arm, "ab")
      if fh then
        fh:close()
      end
      E.quiet_since = nil
      return
    end
  end
  record("disarm|" .. E.stamp .. "|" .. E.tick)
  E.armed = false
  E.quiet_since = nil
  E.since = now
  heartbeat(now)
end

-- The frame. Every frame the request directory is listed and the requests
-- in it are answered in name order, so replies come back in the order
-- requests were published, and a client that publishes several shares the
-- tick between them. Only a name ending in exactly `.req` is a request: a
-- client publishes by rename from `.req.tmp`, and a reader with a looser
-- suffix would meet a half-written file. Listing every frame is the armed
-- shape, and it is the shape a frame has only while a client is asking for
-- something: a frame that is asleep lists nothing, and a frame that has had
-- nothing to list for `QUIET_S` goes back to sleep.
--
-- The tick is held to `TICK_BUDGET_MS` of the frame, counted by `os.clock`
-- from before the listing, so the listing is spent from it too. The budget
-- is read between requests and never inside one: a request once taken runs
-- to its reply, and what bounds a single chunk is not this. Before each
-- request after the first, a tick that has spent its budget stops, and
-- every name it did not reach is left on the disk untouched, to be listed
-- again and answered first on the next frame, so order holds across the
-- break. The first request is never refused the tick, so a listing or a
-- chunk that spends the whole budget alone answers one request a frame
-- rather than none.
--
-- A request `admit` answered `error` and left on the disk is held, whether
-- or not its reply reached the disk. A reply that could not be published,
-- `admit`'s own or an op's, is counted, with its reason kept, and noted in
-- `dcs.log` where there is one; the tick goes on, because one lost reply
-- is no reason to lose the rest. A request gone between the listing and
-- the take is nothing to answer, and nothing is counted.
tick = function()
  E.tick = E.tick + 1
  -- The dormant path. A frame that is not armed advances the counter and
  -- returns: no clock read, no listing, no table, no concatenation, no
  -- record. Measured on the interpreter the harness pins, that path grows
  -- `collectgarbage('count')` by zero over a hundred thousand frames once
  -- the callback wrapper has been entered at least once — the wrapper's
  -- first entries cost a one-off 0.797 KB, `pcall` reserving stack, and
  -- nothing per frame after. Every `PROBE_EVERY`-th frame makes the one
  -- call that can end it, on the arm file a client writes. The branch is
  -- escapable at all because a dormant executor that cannot see that file
  -- is a dead one: nothing else on this path would ever look.
  if not E.armed then
    if E.tick % PROBE_EVERY == 0 and attributes(E.arm, "mode") then
      E.armed = true
      E.quiet_since = nil
      -- The waking frame says so in the events log, and no dormant frame
      -- writes anything, which is why the dormant frame's cost is what it
      -- is. The fields are the executor's own words, so none is cut or
      -- stripped the way a client's spelling is; the first is `arm`,
      -- neither of the two markers a reader keys on, so a supervisor
      -- reading the dispatch pairs passes over this line without being
      -- told about it. ADR 0007 is why the markers are the executor's to
      -- spell and a client's to never.
      record("arm|" .. E.stamp .. "|" .. E.tick)
      -- And the heartbeat that says so, off the one clock reading the waking
      -- frame makes. It is made here and not above the branch because the
      -- path that must cost nothing is the frame that stays asleep, and this
      -- frame is the one leaving it.
      local now = rawget(rawget(_G, "os"), "time")()
      E.since = now
      heartbeat(now)
    end
    return
  end
  began = nil
  local clock = rawget(rawget(_G, "os"), "clock")
  local start = clock()
  local lfs = rawget(_G, "lfs")
  local names, listed = {}, {}
  for name in lfs.dir(E.req) do
    if name:sub(-4) == ".req" then
      listed[name] = true
      if not held[name] then
        names[#names + 1] = name
      end
    end
  end
  for name in pairs(held) do
    if not listed[name] then
      held[name] = nil
    end
  end
  table.sort(names)
  for i, name in ipairs(names) do
    began = clock()
    if i > 1 and (began - start) * 1000 >= TICK_BUDGET_MS then
      break
    end
    local path = E.req .. SEP .. name
    local req, status, why, unpublished = admit(path)
    local lost = unpublished and why
    if req then
      -- The markers a supervisor reads the killer out of, around the
      -- dispatch and nothing else: the request is already off the disk,
      -- so a chunk that takes the process with it leaves an opening
      -- marker with no closing one, and the last such marker names it.
      -- A request refused by `admit` never reached an op and never ran,
      -- so it gets no pair; ADR 0007 holds that and what each field
      -- carries. The closing marker reads the status and the cost off
      -- the reply that was just framed, and an op that framed none — one
      -- a driver hung on the namespace — leaves both fields empty rather
      -- than the request unclosed.
      answered, charged = nil, nil
      record("B|" .. marked(req))
      local ok, failed = dispatch(req)
      record("O|" .. field(req.id) .. "|" .. (answered or "") .. "|" .. (charged or ""))
      if not ok then
        lost = failed
      end
    elseif status == "error" and lfs.attributes(path, "mode") == "file" then
      held[name] = true
    end
    if lost then
      E.unpublished = E.unpublished + 1
      E.last_unpublished = lost
      local log = rawget(_G, "log")
      if type(log) == "table" and type(log.write) == "function" then
        log.write(NAME, log.WARNING, "a reply was not published: " .. tostring(lost))
      end
    end
  end
  began = nil
  -- The quiet window, counted off the listing this frame already made. A
  -- listing that held anything is work, and work closes the window without
  -- a clock being read at all, so an executor with requests in front of it
  -- pays nothing for this; only an armed frame with nothing to do reads
  -- one, and it reads `os.time`, for the reason ADR 0008 gives. A held
  -- request counts as something in front of it: it is still on the disk and
  -- still this session's to answer.
  if next(listed) == nil then
    local now = rawget(rawget(_G, "os"), "time")()
    E.quiet_since = E.quiet_since or now
    if now - E.quiet_since >= QUIET_S then
      disarm(now)
    end
  else
    E.quiet_since = nil
  end
end

-- The leaf of the file DCS loaded, read off the debug library rather than
-- assumed, because the installer decides the name and a copy under
-- another one should say so. A chunk that did not come from a file, or a
-- state whose debug library will not say, is `ABSENT`.
local function source()
  local debug = rawget(_G, "debug")
  local ok, info = pcall(function()
    return debug.getinfo(1, "S")
  end)
  local from = ok and type(info) == "table" and rawget(info, "source")
  if type(from) == "string" and from:sub(1, 1) == "@" then
    local path = from:sub(2)
    return path:match("[^/\\]+$") or path
  end
  return "ABSENT"
end

-- The running build, `_APP_VERSION`, read with `rawget` and never called:
-- the string it is, `ABSENT` where the state has none, and its type where
-- it is something else, so a build that made it a function is reported
-- rather than run.
local function app_version()
  local v = rawget(_G, "_APP_VERSION")
  if type(v) == "string" then
    return v
  elseif v == nil then
    return "ABSENT"
  end
  return type(v)
end

-- The handshake, `<output>\executor.txt`: what a client reads to find this
-- session. It is published once the session exists and before anything is
-- registered, so a file a client can read names a session that is whole,
-- and it is one envelope with no body, rewritten in place at every load
-- through `publish`, so a reader never meets a half-written one and the
-- last launch's is replaced rather than added to. The first line names
-- this project: the specification's table opens the file with the name of
-- the project this one replaces, and ADR 0002 moves every name off that
-- project's; the identity line of a file under `Logs\DcsEval` is a name
-- too. The fields then follow the specification's table in its order.
-- `started` is the clock the stamp was built from, as a wall-clock time
-- for a person reading the file; `install_guard` and `lfs_tempdir` are
-- `ABSENT` where their read did not answer, as `roots` left them. Every
-- value goes through the framer, so a path DCS spelt with a byte past
-- ASCII refuses the file and stops the load with the header named: the
-- wire has no spelling for one.
--
-- `true`, or nil and what refused.
local function handshake()
  local os = rawget(_G, "os")
  local headers = {
    { "executor", "dcs-eval" },
    { "protocol", PROTOCOL },
    { "host", E.host },
    { "stamp", E.stamp },
    { "pid", E.pid },
    { "started", os.date("%Y-%m-%d %H:%M:%S", E.started) },
    { "transport", E.session },
    { "req", E.req },
    { "res", E.res },
    { "arm", E.arm },
    { "output", E.output },
    { "eval", ALLOW_EVAL and "allowed" or "disabled" },
    { "ops", ALLOW_EVAL and "ping,eval" or "ping" },
    { "states", states(E.host) },
    { "namespace", NAME },
    { "source", source() },
    { "lfs_tempdir", E.lfs_tempdir },
    { "transport_source", E.transport_source },
    { "install_guard", E.install_guard },
    { "tick_budget_ms", TICK_BUDGET_MS },
    { "instruction_budget", INSTRUCTION_BUDGET },
    { "instruction_ceiling", INSTRUCTION_CEILING },
    { "probe_every", PROBE_EVERY },
    { "quiet_s", QUIET_S },
    { "app_version", app_version() },
    { "max_request_bytes", MAX_REQUEST_BYTES },
    { "max_result_bytes", MAX_RESULT_BYTES },
  }
  local bytes, why = frame(headers)
  if not bytes then
    return nil, why
  end
  return publish(E.handshake, bytes)
end

-- The heartbeat, `<output>\heartbeat.txt`: what a client reads to decide
-- whether this session is alive and ticking, without asking it anything. It
-- is one envelope with no body, published by rename like the handshake, so a
-- reader never meets a half-written one and the file a client stats is
-- always whole. `now` is the caller's own clock reading: every caller
-- already holds one, and a writer that read a second would put a kernel
-- entry back on the armed frame that ADR 0009 keeps at one.
--
-- The fields are what this session actually keeps, which is not everything
-- the specification's table names; ADR 0010 says which four are absent and
-- why. `since` is the wall-clock time of the last transition, spelt the way
-- the handshake spells `started`.
--
-- `E.beat_at` moves whether or not the write lands, so a session that cannot
-- write this file does not try again every frame. A refusal is counted and
-- swallowed for the reason `record` gives: a file a client reads for
-- liveness is not how a request is answered, and a session that cannot write
-- it goes on answering.
heartbeat = function(now)
  E.beat_at = now
  local os = rawget(_G, "os")
  local bytes, why = frame({
    { "protocol", PROTOCOL },
    { "host", E.host },
    { "stamp", E.stamp },
    { "transport", E.session },
    { "phase", E.phase },
    { "armed", E.armed and "yes" or "no" },
    { "since", os.date("%Y-%m-%d %H:%M:%S", E.since) },
    { "ticks", E.tick },
    { "last_callback", last_callback() },
    { "callbacks", table.concat(E.callbacks, ",") },
  })
  local ok
  if bytes then
    ok, why = publish(E.heartbeat, bytes)
  end
  if ok then
    return true
  end
  E.unbeaten = E.unbeaten + 1
  E.last_unbeaten = E.heartbeat .. ": " .. tostring(why)
  return nil
end

local function main()
  -- Loaded once per state. DCS runs `Export.lua` at every mission start and
  -- whether the export state survives between missions is not measured; a
  -- second run here would chain onto this file's own wrapper. A second copy
  -- of the file under another name in `Scripts\Hooks\` would register the
  -- callbacks twice. Either way the first load stands.
  if rawget(_G, NAME) then
    return
  end
  local host, DCS = detect()
  E = roots(host)
  E.started, E.pid, E.stamp = stamp()
  clocked()
  budgeted()
  open_session(E, rawget(_G, "lfs"), rawget(_G, "os"), rawget(_G, "log"))
  E.frame, E.publish, E.reply, E.take, E.parse, E.admit = frame, publish, reply, take, parse, admit
  E.max_request_bytes = MAX_REQUEST_BYTES
  E.ops, E.callbacks, E.unpublished, E.unrecorded = OPS, {}, 0, 0
  E.unbeaten = 0
  E.host, E.phase, E.raised, E.tick = host, host == "hook" and "menu" or "loaded", 0, 0
  -- The time of the last transition and the time of the last beat, both
  -- seeded at load from one reading. Nothing is written here — the heartbeat
  -- is written at a transition, a phase change or an elapsed interval, and a
  -- load is none of those — but the armed path's arithmetic has to hold on a
  -- session whose first armed frame is also its first transition, and a
  -- `since` has to name something before the first transition names it.
  E.since = rawget(_G, "os").time()
  E.beat_at = E.since
  -- The handshake is how a client finds the session, so one that cannot be
  -- written stops the load the way an output directory that cannot be made
  -- does: an executor nothing can find is not running.
  E.handshake = E.output .. SEP .. "executor.txt"
  E.heartbeat = E.output .. SEP .. "heartbeat.txt"
  local ok, why = handshake()
  if not ok then
    error("the handshake could not be published: " .. why, 0)
  end
  -- The load banner: the first line of the generation the rotation above
  -- opened, so the file a supervisor reads exists from the load and names
  -- the session whose records follow, and a launch that handled nothing
  -- still says it ran. Its first field is `load` and not `B` or `O`, which
  -- is how a reader of the markers passes over it; it is written after the
  -- handshake, so no file under the output names a session a client could
  -- not yet find.
  record("load|" .. E.stamp .. "|" .. E.host .. "|" .. PROTOCOL)
  if host == "hook" then
    register_hook(DCS)
  else
    register_export()
  end
  rawset(_G, NAME, E)
end

-- The one `pcall`. A load that raises anywhere above writes one line to
-- `dcs.log`, under this file's name at level ERROR with the reason on one
-- line, and nothing else happens: no callbacks, no global. Where the state
-- has no `log` (the export state has none) there is nowhere to write before
-- the transport exists, and the failure is silent until something outside
-- asks. The handler is itself guarded, so it cannot be the raise that
-- escapes.
local ok, err = pcall(main)
if not ok then
  pcall(function()
    local log = rawget(_G, "log")
    if type(log) == "table" and type(log.write) == "function" then
      log.write(NAME, log.ERROR, "not loaded: " .. (tostring(err):gsub("[\r\n]+", " ")))
    end
  end)
end
