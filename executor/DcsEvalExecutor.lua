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

-- The hook callbacks registered, and the phase each one moves the executor
-- to, where it moves one. Every callback is guarded, so a raise inside it
-- never reaches DCS, and none returns a value. The `try*` variants are not
-- here on purpose: their return value overrides DCS's own handling and every
-- other hook checking the same thing, so registering one would change
-- behaviour for every tool on the machine. The names past the simulation set
-- were offered in a measured session and not all seen to fire, which is "did
-- not fire" rather than "does not exist"; each costs one table entry.
local HOOK_CALLBACKS = {
  { "onMissionLoadBegin", "load" },
  { "onMissionLoadProgress" },
  { "onMissionLoadEnd" },
  { "onSimulationStart", "sim" },
  { "onSimulationFrame" },
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

-- The export globals chained onto. `LuaExportActivityNextEvent` is not here:
-- it returns the time DCS should call it next, and DCS acts on that return.
local EXPORT_CALLBACKS = {
  { "LuaExportStart", "sim" },
  { "LuaExportBeforeNextFrame" },
  { "LuaExportAfterNextFrame" },
  { "LuaExportStop", "stopped" },
}

-- The namespace: what this file publishes as the global `DcsEvalExecutor`,
-- and only once registration has succeeded, so a failed load leaves no
-- trace of itself. It carries the host, the phase, the raise count, the
-- two write roots with how each was chosen, and the session: its stamp
-- with the time and pid it was built from, its directory with the `req`,
-- `res` and `arm` paths under it, and how many earlier sessions the load
-- swept. `sweep_left` appears when one could not be removed, and
-- `last_raise` on the first raise a guard catches. Once the session exists
-- it also carries the operations on it, `frame`, `publish`, `reply` and
-- `take`, with `max_request_bytes` beside them, so that a driver off DCS
-- can take a request and publish a reply before the tick loop exists, and
-- the tick loop, when it comes, reads them from the same place.
local E

local function nothing() end

-- One wrapper per callback, built here once. DCS calls the wrapper and the
-- wrapper pcalls a body that already exists. Nothing is allocated per call,
-- because one of these is `onSimulationFrame` and the dormant budget is
-- counted in VM instructions. A raise in the body is counted and kept, not
-- rethrown: a raise escaping into a DCS callback takes the session with it.
local function guard(name, phase)
  local body = nothing
  if phase then
    body = function()
      E.phase = phase
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
    callbacks[row[1]] = guard(row[1], row[2])
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
local function chain(name, phase)
  local previous = rawget(_G, name)
  if previous ~= nil and type(previous) ~= "function" then
    return false
  end
  local ours = guard(name, phase)
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
    if chain(row[1], row[2]) then
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
-- client's parser is proved against the bytes this file produces. A name is
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
-- filename here, so the reply takes it whatever it spells. The session's
-- headers are read off the namespace as the reply is framed, so a count
-- the session learns to keep later lands here without the caller changing.
--
-- `true`, or nil and what refused, in which case nothing was written.
local function reply(id, status, headers, body)
  local all = {
    { "status", status },
    { "protocol", 2 },
    { "host", E.host },
    { "stamp", E.stamp },
    { "phase", E.phase },
    { "id", id },
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

-- The most a request may be. Past it the request is answered `bad-request`
-- and never read, so no client can have the executor hold a chunk of that
-- size in a frame. Published on the namespace for the handshake to name.
local MAX_REQUEST_BYTES = 262144

-- One request off the disk: its bytes, with the file gone before they are
-- returned, so that a chunk which kills the process cannot run again at
-- the next listing. The size comes from a stat, and a request over the
-- limit is removed without ever being opened. A file that is not there, or
-- that will not open, is `gone`: it went between the listing and this, or
-- is not a file, and there is nothing to answer and nothing to answer to.
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
  open_session(E, rawget(_G, "lfs"), rawget(_G, "os"), rawget(_G, "log"))
  E.frame, E.publish, E.reply, E.take = frame, publish, reply, take
  E.max_request_bytes = MAX_REQUEST_BYTES
  E.host, E.phase, E.raised = host, host == "hook" and "menu" or "loaded", 0
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
