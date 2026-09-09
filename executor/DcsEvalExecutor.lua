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
-- trace of itself. It carries the host, the phase, the raise count and the
-- two write roots with how each was chosen. `last_raise` appears on the
-- first raise a guard catches.
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
-- the output, which has already passed. Nothing is created here; the
-- session directory is made when the session starts.
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
